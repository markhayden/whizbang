//! The gate of an edit recipe (plan mE, task M2; `SPEC.md` section 8, "Gates" and
//! "Scoring edits"): its file, `edit-gate/1`, frozen before its round; the cases it is
//! scored on, `edit-cases/1`; and the decision, from what each case's run did. What reads
//! the store is the binary's (`whizbang::edits_gate`); this is the file and the arithmetic.
//!
//! **The cases** (`edit-cases/1`): the action the runs are of, and each case's id, its
//! family (a scene: variants of one scene are one family), the request id its run was made
//! under, and, for the contact sheet only, what was to be removed or mended.
//!
//! **How each case counts.** A case whose run the store does not hold, or whose run did
//! not complete, is an operational error and not a success; a later run never replaces a
//! failed one (section 9). A finished edit is a success when the check of it under the
//! gate's judge passed (`from = "check"`, rung 3: `fail` and `unsure` are not), or when a
//! person's marks say the named thing or defect is gone and nothing else was damaged
//! (`from = "marks"`, the only source a rung 4 gate takes: section 8, "Scoring edits"). A
//! finished edit with no check or no mark is unscored: not a success, and the gate is
//! inconclusive until it is scored.
//!
//! **The measures**, each with its bar:
//!
//! - task success: successes over every case. Its lower bound is the lower of two, each at
//!   the level corrected across the round's recipes (Bonferroni over `hypotheses`): the
//!   percentile of a bootstrap that resamples families, at
//!   [`resamples_for`](crate::matrix::resamples_for) the count; and the Wilson bound over
//!   the cases, which does not lean on a few families' spread. The share must reach `bar`
//!   and the bound pass `lower_above`;
//! - preservation, exact: on every finished edit, every pixel outside the support of the
//!   kept mask (weight 0) is the picture's own;
//! - blinded quality: every group of the lab decode of this round's pairs ends in one of
//!   `outcomes`, with at least `min_picked` pairs picked;
//! - operational errors: their share below `below`;
//! - cost: the mean settled cost of a finished run at most `guide_micros` times `times`;
//! - latency of the local work alone: each finished run's time less the time its backend
//!   requests took, over the record's time at the size nearest its picture; the median of
//!   that ratio at most `times`. The backends' time is reported, not gated.
//!
//! **The outcome**: `rejected` when preservation, errors, cost, latency or quality fails;
//! else `inconclusive` when anything is short (fewer families than the minimum, an unscored
//! edit, a tail the resamples cannot resolve, no finished edit, no decode, fewer pairs
//! picked than the minimum, a group the decode leaves inconclusive), whatever task success
//! read, since more data could move it; else `rejected` when task success misses either
//! bar; else `admitted`. Too little data is never a pass.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::bootstrap::{MIN_TAIL_RESAMPLES, draws, quantile_type7};
use crate::lab::{is_digest, is_name, is_plain_relative};
use crate::matrix::resamples_for;
use crate::wilson::wilson;

/// What a gate file of this kind says it is.
pub const GATE: &str = "edit-gate/1";

/// What a cases file says it is.
pub const CASES: &str = "edit-cases/1";

/// What the receipt says it is.
pub const RECEIPT: &str = "edit-receipt/1";

/// The most cases a file lists, and the most characters of a case's `what`.
pub const CASES_LIMIT: usize = 500;
pub const WHAT_LIMIT: usize = 200;

/// The outcomes a group of the decode may be required to reach.
pub const QUALITY_OUTCOMES: [&str; 2] = ["preferred", "not_worse"];

/// `edit-gate/1`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditGate {
    pub gate: String,
    pub name: String,
    /// The action every case's run is of.
    pub recipe: String,
    /// 3 or 4.
    pub rung: u8,
    /// The cases file, beside the gate file, and the SHA-256 of its bytes.
    pub cases: String,
    pub cases_sha256: String,
    /// The hypotheses of the round: one for each recipe its gates decide.
    pub hypotheses: usize,
    /// The two-sided level before the correction, as 0.95.
    pub level: f64,
    /// The seed of the families' resamples.
    pub seed: u64,
    pub min_families: usize,
    pub success: Success,
    pub preservation: Preservation,
    pub quality: Quality,
    pub errors: Errors,
    pub cost: Cost,
    pub latency: Latency,
    #[serde(default)]
    pub admits: Option<Admits>,
}

/// Where task success is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadFrom {
    /// The check of each finished edit (rung 3).
    Check,
    /// A person's marks of each finished picture.
    Marks,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Success {
    pub from: ReadFrom,
    /// The judge a check counts under, by the backend name its run was given; any when
    /// not said.
    #[serde(default)]
    pub judge: Option<String>,
    pub bar: f64,
    pub lower_above: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preservation {
    /// Always true: preservation outside the mask is exact.
    pub exact: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quality {
    /// The lab's protocol the pairs were drawn under, beside the gate file, and its digest.
    pub protocol: String,
    pub protocol_sha256: String,
    pub outcomes: Vec<String>,
    pub min_picked: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Errors {
    pub below: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cost {
    /// The recipe's guide figure of one run, in millionths of a US dollar.
    pub guide_micros: u64,
    pub times: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Latency {
    pub times: f64,
    /// The record's times: a picture's pixels and the milliseconds of a run at that size.
    pub record: Vec<Size>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Size {
    pub pixels: u64,
    pub ms: u64,
}

/// What the gate admits, when its outcome is admitted on rung 4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Admits {
    pub rung: u8,
    /// `recipe`.
    pub kind: String,
    pub name: String,
}

fn share(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

impl EditGate {
    /// A gate file, read strictly and checked: its kind; plain names; rung 3 or 4, and a
    /// rung 4 gate reading task success from marks; files beside it, each pinned; at least
    /// one hypothesis and one family; the bars shares; exact preservation; outcomes of the
    /// decode among [`QUALITY_OUTCOMES`]; positive multiples; the record's times; and what
    /// it admits, when said, being this recipe at its rung.
    pub fn parse(text: &str) -> Result<EditGate, String> {
        let gate: EditGate =
            toml::from_str(text).map_err(|e| crate::lab::unreadable("the edit gate", text, &e))?;
        let refuse = |why: &str| Err(format!("the edit gate: {why}"));
        if gate.gate != GATE {
            return refuse(&format!("it is of the kind {GATE}"));
        }
        if !is_name(&gate.name) || !is_name(&gate.recipe) {
            return refuse("its name and its recipe are plain names");
        }
        if !matches!(gate.rung, 3 | 4) {
            return refuse("its rung is 3 or 4");
        }
        if gate.rung == 4 && gate.success.from != ReadFrom::Marks {
            return refuse("a rung 4 gate reads task success from a person's marks, not a check");
        }
        if gate.success.judge.is_some() && gate.success.from != ReadFrom::Check {
            return refuse("a judge is named only for task success read from checks");
        }
        for (file, pin) in [
            (&gate.cases, &gate.cases_sha256),
            (&gate.quality.protocol, &gate.quality.protocol_sha256),
        ] {
            if !is_plain_relative(file) || !is_digest(pin) {
                return refuse("each file it names is beside it and pinned by sha256:<64 hex>");
            }
        }
        if gate.hypotheses == 0 || gate.min_families == 0 {
            return refuse("hypotheses and min_families are at least 1");
        }
        let level_ok = gate.level.is_finite() && gate.level > 0.5 && gate.level < 1.0;
        let bars = [
            gate.success.bar,
            gate.success.lower_above,
            gate.errors.below,
        ];
        if !level_ok || !bars.iter().all(|b| share(*b)) {
            return refuse("its level is between 0.5 and 1, and its bars are shares");
        }
        if !gate.preservation.exact {
            return refuse("preservation outside the mask is exact");
        }
        let outcomes = &gate.quality.outcomes;
        if outcomes.is_empty() || !(outcomes.iter()).all(|o| QUALITY_OUTCOMES.contains(&o.as_str()))
        {
            return refuse("the quality's outcomes are among preferred and not_worse");
        }
        if gate.quality.min_picked == 0 {
            return refuse("min_picked is at least 1");
        }
        let positive = |x: f64| x.is_finite() && x > 0.0;
        if !positive(gate.cost.times) || !positive(gate.latency.times) {
            return refuse("the multiples of cost and latency are positive");
        }
        if gate.latency.record.is_empty() || gate.latency.record.iter().any(|s| s.ms == 0) {
            return refuse("the latency names the record's times, none of 0 ms");
        }
        if let Some(admits) = &gate.admits {
            let ours = admits.kind == "recipe" && admits.name == gate.recipe;
            if !ours || admits.rung != gate.rung {
                return refuse("it admits only its own recipe, kind recipe, at its own rung");
            }
        }
        Ok(gate)
    }

    /// The resamples of its bootstrap.
    pub fn resamples(&self) -> usize {
        resamples_for(self.hypotheses)
    }
}

/// `edit-cases/1`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cases {
    pub kind: String,
    /// The action every case's run is of.
    pub action: String,
    #[serde(default)]
    pub case: Vec<Case>,
}

/// One case: its id, its family, the request id of its run, and what the sheet says was to
/// be removed or mended.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub family: String,
    pub run: String,
    #[serde(default)]
    pub what: Option<String>,
}

impl Cases {
    /// A cases file, read strictly: its kind, a plain action, 1 to [`CASES_LIMIT`] cases,
    /// each with a plain id of its own, a plain family, a request id, and a `what` of one
    /// line of at most [`WHAT_LIMIT`] characters; no two cases under one request id.
    pub fn parse(text: &str) -> Result<Cases, String> {
        let cases: Cases =
            toml::from_str(text).map_err(|e| crate::lab::unreadable("the cases", text, &e))?;
        if cases.kind != CASES || !is_name(&cases.action) {
            return Err(format!(
                "the cases: of the kind {CASES}, with a plain action"
            ));
        }
        if cases.case.is_empty() || cases.case.len() > CASES_LIMIT {
            return Err(format!("the cases: 1 to {CASES_LIMIT} cases"));
        }
        let (mut ids, mut runs) = (BTreeSet::new(), BTreeSet::new());
        for (at, case) in cases.case.iter().enumerate() {
            let place = at + 1;
            if !is_name(&case.id) || !ids.insert(case.id.as_str()) || !is_name(&case.family) {
                return Err(format!(
                    "the cases: case {place} of the list has an id that is not plain or not \
                     its own, or a family that is not plain"
                ));
            }
            let request = whizbang_core::contract::RequestId::parse(&case.run).is_ok();
            if !request || !runs.insert(case.run.as_str()) {
                return Err(format!(
                    "the cases: case {place} of the list names no request id, or one another \
                     case names"
                ));
            }
            let what = case.what.as_deref().unwrap_or("x");
            let one_line = !what.trim().is_empty() && !what.chars().any(char::is_control);
            if !one_line || what.chars().count() > WHAT_LIMIT {
                return Err(format!(
                    "the cases: case {place} of the list says what of more than one line, or \
                     past {WHAT_LIMIT} characters"
                ));
            }
        }
        Ok(cases)
    }

    /// The families the cases fall in.
    pub fn families(&self) -> usize {
        (self.case.iter().map(|c| c.family.as_str()))
            .collect::<BTreeSet<_>>()
            .len()
    }
}

/// What one case's run did, as the store says.
#[derive(Debug, Clone, PartialEq)]
pub enum Observed {
    /// No completed run: none, or one that failed or was interrupted.
    Error,
    Finished(Finished),
}

/// A finished edit.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    /// Whether it succeeded; none when it has no check or no mark.
    pub success: Option<bool>,
    /// Pixels outside the kept mask's support that are not the picture's own.
    pub changed_outside: u64,
    pub settled_micros: u64,
    /// The run's time less its backend requests' time, and that time.
    pub local_ms: u64,
    pub backend_ms: u64,
    /// The pixels of the picture it was given.
    pub pixels: u64,
}

/// A case: its family and what its run did.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub family: String,
    pub observed: Observed,
}

/// One group of the lab decode of the round's pairs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupRead {
    pub group: String,
    pub pairs: u32,
    pub decided: u32,
    pub wins: u32,
    pub losses: u32,
    pub same: u32,
    pub neither: u32,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SuccessRead {
    pub from: ReadFrom,
    pub successes: usize,
    pub of: usize,
    pub unscored: usize,
    pub share: f64,
    pub hypotheses: usize,
    pub seed: u64,
    pub resamples: usize,
    pub lower_percentile: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_lower: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wilson_lower: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower: Option<f64>,
    pub insufficient_tail: bool,
    pub bar: f64,
    pub lower_above: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PreservationRead {
    pub finished: usize,
    pub preserved: usize,
    pub changed_pixels: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QualityRead {
    pub decoded: bool,
    pub picked: u32,
    pub min_picked: u32,
    pub groups: Vec<GroupRead>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostRead {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean_settled_micros: Option<f64>,
    pub limit_micros: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LatencyRead {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_local_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_backend_ms: Option<f64>,
    pub times: f64,
}

/// The decision, with every measure as it was read: aggregates only.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Decided {
    /// `admitted`, `rejected` or `inconclusive`.
    pub outcome: String,
    /// Why, in fixed words: each measure that failed, then each thing that was short.
    pub why: Vec<String>,
    pub cases: usize,
    pub families: usize,
    pub min_families: usize,
    pub finished: usize,
    pub errors: usize,
    pub error_share: f64,
    pub errors_below: f64,
    pub success: SuccessRead,
    pub preservation: PreservationRead,
    pub quality: QualityRead,
    pub cost: CostRead,
    pub latency: LatencyRead,
}

/// The quantile of the standard normal at `p` (0 < p < 1): Acklam's rational
/// approximation, whose relative error is under 1.2e-9.
pub fn normal_quantile(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    let tail = |q: f64| {
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    let low = 0.024_25;
    if p < low {
        tail((-2.0 * p.ln()).sqrt())
    } else if p > 1.0 - low {
        -tail((-2.0 * (1.0 - p).ln()).sqrt())
    } else {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => Some(values[n / 2]),
        _ => Some((values[n / 2 - 1] + values[n / 2]) / 2.0),
    }
}

/// Successes and cases of each family, in the order of the families' names.
fn by_family(observations: &[Observation]) -> Vec<(usize, usize)> {
    let mut families: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    for o in observations {
        let held = families.entry(o.family.as_str()).or_default();
        held.1 += 1;
        if let Observed::Finished(f) = &o.observed {
            held.0 += usize::from(f.success == Some(true));
        }
    }
    families.into_values().collect()
}

/// The gate decided on its cases and the decode of its pairs (none when not decoded yet).
pub fn decide(
    gate: &EditGate,
    observations: &[Observation],
    quality: Option<(u32, Vec<GroupRead>)>,
) -> Result<Decided, String> {
    // A measure that failed whatever more data would say; one of task success, which more
    // data could move; and what is short.
    let (mut failed, mut low, mut short): (Vec<&str>, Vec<&str>, Vec<&str>) =
        (Vec::new(), Vec::new(), Vec::new());
    let finished: Vec<&Finished> = (observations.iter())
        .filter_map(|o| match &o.observed {
            Observed::Finished(f) => Some(f),
            Observed::Error => None,
        })
        .collect();
    let cases = observations.len();
    let families = by_family(observations);
    if families.len() < gate.min_families {
        short.push("too_few_families");
    }
    if finished.is_empty() {
        short.push("no_finished_edit");
    }
    // Task success.
    let successes = finished.iter().filter(|f| f.success == Some(true)).count();
    let unscored = finished.iter().filter(|f| f.success.is_none()).count();
    if unscored > 0 {
        short.push("an_edit_is_unscored");
    }
    let alpha = (1.0 - gate.level) / 2.0 / gate.hypotheses as f64;
    let resamples = gate.resamples();
    let mut bootstrap_lower = None;
    if !families.is_empty() {
        // The draws `resample_indices` makes, one family at a time, in one list.
        let k = families.len();
        let count = u32::try_from(k).map_err(|_| "too many families to resample")?;
        let drawn = draws(gate.seed, count, resamples * k);
        let mut estimates = Vec::with_capacity(resamples);
        for picks in drawn.chunks(k) {
            let (s, n) = (picks.iter()).fold((0, 0), |(s, n), &i| {
                let (successes, cases) = families[i as usize];
                (s + successes, n + cases)
            });
            estimates.push(s as f64 / n.max(1) as f64);
        }
        estimates.sort_by(f64::total_cmp);
        bootstrap_lower = Some(quantile_type7(&estimates, alpha));
    }
    let insufficient_tail = alpha * (resamples as f64) < MIN_TAIL_RESAMPLES;
    if insufficient_tail {
        short.push("a_tail_the_resamples_cannot_resolve");
    }
    let z = normal_quantile(1.0 - alpha);
    let wilson_lower = wilson(successes as u64, cases as u64, z).map(|(low, _)| low);
    let lower = match (bootstrap_lower, wilson_lower) {
        (Some(b), Some(w)) => Some(b.min(w)),
        _ => None,
    };
    let rate = successes as f64 / cases.max(1) as f64;
    if cases > 0 && rate < gate.success.bar {
        low.push("task_success_below_its_bar");
    }
    if lower.is_some_and(|l| l <= gate.success.lower_above) {
        low.push("task_success_bound_not_above_its_bar");
    }
    // Preservation.
    let preserved = finished.iter().filter(|f| f.changed_outside == 0).count();
    if preserved < finished.len() {
        failed.push("a_pixel_outside_the_mask_changed");
    }
    // Operational errors.
    let errors = cases - finished.len();
    let error_share = errors as f64 / cases.max(1) as f64;
    if error_share >= gate.errors.below {
        failed.push("operational_errors_over_their_bar");
    }
    // Cost and latency, over the finished edits.
    let limit_micros = gate.cost.guide_micros as f64 * gate.cost.times;
    let n = finished.len();
    let mean_settled = (n > 0).then(|| {
        finished
            .iter()
            .map(|f| f.settled_micros as f64)
            .sum::<f64>()
            / n as f64
    });
    if mean_settled.is_some_and(|m| m > limit_micros) {
        failed.push("cost_over_its_limit");
    }
    let nearest = |pixels: u64| {
        (gate.latency.record.iter())
            .min_by_key(|s| s.pixels.abs_diff(pixels))
            .map_or(1, |s| s.ms)
    };
    let median_ratio = median(
        (finished.iter())
            .map(|f| f.local_ms as f64 / nearest(f.pixels) as f64)
            .collect(),
    );
    if median_ratio.is_some_and(|r| r > gate.latency.times) {
        failed.push("latency_over_its_limit");
    }
    // Blinded quality.
    let (decoded, picked, groups) = match quality {
        Some((picked, groups)) => (true, picked, groups),
        None => (false, 0, Vec::new()),
    };
    if !decoded {
        short.push("no_decode_of_the_pairs");
    } else if picked < gate.quality.min_picked {
        short.push("too_few_pairs_picked");
    }
    for g in &groups {
        if g.outcome == "inconclusive" {
            short.push("a_group_of_the_decode_is_inconclusive");
        } else if !gate.quality.outcomes.contains(&g.outcome) {
            failed.push("blinded_quality_below_its_bar");
        }
    }
    let outcome = if !failed.is_empty() {
        "rejected"
    } else if !short.is_empty() {
        "inconclusive"
    } else if !low.is_empty() {
        "rejected"
    } else {
        "admitted"
    };
    let mut why: Vec<String> = failed.into_iter().map(str::to_owned).collect();
    for s in low.into_iter().chain(short) {
        if !why.iter().any(|w| w == s) {
            why.push(s.to_owned());
        }
    }
    Ok(Decided {
        outcome: outcome.to_owned(),
        why,
        cases,
        families: families.len(),
        min_families: gate.min_families,
        finished: n,
        errors,
        error_share,
        errors_below: gate.errors.below,
        success: SuccessRead {
            from: gate.success.from,
            successes,
            of: cases,
            unscored,
            share: rate,
            hypotheses: gate.hypotheses,
            seed: gate.seed,
            resamples,
            lower_percentile: alpha,
            bootstrap_lower,
            wilson_lower,
            lower,
            insufficient_tail,
            bar: gate.success.bar,
            lower_above: gate.success.lower_above,
        },
        preservation: PreservationRead {
            finished: n,
            preserved,
            changed_pixels: finished.iter().map(|f| f.changed_outside).sum(),
        },
        quality: QualityRead {
            decoded,
            picked,
            min_picked: gate.quality.min_picked,
            groups,
        },
        cost: CostRead {
            mean_settled_micros: mean_settled,
            limit_micros,
        },
        latency: LatencyRead {
            median_ratio,
            median_local_ms: median(finished.iter().map(|f| f.local_ms as f64).collect()),
            median_backend_ms: median(finished.iter().map(|f| f.backend_ms as f64).collect()),
            times: gate.latency.times,
        },
    })
}
