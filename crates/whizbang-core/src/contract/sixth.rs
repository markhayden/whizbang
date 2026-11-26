//! What `run/6` adds to a run's record (plan m4, task K1, item 4; ADR 0005, "run/6"):
//! the plan of a call, where a recipe's run started and what it was given, the
//! findings a run handed back, and a loop's attempts. Each is a group of its own at
//! the end of the record, written only where the run has it; a record that has none
//! of them is written as the version before, which holds all of it.
//!
//! The types are part of the record ([`super::record`] names them again), and are
//! read from JSON only as part of a stored record.

use serde::{Deserialize, Serialize};

use crate::artifact::ArtifactType;
use crate::identity::Digest;

use super::eighth::AttemptsEnded;
use super::findings::{
    FINDING_OBLIGATIONS_LIMIT, FINDING_PLACES_LIMIT, FINDINGS_LIMIT, FindingCount, Findings,
    FoundAt, LeftOut, is_obligation_id,
};
use super::plan::PLAN_ATTEMPTS_MAX;
use super::record::{Decision, type_name};
use super::{RunId, text};

/// The most artifacts a recipe's run from a step lists as given.
pub const GIVEN_LIMIT: usize = 64;

/// The most runs one attempt lists.
pub const ATTEMPT_RUNS_LIMIT: usize = 64;

/// The most crops one attempt's retry lists.
pub const ATTEMPT_CROPS_LIMIT: usize = FINDING_PLACES_LIMIT;

/// The most characters of a reason an attempt or a loop's end gives.
pub const ATTEMPT_TEXT_LIMIT: usize = 200;

/// One artifact a recipe's run from a step was given in place of an output of a step
/// before it: the step, its output port, and the artifact's type and identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Given {
    pub step: String,
    pub port: String,
    #[serde(rename = "type", with = "type_name")]
    pub artifact_type: ArtifactType,
    pub artifact: Digest,
}

/// Where a recipe's run started (ADR 0033, "A run from a step"): the step it ran
/// from, and what it was given in place of the outputs of the steps before it, which
/// did not run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Started {
    pub from: String,
    pub given: Vec<Given>,
}

impl Started {
    pub(super) fn holds(&self) -> bool {
        let named = |name: &str| text::action_name(name).is_ok();
        let port = |name: &str| text::field_name(name).is_ok();
        named(&self.from)
            && self.given.len() <= GIVEN_LIMIT
            && (self.given.iter()).all(|given| named(&given.step) && port(&given.port))
    }
}

/// What a record keeps of the findings a run handed back (ADR 0035, "Findings"): the
/// decision they carry, whether they were complete and what was left out, how many
/// cite each obligation with each disposition and confidence, and the places.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Found {
    pub decision: Decision,
    pub complete: bool,
    pub left_out: LeftOut,
    pub counts: Vec<FindingCount>,
    pub places: Vec<FoundAt>,
}

impl Found {
    /// What a record keeps of these findings.
    pub(super) fn of(findings: &Findings) -> Found {
        Found {
            decision: findings.decision(),
            complete: findings.is_complete(),
            left_out: findings.left_out(),
            counts: findings.counts(),
            places: findings.places(),
        }
    }

    pub(super) fn holds(&self) -> bool {
        let key = |count: &FindingCount| {
            (
                count.obligation.clone(),
                count.disposition,
                count.confidence,
            )
        };
        let whole = self.left_out == LeftOut::default();
        self.complete == whole
            && self.counts.len() <= FINDINGS_LIMIT * FINDING_OBLIGATIONS_LIMIT
            && self.counts.is_sorted_by(|a, b| key(a) < key(b))
            && (self.counts.iter())
                .all(|count| is_obligation_id(&count.obligation) && count.count > 0)
            && self.places.len() <= FINDING_PLACES_LIMIT
            && self.places.iter().all(|place| place.fault().is_none())
    }
}

/// One crop a located retry cut, by the image's identity, and the mask it gave, or
/// null when it gave none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crop {
    pub crop: Digest,
    pub mask: Option<Digest>,
}

/// What a retry changed to make its attempt: a wider `grow`, by this many pixels; or
/// the crops it cut and the masks they gave.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case", deny_unknown_fields)]
pub enum Changed {
    Wider { px: f64 },
    Located { crops: Vec<Crop> },
}

/// One attempt of a loop: its number, from 1; the runs it made, in order; its verdict,
/// or the reason there is none; what the retry changed to make it, null for the
/// first; and what its requests settled, in millionths of a US dollar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub attempt: u32,
    pub runs: Vec<String>,
    pub verdict: Option<Decision>,
    pub unchecked: Option<String>,
    pub changed: Option<Changed>,
    pub settled_micros: u64,
}

fn reason(text: &str) -> bool {
    (1..=ATTEMPT_TEXT_LIMIT).contains(&text.chars().count()) && !text.chars().any(char::is_control)
}

impl Attempt {
    /// The attempt of this number with this verdict.
    pub fn judged(attempt: u32, verdict: Decision) -> Attempt {
        Attempt {
            attempt,
            runs: Vec::new(),
            verdict: Some(verdict),
            unchecked: None,
            changed: None,
            settled_micros: 0,
        }
    }

    /// The attempt of this number that has no verdict, for this reason, in one line.
    pub fn unchecked(attempt: u32, why: &str) -> Attempt {
        Attempt {
            attempt,
            runs: Vec::new(),
            verdict: None,
            unchecked: Some(why.to_owned()),
            changed: None,
            settled_micros: 0,
        }
    }

    /// The same, having made this run after those already listed.
    pub fn ran(mut self, run: &RunId) -> Attempt {
        self.runs.push(run.as_str().to_owned());
        self
    }

    /// The same, made by a retry that changed this.
    pub fn changed(mut self, changed: Changed) -> Attempt {
        self.changed = Some(changed);
        self
    }

    /// The same, whose requests settled this much.
    pub fn settled(mut self, micros: u64) -> Attempt {
        self.settled_micros = micros;
        self
    }

    pub(super) fn holds(&self) -> bool {
        let changed = match &self.changed {
            None => true,
            Some(_) if self.attempt == 1 => false,
            Some(Changed::Wider { px }) => px.is_finite() && *px >= 0.0,
            Some(Changed::Located { crops }) => (1..=ATTEMPT_CROPS_LIMIT).contains(&crops.len()),
        };
        changed
            && self.runs.len() <= ATTEMPT_RUNS_LIMIT
            && self.runs.iter().all(|run| RunId::parse(run).is_ok())
            && self.verdict.is_some() != self.unchecked.is_some()
            && self.unchecked.as_deref().is_none_or(reason)
    }
}

/// A loop's attempts: each, in order; which was kept, by its number, or null when
/// none was; why the attempts ended, in one line; and, in `run/8` alone, the same as
/// one word of a closed set ([`AttemptsEnded`]), which a record of an older version
/// does not hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempted {
    pub list: Vec<Attempt>,
    pub kept: Option<u32>,
    pub ended: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<AttemptsEnded>,
}

/// What is said of attempts whose loop never said why they ended.
pub const ATTEMPTS_NOT_ENDED: &str = "the run ended before it said why its attempts did";

impl Attempted {
    pub(super) fn holds(&self) -> bool {
        let numbered = (self.list.iter().zip(1u32..)).all(|(attempt, at)| attempt.attempt == at);
        self.list.len() <= PLAN_ATTEMPTS_MAX as usize
            && numbered
            && self.list.iter().all(Attempt::holds)
            && (self.kept).is_none_or(|kept| (1..=self.list.len() as u32).contains(&kept))
            && reason(&self.ended)
    }
}
