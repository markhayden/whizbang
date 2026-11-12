//! A run's id, and how a run ends.

use crate::artifact::{ArtifactId, ArtifactType};

/// The longest run id, in characters.
pub const RUN_ID_LIMIT: usize = 64;

/// The id of a run: a lower-case letter or a digit, then lower-case letters, digits, `_`
/// or `-`; 64 characters at most.
///
/// So an id is one path segment, one word on a command line and one part of an address,
/// with nothing to escape in any of them, and it never differs from another only by
/// letter case. The executor is what makes a run, and it names it with a `RunId`; nothing
/// else gives the store a run. The store's own check ([`crate::store::Publication::put_in`])
/// is the looser one, not empty and no control character, and stays as the floor under
/// this rule.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunId(String);

impl RunId {
    /// The id this text is, or why it is none. The refusal shows the text escaped, and
    /// its first 64 characters at most.
    pub fn parse(id: &str) -> Result<RunId, String> {
        let first = id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
        let rest = id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
        if !first || !rest || id.len() > RUN_ID_LIMIT {
            // What was refused can be anything, of any length. It is shown escaped, and
            // only as far as the longest id goes.
            let shown: String = id.chars().take(RUN_ID_LIMIT).collect();
            let length = id.chars().count();
            let cut = if length > RUN_ID_LIMIT {
                format!(" and so on ({length} characters)")
            } else {
                String::new()
            };
            return Err(format!(
                "{shown:?}{cut} is no run id: a run id is a lower-case letter or a digit, \
                 then lower-case letters, digits, _ or -, {RUN_ID_LIMIT} characters at most"
            ));
        }
        Ok(RunId(id.to_owned()))
    }

    /// The id the executor gives a new run: the time it started, in milliseconds since
    /// the Unix epoch, a hyphen, and a count. Digits and one hyphen, 41 characters at
    /// most, so it keeps the rule above. Runs that started at different milliseconds
    /// sort, by id, in the order they started, while the time has as many digits.
    pub(super) fn minted(now_ms: u64, count: u64) -> RunId {
        RunId(format!("{now_ms}-{count}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One output a run committed: the port it was returned at, its type and its identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    port: String,
    artifact: ArtifactType,
    id: ArtifactId,
}

impl Committed {
    pub fn new(port: &str, artifact: ArtifactType, id: ArtifactId) -> Committed {
        Committed {
            port: port.to_owned(),
            artifact,
            id,
        }
    }

    pub fn port(&self) -> &str {
        &self.port
    }

    pub fn artifact(&self) -> ArtifactType {
        self.artifact
    }

    pub fn id(&self) -> ArtifactId {
        self.id
    }
}

/// How a run ended (`SPEC.md` section 3): completed, cancelled, error or invalid. A
/// refusal is not one of these: it happens before any run exists ([`super::Refusal`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The action finished and its outputs were committed together.
    Completed { outputs: Vec<Committed> },
    /// A cancellation was accepted before the outputs were committed. None were.
    Cancelled,
    /// The action failed: the type of the failure and what it said. No output was
    /// committed.
    Error { kind: String, message: String },
    /// The action ran and found that what it was given cannot be worked on. No output
    /// was committed.
    Invalid { why: String },
}

impl Outcome {
    /// The outcome's name, as a record spells it.
    pub fn name(&self) -> &'static str {
        match self {
            Outcome::Completed { .. } => "completed",
            Outcome::Cancelled => "cancelled",
            Outcome::Error { .. } => "error",
            Outcome::Invalid { .. } => "invalid",
        }
    }

    /// The committed outputs: those of a completed run, and none of any other.
    pub fn outputs(&self) -> &[Committed] {
        match self {
            Outcome::Completed { outputs } => outputs,
            _ => &[],
        }
    }
}

/// A run that ended: which run, and how. The run's record is found by its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    run: RunId,
    outcome: Outcome,
}

impl Finished {
    pub fn new(run: RunId, outcome: Outcome) -> Finished {
        Finished { run, outcome }
    }

    pub fn run(&self) -> &RunId {
        &self.run
    }

    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }
}
