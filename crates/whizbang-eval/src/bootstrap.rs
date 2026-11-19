//! The paired bootstrap, resampling by family (`SPEC.md` section 8, "Gates"). Ported from
//! squint's `crates/squint-eval/src/bootstrap.rs`, at the commit `docs/sources.toml` pins,
//! with the statistic made a parameter: squint resampled one estimand (the difference in
//! detected failures); a gate here resamples the difference in balanced accuracy and the
//! difference in false rejects ([`crate::metrics`]).
//!
//! **Reproducibility is the contract, and it is squint's.** Families are sorted by id and
//! items by id within a family; the random numbers are those of ChaCha20 seeded from a
//! 64-bit seed as `rand_core` 0.9 seeds it, and each family of a resample is drawn as
//! `rand` 0.9.5 draws `random_range(0..families)`, one draw per family, in order. Squint
//! took both from those crates, pinned exactly; whizbang writes the two steps out below
//! (about sixty lines) rather than take two crates for them, and its tests hold them to
//! squint's own output on the same seeds (`metrics::the_ported_vectors_of_squint_give_squints_numbers`).
//! The interval is formed from percentiles by the Hyndman and Fan type 7 quantile, with
//! Bonferroni-adjusted percentiles for `m` hypotheses.

use crate::metrics::{Computed, Expected};

/// One labelled item under both arms of a paired comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedItem {
    pub family: String,
    pub id: String,
    pub expected: Expected,
    pub control: Computed,
    pub treatment: Computed,
}

/// A family in canonical order: its id, and its items sorted by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    pub id: String,
    pub items: Vec<PairedItem>,
}

impl Family {
    /// Whether an item's verdict differs between the arms.
    pub fn differs(&self) -> bool {
        self.items.iter().any(|i| i.control != i.treatment)
    }

    fn expected_fails(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.expected == Expected::Fail)
            .count()
    }
}

/// Every family, sorted by id, with its items sorted by id. Two items of one id in a
/// family are refused.
pub fn families(items: &[PairedItem]) -> Result<Vec<Family>, String> {
    let mut families: Vec<Family> = Vec::new();
    for item in items {
        match families.iter_mut().find(|f| f.id == item.family) {
            Some(f) => f.items.push(item.clone()),
            None => families.push(Family {
                id: item.family.clone(),
                items: vec![item.clone()],
            }),
        }
    }
    families.sort_by(|a, b| a.id.cmp(&b.id));
    for f in &mut families {
        f.items.sort_by(|a, b| a.id.cmp(&b.id));
        if f.items.windows(2).any(|w| w[0].id == w[1].id) {
            return Err(format!("family {:?} has two items of one id", f.id));
        }
    }
    Ok(families)
}

/// ChaCha20 with a 64-bit block counter and stream 0, as `rand_chacha` 0.9 runs it: its
/// words in order, block after block.
struct ChaCha {
    state: [u32; 16],
    block: [u32; 16],
    next: usize,
}

impl ChaCha {
    /// Seeded from a 64-bit number as `rand_core` 0.9's `seed_from_u64` does: eight
    /// outputs of PCG32 make the key.
    fn seeded(mut seed: u64) -> ChaCha {
        let mut state = [0u32; 16];
        state[..4].copy_from_slice(&[0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]);
        for word in &mut state[4..12] {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(11_634_580_027_462_260_723);
            let xorshifted = (((seed >> 18) ^ seed) >> 27) as u32;
            *word = xorshifted.rotate_right((seed >> 59) as u32);
        }
        ChaCha {
            state,
            block: [0; 16],
            next: 16,
        }
    }

    fn next_u32(&mut self) -> u32 {
        if self.next == 16 {
            let mut x = self.state;
            for _ in 0..10 {
                for [a, b, c, d] in [
                    [0, 4, 8, 12],
                    [1, 5, 9, 13],
                    [2, 6, 10, 14],
                    [3, 7, 11, 15],
                    [0, 5, 10, 15],
                    [1, 6, 11, 12],
                    [2, 7, 8, 13],
                    [3, 4, 9, 14],
                ] {
                    x[a] = x[a].wrapping_add(x[b]);
                    x[d] = (x[d] ^ x[a]).rotate_left(16);
                    x[c] = x[c].wrapping_add(x[d]);
                    x[b] = (x[b] ^ x[c]).rotate_left(12);
                    x[a] = x[a].wrapping_add(x[b]);
                    x[d] = (x[d] ^ x[a]).rotate_left(8);
                    x[c] = x[c].wrapping_add(x[d]);
                    x[b] = (x[b] ^ x[c]).rotate_left(7);
                }
            }
            for (out, (mixed, start)) in self.block.iter_mut().zip(x.iter().zip(&self.state)) {
                *out = mixed.wrapping_add(*start);
            }
            let counter = (u64::from(self.state[13]) << 32 | u64::from(self.state[12])) + 1;
            self.state[12] = counter as u32;
            self.state[13] = (counter >> 32) as u32;
            self.next = 0;
        }
        self.next += 1;
        self.block[self.next - 1]
    }

    /// A number below `n` (1 to `u32::MAX`), as `rand` 0.9.5 draws `random_range(0..n)`
    /// for a `usize` that fits 32 bits: Canon's method, biased form, one word and at
    /// most one more.
    fn below(&mut self, n: u32) -> u32 {
        let wide = u64::from(self.next_u32()) * u64::from(n);
        let (mut high, low) = ((wide >> 32) as u32, wide as u32);
        if low > n.wrapping_neg() {
            let more = ((u64::from(self.next_u32()) * u64::from(n)) >> 32) as u32;
            if low.checked_add(more).is_none() {
                high += 1;
            }
        }
        high
    }
}

/// The resamples for a seed: `resamples` lists of `families` indices each, drawn with
/// replacement.
pub fn resample_indices(
    seed: u64,
    families: usize,
    resamples: usize,
) -> Result<Vec<Vec<usize>>, String> {
    let n = u32::try_from(families)
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("{families} families cannot be resampled"))?;
    let mut rng = ChaCha::seeded(seed);
    Ok((0..resamples)
        .map(|_| (0..n).map(|_| rng.below(n) as usize).collect())
        .collect())
}

/// `count` numbers below `n` from a seed, drawn as each family of a resample is: what
/// [`resample_indices`] rests on, open to a test at a range no resample reaches.
pub fn draws(seed: u64, n: u32, count: usize) -> Vec<u32> {
    let mut rng = ChaCha::seeded(seed);
    (0..count).map(|_| rng.below(n.max(1))).collect()
}

/// Hyndman and Fan type 7: linear interpolation between order statistics, `h = (n - 1) p`.
pub fn quantile_type7(sorted: &[f64], p: f64) -> f64 {
    match sorted.len() {
        0 => f64::NAN,
        1 => sorted[0],
        n => {
            let h = (n - 1) as f64 * p.clamp(0.0, 1.0);
            let lo = h.floor() as usize;
            let hi = (lo + 1).min(n - 1);
            sorted[lo] + (h - lo as f64) * (sorted[hi] - sorted[lo])
        }
    }
}

/// Fewer resamples than this expected below the lower percentile cannot resolve the tail.
pub const MIN_TAIL_RESAMPLES: f64 = 20.0;

/// What one bootstrap gave.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Interval {
    pub seed: u64,
    pub resamples: usize,
    pub hypotheses: usize,
    /// The families resampled.
    pub families: usize,
    /// Families with an item whose verdict differs between the arms.
    pub differing_families: usize,
    /// The statistic on the whole sample.
    pub point: f64,
    pub lower_percentile: f64,
    pub upper_percentile: f64,
    pub lower: f64,
    pub upper: f64,
    /// The expected count of resamples below the lower percentile is under
    /// [`MIN_TAIL_RESAMPLES`].
    pub insufficient_tail: bool,
}

/// The paired bootstrap of `statistic` over `families`: each resample draws as many
/// families as there are, with replacement, every item of a drawn family kept; the
/// statistic is given the drawn families, with their multiplicity. `alpha` is the
/// two-sided level (0.05 for 95%), split over `hypotheses` (Bonferroni).
pub fn interval(
    families: &[Family],
    seed: u64,
    resamples: usize,
    hypotheses: usize,
    alpha: f64,
    statistic: impl Fn(&[&Family]) -> Option<f64>,
) -> Result<Interval, String> {
    if resamples == 0 || hypotheses == 0 {
        return Err("resamples and hypotheses must be positive".into());
    }
    let all: Vec<&Family> = families.iter().collect();
    let point = statistic(&all).ok_or("the statistic has nothing to stand on")?;
    let mut estimates = Vec::with_capacity(resamples);
    for picks in resample_indices(seed, families.len(), resamples)? {
        let drawn: Vec<&Family> = picks.iter().map(|&i| &families[i]).collect();
        // A resample the statistic cannot be worked out on (no expected fail drawn,
        // say) counts as no change.
        estimates.push(statistic(&drawn).unwrap_or(0.0));
    }
    estimates.sort_by(f64::total_cmp);
    let lower_percentile = alpha / 2.0 / hypotheses as f64;
    let upper_percentile = 1.0 - lower_percentile;
    Ok(Interval {
        seed,
        resamples,
        hypotheses,
        families: families.len(),
        differing_families: families.iter().filter(|f| f.differs()).count(),
        point,
        lower_percentile,
        upper_percentile,
        lower: quantile_type7(&estimates, lower_percentile),
        upper: quantile_type7(&estimates, upper_percentile),
        insufficient_tail: lower_percentile * (resamples as f64) < MIN_TAIL_RESAMPLES,
    })
}

/// Squint's estimand, as squint ran it: the item-weighted difference in the share of
/// expected-fail items detected (computed `fail`), treatment minus control, over the
/// families that hold an expected-fail item. Kept so that its numbers can be held to
/// squint's on the same input.
pub fn detection_difference(
    items: &[PairedItem],
    seed: u64,
    resamples: usize,
    hypotheses: usize,
) -> Result<Interval, String> {
    let kept: Vec<Family> = families(items)?
        .into_iter()
        .filter(|f| f.expected_fails() > 0)
        .collect();
    if kept.is_empty() {
        return Err("no family has an expected-fail item".into());
    }
    let mut result = interval(&kept, seed, resamples, hypotheses, 0.05, |drawn| {
        let (mut difference, mut fails) = (0i64, 0usize);
        for family in drawn {
            for i in family.items.iter().filter(|i| i.expected == Expected::Fail) {
                difference += i64::from(i.treatment == Computed::Fail)
                    - i64::from(i.control == Computed::Fail);
                fails += 1;
            }
        }
        Some(if fails == 0 {
            0.0
        } else {
            difference as f64 / fails as f64
        })
    })?;
    // Squint counted a family as differing only on an expected-fail item.
    result.differing_families = kept
        .iter()
        .filter(|f| {
            f.items.iter().any(|i| {
                i.expected == Expected::Fail
                    && (i.control == Computed::Fail) != (i.treatment == Computed::Fail)
            })
        })
        .count();
    Ok(result)
}
