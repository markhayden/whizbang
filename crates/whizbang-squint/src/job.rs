//! The job a check judges (plan 2C, task A1, item 4; ADR 0028): what is judged, against
//! what, under which use case, with which requirements. Ported from squint
//! (`crates/squint-core/src/job.rs`, at the commit `docs/sources.toml` pins), and reshaped
//! for whizbang, where an image is an artifact in the store: a job's images are bound, in
//! the job's order, to the images the call of `check` is given, and an image's `source`
//! says where the picture came from and is otherwise not read.
//!
//! A job travels in two forms. A squint job file, TOML with a table `[job]`, is read
//! whole ([`Job::from_toml`]). The argument `job` of `check` holds the same job as JSON
//! in the contract `job/1` ([`Job::to_json`], [`Job::from_json`]): the job's fields and a
//! field `contract` that says `job/1`.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::use_case::Obligation;

/// The name and version of the contract the argument `job` holds.
pub const CONTRACT: &str = "job/1";

/// What an image is for. Exactly one candidate; at most one source; any number of
/// references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Candidate,
    Source,
    Reference,
}

impl Role {
    /// The role's word, as a job and a rule write it.
    pub fn word(self) -> &'static str {
        match self {
            Role::Candidate => "candidate",
            Role::Source => "source",
            Role::Reference => "reference",
        }
    }
}

/// Where a picture came from, as a squint job file says: a file, pinned to its SHA-256
/// when the job pins it, or the picture inline. Recorded with the job; a command that
/// reads a job file takes the picture from its path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

/// One image of a job: its id, which names it in the prompt, its checklist and its
/// frame; its role; and its label, the words a person gave it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobImage {
    pub id: String,
    pub role: Role,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// `flat`, the only projection a check judges here, when it is written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<String>,
}

/// One job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub use_case: String,
    /// The job's requirements in words, as data for the judge.
    #[serde(default)]
    pub brief: Option<String>,
    /// Structured requirements, with the semantics of rules; ids `RQ<n>`.
    #[serde(default)]
    pub requirements: Vec<Obligation>,
    #[serde(default)]
    pub images: Vec<JobImage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JobFile {
    job: Job,
}

/// Image ids name frames in prompts, tool arguments and checklist ids, so they are
/// plain tokens.
pub fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '/' | '-')
}

impl Job {
    /// The job the command line states (squint's `Job::from_shorthand`): the candidate,
    /// then the source when one is given, then each reference in the order given, as
    /// `reference-1`, `reference-2`, … labelled `reference 1`, `reference 2`, …; the
    /// brief as the job's words, and no structured requirement.
    pub fn shorthand(use_case: &str, source: bool, references: usize, brief: Option<&str>) -> Job {
        let image = |id: String, role, label| JobImage {
            id,
            role,
            label,
            source: None,
            projection: None,
        };
        let mut images = vec![image("candidate".to_owned(), Role::Candidate, None)];
        if source {
            images.push(image("source".to_owned(), Role::Source, None));
        }
        for at in 1..=references {
            let label = Some(format!("reference {at}"));
            images.push(image(format!("reference-{at}"), Role::Reference, label));
        }
        Job {
            use_case: use_case.to_owned(),
            brief: brief.map(str::to_owned),
            requirements: Vec::new(),
            images,
        }
    }

    /// A squint job file, read whole, and checked ([`Job::validate`]).
    pub fn from_toml(text: &str) -> Result<Job, String> {
        let file: JobFile = toml::from_str(text).map_err(|e| format!("the job file: {e}"))?;
        file.job.validate()?;
        Ok(file.job)
    }

    /// The job as the argument `job` holds it: JSON, in the contract `job/1`.
    pub fn to_json(&self) -> String {
        let mut value = serde_json::to_value(self).unwrap_or(Json::Null);
        if let Json::Object(fields) = &mut value {
            fields.insert("contract".to_owned(), Json::from(CONTRACT));
        }
        value.to_string()
    }

    /// The job an argument `job` holds, and checked ([`Job::validate`]).
    pub fn from_json(text: &str) -> Result<Job, String> {
        let mut value: Json =
            serde_json::from_str(text).map_err(|e| format!("the job is not JSON: {e}"))?;
        let contract = value.as_object_mut().and_then(|o| o.remove("contract"));
        if contract != Some(Json::from(CONTRACT)) {
            return Err(format!(
                "the job does not say it is in the contract {CONTRACT}"
            ));
        }
        let job: Job = serde_json::from_value(value).map_err(|e| format!("the job: {e}"))?;
        job.validate()?;
        Ok(job)
    }

    /// Exactly one candidate, at most one source, ids that are plain tokens and unique,
    /// and every image flat (squint's `Job::validate`, and the projection whizbang
    /// judges).
    pub fn validate(&self) -> Result<(), String> {
        let count = |role| self.images.iter().filter(|i| i.role == role).count();
        let candidates = count(Role::Candidate);
        if candidates != 1 {
            return Err(format!(
                "a job has exactly one image with role = \"candidate\"; found {candidates}"
            ));
        }
        let sources = count(Role::Source);
        if sources > 1 {
            return Err(format!(
                "a job has at most one image with role = \"source\"; found {sources}"
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for image in &self.images {
            if image.id.is_empty() {
                return Err("an image id must not be empty".to_owned());
            }
            if !image.id.chars().all(is_id_char) {
                return Err(format!(
                    "image id {:?}: ids are ASCII letters, digits and `_ . : / -` only",
                    image.id
                ));
            }
            if !seen.insert(image.id.as_str()) {
                return Err(format!("duplicate image id {:?}", image.id));
            }
            if image.projection.as_deref().is_some_and(|p| p != "flat") {
                return Err(format!(
                    "image {:?}: a check judges flat images only",
                    image.id
                ));
            }
        }
        Ok(())
    }
}
