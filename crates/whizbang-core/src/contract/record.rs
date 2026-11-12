//! The run record (`SPEC.md` section 3, "What every run records"; ADR 0005): the one
//! account of a run, written whatever its outcome, with a fixed, versioned shape.
//!
//! A record holds every group the spec lists. A group that nothing can fill yet is there
//! and empty, and its type has no value: such a list cannot hold anything in `run/1`.
//! Whatever first fills one writes a new version of the record.
//!
//! A record is written one way: as JSON on one line, its fields in the order of the
//! types below, followed by a newline. It is read strictly: a field that is not known, a
//! version this build does not read, and bytes that are not the bytes this code would
//! write for the same record, are each refused.

use serde::{Deserialize, Serialize};

use crate::artifact::ArtifactType;
use crate::identity::Digest;
use crate::store::{self, Entry, Generation, Kind};

use super::journal::{JournalLine, journal_bytes};
use super::{Outcome, PreparedCall, Secrets, StoreFailure, Value};

/// The record's kind, with its version.
pub const RUN_RECORD: &str = "run/1";

/// The JSON Schema of a `run/1` record, as a document.
pub const RUN_RECORD_SCHEMA: &str = include_str!("../../schemas/run-1.json");

/// The most bytes a stored record may be: four mebibytes. A record is read whole and
/// written again to be compared, so what is read is bounded before it is read.
pub const RECORD_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// The name, among a run's names in the store, of its record.
pub const RECORD_NAME: &str = "record";

/// The name, among a run's names, of its journal: the progress events, one to a line.
pub const JOURNAL_NAME: &str = "journal";

/// What the name of every diagnostic an action wrote starts with, among a run's names.
pub const DIAGNOSTICS_PREFIX: &str = "diagnostics/";

/// A field that may be null and may not be left out: every field of a record is always
/// there.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

/// How many characters of a reason that can quote stored bytes a refusal shows.
pub const QUOTED_LIMIT: usize = 64;

/// A reason that can quote what was read, as a refusal shows it: its first 64
/// characters, escaped, and three dots when there were more.
pub(super) fn quoted(reason: &str) -> String {
    let shown: String = reason.chars().take(QUOTED_LIMIT).collect();
    let more = if reason.chars().count() > QUOTED_LIMIT {
        "..."
    } else {
        ""
    };
    format!("{}{more}", shown.escape_debug())
}

/// The longest name of a diagnostic an action wrote, in characters.
pub const DIAGNOSTIC_NAME_LIMIT: usize = 64;

/// Whether this can be the name an action gives a diagnostic: a lower-case letter or a
/// digit, then lower-case letters, digits, `_`, `-` or `.`; 64 characters at most. So it
/// is one path segment and one word on a command line, as a run's id is.
pub fn is_diagnostic_name(name: &str) -> bool {
    let plain = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    name.len() <= DIAGNOSTIC_NAME_LIMIT
        && name.chars().next().is_some_and(plain)
        && name
            .chars()
            .all(|c| plain(c) || c == '_' || c == '-' || c == '.')
}

/// A list that holds nothing in this version of the record: its element has no value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Nothing {}

/// Identity: which run this is, and what it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    /// The run's id.
    pub run: String,
    /// The caller's request id. None until a call can carry one.
    #[serde(deserialize_with = "present")]
    pub request: Option<String>,
    /// The run that called this one. None for a run that nothing nested.
    #[serde(deserialize_with = "present")]
    pub parent: Option<String>,
    /// The version of whizbang that ran it.
    pub whizbang: String,
}

/// The value of one argument, under the name of its kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentValue {
    Number(f64),
    Integer(i64),
    Boolean(bool),
    Text(String),
}

/// One argument as the run was given it, defaults applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Argument {
    pub name: String,
    pub value: ArgumentValue,
}

/// What was asked: the action, its exact arguments, and the settings it ran under.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asked {
    pub action: String,
    /// The action's version, as its descriptor gave it.
    pub version: String,
    /// Every argument that had a value, in the descriptor's order.
    pub arguments: Vec<Argument>,
    /// The resolved settings. There is no config yet, so there are none.
    pub settings: Vec<Nothing>,
    /// The digest of the config the settings came from.
    pub config: Digest,
}

/// One input: the port, the artifact's type and identity, and the identity of the
/// binding the artifact belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub port: String,
    #[serde(rename = "type", with = "type_name")]
    pub artifact_type: ArtifactType,
    pub artifact: Digest,
    pub binding: Digest,
}

/// One committed output: the port, and the artifact's type and identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub port: String,
    #[serde(rename = "type", with = "type_name")]
    pub artifact_type: ArtifactType,
    pub artifact: Digest,
}

/// How a run ended, as a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Completed,
    Cancelled,
    Error,
    Invalid,
}

impl Status {
    /// The word a record spells it with.
    pub fn name(self) -> &'static str {
        match self {
            Status::Completed => "completed",
            Status::Cancelled => "cancelled",
            Status::Error => "error",
            Status::Invalid => "invalid",
        }
    }
}

/// An error's type and what it said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Failed {
    pub kind: String,
    pub message: String,
}

/// The outcome: the status; for an error, its type and message; for an invalid run, why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ended {
    pub status: Status,
    /// There exactly when the status is `error`.
    #[serde(deserialize_with = "present")]
    pub error: Option<Failed>,
    /// There exactly when the status is `invalid`.
    #[serde(deserialize_with = "present")]
    pub invalid: Option<String>,
    /// What was refused inside the run, and why. Nothing is refused inside a run yet.
    pub refusals: Vec<Nothing>,
}

/// Time: the start and the end, in milliseconds since the Unix epoch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Time {
    pub start_ms: u64,
    pub end_ms: u64,
    /// The duration of each step, tool call and capability request. A run has none yet.
    pub durations: Vec<Nothing>,
}

/// One diagnostic of the run: its name among the run's names in the store, and the
/// digest and length of what is stored under it.
///
/// It is written as JSON and cannot be read from JSON by anything but this module: a
/// diagnostic comes from one of its two constructors, or from a stored record that was
/// read and checked ([`RunRecord::from_bytes`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    name: String,
    digest: Digest,
    bytes: u64,
}

impl Diagnostic {
    /// The journal of a run, as a diagnostic, with the bytes to store under its name.
    /// Each line had the secrets taken out when it was made ([`JournalLine::new`]).
    pub fn journal(lines: &[JournalLine]) -> (Diagnostic, Vec<u8>) {
        let stored = journal_bytes(lines);
        (Diagnostic::naming(JOURNAL_NAME.to_owned(), &stored), stored)
    }

    /// Something an action wrote under a name of its own, as a diagnostic, with the
    /// bytes to store under its name: the content with the secrets taken out. The name
    /// must be a plain one ([`is_diagnostic_name`]); in the run it is `diagnostics/` and
    /// that name. The digest and the length are of what is stored.
    ///
    /// This and [`Diagnostic::journal`] are the two ways to make one, beside reading a
    /// stored record: a diagnostic that a record lists was redacted when it was made.
    pub fn written(
        name: &str,
        content: &[u8],
        secrets: &Secrets,
    ) -> Result<(Diagnostic, Vec<u8>), String> {
        if !is_diagnostic_name(name) {
            let shown: String = name.chars().take(DIAGNOSTIC_NAME_LIMIT).collect();
            return Err(format!(
                "{shown:?} is no name for a diagnostic: a name is a lower-case letter or a \
                 digit, then lower-case letters, digits, _, - or ., {DIAGNOSTIC_NAME_LIMIT} \
                 characters at most"
            ));
        }
        let stored = secrets.redact_bytes(content);
        let diagnostic = Diagnostic::naming(format!("{DIAGNOSTICS_PREFIX}{name}"), &stored);
        Ok((diagnostic, stored))
    }

    fn naming(name: String, stored: &[u8]) -> Diagnostic {
        Diagnostic {
            name,
            digest: Digest::of_bytes(stored),
            bytes: stored.len() as u64,
        }
    }

    /// Its name among the run's names in the store.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The digest of what is stored under the name.
    pub fn digest(&self) -> Digest {
        self.digest
    }

    /// The length of what is stored under the name.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// A diagnostic as a stored record holds it. Private: only reading a record makes a
/// diagnostic from JSON.
#[derive(Deserialize)]
#[serde(remote = "Diagnostic", deny_unknown_fields)]
struct DiagnosticWire {
    name: String,
    digest: Digest,
    bytes: u64,
}

fn diagnostics_wire<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Diagnostic>, D::Error> {
    #[derive(Deserialize)]
    struct One(#[serde(with = "DiagnosticWire")] Diagnostic);
    let read = Vec::<One>::deserialize(deserializer)?;
    Ok(read.into_iter().map(|One(diagnostic)| diagnostic).collect())
}

/// A record as it is stored. Private: a record is read from JSON by
/// [`RunRecord::from_bytes`] and by nothing else, so none is read without being checked.
#[derive(Deserialize)]
#[serde(remote = "RunRecord", deny_unknown_fields)]
struct RunRecordWire {
    record: String,
    identity: Identity,
    asked: Asked,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    outcome: Ended,
    time: Time,
    models: Vec<Nothing>,
    spend: Vec<Nothing>,
    egress: Vec<Nothing>,
    steps: Vec<Nothing>,
    judge: Vec<Nothing>,
    #[serde(deserialize_with = "diagnostics_wire")]
    diagnostics: Vec<Diagnostic>,
}

/// The record of one run. It is written as JSON, and read only by
/// [`RunRecord::from_bytes`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunRecord {
    record: String,
    identity: Identity,
    asked: Asked,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    outcome: Ended,
    time: Time,
    models: Vec<Nothing>,
    spend: Vec<Nothing>,
    egress: Vec<Nothing>,
    steps: Vec<Nothing>,
    judge: Vec<Nothing>,
    diagnostics: Vec<Diagnostic>,
}

/// Why bytes are not a run record this build reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordError {
    /// A record of another version. A store written by other code is not migrated.
    OtherVersion { found: String },
    /// Not a record: not JSON, or not of the record's shape.
    Malformed(String),
    /// A record of the right shape that says what no run can: an error without its
    /// message, outputs of a run that did not complete, an end before the start.
    Inconsistent(String),
    /// A record that is not written the one way a record is written.
    NotCanonical,
    /// More bytes than a record may be ([`RECORD_MAX_BYTES`]). They were not read.
    TooLarge { bytes: u64 },
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordError::OtherVersion { found } => write!(
                f,
                "a record of another version: {}, and this build reads {RUN_RECORD}",
                found.escape_debug()
            ),
            // The reason can quote what was read: it is shown escaped and cut.
            RecordError::Malformed(why) => write!(f, "not a run record: {}", quoted(why)),
            RecordError::Inconsistent(why) => write!(f, "not a run record: {why}"),
            RecordError::TooLarge { bytes } => write!(
                f,
                "not a run record: {bytes} bytes, and a record is at most {RECORD_MAX_BYTES}"
            ),
            RecordError::NotCanonical => {
                f.write_str("not a run record: it is not written the one way a record is written")
            }
        }
    }
}

impl std::error::Error for RecordError {}

/// What a record is made from, for a run that just ended.
pub struct Ran<'a> {
    /// The run's id.
    pub run: &'a str,
    /// The version of whizbang.
    pub whizbang: &'a str,
    /// The call as it was prepared.
    pub prepared: &'a PreparedCall,
    /// How the run ended.
    pub outcome: &'a Outcome,
    pub start_ms: u64,
    pub end_ms: u64,
    /// The run's diagnostics, as they are stored.
    pub diagnostics: Vec<Diagnostic>,
}

impl RunRecord {
    /// The record of a run that just ended. Every text that came from outside the build
    /// (an argument's text, an error's type and message, the reason a run was invalid)
    /// has the secrets taken out before it enters the record.
    pub fn of(ran: Ran<'_>, secrets: &Secrets) -> RunRecord {
        let prepared = ran.prepared;
        let arguments = prepared
            .arguments()
            .iter()
            .map(|(name, value)| Argument {
                name: name.clone(),
                value: match value {
                    Value::Number(number) => ArgumentValue::Number(*number),
                    Value::Integer(number) => ArgumentValue::Integer(*number),
                    Value::Boolean(boolean) => ArgumentValue::Boolean(*boolean),
                    Value::Text(text) => ArgumentValue::Text(secrets.redact(text)),
                },
            })
            .collect();
        let inputs = prepared
            .inputs()
            .iter()
            .map(|input| Input {
                port: input.port().to_owned(),
                artifact_type: input.artifact(),
                artifact: input.id().0,
                binding: input.binding().0,
            })
            .collect();
        let outputs = ran
            .outcome
            .outputs()
            .iter()
            .map(|output| Output {
                port: output.port().to_owned(),
                artifact_type: output.artifact(),
                artifact: output.id().0,
            })
            .collect();
        let (status, error, invalid) = match ran.outcome {
            Outcome::Completed { .. } => (Status::Completed, None, None),
            Outcome::Cancelled => (Status::Cancelled, None, None),
            Outcome::Error { kind, message } => (
                Status::Error,
                Some(Failed {
                    kind: secrets.redact(kind),
                    message: secrets.redact(message),
                }),
                None,
            ),
            Outcome::Invalid { why } => (Status::Invalid, None, Some(secrets.redact(why))),
        };
        let mut diagnostics = ran.diagnostics;
        diagnostics.sort_by(|a, b| a.name.cmp(&b.name));
        RunRecord {
            record: RUN_RECORD.to_owned(),
            identity: Identity {
                run: ran.run.to_owned(),
                request: None,
                parent: None,
                whizbang: ran.whizbang.to_owned(),
            },
            asked: Asked {
                action: prepared.action().to_owned(),
                version: prepared.version().to_owned(),
                arguments,
                settings: Vec::new(),
                config: prepared.config(),
            },
            inputs,
            outputs,
            outcome: Ended {
                status,
                error,
                invalid,
                refusals: Vec::new(),
            },
            time: Time {
                start_ms: ran.start_ms,
                // A clock that went back does not make a run end before it started.
                end_ms: ran.end_ms.max(ran.start_ms),
                durations: Vec::new(),
            },
            models: Vec::new(),
            spend: Vec::new(),
            egress: Vec::new(),
            steps: Vec::new(),
            judge: Vec::new(),
            diagnostics,
        }
    }

    /// The record as it is stored: JSON on one line, then a newline.
    pub fn to_bytes(&self) -> Vec<u8> {
        // A record holds strings, whole numbers, finite numbers and lists of these;
        // writing one as JSON cannot fail.
        let mut bytes = serde_json::to_vec(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// Reads a stored record. The version is looked at first, by itself: a record of
    /// another version does not have this one's shape, and is another version, not
    /// damage.
    pub fn from_bytes(bytes: &[u8]) -> Result<RunRecord, RecordError> {
        if bytes.len() as u64 > RECORD_MAX_BYTES {
            return Err(RecordError::TooLarge {
                bytes: bytes.len() as u64,
            });
        }
        #[derive(Deserialize)]
        struct Version {
            record: String,
        }
        match serde_json::from_slice::<Version>(bytes) {
            Ok(Version { record }) if record != RUN_RECORD => {
                // What it says its version is can be anything: its first 64 characters
                // are kept.
                return Err(RecordError::OtherVersion {
                    found: record.chars().take(QUOTED_LIMIT).collect(),
                });
            }
            _ => {}
        }
        let malformed = |error: serde_json::Error| RecordError::Malformed(error.to_string());
        let mut reader = serde_json::Deserializer::from_slice(bytes);
        let record = RunRecordWire::deserialize(&mut reader).map_err(malformed)?;
        // Nothing but white space after the record.
        reader.end().map_err(malformed)?;
        record.check()?;
        if record.to_bytes() != bytes {
            return Err(RecordError::NotCanonical);
        }
        Ok(record)
    }

    /// What the shape alone does not hold.
    fn check(&self) -> Result<(), RecordError> {
        let refuse = |why: &str| Err(RecordError::Inconsistent(why.to_owned()));
        let named = [
            ("the run's id", &self.identity.run),
            ("the version of whizbang", &self.identity.whizbang),
            ("the action's name", &self.asked.action),
            ("the action's version", &self.asked.version),
        ];
        let arguments = self
            .asked
            .arguments
            .iter()
            .map(|a| ("an argument's name", &a.name));
        let inputs = self.inputs.iter().map(|i| ("an input's port", &i.port));
        let outputs = self.outputs.iter().map(|o| ("an output's port", &o.port));
        for (what, text) in named
            .into_iter()
            .chain(arguments)
            .chain(inputs)
            .chain(outputs)
        {
            if text.is_empty() {
                return refuse(&format!("{what} is empty"));
            }
        }
        // A diagnostic is the journal, or something the action wrote under a plain name.
        // The record itself is one of the run's names and is not one of its diagnostics.
        let known = |name: &str| {
            name == JOURNAL_NAME
                || name
                    .strip_prefix(DIAGNOSTICS_PREFIX)
                    .is_some_and(is_diagnostic_name)
        };
        if !self.diagnostics.iter().all(|d| known(&d.name)) {
            return refuse(
                "a diagnostic is named journal, or diagnostics/ and a plain name: a \
                 lower-case letter or a digit, then lower-case letters, digits, _, - or ., \
                 64 characters at most",
            );
        }
        let outcome = &self.outcome;
        if outcome.error.is_some() != (outcome.status == Status::Error) {
            return refuse("an error has its type and message, and nothing else has");
        }
        if outcome.invalid.is_some() != (outcome.status == Status::Invalid) {
            return refuse("an invalid run has its reason, and nothing else has");
        }
        if outcome.status != Status::Completed && !self.outputs.is_empty() {
            return refuse("only a completed run has outputs");
        }
        if self.time.end_ms < self.time.start_ms {
            return refuse("the run ends before it starts");
        }
        if !self.diagnostics.is_sorted_by(|a, b| a.name < b.name) {
            return refuse("the diagnostics are not in the order of their names, each once");
        }
        Ok(())
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    pub fn asked(&self) -> &Asked {
        &self.asked
    }

    pub fn inputs(&self) -> &[Input] {
        &self.inputs
    }

    /// A reference to every committed output: those of a completed run, and none of any
    /// other.
    pub fn outputs(&self) -> &[Output] {
        &self.outputs
    }

    pub fn outcome(&self) -> &Ended {
        &self.outcome
    }

    pub fn time(&self) -> &Time {
        &self.time
    }

    /// The run's diagnostics, in the order of their names.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Why a run's record could not be had from a store.
#[derive(Debug)]
pub enum StoredRecordError {
    /// The run has no name `record`: something else made it.
    NoRecord,
    /// The store failed to give the record's content. Its text and what `{:?}` prints
    /// of it hold no path; the store's own error is [`StoreFailure::cause`].
    Store(StoreFailure),
    /// What the run holds under `record` is not a record this build reads.
    Record(RecordError),
    /// The record names another run than the one it is stored in.
    OtherRun { names: String },
}

impl std::fmt::Display for StoredRecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoredRecordError::NoRecord => f.write_str("the run has no record"),
            StoredRecordError::Store(error) => error.fmt(f),
            StoredRecordError::Record(error) => error.fmt(f),
            StoredRecordError::OtherRun { names } => {
                write!(f, "the record names another run: {}", names.escape_debug())
            }
        }
    }
}

impl std::error::Error for StoredRecordError {}

/// The record of the run `id` in this generation, read with `read`, which gives the
/// content of an entry of either kind ([`store::Reader::read`], [`store::Store::read`]).
///
/// `id` is whatever the store holds a run under: it is not held to the rule of a
/// [`super::RunId`], because a store can hold runs that other code made. `None` when the
/// generation holds no such run.
pub fn stored(
    generation: &Generation,
    id: &str,
    read: impl Fn(&Entry) -> store::Result<Vec<u8>>,
) -> Option<Result<RunRecord, StoredRecordError>> {
    let run = generation.run(id)?;
    let Some(entry) = run.get(RECORD_NAME) else {
        return Some(Err(StoredRecordError::NoRecord));
    };
    if entry.bytes > RECORD_MAX_BYTES {
        return Some(Err(StoredRecordError::Record(RecordError::TooLarge {
            bytes: entry.bytes,
        })));
    }
    let record = read(entry)
        .map_err(|error| StoredRecordError::Store(StoreFailure::new(error)))
        .and_then(|bytes| RunRecord::from_bytes(&bytes).map_err(StoredRecordError::Record))
        .and_then(|record| {
            if record.identity.run == id {
                Ok(record)
            } else {
                Err(StoredRecordError::OtherRun {
                    names: record.identity.run.clone(),
                })
            }
        });
    Some(record)
}

/// The kind of reference a run's record and its diagnostics are stored as: they are
/// kept and readable, and never usable as an output.
pub const RECORD_KIND: Kind = Kind::Diagnostic;

/// An artifact type as a record spells it: its name.
mod type_name {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::artifact::ArtifactType;

    pub fn serialize<S: Serializer>(artifact: &ArtifactType, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(artifact.name())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ArtifactType, D::Error> {
        let name = String::deserialize(d)?;
        ArtifactType::ALL
            .into_iter()
            .find(|artifact| artifact.name() == name)
            .ok_or_else(|| serde::de::Error::custom("not the name of an artifact type"))
    }
}
