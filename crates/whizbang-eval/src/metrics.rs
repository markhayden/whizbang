//! Measures over scored decisions (`SPEC.md` section 8, "Gates"; plan 2C, task E2, item 1).
//! Ported in part from squint's `crates/squint-eval/src/metrics.rs`, at the commit
//! `docs/sources.toml` pins: the counts and the confusion of expected against computed,
//! the rates with their Wilson intervals, the cost and wall medians, the agreement of later
//! repetitions with the first, and the paired comparison of two arms on the items both
//! scored. New here: balanced accuracy, the gate's primary measure, and the paired
//! differences a gate decides by, each resampled by family ([`crate::bootstrap`]).
//!
//! Not ported: squint's localisation (the regions a judge pointed at, matched to labelled
//! regions), which no gate decides by; its report (milestone 5's viewer).
//!
//! **Every headline number comes from repetition 1**, the decision repetition; later
//! repetitions give only the self-disagreement.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use crate::sets::Expected;

use crate::bootstrap::{self, Family, Interval, PairedItem};
use crate::wilson::{Z_95, wilson};

/// What one decision came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Computed {
    Pass,
    Fail,
    Unsure,
    /// The run ended without a verdict.
    Error,
}

/// One scored decision.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub set: String,
    pub item: String,
    /// The item's family in its set.
    pub family: String,
    pub repetition: u32,
    pub expected: Expected,
    pub computed: Computed,
    pub cost_usd: f64,
    pub wall_ms: u64,
}

/// A proportion with its 95% Wilson interval. With nothing to count, the value is 0 and
/// the interval the whole unit interval.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Proportion {
    pub count: usize,
    pub total: usize,
    pub value: f64,
    pub low: f64,
    pub high: f64,
}

impl Proportion {
    pub fn of(count: usize, total: usize) -> Proportion {
        match wilson(count as u64, total as u64, Z_95) {
            Some((low, high)) => Proportion {
                count,
                total,
                value: count as f64 / total as f64,
                low,
                high,
            },
            None => Proportion {
                count,
                total,
                value: 0.0,
                low: 0.0,
                high: 1.0,
            },
        }
    }
}

/// Counts over the decision repetition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub items: usize,
    pub expected_fail: usize,
    pub expected_pass: usize,
    /// Expected fail, computed fail: a catch.
    pub detected: usize,
    /// Expected pass, computed pass.
    pub passed: usize,
    /// Expected pass, computed fail.
    pub false_rejects: usize,
    /// Expected fail, computed pass (plan m3, task U1: a guardrail a gate may state).
    pub false_accepts: usize,
    pub unsure: usize,
    pub errors: usize,
}

impl Counts {
    pub fn of<'a>(rows: impl IntoIterator<Item = &'a Row>) -> Counts {
        let mut c = Counts::default();
        for r in rows {
            c.items += 1;
            match (r.expected, r.computed) {
                (Expected::Fail, computed) => {
                    c.expected_fail += 1;
                    c.detected += usize::from(computed == Computed::Fail);
                    c.false_accepts += usize::from(computed == Computed::Pass);
                }
                (Expected::Pass, computed) => {
                    c.expected_pass += 1;
                    c.passed += usize::from(computed == Computed::Pass);
                    c.false_rejects += usize::from(computed == Computed::Fail);
                }
            }
            c.unsure += usize::from(r.computed == Computed::Unsure);
            c.errors += usize::from(r.computed == Computed::Error);
        }
        c
    }

    /// The mean of the share of expected fails caught and the share of expected passes
    /// passed. An `unsure` or an error is neither a catch nor a pass, so it counts against
    /// whichever share its item belongs to. `None` without an item of either kind.
    pub fn balanced_accuracy(&self) -> Option<f64> {
        if self.expected_fail == 0 || self.expected_pass == 0 {
            return None;
        }
        let caught = self.detected as f64 / self.expected_fail as f64;
        let passed = self.passed as f64 / self.expected_pass as f64;
        Some((caught + passed) / 2.0)
    }

    /// The share of every item decided correctly: an expected fail caught or an expected
    /// pass passed, over every item (plan e1, task S1). An `unsure` or an error is neither.
    /// `None` without an item.
    pub fn decided_correctly(&self) -> Option<f64> {
        (self.items > 0).then(|| (self.detected + self.passed) as f64 / self.items as f64)
    }

    /// The share of expected fails passed; `None` without one.
    pub fn false_accept_rate(&self) -> Option<f64> {
        (self.expected_fail > 0).then(|| self.false_accepts as f64 / self.expected_fail as f64)
    }

    /// The share of expected passes failed; `None` without one.
    pub fn false_reject_rate(&self) -> Option<f64> {
        (self.expected_pass > 0).then(|| self.false_rejects as f64 / self.expected_pass as f64)
    }
}

/// The measures of one arm (one cell), over its rows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CellMetrics {
    pub counts: Counts,
    /// Rows expected fail and expected pass; columns computed pass, fail, unsure, error.
    pub confusion: [[usize; 4]; 2],
    pub balanced_accuracy: Option<f64>,
    /// Over expected fails.
    pub detected: Proportion,
    /// Over expected passes.
    pub false_rejects: Proportion,
    /// Over expected fails.
    pub false_accepts: Proportion,
    /// Over every decision.
    pub unsure: Proportion,
    pub errors: Proportion,
    pub cost_total_usd: f64,
    pub cost_median_usd: f64,
    pub wall_total_ms: u64,
    /// The middle value, or the mean of the two middle values rounded down.
    pub wall_median_ms: u64,
    /// The highest repetition seen.
    pub repetitions: u32,
    /// Of every (item, repetition above 1), the share whose verdict differs from the
    /// item's repetition 1: the arm's self-disagreement. `None` with one repetition.
    pub self_disagreement: Option<f64>,
}

fn column(c: Computed) -> usize {
    match c {
        Computed::Pass => 0,
        Computed::Fail => 1,
        Computed::Unsure => 2,
        Computed::Error => 3,
    }
}

/// The measures of one arm.
pub fn cell(rows: &[Row]) -> CellMetrics {
    let headline: Vec<&Row> = rows.iter().filter(|r| r.repetition == 1).collect();
    let counts = Counts::of(headline.iter().copied());
    let mut confusion = [[0usize; 4]; 2];
    for r in &headline {
        let row = usize::from(r.expected == Expected::Pass);
        confusion[row][column(r.computed)] += 1;
    }
    let first: BTreeMap<(&str, &str), Computed> = headline
        .iter()
        .map(|r| ((r.set.as_str(), r.item.as_str()), r.computed))
        .collect();
    let (mut differ, mut later, mut repetitions) = (0usize, 0usize, 1u32);
    for r in rows.iter().filter(|r| r.repetition > 1) {
        repetitions = repetitions.max(r.repetition);
        if let Some(c) = first.get(&(r.set.as_str(), r.item.as_str())) {
            later += 1;
            differ += usize::from(*c != r.computed);
        }
    }
    let mut costs: Vec<f64> = headline.iter().map(|r| r.cost_usd).collect();
    let mut walls: Vec<u64> = headline.iter().map(|r| r.wall_ms).collect();
    CellMetrics {
        counts,
        confusion,
        balanced_accuracy: counts.balanced_accuracy(),
        detected: Proportion::of(counts.detected, counts.expected_fail),
        false_rejects: Proportion::of(counts.false_rejects, counts.expected_pass),
        false_accepts: Proportion::of(counts.false_accepts, counts.expected_fail),
        unsure: Proportion::of(counts.unsure, counts.items),
        errors: Proportion::of(counts.errors, counts.items),
        cost_total_usd: costs.iter().sum(),
        cost_median_usd: median_f64(&mut costs),
        wall_total_ms: walls.iter().sum(),
        wall_median_ms: median_u64(&mut walls),
        repetitions,
        self_disagreement: (later > 0).then(|| differ as f64 / later as f64),
    }
}

/// How a paired comparison is resampled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resampling {
    pub seed: u64,
    pub resamples: usize,
    pub hypotheses: usize,
    /// Two-sided: 0.05 for a 95% interval.
    pub alpha: f64,
}

/// Two arms compared on the items both scored at repetition 1.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Paired {
    pub control: CellMetrics,
    pub treatment: CellMetrics,
    /// Items scored under both arms.
    pub items: usize,
    /// Families of those items.
    pub families: usize,
    /// Families with an item whose verdict differs between the arms.
    pub differing_families: usize,
    /// Treatment minus control, in balanced accuracy. `None` when the paired items lack
    /// an expected fail or an expected pass.
    pub gain: Option<Interval>,
    /// Treatment minus control, in the share of expected passes failed.
    pub false_reject_increase: Option<Interval>,
    /// Treatment minus control, in the share of expected fails passed.
    pub false_accept_increase: Option<Interval>,
    /// Treatment minus control, in the share of items decided correctly (plan e1, task S1).
    /// Not written in a gate's receipt, whose form is frozen by its version.
    #[serde(skip_serializing)]
    pub decided_correctly_difference: Option<Interval>,
}

/// Compares two arms' rows. A family is named by its set and its id, so two sets never
/// share one.
pub fn paired(control: &[Row], treatment: &[Row], how: Resampling) -> Result<Paired, String> {
    let first = |rows: &[Row]| -> BTreeMap<(String, String), Row> {
        rows.iter()
            .filter(|r| r.repetition == 1)
            .map(|r| ((r.set.clone(), r.item.clone()), r.clone()))
            .collect()
    };
    let (c, x) = (first(control), first(treatment));
    let mut items = Vec::new();
    for (key, c) in &c {
        let Some(x) = x.get(key) else { continue };
        if c.expected != x.expected || c.family != x.family {
            return Err(format!(
                "the item {}/{} is labelled differently in the two arms",
                key.0, key.1
            ));
        }
        items.push(PairedItem {
            family: format!("{}/{}", key.0, c.family),
            id: format!("{}/{}", key.0, key.1),
            expected: c.expected,
            control: c.computed,
            treatment: x.computed,
        });
    }
    let families = bootstrap::families(&items)?;
    let pooled = |drawn: &[&Family], arm: fn(&PairedItem) -> Computed| {
        let rows: Vec<Row> = drawn
            .iter()
            .flat_map(|f| &f.items)
            .map(|i| Row {
                set: String::new(),
                item: String::new(),
                family: String::new(),
                repetition: 1,
                expected: i.expected,
                computed: arm(i),
                cost_usd: 0.0,
                wall_ms: 0,
            })
            .collect();
        Counts::of(&rows)
    };
    let difference = |measure: fn(&Counts) -> Option<f64>| {
        move |drawn: &[&Family]| {
            let c = measure(&pooled(drawn, |i| i.control))?;
            let x = measure(&pooled(drawn, |i| i.treatment))?;
            Some(x - c)
        }
    };
    let resample = |measure: fn(&Counts) -> Option<f64>| {
        if families.is_empty() {
            return None;
        }
        bootstrap::interval(
            &families,
            how.seed,
            how.resamples,
            how.hypotheses,
            how.alpha,
            difference(measure),
        )
        .ok()
    };
    let paired_rows = |want: &[Row]| -> Vec<Row> {
        want.iter()
            .filter(|r| {
                let key = (r.set.clone(), r.item.clone());
                c.contains_key(&key) && x.contains_key(&key)
            })
            .cloned()
            .collect()
    };
    Ok(Paired {
        control: cell(&paired_rows(control)),
        treatment: cell(&paired_rows(treatment)),
        items: items.len(),
        families: families.len(),
        differing_families: families.iter().filter(|f| f.differs()).count(),
        gain: resample(Counts::balanced_accuracy),
        false_reject_increase: resample(Counts::false_reject_rate),
        false_accept_increase: resample(Counts::false_accept_rate),
        decided_correctly_difference: resample(Counts::decided_correctly),
    })
}

fn median_f64(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn median_u64(v: &mut [u64]) -> u64 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2
    }
}

/// Two arms compared by the share of expected failures caught (plan mE, task M1, item 14;
/// the study's measure, `docs/results/2026-11-tool-study.md`, "Method"): over the items
/// labelled fail that both arms scored, resampled over the families that hold one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatchPaired {
    /// Items labelled fail scored under both arms.
    pub items: usize,
    /// Families with at least one of them: what is resampled.
    pub families: usize,
    pub control: f64,
    pub treatment: f64,
    /// Treatment minus control in the share caught; `None` without an item labelled fail.
    pub gain: Option<Interval>,
}

/// The catch-rate comparison of two arms. An item's catch is whether its verdict is a fail:
/// repetition 1 alone, or, with `mean`, the mean over every repetition each arm scored.
pub fn paired_catch(
    control: &[Row],
    treatment: &[Row],
    how: Resampling,
    mean: bool,
) -> Result<CatchPaired, String> {
    // Each item's share caught under one arm, with its family.
    let shares = |rows: &[Row]| -> BTreeMap<(String, String), (String, f64)> {
        let mut by: BTreeMap<(String, String), (String, usize, usize)> = BTreeMap::new();
        for r in rows.iter().filter(|r| r.expected == Expected::Fail) {
            if !mean && r.repetition != 1 {
                continue;
            }
            let entry = (by.entry((r.set.clone(), r.item.clone())))
                .or_insert_with(|| (r.family.clone(), 0, 0));
            entry.1 += usize::from(r.computed == Computed::Fail);
            entry.2 += 1;
        }
        (by.into_iter())
            .map(|(key, (family, caught, seen))| (key, (family, caught as f64 / seen as f64)))
            .collect()
    };
    let (c, x) = (shares(control), shares(treatment));
    // By family: the control's catches, the treatment's, and the items.
    let mut families: BTreeMap<String, (f64, f64, usize)> = BTreeMap::new();
    for (key, (family, caught)) in &c {
        let Some((other, treated)) = x.get(key) else {
            continue;
        };
        if other != family {
            return Err(format!(
                "the item {}/{} is in two families in the two arms",
                key.0, key.1
            ));
        }
        let entry = families.entry(format!("{}/{family}", key.0)).or_default();
        entry.0 += caught;
        entry.1 += treated;
        entry.2 += 1;
    }
    let families: Vec<(f64, f64, usize)> = families.into_values().collect();
    let total = |drawn: &mut dyn Iterator<Item = &(f64, f64, usize)>| {
        let (mut c, mut t, mut n) = (0.0, 0.0, 0usize);
        for (fc, ft, fn_) in drawn {
            c += fc;
            t += ft;
            n += fn_;
        }
        (c, t, n)
    };
    let (sc, st, n) = total(&mut families.iter());
    let rate = |v: f64| if n == 0 { 0.0 } else { v / n as f64 };
    let gain = if n == 0 {
        None
    } else {
        let mut estimates = Vec::with_capacity(how.resamples);
        for picks in bootstrap::resample_indices(how.seed, families.len(), how.resamples)? {
            let (dc, dt, dn) = total(&mut picks.iter().map(|&i| &families[i]));
            estimates.push(if dn == 0 { 0.0 } else { (dt - dc) / dn as f64 });
        }
        estimates.sort_by(f64::total_cmp);
        let lower_percentile = how.alpha / 2.0 / how.hypotheses.max(1) as f64;
        Some(Interval {
            seed: how.seed,
            resamples: how.resamples,
            hypotheses: how.hypotheses,
            families: families.len(),
            differing_families: (families.iter()).filter(|(fc, ft, _)| fc != ft).count(),
            point: (st - sc) / n as f64,
            lower_percentile,
            upper_percentile: 1.0 - lower_percentile,
            lower: bootstrap::quantile_type7(&estimates, lower_percentile),
            upper: bootstrap::quantile_type7(&estimates, 1.0 - lower_percentile),
            insufficient_tail: lower_percentile * (how.resamples as f64)
                < bootstrap::MIN_TAIL_RESAMPLES,
        })
    };
    Ok(CatchPaired {
        items: n,
        families: families.len(),
        control: rate(sc),
        treatment: rate(st),
        gain,
    })
}
