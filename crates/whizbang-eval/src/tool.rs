//! How a tool's measurement is scored on rung 2 (plan `docs/plans/m2c-milestone-2.md`, task
//! X1, item 8): `whizbang eval tool` reads one number of a tool on each item of a set, and
//! this scores that number's separation of the set's fail items from its pass items.
//!
//! - **The number.** A tool's primary measurement, with the way that means "changed"
//!   ([`Changed`]): larger, or smaller. A tool that reports several regions or subjects has
//!   reduced them already, by its declared rule.
//! - **The area under the ROC curve** of that number, a fail item the positive: the chance
//!   that a fail item reads more changed than a pass item, a tie counting half (the
//!   Mann-Whitney form). With a 95% interval resampled by family, as E2's bootstrap draws
//!   them ([`crate::bootstrap::resample_indices`], [`crate::bootstrap::quantile_type7`]); a
//!   resample that draws no fail item or no pass item reads one half, no separation.
//! - **The balanced accuracy at a threshold chosen on one half and read on the other.** The
//!   families of every set scored are pooled, sorted, and shuffled by a fixed seed (a
//!   Fisher-Yates shuffle on the bootstrap's draws), so that no family is on both sides; the
//!   first half (rounded down) chooses the threshold that gives it the best balanced
//!   accuracy, the second is read at it. With fewer than [`HALVED_LEAST`] families pooled, or
//!   a half without a fail or a pass item, no threshold is chosen and the accuracy is
//!   inconclusive: the area alone stands.
//! - **An item whose tool failed or answered nothing** is an error: counted and named apart,
//!   and never a pass or a fail.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::bootstrap::{draws, quantile_type7, resample_indices};
use crate::sets::Expected;

/// The fewest families pooled for a threshold to be chosen on half of them.
pub const HALVED_LEAST: usize = 4;

/// Which way of a measurement means "changed".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Changed {
    Larger,
    Smaller,
}

impl Changed {
    /// The measurement turned so that larger is more changed.
    fn oriented(self, value: f64) -> f64 {
        match self {
            Changed::Larger => value,
            Changed::Smaller => -value,
        }
    }
}

/// A tool's read of one item: its set, item and family, its label, and the number, or why
/// there is none.
#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    pub set: String,
    pub item: String,
    pub family: String,
    pub expected: Expected,
    pub value: Result<f64, String>,
}

/// An item whose tool gave no number.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Errored {
    pub set: String,
    pub item: String,
    pub why: String,
}

/// The balanced accuracy at a threshold chosen on half of the families.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Accuracy {
    /// No threshold was chosen; the sentence says why.
    Inconclusive { why: String },
    /// Chosen on `chosen_on` (families, as `set/family`), read on `read_on`.
    Read {
        /// The threshold, on the measurement as the tool reports it: an item at or past
        /// it, in the way of [`Changed`], reads fail.
        threshold: f64,
        /// The balanced accuracy on the half that is read.
        balanced_accuracy: f64,
        /// The balanced accuracy on the half that chose it.
        chosen_accuracy: f64,
        chosen_on: Vec<String>,
        read_on: Vec<String>,
    },
}

/// What scoring a tool on its reads gave.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scored {
    pub changed: Changed,
    /// Items with a number, and of them the fail and the pass items.
    pub items: usize,
    pub fail_items: usize,
    pub pass_items: usize,
    /// Families pooled, as `set/family`.
    pub families: usize,
    pub auc: f64,
    pub auc_lower: f64,
    pub auc_upper: f64,
    pub seed: u64,
    pub resamples: usize,
    pub accuracy: Accuracy,
    pub errors: Vec<Errored>,
}

/// The area under the ROC curve of these numbers, a fail item the positive and larger more
/// changed: none without a fail item and a pass item.
pub fn auc(scored: &[(f64, Expected)]) -> Option<f64> {
    let fails: Vec<f64> = pick(scored, Expected::Fail);
    let passes: Vec<f64> = pick(scored, Expected::Pass);
    if fails.is_empty() || passes.is_empty() {
        return None;
    }
    let mut wins = 0.0;
    for f in &fails {
        for p in &passes {
            wins += match f.total_cmp(p) {
                std::cmp::Ordering::Greater => 1.0,
                std::cmp::Ordering::Equal => 0.5,
                std::cmp::Ordering::Less => 0.0,
            };
        }
    }
    Some(wins / (fails.len() * passes.len()) as f64)
}

fn pick(scored: &[(f64, Expected)], which: Expected) -> Vec<f64> {
    scored
        .iter()
        .filter(|(_, expected)| *expected == which)
        .map(|(value, _)| *value)
        .collect()
}

/// The balanced accuracy of reading fail at or past `threshold` (larger more changed):
/// none without a fail item and a pass item.
pub fn balanced_accuracy(scored: &[(f64, Expected)], threshold: f64) -> Option<f64> {
    let fails = pick(scored, Expected::Fail);
    let passes = pick(scored, Expected::Pass);
    if fails.is_empty() || passes.is_empty() {
        return None;
    }
    let caught = fails.iter().filter(|v| **v >= threshold).count() as f64;
    let kept = passes.iter().filter(|v| **v < threshold).count() as f64;
    Some((caught / fails.len() as f64 + kept / passes.len() as f64) / 2.0)
}

/// The threshold that gives these numbers the best balanced accuracy, and that accuracy:
/// each candidate is a midpoint between two neighbouring distinct values, or past either
/// end; the first best, from the lowest, is taken.
pub fn choose(scored: &[(f64, Expected)]) -> Option<(f64, f64)> {
    let mut values: Vec<f64> = scored.iter().map(|(value, _)| *value).collect();
    values.sort_by(f64::total_cmp);
    values.dedup();
    let (first, last) = (*values.first()?, *values.last()?);
    let mut candidates = vec![first - 1.0];
    candidates.extend(values.windows(2).map(|pair| (pair[0] + pair[1]) / 2.0));
    candidates.push(last + 1.0);
    let mut best: Option<(f64, f64)> = None;
    for threshold in candidates {
        let accuracy = balanced_accuracy(scored, threshold)?;
        if best.is_none_or(|(_, held)| accuracy > held) {
            best = Some((threshold, accuracy));
        }
    }
    best
}

/// The pooled families, as `set/family`, in the order a fixed seed shuffles them.
pub fn shuffled(mut families: Vec<String>, seed: u64) -> Vec<String> {
    families.sort();
    families.dedup();
    let n = families.len();
    if n < 2 {
        return families;
    }
    // Fisher-Yates from the end: the k-th swap's place is the first draw below n - k of
    // the seed moved on by k.
    for k in 0..n - 1 {
        let top = n - k;
        let below = u32::try_from(top).unwrap_or(u32::MAX);
        let j = draws(seed.wrapping_add(k as u64), below, 1)[0] as usize;
        families.swap(top - 1, j);
    }
    families
}

/// Scores a tool's reads: the area with its interval, and the balanced accuracy across the
/// halves. `resamples` is the bootstrap's count (the tests use at most 200).
pub fn score(
    reads: &[Read],
    changed: Changed,
    seed: u64,
    resamples: usize,
) -> Result<Scored, String> {
    let family = |read: &Read| format!("{}/{}", read.set, read.family);
    let mut errors = Vec::new();
    let mut by_family: BTreeMap<String, Vec<(f64, Expected)>> = BTreeMap::new();
    for read in reads {
        match &read.value {
            Ok(value) if value.is_finite() => by_family
                .entry(family(read))
                .or_default()
                .push((changed.oriented(*value), read.expected)),
            Ok(_) => errors.push(Errored {
                set: read.set.clone(),
                item: read.item.clone(),
                why: "the tool's number is not finite".to_owned(),
            }),
            Err(why) => errors.push(Errored {
                set: read.set.clone(),
                item: read.item.clone(),
                why: why.clone(),
            }),
        }
    }
    let all: Vec<(f64, Expected)> = by_family.values().flatten().copied().collect();
    let point = auc(&all).ok_or("the reads hold no fail item or no pass item with a number")?;
    if resamples == 0 {
        return Err("resamples must be positive".to_owned());
    }
    let groups: Vec<&Vec<(f64, Expected)>> = by_family.values().collect();
    let mut estimates = Vec::with_capacity(resamples);
    for picks in resample_indices(seed, groups.len(), resamples)? {
        let drawn: Vec<(f64, Expected)> = picks.iter().flat_map(|&i| groups[i].clone()).collect();
        estimates.push(auc(&drawn).unwrap_or(0.5));
    }
    estimates.sort_by(f64::total_cmp);
    let names: Vec<String> = by_family.keys().cloned().collect();
    let accuracy = across_halves(&by_family, names.clone(), changed, seed);
    Ok(Scored {
        changed,
        items: all.len(),
        fail_items: pick(&all, Expected::Fail).len(),
        pass_items: pick(&all, Expected::Pass).len(),
        families: names.len(),
        auc: point,
        auc_lower: quantile_type7(&estimates, 0.025),
        auc_upper: quantile_type7(&estimates, 0.975),
        seed,
        resamples,
        accuracy,
        errors,
    })
}

fn across_halves(
    by_family: &BTreeMap<String, Vec<(f64, Expected)>>,
    names: Vec<String>,
    changed: Changed,
    seed: u64,
) -> Accuracy {
    let pooled = names.len();
    if pooled < HALVED_LEAST {
        return Accuracy::Inconclusive {
            why: format!(
                "{pooled} families pooled, fewer than {HALVED_LEAST}: no threshold is chosen"
            ),
        };
    }
    let order = shuffled(names, seed);
    let (chosen_on, read_on) = order.split_at(pooled / 2);
    let of = |half: &[String]| -> Vec<(f64, Expected)> {
        half.iter()
            .flat_map(|name| by_family.get(name).cloned().unwrap_or_default())
            .collect()
    };
    let Some((threshold, chosen_accuracy)) = choose(&of(chosen_on)) else {
        return Accuracy::Inconclusive {
            why: "the half that chooses holds no fail item or no pass item".to_owned(),
        };
    };
    let Some(balanced_accuracy) = balanced_accuracy(&of(read_on), threshold) else {
        return Accuracy::Inconclusive {
            why: "the half that is read holds no fail item or no pass item".to_owned(),
        };
    };
    Accuracy::Read {
        // Back to the measurement as the tool reports it.
        threshold: match changed {
            Changed::Larger => threshold,
            Changed::Smaller => -threshold,
        },
        balanced_accuracy,
        chosen_accuracy,
        chosen_on: chosen_on.to_vec(),
        read_on: read_on.to_vec(),
    }
}
