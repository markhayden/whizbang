//! Gates (`SPEC.md` section 8, "Gates"; plan 2C, task E2, item 2): the file a gate is
//! frozen in before a rung 3 or 4 run starts, and how a gate is decided from a paired
//! comparison ([`crate::metrics::paired`]). The deciding part is ported from squint's
//! `crates/squint-eval/src/gate_rules.rs`, at the commit `docs/sources.toml` pins, and
//! changed to the spec's measures: squint gated on the count of failures caught; a gate
//! here gates on balanced accuracy, with the false rejects, the errors, the cost and the
//! latency as guardrails.
//!
//! **A gate file states every item the spec lists, and nothing else.** No item has a
//! default: a file that leaves one out is refused, and so is a key the format does not
//! name. Where the spec fixes how something counts (the primary measure, how each
//! outcome counts, resampling by family, that too little data is inconclusive), the file
//! states it, and a file that states anything else is refused rather than half-honoured.
//!
//! **Too little data is "inconclusive", never a pass**: fewer families than the gate
//! requires, an interval that cannot be worked out, or a tail the resamples cannot resolve.
//!
//! **A gate names its matrix** (plan m3, task G1, item 1; version 2 of the file): the
//! matrix file, beside the gate, and its identity, so a matrix changed after the gate was
//! frozen is not run under it. Each judge of the matrix is one comparison of the gate's two
//! arms, decided alone ([`Gate::score`]); the gate is admitted when every judge's is, and
//! rejected when any judge's is.

use serde::{Deserialize, Serialize};

use crate::metrics::{Paired, Resampling, Row};

/// The version of the gate file this build reads: 2 names the matrix (`[run]`).
pub const GATE_VERSION: u64 = 2;

/// The version of the receipt a gate's run writes.
pub const RECEIPT_VERSION: u64 = 1;

/// A gate file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    pub gate_version: u64,
    /// The matrix the gate is run over.
    pub run: Run,
    /// The arm compared against, and the arm on trial.
    pub baseline: Arms,
    pub measure: Measure,
    pub confidence: Confidence,
    pub families: Families,
    pub repetitions: Repetitions,
    pub admission: Admission,
    pub guardrails: Guardrails,
    pub limits: Limits,
    pub too_little_data: TooLittleData,
}

/// The matrix a gate is run over: its file, relative to the gate file's folder, and its
/// identity (`Matrix::identity`, 64 hex digits).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub matrix: String,
    pub matrix_identity: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Arms {
    pub control: String,
    pub treatment: String,
}

/// The primary measure and how each outcome counts toward it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measure {
    pub primary: Primary,
    pub pass: Counts,
    pub fail: Counts,
    pub unsure: Counts,
    pub error: Counts,
    pub missing_label: MissingLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Primary {
    BalancedAccuracy,
}

/// What an outcome is to the measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Counts {
    /// A pass.
    Pass,
    /// A catch.
    Catch,
    /// Neither a catch nor a pass.
    Neither,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingLabel {
    /// An item without a label stops the scoring.
    Refuse,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Confidence {
    /// Two-sided, as 0.95.
    pub level: f64,
    pub correction: Correction,
    /// The hypotheses the correction divides by.
    pub hypotheses: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Correction {
    Bonferroni,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Families {
    /// The fewest independent families a decision may rest on.
    pub minimum: usize,
    pub resampling: ResampleBy,
    pub seed: u64,
    pub resamples: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResampleBy {
    Family,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repetitions {
    pub count: u32,
    pub combine: Combine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Combine {
    /// Repetition 1 decides; the others give the self-disagreement.
    First,
}

/// The smallest gain worth having, in balanced accuracy (0.03 is 3 points), and the
/// lower bound of its corrected interval must be above `lower_bound_above`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Admission {
    pub min_gain: f64,
    pub lower_bound_above: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guardrails {
    /// The upper bound of the increase in false rejects must be under this.
    pub false_reject_increase_max: f64,
    /// The treatment's share of decisions that ended in an error must be under this.
    pub error_rate_max: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// The treatment's mean cost a decision over the control's, at most.
    pub cost_ratio_max: f64,
    /// The treatment's median wall time a decision over the control's, at most.
    pub latency_ratio_max: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TooLittleData {
    pub outcome: Outcome,
}

/// A gate's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Admitted,
    Rejected,
    Inconclusive,
}

impl Gate {
    /// Reads a gate file, refusing one that leaves out an item of the spec, names
    /// anything else, or states what the spec does not allow.
    pub fn parse(text: &str) -> Result<Gate, String> {
        let gate: Gate = toml::from_str(text).map_err(|e| format!("the gate file: {e}"))?;
        if gate.gate_version != GATE_VERSION {
            return Err(format!(
                "the gate file is version {}; this build reads {GATE_VERSION}",
                gate.gate_version
            ));
        }
        let m = &gate.measure;
        if (m.pass, m.fail, m.unsure, m.error)
            != (
                Counts::Pass,
                Counts::Catch,
                Counts::Neither,
                Counts::Neither,
            )
        {
            return Err(
                "the gate file: pass counts as a pass, fail as a catch, and unsure \
                        and errors as neither"
                    .into(),
            );
        }
        if gate.too_little_data.outcome != Outcome::Inconclusive {
            return Err("the gate file: too little data is inconclusive, never a pass".into());
        }
        let c = &gate.confidence;
        let unit = |x: f64| x.is_finite() && x > 0.0 && x < 1.0;
        if !unit(c.level) || c.hypotheses == 0 {
            return Err(
                "the gate file: a confidence level between 0 and 1, and at least one \
                        hypothesis"
                    .into(),
            );
        }
        let r = &gate.run;
        let hex = |c: char| c.is_ascii_digit() || ('a'..='f').contains(&c);
        if r.matrix_identity.len() != 64 || !r.matrix_identity.chars().all(hex) {
            return Err("the gate file: matrix_identity is 64 lowercase hex digits".into());
        }
        let plain = |part: &str| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && (part.chars()).all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if !r.matrix.split('/').all(plain) {
            return Err("the gate file: matrix is a plain path beside the gate".into());
        }
        if gate.baseline.control == gate.baseline.treatment {
            return Err("the gate file: the control and the treatment are two arms".into());
        }
        let f = &gate.families;
        if f.minimum == 0 || f.resamples == 0 || gate.repetitions.count == 0 {
            return Err("the gate file: families, resamples and repetitions are positive".into());
        }
        let numbers = [
            gate.admission.min_gain,
            gate.admission.lower_bound_above,
            gate.guardrails.false_reject_increase_max,
            gate.guardrails.error_rate_max,
            gate.limits.cost_ratio_max,
            gate.limits.latency_ratio_max,
        ];
        if numbers.iter().any(|x| !x.is_finite()) {
            return Err("the gate file: every margin is a finite number".into());
        }
        Ok(gate)
    }

    /// How the gate's comparison is resampled.
    pub fn resampling(&self) -> Resampling {
        Resampling {
            seed: self.families.seed,
            resamples: self.families.resamples,
            hypotheses: self.confidence.hypotheses,
            alpha: 1.0 - self.confidence.level,
        }
    }

    /// Decides the gate on a comparison made with [`Gate::resampling`]. `failed` names
    /// each item that did not hold, in the order of the file.
    pub fn decide(&self, paired: &Paired) -> Verdict {
        let mut short = Vec::new();
        if paired.families < self.families.minimum {
            short.push("families");
        }
        let (Some(gain), Some(fr)) = (&paired.gain, &paired.false_reject_increase) else {
            short.push("intervals");
            return Verdict::inconclusive(short);
        };
        if gain.insufficient_tail || fr.insufficient_tail {
            short.push("resamples");
        }
        if !short.is_empty() {
            return Verdict::inconclusive(short);
        }
        let (c, x) = (&paired.control, &paired.treatment);
        let per = |m: &crate::metrics::CellMetrics| m.cost_total_usd / m.counts.items.max(1) as f64;
        let ratio = |x: f64, c: f64| {
            if c > 0.0 {
                x / c
            } else if x > 0.0 {
                f64::INFINITY
            } else {
                1.0
            }
        };
        let cost = ratio(per(x), per(c));
        let latency = ratio(x.wall_median_ms as f64, c.wall_median_ms as f64);
        let a = &self.admission;
        let g = &self.guardrails;
        let l = &self.limits;
        let mut failed = Vec::new();
        let mut rejected = false;
        if gain.point < a.min_gain || gain.lower <= a.lower_bound_above {
            failed.push("admission");
            rejected |= gain.upper <= a.lower_bound_above;
        }
        if fr.upper >= g.false_reject_increase_max {
            failed.push("false_rejects");
            rejected |= fr.lower >= g.false_reject_increase_max;
        }
        if x.errors.value >= g.error_rate_max {
            failed.push("errors");
            rejected = true;
        }
        if cost > l.cost_ratio_max {
            failed.push("cost");
            rejected = true;
        }
        if latency > l.latency_ratio_max {
            failed.push("latency");
            rejected = true;
        }
        let outcome = match (failed.is_empty(), rejected) {
            (true, _) => Outcome::Admitted,
            (false, true) => Outcome::Rejected,
            (false, false) => Outcome::Inconclusive,
        };
        Verdict {
            outcome,
            failed: failed.into_iter().map(String::from).collect(),
        }
    }
}

/// A gate's outcome, with the items that did not hold.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Verdict {
    pub outcome: Outcome,
    pub failed: Vec<String>,
}

impl Verdict {
    fn inconclusive(failed: Vec<&str>) -> Verdict {
        Verdict {
            outcome: Outcome::Inconclusive,
            failed: failed.into_iter().map(String::from).collect(),
        }
    }
}

/// One judge's scored decisions under the gate's two arms.
#[derive(Debug, Clone, PartialEq)]
pub struct Judged {
    pub backend: String,
    pub model: String,
    pub control: Vec<Row>,
    pub treatment: Vec<Row>,
}

/// What one judge's comparison came to: aggregates only.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JudgeScore {
    pub backend: String,
    pub model: String,
    pub outcome: Outcome,
    pub failed: Vec<String>,
    pub paired: Paired,
}

/// What the gate came to over every judge of its matrix.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scored {
    pub outcome: Outcome,
    pub judges: Vec<JudgeScore>,
}

impl Gate {
    /// Decides the gate for each judge on its own paired comparison, then over all of them:
    /// admitted when every judge's comparison is admitted, rejected when any is rejected,
    /// and inconclusive otherwise and with no judge at all. The correction the file states
    /// must cover every judge: a matrix of more judges than its hypotheses is refused.
    pub fn score(&self, judged: &[Judged]) -> Result<Scored, String> {
        if judged.len() > self.confidence.hypotheses {
            return Err(format!(
                "the gate corrects for {} hypotheses, and its matrix has {} judges, each one",
                self.confidence.hypotheses,
                judged.len()
            ));
        }
        let mut judges = Vec::new();
        for one in judged {
            let paired = crate::metrics::paired(&one.control, &one.treatment, self.resampling())?;
            let verdict = self.decide(&paired);
            judges.push(JudgeScore {
                backend: one.backend.clone(),
                model: one.model.clone(),
                outcome: verdict.outcome,
                failed: verdict.failed,
                paired,
            });
        }
        let any = |o: Outcome| judges.iter().any(|j| j.outcome == o);
        let outcome = if judges.is_empty() {
            Outcome::Inconclusive
        } else if any(Outcome::Rejected) {
            Outcome::Rejected
        } else if judges.iter().all(|j| j.outcome == Outcome::Admitted) {
            Outcome::Admitted
        } else {
            Outcome::Inconclusive
        };
        Ok(Scored { outcome, judges })
    }
}

/// A gate run's receipt: what was run and what it came to, in aggregates and digests, and
/// never an item, a family, a label or a verdict of one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Receipt {
    pub receipt_version: u64,
    /// The SHA-256 of the gate file's bytes: the digest in every run the gate made.
    pub gate_sha256: String,
    pub matrix: String,
    pub matrix_identity: String,
    pub sets: Vec<String>,
    /// Each set's labels file as it was read, by its SHA-256.
    pub labels_sha256: std::collections::BTreeMap<String, String>,
    /// Cells of the gate's two arms that were scored.
    pub cells: usize,
    pub control: String,
    pub treatment: String,
    #[serde(flatten)]
    pub scored: Scored,
}
