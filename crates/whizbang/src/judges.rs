//! `whizbang eval judges` (plan e1, task S1): judges scored against each other, or one arm
//! against another within each judge, from the runs that matrices made, with cost beside
//! accuracy; and the two decision rules of the slice, `judge-pick/1` and `submission/1`,
//! applied in code so that neither choice is made by hand.
//!
//! **What is read.** The store only: each cell of each matrix named, by the matrix's
//! identity in the run's argument `cell` (runs made under a gate are not a matrix's), its last
//! attempt; the run's record for its verdict, what it settled at and its wall time; and the
//! run's diagnostic `rejections.json`, which task T1 adds to `check` (each rejection's class
//! and the cause the run ended by). A run without that diagnostic counts as **unknown**, never
//! as clean. Labels are read through [`crate::gating::labelled`], as `eval run --gate` reads
//! them: a blind set's from the labels' folder, held to its seal, never from under the sets'
//! folder.
//!
//! **The sample, frozen.** Selection measures (balanced accuracy, decided correctly, catch
//! rate, false rejects, false accepts, unsure, errors, cost per decision, median time) use
//! repetition 1 only, so every judge is scored on the same items with the same weight.
//! Repetition 2 gives self-agreement. Submission measures use every decision of the cells
//! compared, every repetition. Total settled spend is apart from cost per decision.
//!
//! **Scopes.** Each use case alone (the sets of `<use case>/...`, so `source-fidelity` is
//! `scenes-v1`, its adversarial set and `blind-v3` pooled), the sets of split `dev` pooled,
//! the sets of split `blind` pooled (today `blind-v3` alone), and all pooled.
//!
//! **What is written** (`judges/1`, TOML): aggregates only. Never an item, a family, a verdict
//! of an item or a word of a judge.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use whizbang_core::contract::Capability;
use whizbang_core::contract::record::{DIAGNOSTICS_PREFIX, Decision as Verdict};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Reader};
use whizbang_eval::bootstrap::{quantile_type7, resample_indices};
use whizbang_eval::matrix::{Cell, Matrix};
use whizbang_eval::metrics::{self, Computed, Counts, Expected, Resampling, Row};
use whizbang_eval::sets;

use whizbang_squint::check::{Cause, REJECTIONS};
use whizbang_squint::use_case::{Class, FORMATS, Recovery};

use crate::door::printable;
use crate::evaluation::{Ended, cell_of};

/// The form of what the command writes.
pub const SCHEMA: &str = "judges/1";
/// The contracts of the rejections diagnostic read: T1's, and F1's
/// ([`whizbang_squint::check::REJECTIONS_CONTRACT`]), which adds the format the run resolved,
/// the further classes of a rejection and the recoveries. Any other is unknown.
pub const CONTRACTS: [&str; 2] = ["rejections/1", "rejections/2"];
/// The word a class, cause or kind the build does not know is counted under: nothing read
/// from the store is written as it was read.
pub const UNKNOWN: &str = "unknown";
/// What the record says of how rejections are counted by class.
pub const CLASS_COUNTING: &str = "rejections_by_class counts each rejection's first class \
    (its class); also_by_class counts the further classes a rejection named (its also)";
/// The default judge a rule keeps when it picks none: the model the config guide's first
/// example names today.
pub const DEFAULT_JUDGE: &str = "claude-haiku-5-5";
/// The seed and the resamples of every interval the command works out.
pub const SEED: u64 = 20_261_120;
pub const RESAMPLES: usize = 2_000;

/// `judge-pick/1`: an eligible judge's errors are under this share of its decisions.
pub const ERRORS_UNDER: f64 = 0.02;
/// `judge-pick/1`: an eligible judge decides at least this share correctly.
pub const DECIDED_AT_LEAST: f64 = 0.80;
/// `judge-pick/1`: the lower bound of a judge's difference from the best is at least minus
/// this.
pub const MARGIN: f64 = 0.03;
/// `judge-pick/1`: a judge's median time is at most this many times the fastest's.
pub const TIME_RATIO: f64 = 2.0;
/// `judge-pick/1`: a judge's decided correctly is at least the best's minus this.
pub const DECIDED_BEHIND: f64 = 0.05;
/// `submission/1`, clause (a): the least submission failures of the control that are
/// halved, and else the least first rejections.
pub const FAILURES_LEAST: usize = 5;
pub const FIRST_REJECTIONS_LEAST: usize = 10;
/// `submission/1`, clause (c): the most a judge's balanced accuracy may fall, or its false
/// reject or false accept rate rise, in points.
pub const ACCURACY_GIVEN: f64 = 0.02;
/// What a comparison of two shares at a threshold allows for the rounding of a float, so
/// that a difference of exactly the threshold is on its side.
const TOLERANCE: f64 = 1e-9;

/// The two causes of a run's end that are submission failures (plan e1, "Words").
pub const FAILURE_CAUSES: [Cause; 2] = [Cause::InvalidSubmission, Cause::NoSubmissionAfterRepair];

/// How the judges are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Each judge against the best, under one arm, on the same items.
    Judges,
    /// Within each judge, the treatment arm against the control arm.
    Arms,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Judges => "judges",
            Mode::Arms => "arms",
        }
    }
}

/// A decision rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// The default judge (plan e1, task S1, item 5).
    JudgePick,
    /// The submission format (plan e1, task F2, item 2).
    Submission,
}

impl Rule {
    pub fn name(self) -> &'static str {
        match self {
            Rule::JudgePick => "judge-pick/1",
            Rule::Submission => "submission/1",
        }
    }

    /// A rule by its name.
    pub fn named(name: &str) -> Result<Rule, String> {
        match name {
            "judge-pick/1" => Ok(Rule::JudgePick),
            "submission/1" => Ok(Rule::Submission),
            _ => Err("the rule is judge-pick/1 or submission/1".to_owned()),
        }
    }
}

/// What task T1 kept of one decision's submissions: each rejection's first class in order
/// and the further classes it named, the cause the run ended by, the submission format the
/// run resolved, and each recovery's kind (submission v2, task F1). A name the build does not
/// know is kept as [`UNKNOWN`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Submissions {
    pub classes: Vec<String>,
    pub also: Vec<String>,
    pub cause: String,
    /// `v1` or `v2`; `v1` under `rejections/1`, which knew no other.
    pub submission: String,
    pub recoveries: Vec<String>,
}

/// A name read from the store as it is counted: itself when it is one of `known`, else
/// [`UNKNOWN`].
fn known_or_unknown(name: &str, known: &[&str]) -> String {
    if known.contains(&name) {
        name.to_owned()
    } else {
        UNKNOWN.to_owned()
    }
}

impl Submissions {
    /// The diagnostic as T1 writes it, `{"contract": "rejections/1", "rejections": [{"turn":
    /// n, "class": "..."}, ...], "cause": "..."}`, or as F1 writes it, `rejections/2` with
    /// `"submission"`, `"also": [...]` on a rejection, and `"recoveries": [{"turn": n, "kind":
    /// "..."}]`. `None` when it is of another contract or not of its form: such a run is
    /// unknown.
    pub fn parse(bytes: &[u8]) -> Option<Submissions> {
        let json: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let contract = json.get("contract")?.as_str()?;
        if !CONTRACTS.contains(&contract) {
            return None;
        }
        let classes_known: Vec<&str> = Class::ALL.iter().map(|c| c.name()).collect();
        let causes_known: Vec<&str> = Cause::ALL.iter().map(|c| c.name()).collect();
        let mut classes = Vec::new();
        let mut also = Vec::new();
        for one in json.get("rejections")?.as_array()? {
            classes.push(known_or_unknown(
                one.get("class")?.as_str()?,
                &classes_known,
            ));
            match one.get("also") {
                None | Some(serde_json::Value::Null) => {}
                Some(list) => {
                    for more in list.as_array()? {
                        also.push(known_or_unknown(more.as_str()?, &classes_known));
                    }
                }
            }
        }
        let recoveries = match json.get("recoveries") {
            None | Some(serde_json::Value::Null) => Vec::new(),
            Some(list) => (list.as_array()?.iter())
                .map(|one| {
                    let kind = one.get("kind").and_then(serde_json::Value::as_str)?;
                    let kinds: Vec<&str> = Recovery::ALL.iter().map(|r| r.name()).collect();
                    Some(known_or_unknown(kind, &kinds))
                })
                .collect::<Option<Vec<String>>>()?,
        };
        let submission = match (contract, json.get("submission")) {
            ("rejections/1", None) => "v1".to_owned(),
            (_, Some(word)) => known_or_unknown(word.as_str()?, &FORMATS),
            _ => return None,
        };
        Some(Submissions {
            classes,
            also,
            cause: known_or_unknown(json.get("cause")?.as_str()?, &causes_known),
            submission,
            recoveries,
        })
    }

    /// Whether the decision ended because of a rejection.
    pub fn failed(&self) -> bool {
        FAILURE_CAUSES.iter().any(|c| c.name() == self.cause)
    }

    /// Whether its first submission was rejected, whether or not the repair saved it: the
    /// first rejection is always the first submission's, since it is what asks the repair.
    pub fn first_rejected(&self) -> bool {
        !self.classes.is_empty()
    }
}

/// The submission format an arm names (`C;submission=v2`), when it names one.
pub fn format_of(arm: &str) -> Option<&str> {
    arm.split(';')
        .find_map(|setting| setting.strip_prefix("submission="))
}

/// One ended decision: the last attempt of a cell, completed or failed (a failed one is an
/// error), with its item's label.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub set: String,
    pub item: String,
    /// The item's family in its set.
    pub family: String,
    pub use_case: String,
    /// The set's split: `dev` or `blind`.
    pub split: String,
    pub backend: String,
    pub model: String,
    pub arm: String,
    pub repetition: u32,
    pub expected: Expected,
    pub computed: Computed,
    /// What its paid requests settled at, with what was unresolved, in US dollars.
    pub cost_usd: f64,
    /// What they settled at alone.
    pub settled_usd: f64,
    pub wall_ms: u64,
    /// What T1 kept; `None` when the run kept nothing of it.
    pub submissions: Option<Submissions>,
}

/// What the matrices hold, ready to score.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Population {
    pub decisions: Vec<Decision>,
    /// Every cell the matrices name.
    pub cells: usize,
    /// Cells whose last attempt was interrupted, or that have none.
    pub open: usize,
    /// Ended cells of an item with no label.
    pub unlabelled: usize,
}

/// What the command is asked beside the population.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    pub mode: Mode,
    /// In `judges` mode, the one arm every judge is scored under; in `arms` mode, the
    /// control.
    pub arm: Option<String>,
    /// In `arms` mode, the arm against the control.
    pub treatment: Option<String>,
    pub rule: Option<Rule>,
    pub resampling: Resampling,
}

/// The resampling the command uses: 95%, one hypothesis.
pub fn resampling() -> Resampling {
    Resampling {
        seed: SEED,
        resamples: RESAMPLES,
        hypotheses: 1,
        alpha: 0.05,
    }
}

/// A share with its family-resampled 95% interval.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Estimate {
    pub point: f64,
    pub lower: f64,
    pub upper: f64,
}

impl From<&whizbang_eval::bootstrap::Interval> for Estimate {
    fn from(i: &whizbang_eval::bootstrap::Interval) -> Estimate {
        Estimate {
            point: i.point,
            lower: i.lower,
            upper: i.upper,
        }
    }
}

/// The submission measures of a cell, over every decision of it, every repetition.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SubmissionMeasures {
    pub decisions: usize,
    /// Decisions whose run kept no rejections diagnostic of a contract this build reads:
    /// never counted as clean.
    pub unknown: usize,
    /// Decisions whose diagnostic says another submission format than their arm names:
    /// counted here, and not scored.
    pub mismatched: usize,
    pub failures: usize,
    /// Failures over the decisions whose diagnostic was kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_rate: Option<f64>,
    pub first_rejections: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_rejection_rate: Option<f64>,
    /// Each rejection by its first class ([`CLASS_COUNTING`]).
    pub rejections_by_class: BTreeMap<String, usize>,
    /// The further classes rejections named.
    pub also_by_class: BTreeMap<String, usize>,
    pub causes: BTreeMap<String, usize>,
    pub recoveries_by_kind: BTreeMap<String, usize>,
}

impl SubmissionMeasures {
    fn of<'a>(decisions: impl IntoIterator<Item = &'a Decision>) -> SubmissionMeasures {
        let mut m = SubmissionMeasures::default();
        for d in decisions {
            m.decisions += 1;
            let Some(s) = &d.submissions else {
                m.unknown += 1;
                continue;
            };
            if format_of(&d.arm).is_some_and(|named| named != s.submission) {
                m.mismatched += 1;
                continue;
            }
            m.failures += usize::from(s.failed());
            m.first_rejections += usize::from(s.first_rejected());
            for class in &s.classes {
                *m.rejections_by_class.entry(class.clone()).or_default() += 1;
            }
            for class in &s.also {
                *m.also_by_class.entry(class.clone()).or_default() += 1;
            }
            *m.causes.entry(s.cause.clone()).or_default() += 1;
            for kind in &s.recoveries {
                *m.recoveries_by_kind.entry(kind.clone()).or_default() += 1;
            }
        }
        let known = m.decisions - m.unknown - m.mismatched;
        m.failure_rate = (known > 0).then(|| m.failures as f64 / known as f64);
        m.first_rejection_rate = (known > 0).then(|| m.first_rejections as f64 / known as f64);
        m
    }
}

/// The measures of one judge under one arm in one scope.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Measures {
    pub backend: String,
    pub model: String,
    pub arm: String,
    /// Decisions of repetition 1: the selection measures' denominator.
    pub decisions: usize,
    pub expected_fail: usize,
    pub expected_pass: usize,
    pub caught: usize,
    pub passed: usize,
    pub false_rejects: usize,
    pub false_accepts: usize,
    pub unsure: usize,
    pub errors: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balanced_accuracy: Option<Estimate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_correctly: Option<Estimate>,
    /// The resamples balanced accuracy or decided correctly could not be worked out on (no
    /// expected fail or no expected pass drawn): with any, that interval is not given.
    pub unworked_resamples: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catch_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_reject_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_accept_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsure_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_rate: Option<f64>,
    pub cost_per_decision_usd: f64,
    pub median_ms: u64,
    /// Items with a repetition 2 beside their repetition 1.
    pub repeated_items: usize,
    /// Of those, the share with the same verdict both times.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_agreement: Option<f64>,
    /// What every decision of the cell settled at, every repetition, in US dollars.
    pub settled_usd: f64,
    pub submissions: SubmissionMeasures,
}

/// One comparison: the treatment minus the control, on the items both scored at
/// repetition 1, each difference resampled by family.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comparison {
    pub backend: String,
    pub model: String,
    pub arm: String,
    pub against_backend: String,
    pub against_model: String,
    pub against_arm: String,
    pub items: usize,
    pub families: usize,
    pub differing_families: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balanced_accuracy: Option<Estimate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_correctly: Option<Estimate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_reject_rate: Option<Estimate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_accept_rate: Option<Estimate>,
}

/// Everything worked out in one scope.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scope {
    pub name: String,
    pub measures: Vec<Measures>,
    pub comparisons: Vec<Comparison>,
}

/// The clauses of `judge-pick/1` for one judge, each passed or failed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Clauses {
    pub backend: String,
    pub model: String,
    pub errors_under_2_percent: bool,
    pub decided_correctly_at_least_0_80: bool,
    pub eligible: bool,
    /// The lower bound of the 95% interval of its balanced accuracy minus the best's, the
    /// best taken again inside each resample; for an eligible judge only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difference_from_best_lower: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difference_from_best_point: Option<f64>,
    pub lower_bound_at_least_minus_3_points: bool,
    pub median_time_at_most_twice_the_fastest: bool,
    pub decided_correctly_within_5_points_of_the_best: bool,
    pub within_the_margin: bool,
    pub cost_per_decision_usd: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_reject_rate: Option<f64>,
}

/// What `judge-pick/1` came to in one scope.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pick {
    pub scope: String,
    /// `pick` or `no_pick`.
    pub outcome: String,
    /// The judge picked, or under no pick the default kept.
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    pub why: String,
    pub judges: Vec<Clauses>,
}

/// One judge's clauses (b) and (c) of `submission/1`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FormatClauses {
    pub backend: String,
    pub model: String,
    pub control_failures: usize,
    pub treatment_failures: usize,
    pub no_new_failures: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balanced_accuracy_change: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_reject_rate_change: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_accept_rate_change: Option<f64>,
    pub accuracy_kept: bool,
}

/// What `submission/1` came to.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FormatDecision {
    /// `adopted`, `not_adopted`, `too_few_failures` or `no_decision`.
    pub outcome: String,
    /// The clause that settled it.
    pub settled_by: String,
    /// The format kept or adopted: the treatment arm when adopted, the control otherwise.
    pub format: String,
    pub control: String,
    pub treatment: String,
    pub why: String,
    pub control_failures: usize,
    pub treatment_failures: usize,
    pub control_first_rejections: usize,
    pub treatment_first_rejections: usize,
    pub unknown: usize,
    /// Decisions whose diagnostic names another format than their arm.
    pub mismatched: usize,
    /// (a): `failures`, `first_rejections` or `too_few`; and whether it held.
    pub fewer_failures_by: String,
    pub fewer_failures: bool,
    pub judges: Vec<FormatClauses>,
}

/// What the scoring came to, before what it was read from is added.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scored {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treatment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    pub cells: usize,
    pub decisions: usize,
    pub open: usize,
    pub unlabelled: usize,
    /// Why the population is not complete; `None` when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete: Option<String>,
    pub seed: u64,
    pub resamples: usize,
    /// How rejections are counted by class ([`CLASS_COUNTING`]).
    pub class_counting: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub picks: Vec<Pick>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<FormatDecision>,
    pub scopes: Vec<Scope>,
}

/// A judge: its backend and its model.
type JudgeKey = (String, String);

fn key_of(d: &Decision) -> JudgeKey {
    (d.backend.clone(), d.model.clone())
}

fn row_of(d: &Decision) -> Row {
    Row {
        set: d.set.clone(),
        item: d.item.clone(),
        family: d.family.clone(),
        repetition: d.repetition,
        expected: d.expected,
        computed: d.computed,
        cost_usd: d.cost_usd,
        wall_ms: d.wall_ms,
    }
}

/// The completeness check both rules share, run before any comparison: every cell of the
/// matrices ended, completed or failed; every item labelled; and in `judges` mode, every
/// judge with a decision of repetition 1 under the arm on every item of the population.
pub fn incomplete(population: &Population, asked: &Asked) -> Option<String> {
    let mut why = Vec::new();
    if population.open > 0 {
        why.push(format!(
            "{} of the matrices' {} cells have not ended (not run, or interrupted)",
            population.open, population.cells
        ));
    }
    if population.unlabelled > 0 {
        why.push(format!(
            "{} ended cells are of items with no label",
            population.unlabelled
        ));
    }
    if asked.mode == Mode::Judges
        && let Some(arm) = &asked.arm
    {
        let items: BTreeSet<(&str, &str)> = (population.decisions.iter())
            .map(|d| (d.set.as_str(), d.item.as_str()))
            .collect();
        let judges: BTreeSet<JudgeKey> = population.decisions.iter().map(key_of).collect();
        let held: BTreeSet<(JudgeKey, &str, &str)> = (population.decisions.iter())
            .filter(|d| d.repetition == 1 && &d.arm == arm)
            .map(|d| (key_of(d), d.set.as_str(), d.item.as_str()))
            .collect();
        let missing = judges.len() * items.len() - held.len();
        if missing > 0 {
            why.push(format!(
                "{missing} pairs of a judge and an item have no decision of repetition 1 under \
                 the arm {}",
                printable(arm)
            ));
        }
    }
    (!why.is_empty()).then(|| why.join("; "))
}

/// The scopes over the population, in order: all, dev, blind, then each use case.
fn scopes(decisions: &[Decision]) -> Vec<(String, Vec<&Decision>)> {
    let mut out: Vec<(String, Vec<&Decision>)> =
        vec![("all".to_owned(), decisions.iter().collect())];
    for split in ["dev", "blind"] {
        let these: Vec<&Decision> = decisions.iter().filter(|d| d.split == split).collect();
        if !these.is_empty() {
            out.push((split.to_owned(), these));
        }
    }
    let cases: BTreeSet<&str> = decisions.iter().map(|d| d.use_case.as_str()).collect();
    for case in cases {
        let these = decisions.iter().filter(|d| d.use_case == case).collect();
        out.push((case.to_owned(), these));
    }
    out
}

/// The families of a set of decisions, each named by its set and its id, in order, each
/// holding the indices of its decisions.
fn families_of(rows: &[&Decision]) -> Vec<Vec<Row>> {
    let mut by: BTreeMap<(&str, &str), Vec<Row>> = BTreeMap::new();
    for d in rows {
        (by.entry((d.set.as_str(), d.family.as_str())).or_default())
            .push(bare(d.expected, d.computed));
    }
    by.into_values().collect()
}

/// A row of a label and a verdict alone: what a count needs, with no string to copy.
fn bare(expected: Expected, computed: Computed) -> Row {
    Row {
        set: String::new(),
        item: String::new(),
        family: String::new(),
        repetition: 1,
        expected,
        computed,
        cost_usd: 0.0,
        wall_ms: 0,
    }
}

/// The 95% interval of a statistic of the decisions, resampled by family: the statistic on
/// the whole, and the percentiles of it over the resamples. Not worked out (`Err`, with the
/// resamples it could not be worked out on) when the statistic cannot be worked out on the
/// whole or on any resample: a resample is never filled in with a value.
fn estimate(
    rows: &[&Decision],
    how: &Resampling,
    statistic: fn(&Counts) -> Option<f64>,
) -> Result<Estimate, usize> {
    let families = families_of(rows);
    let point = statistic(&Counts::of(families.iter().flatten())).ok_or(0usize)?;
    let draws = resample_indices(how.seed, families.len(), how.resamples).map_err(|_| 0usize)?;
    let mut values = Vec::with_capacity(draws.len());
    let mut unworked = 0usize;
    for picks in &draws {
        match statistic(&Counts::of(picks.iter().flat_map(|&f| &families[f]))) {
            Some(value) => values.push(value),
            None => unworked += 1,
        }
    }
    if unworked > 0 {
        return Err(unworked);
    }
    values.sort_by(f64::total_cmp);
    let low = how.alpha / 2.0 / how.hypotheses as f64;
    Ok(Estimate {
        point,
        lower: quantile_type7(&values, low),
        upper: quantile_type7(&values, 1.0 - low),
    })
}

fn rate(count: usize, total: usize) -> Option<f64> {
    (total > 0).then(|| count as f64 / total as f64)
}

fn median(mut v: Vec<u64>) -> u64 {
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

/// The measures of one judge under one arm over these decisions (every repetition).
fn measures(judge: &JudgeKey, arm: &str, all: &[&Decision], how: &Resampling) -> Measures {
    let first: Vec<&Decision> = all.iter().copied().filter(|d| d.repetition == 1).collect();
    let rows: Vec<Row> = first.iter().map(|d| bare(d.expected, d.computed)).collect();
    let c = Counts::of(&rows);
    let by_item: BTreeMap<(&str, &str), Computed> = (first.iter())
        .map(|d| ((d.set.as_str(), d.item.as_str()), d.computed))
        .collect();
    let (mut repeated, mut same) = (0usize, 0usize);
    for d in all.iter().filter(|d| d.repetition == 2) {
        if let Some(computed) = by_item.get(&(d.set.as_str(), d.item.as_str())) {
            repeated += 1;
            same += usize::from(*computed == d.computed);
        }
    }
    let cost: f64 = first.iter().map(|d| d.cost_usd).sum();
    let balanced = estimate(&first, how, Counts::balanced_accuracy);
    let decided = estimate(&first, how, Counts::decided_correctly);
    let unworked = |e: &Result<Estimate, usize>| *e.as_ref().err().unwrap_or(&0);
    Measures {
        backend: judge.0.clone(),
        model: judge.1.clone(),
        arm: arm.to_owned(),
        decisions: c.items,
        expected_fail: c.expected_fail,
        expected_pass: c.expected_pass,
        caught: c.detected,
        passed: c.passed,
        false_rejects: c.false_rejects,
        false_accepts: c.false_accepts,
        unsure: c.unsure,
        errors: c.errors,
        unworked_resamples: unworked(&balanced).max(unworked(&decided)),
        balanced_accuracy: balanced.ok(),
        decided_correctly: decided.ok(),
        catch_rate: rate(c.detected, c.expected_fail),
        false_reject_rate: c.false_reject_rate(),
        false_accept_rate: c.false_accept_rate(),
        unsure_rate: rate(c.unsure, c.items),
        error_rate: rate(c.errors, c.items),
        cost_per_decision_usd: if c.items == 0 {
            0.0
        } else {
            cost / c.items as f64
        },
        median_ms: median(first.iter().map(|d| d.wall_ms).collect()),
        repeated_items: repeated,
        self_agreement: rate(same, repeated),
        settled_usd: all.iter().map(|d| d.settled_usd).sum(),
        submissions: SubmissionMeasures::of(all.iter().copied()),
    }
}

fn compared(
    of: (&JudgeKey, &str, &[&Decision]),
    against: (&JudgeKey, &str, &[&Decision]),
    how: &Resampling,
) -> Option<Comparison> {
    let rows = |list: &[&Decision]| -> Vec<Row> { list.iter().map(|d| row_of(d)).collect() };
    let paired = metrics::paired(&rows(against.2), &rows(of.2), *how).ok()?;
    let e = |i: &Option<whizbang_eval::bootstrap::Interval>| i.as_ref().map(Estimate::from);
    Some(Comparison {
        backend: of.0.0.clone(),
        model: of.0.1.clone(),
        arm: of.1.to_owned(),
        against_backend: against.0.0.clone(),
        against_model: against.0.1.clone(),
        against_arm: against.1.to_owned(),
        items: paired.items,
        families: paired.families,
        differing_families: paired.differing_families,
        balanced_accuracy: e(&paired.gain),
        decided_correctly: e(&paired.decided_correctly_difference),
        false_reject_rate: e(&paired.false_reject_increase),
        false_accept_rate: e(&paired.false_accept_increase),
    })
}

/// The decisions of each judge under an arm, in the order of the judges.
fn by_judge<'a>(rows: &[&'a Decision], arm: &str) -> BTreeMap<JudgeKey, Vec<&'a Decision>> {
    let mut out: BTreeMap<JudgeKey, Vec<&Decision>> = BTreeMap::new();
    for d in rows.iter().copied().filter(|d| d.arm == arm) {
        out.entry(key_of(d)).or_default().push(d);
    }
    out
}

/// `judge-pick/1` over the decisions of one scope under one arm (plan e1, task S1, item 5).
/// `incomplete` is the completeness check's finding, which makes the outcome no pick.
pub fn judge_pick(
    scope: &str,
    rows: &[&Decision],
    arm: &str,
    incomplete: Option<&str>,
    how: &Resampling,
) -> Pick {
    let no_pick = |why: String, judges: Vec<Clauses>| Pick {
        scope: scope.to_owned(),
        outcome: "no_pick".to_owned(),
        model: DEFAULT_JUDGE.to_owned(),
        backend: None,
        why: format!("{why}; the current default, {DEFAULT_JUDGE}, is kept"),
        judges,
    };
    if let Some(why) = incomplete {
        return no_pick(format!("the matrices are incomplete: {why}"), Vec::new());
    }
    let judges = by_judge(rows, arm);
    let judges: Vec<(JudgeKey, Vec<&Decision>)> = (judges.into_iter())
        .map(|(k, list)| (k, list.into_iter().filter(|d| d.repetition == 1).collect()))
        .collect();
    if judges.is_empty() {
        return no_pick(
            "no judge has a decision in this scope".to_owned(),
            Vec::new(),
        );
    }
    let counts: Vec<Counts> = (judges.iter())
        .map(|(_, list)| {
            Counts::of(
                &list
                    .iter()
                    .map(|d| bare(d.expected, d.computed))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let decided: Vec<Option<f64>> = counts.iter().map(Counts::decided_correctly).collect();
    let balanced: Vec<Option<f64>> = counts.iter().map(Counts::balanced_accuracy).collect();
    if balanced.iter().chain(&decided).any(Option::is_none) {
        return no_pick(
            "a judge's balanced accuracy or decided correctly cannot be worked out here (an \
             expected fail or an expected pass is missing)"
                .to_owned(),
            Vec::new(),
        );
    }
    let decided: Vec<f64> = decided.into_iter().flatten().collect();
    let eligible: Vec<bool> = (counts.iter().zip(&decided))
        .map(|(c, dc)| {
            (c.errors as f64) < ERRORS_UNDER * c.items as f64 && *dc + TOLERANCE >= DECIDED_AT_LEAST
        })
        .collect();
    let medians: Vec<u64> = (judges.iter())
        .map(|(_, list)| median(list.iter().map(|d| d.wall_ms).collect()))
        .collect();
    let costs: Vec<f64> = (judges.iter())
        .map(|(_, list)| list.iter().map(|d| d.cost_usd).sum::<f64>() / list.len() as f64)
        .collect();
    let chosen: Vec<usize> = (0..judges.len()).filter(|&j| eligible[j]).collect();
    let margins = if chosen.is_empty() {
        None
    } else {
        reselected(&judges, &chosen, how)
    };
    if !chosen.is_empty() && margins.is_none() {
        return no_pick(
            "the interval of a judge's difference from the best cannot be worked out".to_owned(),
            Vec::new(),
        );
    }
    let fastest = chosen.iter().map(|&j| medians[j]).min().unwrap_or(0);
    let best_decided = (chosen.iter())
        .map(|&j| decided[j])
        .fold(f64::NEG_INFINITY, f64::max);
    let mut clauses = Vec::new();
    for (j, (judge, _)) in judges.iter().enumerate() {
        let c = &counts[j];
        let margin = margins.as_ref().and_then(|m| m.get(&j).copied());
        let lower_ok = margin.is_some_and(|(_, lower)| lower + TOLERANCE >= -MARGIN);
        let time_ok = eligible[j] && (medians[j] as f64) <= TIME_RATIO * fastest as f64;
        let decided_ok = eligible[j] && decided[j] + TOLERANCE >= best_decided - DECIDED_BEHIND;
        clauses.push(Clauses {
            backend: judge.0.clone(),
            model: judge.1.clone(),
            errors_under_2_percent: (c.errors as f64) < ERRORS_UNDER * c.items as f64,
            decided_correctly_at_least_0_80: decided[j] + TOLERANCE >= DECIDED_AT_LEAST,
            eligible: eligible[j],
            difference_from_best_lower: margin.map(|(_, lower)| lower),
            difference_from_best_point: margin.map(|(point, _)| point),
            lower_bound_at_least_minus_3_points: lower_ok,
            median_time_at_most_twice_the_fastest: time_ok,
            decided_correctly_within_5_points_of_the_best: decided_ok,
            within_the_margin: eligible[j] && lower_ok && time_ok && decided_ok,
            cost_per_decision_usd: costs[j],
            false_reject_rate: c.false_reject_rate(),
        });
    }
    if chosen.is_empty() {
        return no_pick("no judge is eligible".to_owned(), clauses);
    }
    let within: Vec<usize> = (0..judges.len())
        .filter(|&j| clauses[j].within_the_margin)
        .collect();
    // The cheapest within the margin; ties to the lower false-reject rate, then the order
    // of the judges.
    let picked = within.iter().copied().min_by(|&a, &b| {
        costs[a].total_cmp(&costs[b]).then(
            (clauses[a].false_reject_rate.unwrap_or(1.0))
                .total_cmp(&clauses[b].false_reject_rate.unwrap_or(1.0)),
        )
    });
    let Some(picked) = picked else {
        return no_pick("no judge is within the margin".to_owned(), clauses);
    };
    let (backend, model) = judges[picked].0.clone();
    Pick {
        scope: scope.to_owned(),
        outcome: "pick".to_owned(),
        why: format!(
            "{} of the {} judges are within the margin, and {} has the lowest cost per decision \
             of them",
            within.len(),
            judges.len(),
            printable(&model)
        ),
        model,
        backend: Some(backend),
        judges: clauses,
    }
}

/// For each eligible judge, its balanced accuracy minus the highest among the eligible
/// judges: on the whole sample, and the lower bound of its 95% interval resampled by family,
/// with the highest taken again inside each resample, so that choosing the best is inside the
/// interval. `None` when a resample cannot be drawn, or when the statistic cannot be worked
/// out on the whole or on any resample.
/// An item's label, and each chosen judge's verdict of it (none where it has no decision).
type Verdicts = (Expected, Vec<Option<Computed>>);

fn reselected(
    judges: &[(JudgeKey, Vec<&Decision>)],
    chosen: &[usize],
    how: &Resampling,
) -> Option<BTreeMap<usize, (f64, f64)>> {
    // Every item's family and label, and each chosen judge's verdict of it.
    let mut items: BTreeMap<(String, String), (String, Verdicts)> = BTreeMap::new();
    for (at, &j) in chosen.iter().enumerate() {
        for d in &judges[j].1 {
            let entry = items
                .entry((d.set.clone(), d.item.clone()))
                .or_insert_with(|| {
                    (
                        format!("{}/{}", d.set, d.family),
                        (d.expected, vec![None; chosen.len()]),
                    )
                });
            entry.1.1[at] = Some(d.computed);
        }
    }
    let mut families: BTreeMap<String, Vec<Verdicts>> = BTreeMap::new();
    for (_, (family, (expected, verdicts))) in items {
        families
            .entry(family)
            .or_default()
            .push((expected, verdicts));
    }
    let families: Vec<Vec<Verdicts>> = families.into_values().collect();
    // Each chosen judge's rows of each family, made once: a resample only points at them.
    let rows: Vec<Vec<Vec<Row>>> = (0..chosen.len())
        .map(|at| {
            (families.iter())
                .map(|family| {
                    (family.iter())
                        .filter_map(|(expected, verdicts)| {
                            verdicts[at].map(|computed| bare(*expected, computed))
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    let differences = |drawn: &[usize]| -> Option<Vec<f64>> {
        let values: Vec<f64> = (rows.iter())
            .map(|mine| Counts::of(drawn.iter().flat_map(|&f| &mine[f])).balanced_accuracy())
            .collect::<Option<Vec<f64>>>()?;
        let best = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        Some(values.iter().map(|v| v - best).collect())
    };
    let whole: Vec<usize> = (0..families.len()).collect();
    let points = differences(&whole)?;
    let draws = resample_indices(how.seed, families.len(), how.resamples).ok()?;
    let mut by_judge: Vec<Vec<f64>> = vec![Vec::with_capacity(draws.len()); chosen.len()];
    for picks in &draws {
        // A resample the statistic cannot be worked out on is never filled in: the
        // interval is then not worked out, and the rule makes no pick.
        let found = differences(picks)?;
        for (at, value) in found.into_iter().enumerate() {
            by_judge[at].push(value);
        }
    }
    let low = how.alpha / 2.0 / how.hypotheses as f64;
    let mut out = BTreeMap::new();
    for (at, mut values) in by_judge.into_iter().enumerate() {
        values.sort_by(f64::total_cmp);
        out.insert(chosen[at], (points[at], quantile_type7(&values, low)));
    }
    Some(out)
}

/// `submission/1` (plan e1, task F2, item 2) over the population, the control against the
/// treatment. `incomplete` is the completeness check's finding, which makes it no decision.
pub fn submission_rule(
    decisions: &[Decision],
    control: &str,
    treatment: &str,
    incomplete: Option<&str>,
) -> FormatDecision {
    let of = |arm: &str| -> Vec<&Decision> { decisions.iter().filter(|d| d.arm == arm).collect() };
    let (c, t) = (of(control), of(treatment));
    let sc = SubmissionMeasures::of(c.iter().copied());
    let st = SubmissionMeasures::of(t.iter().copied());
    let mut out = FormatDecision {
        outcome: String::new(),
        settled_by: String::new(),
        format: control.to_owned(),
        control: control.to_owned(),
        treatment: treatment.to_owned(),
        why: String::new(),
        control_failures: sc.failures,
        treatment_failures: st.failures,
        control_first_rejections: sc.first_rejections,
        treatment_first_rejections: st.first_rejections,
        unknown: sc.unknown + st.unknown,
        mismatched: sc.mismatched + st.mismatched,
        fewer_failures_by: "too_few".to_owned(),
        fewer_failures: false,
        judges: Vec::new(),
    };
    let settle = |out: &mut FormatDecision, outcome: &str, clause: &str, why: String| {
        out.outcome = outcome.to_owned();
        out.settled_by = clause.to_owned();
        out.why = why;
    };
    if let Some(why) = incomplete {
        settle(
            &mut out,
            "no_decision",
            "complete",
            format!("the matrices are incomplete: {why}; the control is kept"),
        );
        return out;
    }
    if out.unknown > 0 || out.mismatched > 0 {
        let why = format!(
            "{} decisions kept no rejections diagnostic and {} name another format than their \
             arm; the control is kept",
            out.unknown, out.mismatched
        );
        settle(&mut out, "no_decision", "d", why);
        return out;
    }
    // (a) Fewer failures, measurably.
    let halved = |control: usize, treatment: usize| 2 * treatment <= control;
    if sc.failures >= FAILURES_LEAST {
        out.fewer_failures_by = "failures".to_owned();
        out.fewer_failures = halved(sc.failures, st.failures);
    } else if sc.first_rejections >= FIRST_REJECTIONS_LEAST {
        out.fewer_failures_by = "first_rejections".to_owned();
        out.fewer_failures = halved(sc.first_rejections, st.first_rejections);
    }
    // (b) and (c), per judge, over the judges of either arm.
    let judges: BTreeSet<JudgeKey> = c.iter().chain(&t).map(|d| key_of(d)).collect();
    for judge in &judges {
        let mine = |list: &[&Decision]| -> Vec<Decision> {
            (list.iter())
                .filter(|d| &key_of(d) == judge)
                .map(|d| (*d).clone())
                .collect()
        };
        let (jc, jt) = (mine(&c), mine(&t));
        let fc = jc
            .iter()
            .filter(|d| d.submissions.as_ref().is_some_and(Submissions::failed))
            .count();
        let ft = jt
            .iter()
            .filter(|d| d.submissions.as_ref().is_some_and(Submissions::failed))
            .count();
        let counts = |list: &[Decision]| {
            Counts::of(
                &(list
                    .iter()
                    .filter(|d| d.repetition == 1)
                    .map(|d| bare(d.expected, d.computed))
                    .collect::<Vec<_>>()),
            )
        };
        let (cc, ct) = (counts(&jc), counts(&jt));
        let change = |m: fn(&Counts) -> Option<f64>| Some(m(&ct)? - m(&cc)?);
        let ba = change(Counts::balanced_accuracy);
        let fr = change(Counts::false_reject_rate);
        let fa = change(Counts::false_accept_rate);
        // A change that cannot be worked out does not hold the clause.
        let kept = ba.is_some_and(|v| v + TOLERANCE >= -ACCURACY_GIVEN)
            && fr.is_none_or(|v| v <= ACCURACY_GIVEN + TOLERANCE)
            && fa.is_none_or(|v| v <= ACCURACY_GIVEN + TOLERANCE);
        out.judges.push(FormatClauses {
            backend: judge.0.clone(),
            model: judge.1.clone(),
            control_failures: fc,
            treatment_failures: ft,
            no_new_failures: ft <= fc,
            balanced_accuracy_change: ba,
            false_reject_rate_change: fr,
            false_accept_rate_change: fa,
            accuracy_kept: kept,
        });
    }
    if out.fewer_failures_by == "too_few" {
        settle(
            &mut out,
            "too_few_failures",
            "a",
            format!(
                "the control had {} submission failures (under {FAILURES_LEAST}) and {} first \
                 rejections (under {FIRST_REJECTIONS_LEAST}); the control is kept",
                sc.failures, sc.first_rejections
            ),
        );
        return out;
    }
    if !out.fewer_failures {
        let why = format!(
            "the treatment did not halve the control's {}; the control is kept",
            out.fewer_failures_by.replace('_', " ")
        );
        settle(&mut out, "not_adopted", "a", why);
        return out;
    }
    let more: Vec<String> = (out.judges.iter())
        .filter(|j| !j.no_new_failures)
        .map(|j| {
            format!(
                "{} ({} against {})",
                printable(&j.model),
                j.treatment_failures,
                j.control_failures
            )
        })
        .collect();
    if !more.is_empty() {
        let why = format!(
            "under the treatment these judges have more submission failures than under the \
             control: {}; the control is kept",
            more.join("; ")
        );
        settle(&mut out, "not_adopted", "b", why);
        return out;
    }
    let lost: Vec<String> = (out.judges.iter())
        .filter(|j| !j.accuracy_kept)
        .map(|j| format!("{} ({})", printable(&j.model), breaches(j).join(", ")))
        .collect();
    if !lost.is_empty() {
        let why = format!(
            "under the treatment these judges move by more than 2 points: {}; the control is \
             kept",
            lost.join("; ")
        );
        settle(&mut out, "not_adopted", "c", why);
        return out;
    }
    out.format = treatment.to_owned();
    settle(
        &mut out,
        "adopted",
        "a, b and c",
        "fewer failures, no new failures for any judge, and accuracy kept".to_owned(),
    );
    out
}

/// The measures by which a judge broke clause (c) of `submission/1`, each with its change in
/// points: balanced accuracy lower, or the false-reject or false-accept rate higher, by more
/// than [`ACCURACY_GIVEN`]; or balanced accuracy that cannot be worked out.
fn breaches(j: &FormatClauses) -> Vec<String> {
    // To two places, so a breach just past the line never reads as on it.
    let points = |v: f64| format!("{:+.2} points", v * 100.0);
    let mut out = Vec::new();
    match j.balanced_accuracy_change {
        None => out.push("balanced accuracy cannot be worked out".to_owned()),
        Some(v) if v + TOLERANCE < -ACCURACY_GIVEN => {
            out.push(format!("balanced accuracy {}", points(v)));
        }
        Some(_) => {}
    }
    for (name, change) in [
        ("false-reject rate", j.false_reject_rate_change),
        ("false-accept rate", j.false_accept_rate_change),
    ] {
        if let Some(v) = change.filter(|v| *v > ACCURACY_GIVEN + TOLERANCE) {
            out.push(format!("{name} {}", points(v)));
        }
    }
    out
}

/// Scores the population as asked: the measures of each cell compared in each scope, the
/// comparisons of the mode, and the rule asked for.
pub fn score(population: &Population, asked: &Asked) -> Result<Scored, String> {
    let how = &asked.resampling;
    let arms: Vec<String> = match asked.mode {
        Mode::Judges => vec![asked.arm.clone().ok_or("judges mode is given --arm")?],
        Mode::Arms => {
            let control = asked.arm.clone().ok_or("arms mode names its control arm")?;
            let treatment = asked
                .treatment
                .clone()
                .ok_or("arms mode names its treatment arm")?;
            vec![control, treatment]
        }
    };
    match (asked.mode, asked.rule) {
        (Mode::Arms, Some(Rule::JudgePick)) => {
            return Err("judge-pick/1 is applied in judges mode".to_owned());
        }
        (Mode::Judges, Some(Rule::Submission)) => {
            return Err("submission/1 is applied in arms mode".to_owned());
        }
        _ => {}
    }
    let incomplete = incomplete(population, asked);
    let mut out = Scored {
        mode: asked.mode.name().to_owned(),
        arm: asked.arm.clone(),
        treatment: asked.treatment.clone(),
        rule: asked.rule.map(|r| r.name().to_owned()),
        cells: population.cells,
        decisions: population.decisions.len(),
        open: population.open,
        unlabelled: population.unlabelled,
        incomplete: incomplete.clone(),
        seed: how.seed,
        resamples: how.resamples,
        class_counting: CLASS_COUNTING.to_owned(),
        picks: Vec::new(),
        format: None,
        scopes: Vec::new(),
    };
    for (name, rows) in scopes(&population.decisions) {
        let mut scope = Scope {
            name: name.clone(),
            measures: Vec::new(),
            comparisons: Vec::new(),
        };
        for arm in &arms {
            for (judge, list) in by_judge(&rows, arm) {
                scope.measures.push(measures(&judge, arm, &list, how));
            }
        }
        // Comparisons are of complete matrices only: on part of them, `paired` would compare
        // whatever both sides happened to have.
        match asked.mode {
            _ if incomplete.is_some() => {
                if asked.rule == Some(Rule::JudgePick)
                    && (name == "all" || name == "source-fidelity")
                {
                    out.picks.push(judge_pick(
                        &name,
                        &rows,
                        &arms[0],
                        incomplete.as_deref(),
                        how,
                    ));
                }
            }
            Mode::Judges => {
                let arm = &arms[0];
                let judged = by_judge(&rows, arm);
                // Each judge against the best: the highest balanced accuracy among the
                // eligible judges, or among all when none is eligible.
                let pick = judge_pick(&name, &rows, arm, None, how);
                let best = (pick.judges.iter())
                    .filter(|c| c.eligible)
                    .find(|c| c.difference_from_best_point == Some(0.0))
                    .map(|c| (c.backend.clone(), c.model.clone()))
                    .or_else(|| {
                        (scope.measures.iter())
                            .filter(|m| m.balanced_accuracy.is_some())
                            .max_by(|a, b| {
                                let ba =
                                    |m: &Measures| m.balanced_accuracy.map_or(0.0, |e| e.point);
                                ba(a).total_cmp(&ba(b))
                            })
                            .map(|m| (m.backend.clone(), m.model.clone()))
                    });
                if let Some(best) = best
                    && let Some(against) = judged.get(&best)
                {
                    for (judge, list) in &judged {
                        if judge == &best {
                            continue;
                        }
                        if let Some(one) = compared((judge, arm, list), (&best, arm, against), how)
                        {
                            scope.comparisons.push(one);
                        }
                    }
                }
                if asked.rule == Some(Rule::JudgePick)
                    && (name == "all" || name == "source-fidelity")
                {
                    out.picks
                        .push(judge_pick(&name, &rows, arm, incomplete.as_deref(), how));
                }
            }
            Mode::Arms => {
                let (control, treatment) = (&arms[0], &arms[1]);
                let c = by_judge(&rows, control);
                let t = by_judge(&rows, treatment);
                for (judge, list) in &t {
                    if let Some(against) = c.get(judge)
                        && let Some(one) =
                            compared((judge, treatment, list), (judge, control, against), how)
                    {
                        scope.comparisons.push(one);
                    }
                }
            }
        }
        out.scopes.push(scope);
    }
    if asked.rule == Some(Rule::Submission) {
        out.format = Some(submission_rule(
            &population.decisions,
            &arms[0],
            &arms[1],
            incomplete.as_deref(),
        ));
    }
    Ok(out)
}

// --- reading the store -----------------------------------------------------------------------

/// What the command reads, and where it writes.
pub struct Reading<'a> {
    pub matrices: &'a [PathBuf],
    pub store: &'a Path,
    pub sets_root: &'a Path,
    pub labels_root: &'a Path,
    pub out: &'a Path,
    pub mode: Mode,
    pub arm: Option<&'a str>,
    pub rule: Option<Rule>,
    /// The commit the binary was built from.
    pub commit: &'a str,
}

/// A matrix named, by its name and identity.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Named {
    pub name: String,
    pub identity: String,
    pub cells: usize,
}

/// A set scored, by its digests.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetRead {
    pub id: String,
    pub split: String,
    /// The SHA-256 of the set's seal file; none for a set with no seal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seal_sha256: Option<String>,
    pub labels_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrections_sha256: Option<String>,
}

/// A judge as the records hold it: its backend, its model and each version reported.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JudgeRead {
    pub backend: String,
    pub model: String,
    pub versions: Vec<String>,
}

/// What the command writes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Written {
    pub schema: String,
    pub commit: String,
    #[serde(flatten)]
    pub scored: Scored,
    pub matrices: Vec<Named>,
    pub sets: Vec<SetRead>,
    pub judges: Vec<JudgeRead>,
}

/// What a run of a cell came to, read from its record.
struct Attempt {
    attempt: u32,
    ended: Ended,
    computed: Computed,
    cost_usd: f64,
    settled_usd: f64,
    wall_ms: u64,
    submissions: Option<Submissions>,
    versions: Vec<(String, String, String)>,
}

fn read_matrix(file: &Path) -> Result<Matrix, String> {
    let shown = printable(&file.display().to_string());
    let bytes = crate::gating::read_limited(file, "a matrix file")?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{shown}: not UTF-8"))?;
    Matrix::parse(&text).map_err(|why| format!("{shown}: {why}"))
}

/// Reads the matrices' runs from the store, scores them as asked and writes `judges/1` to
/// `out`. Refused before anything is read when `out` is under the sets' or the labels'
/// folder, or where either cannot be resolved.
pub fn run(reading: &Reading<'_>) -> Result<Written, String> {
    for root in [reading.sets_root, reading.labels_root] {
        if crate::gating::under(reading.out, root) {
            return Err(
                "--out is not written under the sets' folder or the labels' folder, nor where \
                 either cannot be resolved"
                    .to_owned(),
            );
        }
    }
    if reading.matrices.is_empty() {
        return Err("name at least one --matrix".to_owned());
    }
    let matrices = (reading.matrices.iter())
        .map(|file| read_matrix(file))
        .collect::<Result<Vec<Matrix>, String>>()?;
    let mut ids: Vec<String> = Vec::new();
    for m in &matrices {
        for id in &m.matrix.sets {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
    }
    let labelled = crate::gating::labelled(reading.sets_root, reading.labels_root, &ids)?;
    let mut items = Vec::new();
    let mut set_reads = Vec::new();
    let mut splits = BTreeMap::new();
    for id in &ids {
        let set = sets::find(reading.sets_root, id)?;
        let digest_of = |name: &str| -> Result<Option<String>, String> {
            if !set.dir.join(name).exists() {
                return Ok(None);
            }
            let bytes = sets::read_under(&set.dir, name, sets::FILE_LIMIT)?;
            Ok(Some(Digest::of_bytes(&bytes).hex()))
        };
        let seal = digest_of(sets::SEAL_FILE)?;
        let corrections = digest_of(sets::CORRECTIONS_FILE)?;
        let split = set.manifest.set.split.to_string();
        splits.insert(id.clone(), split.clone());
        set_reads.push(SetRead {
            id: id.clone(),
            split,
            seal_sha256: seal,
            labels_sha256: labelled.digests.get(id).cloned().unwrap_or_default(),
            corrections_sha256: corrections,
        });
        let its: Vec<String> = set.jobs.iter().map(|(item, _)| item.clone()).collect();
        items.push((id.clone(), its));
    }
    // Every cell of every matrix, each named by one matrix only.
    let mut expected: BTreeMap<Cell, usize> = BTreeMap::new();
    let mut named = Vec::new();
    let mut identities = BTreeMap::new();
    for (at, m) in matrices.iter().enumerate() {
        let mine: Vec<(String, Vec<String>)> = (items.iter())
            .filter(|(id, _)| m.matrix.sets.contains(id))
            .cloned()
            .collect();
        let cells = m.cells(&mine);
        named.push(Named {
            name: m.matrix.name.clone(),
            identity: m.identity(),
            cells: cells.len(),
        });
        let short: String = m.identity().chars().take(16).collect();
        if identities.insert(short, at).is_some() {
            return Err("a matrix is named twice".to_owned());
        }
        for cell in cells {
            if expected.insert(cell, at).is_some() {
                return Err(
                    "a cell is named by two of the matrices; each cell is scored from one"
                        .to_owned(),
                );
            }
        }
    }
    let attempts = attempts_of(reading.store, &identities)?;
    let mut population = Population {
        cells: expected.len(),
        ..Population::default()
    };
    let mut versions: BTreeMap<JudgeKey, BTreeSet<String>> = BTreeMap::new();
    for (cell, at) in &expected {
        let last = (attempts.get(&(*at, cell.clone())))
            .and_then(|list| list.iter().max_by_key(|a| a.attempt));
        let Some(last) = last.filter(|a| a.ended != Ended::Interrupted) else {
            population.open += 1;
            continue;
        };
        for (_, _, version) in &last.versions {
            versions
                .entry((cell.backend.clone(), cell.model.clone()))
                .or_default()
                .insert(version.clone());
        }
        let key = (cell.set.clone(), cell.item.clone());
        let (Some(label), Some(family)) =
            (labelled.expected.get(&key), labelled.families.get(&key))
        else {
            population.unlabelled += 1;
            continue;
        };
        population.decisions.push(Decision {
            set: cell.set.clone(),
            item: cell.item.clone(),
            family: family.clone(),
            use_case: cell.set.split('/').next().unwrap_or_default().to_owned(),
            split: splits.get(&cell.set).cloned().unwrap_or_default(),
            backend: cell.backend.clone(),
            model: cell.model.clone(),
            arm: cell.arm.clone(),
            repetition: cell.repetition,
            expected: *label,
            computed: last.computed,
            cost_usd: last.cost_usd,
            settled_usd: last.settled_usd,
            wall_ms: last.wall_ms,
            submissions: last.submissions.clone(),
        });
    }
    let all_arms: Vec<String> = {
        let mut seen = Vec::new();
        for m in &matrices {
            for arm in &m.matrix.arms {
                let arm = arm.to_string();
                if !seen.contains(&arm) {
                    seen.push(arm);
                }
            }
        }
        seen
    };
    let (arm, treatment) = match reading.mode {
        Mode::Judges => {
            let arm = reading
                .arm
                .ok_or("judges mode is given --arm, the one arm every judge is scored under")?;
            if !all_arms.iter().any(|a| a == arm) {
                return Err(format!("no matrix named has the arm {}", printable(arm)));
            }
            (Some(arm.to_owned()), None)
        }
        Mode::Arms => {
            if all_arms.len() != 2 {
                return Err(format!(
                    "arms mode compares two arms, and the matrices have {}",
                    all_arms.len()
                ));
            }
            let control = reading
                .arm
                .map_or_else(|| all_arms[0].clone(), str::to_owned);
            let Some(treatment) = all_arms.iter().find(|a| **a != control).cloned() else {
                return Err("arms mode compares two arms".to_owned());
            };
            if !all_arms.contains(&control) {
                return Err(format!(
                    "no matrix named has the arm {}",
                    printable(&control)
                ));
            }
            (Some(control), Some(treatment))
        }
    };
    let asked = Asked {
        mode: reading.mode,
        arm,
        treatment,
        rule: reading.rule,
        resampling: resampling(),
    };
    let scored = score(&population, &asked)?;
    let written = Written {
        schema: SCHEMA.to_owned(),
        commit: reading.commit.to_owned(),
        scored,
        matrices: named,
        sets: set_reads,
        judges: (versions.into_iter())
            .map(|((backend, model), versions)| JudgeRead {
                backend,
                model,
                versions: versions.into_iter().collect(),
            })
            .collect(),
    };
    let text = toml_of(&written)?;
    crate::gating::write_new(reading.out, text.as_bytes())?;
    Ok(written)
}

/// Every attempt of each cell of the matrices of these identities (their first 16 hex
/// digits, each to its matrix's index), made under no gate, as its record has it.
fn attempts_of(
    root: &Path,
    identities: &BTreeMap<String, usize>,
) -> Result<BTreeMap<(usize, Cell), Vec<Attempt>>, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let mut out: BTreeMap<(usize, Cell), Vec<Attempt>> = BTreeMap::new();
    crate::harness::each_run(root, &mut |record, id, ended| {
        let Some(argument) = cell_of(record) else {
            return;
        };
        if argument.gate.is_some() {
            return;
        }
        let Some(&at) = identities.get(&argument.identity) else {
            return;
        };
        let computed = match (ended, record.verdict()) {
            (Ended::Completed, Some((Verdict::Pass, _))) => Computed::Pass,
            (Ended::Completed, Some((Verdict::Fail, _))) => Computed::Fail,
            (Ended::Completed, Some((Verdict::Unsure, _))) => Computed::Unsure,
            _ => Computed::Error,
        };
        let requests = &record.spend().requests;
        let settled: u64 = requests.iter().map(|paid| paid.settled_micros()).sum();
        let unresolved: u64 = requests.iter().map(|paid| paid.unresolved_micros()).sum();
        let time = record.time();
        let wall_ms = time
            .start_ms
            .map_or(0, |start| time.end_ms.saturating_sub(start));
        let entry = (generation.run(id))
            .and_then(|held| held.get(&format!("{DIAGNOSTICS_PREFIX}{REJECTIONS}")));
        let submissions = match entry {
            Some(entry) if entry.kind == Kind::Diagnostic => reader
                .read(entry)
                .ok()
                .and_then(|bytes| Submissions::parse(&bytes)),
            _ => None,
        };
        let versions = (record.models().iter())
            .filter(|m| m.capability == Capability::Judge)
            .map(|m| (m.backend.clone(), m.model.clone(), m.version.clone()))
            .collect();
        out.entry((at, argument.cell)).or_default().push(Attempt {
            attempt: argument.attempt,
            ended,
            computed,
            cost_usd: settled.saturating_add(unresolved) as f64 / 1_000_000.0,
            settled_usd: settled as f64 / 1_000_000.0,
            wall_ms,
            submissions,
            versions,
        });
    })?;
    Ok(out)
}

/// What the command prints: the population's counts and each outcome, aggregates only.
pub fn text(written: &Written) -> String {
    let s = &written.scored;
    let mut out = format!(
        "matrices: {}\ncells: {}\ndecisions scored: {}\nnot ended: {}\nunlabelled: {}\n",
        written.matrices.len(),
        s.cells,
        s.decisions,
        s.open,
        s.unlabelled
    );
    if let Some(why) = &s.incomplete {
        out.push_str(&format!("incomplete: {why}\n"));
    }
    for pick in &s.picks {
        out.push_str(&format!(
            "{} ({}): {} {}: {}\n",
            pick.scope,
            Rule::JudgePick.name(),
            pick.outcome,
            printable(&pick.model),
            pick.why
        ));
    }
    if let Some(format) = &s.format {
        out.push_str(&format!(
            "{}: {} ({}), by clause {}: {}\n",
            Rule::Submission.name(),
            format.outcome,
            printable(&format.format),
            format.settled_by,
            format.why
        ));
    }
    out
}

/// A record as TOML: its fields as serde gives them, each table's values first, then its
/// tables, then its arrays of tables; a field with no value is left out. The build reads
/// TOML and writes none, so this is written here rather than taking a writer for it.
pub fn toml_of(record: &impl Serialize) -> Result<String, String> {
    let value = serde_json::to_value(record).map_err(|e| format!("the record: {e}"))?;
    let serde_json::Value::Object(map) = value else {
        return Err("the record is not a table".to_owned());
    };
    let mut out = String::new();
    table(&mut out, "", &map);
    Ok(out)
}

fn toml_key(key: &str) -> String {
    let bare =
        !key.is_empty() && (key.chars()).all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        key.to_owned()
    } else {
        serde_json::Value::String(key.to_owned()).to_string()
    }
}

fn tables_only(value: &serde_json::Value) -> bool {
    matches!(value, serde_json::Value::Array(list) if !list.is_empty() && list.iter().all(serde_json::Value::is_object))
}

/// A value TOML writes inline: a string as a basic string (JSON's escapes are TOML's), a
/// number, a boolean, or an array of them.
fn inline(value: &serde_json::Value) -> Option<String> {
    use serde_json::Value as J;
    match value {
        J::Null => None,
        J::Bool(_) | J::String(_) => Some(value.to_string()),
        J::Number(n) if n.is_f64() => {
            let f = n.as_f64()?;
            let shown = format!("{f:?}");
            Some(if shown.contains(['.', 'e', 'E']) {
                shown
            } else {
                format!("{shown}.0")
            })
        }
        J::Number(n) => Some(n.to_string()),
        J::Array(list) => {
            let all: Vec<String> = list.iter().filter_map(inline).collect();
            Some(format!("[{}]", all.join(", ")))
        }
        J::Object(_) => None,
    }
}

fn table(out: &mut String, path: &str, map: &serde_json::Map<String, serde_json::Value>) {
    let under = |key: &str| {
        if path.is_empty() {
            toml_key(key)
        } else {
            format!("{path}.{}", toml_key(key))
        }
    };
    for (key, value) in map {
        if !value.is_object()
            && !tables_only(value)
            && let Some(text) = inline(value)
        {
            out.push_str(&format!("{} = {text}\n", toml_key(key)));
        }
    }
    for (key, value) in map {
        if let serde_json::Value::Object(inner) = value {
            out.push_str(&format!("\n[{}]\n", under(key)));
            table(out, &under(key), inner);
        }
    }
    for (key, value) in map {
        if let serde_json::Value::Array(list) = value
            && tables_only(value)
        {
            for one in list {
                if let serde_json::Value::Object(inner) = one {
                    out.push_str(&format!("\n[[{}]]\n", under(key)));
                    table(out, &under(key), inner);
                }
            }
        }
    }
}
