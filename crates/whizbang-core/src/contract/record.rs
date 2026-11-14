//! The run record (`SPEC.md` section 3, "What every run records"; ADR 0005): the one
//! account of a run, written whatever its outcome, with a fixed, versioned shape.
//!
//! A record holds every group the spec lists. This build writes `run/3` (ADR 0013),
//! which gives `asked.settings`, `spend.requests` and `egress` their elements, adds
//! the refusals of a capability request, and adds `outcome.lost`: what the record of
//! a run that a dead process left does not know. It reads `run/3`, and `run/2` as the
//! tag `m1a` froze it (ADR 0005), in which those lists hold nothing.
//!
//! A record is written one way: as JSON on one line, its fields in the order of the
//! types below, followed by a newline. It is read strictly: a field that is not known, a
//! version this build does not read, and bytes that are not the bytes this code would
//! write for the same record, are each refused.

use serde::{Deserialize, Serialize};

use crate::artifact::ArtifactType;
use crate::capability::{Excluded, FaultKind, HowEnded, Logged, MODEL_NAME_LIMIT, Stage};
use crate::egress::{Account, Classes, DataClass, Origin, Recipient};
use crate::identity::Digest;
use crate::store::{self, Entry, Generation, Kind};

use super::journal::{JournalLine, journal_bytes};
use super::{
    Capability, Outcome, PreparedCall, RequestId, RunId, Secrets, StoreFailure, Value, Verdict,
    text,
};

/// The record's kind, with the version this build writes.
pub const RUN_RECORD: &str = "run/3";

/// The JSON Schema of a `run/3` record, as a document.
pub const RUN_RECORD_SCHEMA: &str = include_str!("../../schemas/run-3.json");

/// The kind and version of the record checkpoint 1A wrote. It is still read, as the
/// tag `m1a` froze it, and never written again.
pub const RUN_2: &str = "run/2";

/// The JSON Schema of a `run/2` record, as the tag `m1a` froze it.
pub const RUN_2_SCHEMA: &str = include_str!("../../schemas/run-2.json");

/// The most bytes a stored record may be: four mebibytes. A record is read whole and
/// written again to be compared, so what is read is bounded before it is read.
pub const RECORD_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// The name, among a run's names in the store, of its record. A run has it from its end;
/// before that it has its admission alone ([`super::ADMITTED_NAME`]).
pub const RECORD_NAME: &str = "record";

/// The name, among a run's names, of its journal: the progress events, one to a line.
pub const JOURNAL_NAME: &str = "journal";

/// What the name of every diagnostic an action wrote starts with, among a run's names.
pub const DIAGNOSTICS_PREFIX: &str = "diagnostics/";

/// What the name of every reference a run holds to an output it committed starts with,
/// among the run's names: the port follows, a `/`, and the output's place among those
/// returned at that port, from 0 (ADR 0008).
pub const OUTPUTS_PREFIX: &str = "outputs/";

/// A field that may be null and may not be left out: every field of a record is always
/// there.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

/// How many characters a refusal keeps of a name it quotes from stored bytes: a version,
/// the id of another run.
pub const QUOTED_LIMIT: usize = 64;

/// How many characters a refusal shows of a reader's reason, which can quote stored
/// bytes: enough for the reason and for where in the bytes it was met.
pub const REASON_LIMIT: usize = 200;

/// A reader's reason, as a refusal shows it: its first 200 characters, escaped, and
/// three dots when there were more.
pub(super) fn quoted(reason: &str) -> String {
    let shown: String = reason.chars().take(REASON_LIMIT).collect();
    let more = if reason.chars().count() > REASON_LIMIT {
        "..."
    } else {
        ""
    };
    format!("{}{more}", shown.escape_debug())
}

/// The longest name of a diagnostic an action wrote, in characters.
pub const DIAGNOSTIC_NAME_LIMIT: usize = 64;

/// The most diagnostics the action of one run may write. A record lists each, so one
/// more is refused where it is written ([`super::Diagnostics::write`]). The run's
/// journal and the exchanges the executor keeps are not among them: there is one
/// journal, and a run sends a bounded number of requests.
pub const DIAGNOSTICS_LIMIT: usize = 1024;

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

/// The most calls a run's record lists in `steps`: the most runs one run may call. One
/// more is refused where it is made ([`super::Refusal::TooManyCalls`]), so the list
/// is whole. It is as low as it is because a call is two publications, and the cost of
/// a publication grows with what the store holds (`docs/carry-over.md`).
pub const STEPS_LIMIT: usize = 256;

/// The most refusals a run's record lists. Those after are counted
/// (`outcome.refusals_omitted`) and not listed.
pub const REFUSALS_LIMIT: usize = 256;

/// The most characters a record keeps of the sentence of one refusal.
pub const REFUSAL_WHY_LIMIT: usize = 512;

/// What kind of refusal a call from inside a run met, as a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalKind {
    UnknownAction,
    Invalid,
    Input,
    NotReady,
    ReadOnly,
    NoSuchRun,
    Conflict,
    Stale,
    OverAllowance,
    NoBound,
    SecretInRequestId,
    NotARequestId,
    Cancelled,
    DeadlinePassed,
    TooDeep,
    CallBack,
    OverCaller,
    CallerOver,
    TooManyCalls,
    TreeFull,
    TooManyStarted,
    TooManyStartedUnder,
    IdInside,
    WaitAbove,
    WaitCancelled,
    NotOffered,
    NotReadable,
    /// A capability request to a backend that states no enforceable bound.
    RequestNoBound,
    /// A capability request whose bound is over what its run has left.
    RequestOverAllowance,
    /// A capability request to a recipient that is not approved for what it holds.
    RequestNotApproved,
    /// A capability request to a model that is not installed, or is being removed.
    RequestNotReady,
}

impl RefusalKind {
    /// Whether this is a refusal of a capability request that was not sent, which
    /// names no action (ADR 0013), and not of a call.
    pub fn is_of_a_request(self) -> bool {
        matches!(
            self,
            RefusalKind::RequestNoBound
                | RefusalKind::RequestOverAllowance
                | RefusalKind::RequestNotApproved
                | RefusalKind::RequestNotReady
        )
    }

    /// Every kind, in the order above.
    pub const ALL: [RefusalKind; 31] = [
        RefusalKind::UnknownAction,
        RefusalKind::Invalid,
        RefusalKind::Input,
        RefusalKind::NotReady,
        RefusalKind::ReadOnly,
        RefusalKind::NoSuchRun,
        RefusalKind::Conflict,
        RefusalKind::Stale,
        RefusalKind::OverAllowance,
        RefusalKind::NoBound,
        RefusalKind::SecretInRequestId,
        RefusalKind::NotARequestId,
        RefusalKind::Cancelled,
        RefusalKind::DeadlinePassed,
        RefusalKind::TooDeep,
        RefusalKind::CallBack,
        RefusalKind::OverCaller,
        RefusalKind::CallerOver,
        RefusalKind::TooManyCalls,
        RefusalKind::TreeFull,
        RefusalKind::TooManyStarted,
        RefusalKind::TooManyStartedUnder,
        RefusalKind::IdInside,
        RefusalKind::WaitAbove,
        RefusalKind::WaitCancelled,
        RefusalKind::NotOffered,
        RefusalKind::NotReadable,
        RefusalKind::RequestNoBound,
        RefusalKind::RequestOverAllowance,
        RefusalKind::RequestNotApproved,
        RefusalKind::RequestNotReady,
    ];
}

/// One call a run made that was refused: no run was made for it (ADR 0008, "Nested
/// calls"). The action is there when what the call named is an action's name by the
/// rule, registered or not. The sentence is the refusal's, the secrets taken out and
/// then cut at [`REFUSAL_WHY_LIMIT`] characters; it can quote what the caller wrote,
/// so whoever prints it escapes it.
///
/// It is written as JSON, and read from JSON only as part of a stored record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Refused {
    action: Option<String>,
    kind: RefusalKind,
    why: String,
}

impl Refused {
    /// The refusal of this call, as a record keeps it. Redaction comes first, and
    /// only then the cut: a secret that straddles the cut is taken out whole, as for
    /// every text an outcome keeps. A call
    /// whose own texts hold a secret has a sentence of fixed words instead, since a
    /// refusal can quote a part of them that was cut before it came here.
    pub(super) fn of(call: &super::Call, refusal: &super::Refusal, secrets: &Secrets) -> Refused {
        let texts = std::iter::once(call.action())
            .chain(call.inputs().keys().map(String::as_str))
            .chain(call.arguments().iter().flat_map(|(name, value)| {
                let value = match value {
                    Value::Text(text) => Some(text.as_str()),
                    _ => None,
                };
                std::iter::once(name.as_str()).chain(value)
            }));
        let clean = texts.into_iter().all(|text| secrets.redact(text) == text);
        let why = if clean {
            super::engine::said(secrets, &refusal.to_string())
        } else {
            "the call held something that must not be written, so its refusal is not \
             quoted"
                .to_owned()
        };
        let named = clean && text::action_name(call.action()).is_ok();
        Refused {
            action: named.then(|| call.action().to_owned()),
            kind: refusal.kind(),
            why: why.chars().take(REFUSAL_WHY_LIMIT).collect(),
        }
    }

    /// A refusal of something an action asked of a run, which names no action: a
    /// wait that was refused or given up. Its sentence holds a run's id and fixed
    /// words, and goes through the same redaction and cut.
    pub(super) fn of_a_question(refusal: &super::Refusal, secrets: &Secrets) -> Refused {
        let why = super::engine::said(secrets, &refusal.to_string());
        Refused {
            action: None,
            kind: refusal.kind(),
            why: why.chars().take(REFUSAL_WHY_LIMIT).collect(),
        }
    }

    /// The refusal of a capability request that was not sent (ADR 0013): it names no
    /// action, and its kind is one of the four that are of a request. Its sentence
    /// goes through the same redaction and cut as any refusal's. None when the kind
    /// is of a call.
    pub fn of_a_request(kind: RefusalKind, why: &str, secrets: &Secrets) -> Option<Refused> {
        let why = super::engine::said(secrets, why);
        let why: String = why.chars().take(REFUSAL_WHY_LIMIT).collect();
        (kind.is_of_a_request() && !why.is_empty()).then_some(Refused {
            action: None,
            kind,
            why,
        })
    }

    /// The action the call named, when what it named is an action's name by the rule.
    pub fn action(&self) -> Option<&str> {
        self.action.as_deref()
    }

    pub fn kind(&self) -> RefusalKind {
        self.kind
    }

    /// The refusal's sentence. It is not escaped.
    pub fn why(&self) -> &str {
        &self.why
    }
}

#[derive(Deserialize)]
#[serde(remote = "Refused", deny_unknown_fields)]
struct RefusedWire {
    #[serde(deserialize_with = "present")]
    action: Option<String>,
    kind: RefusalKind,
    why: String,
}

fn refusals_wire<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Refused>, D::Error> {
    #[derive(Deserialize)]
    struct One(#[serde(with = "RefusedWire")] Refused);
    let read = Vec::<One>::deserialize(deserializer)?;
    Ok(read.into_iter().map(|One(refused)| refused).collect())
}

/// What kind of step this is, as a word. A call of another action is the one kind
/// there is; recipe steps come with the recipe runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Call,
}

/// One step of a run: a call it made of another action, which was admitted and so has
/// a run of its own (ADR 0008, "Nested calls"). The steps are in the order the calls
/// were admitted. The call's arguments, inputs and outputs are in the record of its
/// run, which this names: that is the reference to its result. `status` is how that
/// run had ended when this record was written, and null when it had not: the store
/// refused its end, and it was abandoned.
///
/// It is written as JSON, and read from JSON only as part of a stored record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Step {
    step: StepKind,
    run: String,
    action: String,
    status: Option<Status>,
}

impl Step {
    pub(super) fn call(run: &RunId, action: &str, status: Option<Status>) -> Step {
        Step {
            step: StepKind::Call,
            run: run.as_str().to_owned(),
            action: action.to_owned(),
            status,
        }
    }

    pub fn kind(&self) -> StepKind {
        self.step
    }

    /// The run the call made.
    pub fn run(&self) -> &str {
        &self.run
    }

    /// The action that was called.
    pub fn action(&self) -> &str {
        &self.action
    }

    /// How the call's run had ended when the record was written, if it had.
    pub fn status(&self) -> Option<Status> {
        self.status
    }
}

#[derive(Deserialize)]
#[serde(remote = "Step", deny_unknown_fields)]
struct StepWire {
    step: StepKind,
    run: String,
    action: String,
    #[serde(deserialize_with = "present")]
    status: Option<Status>,
}

fn steps_wire<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Step>, D::Error> {
    #[derive(Deserialize)]
    struct One(#[serde(with = "StepWire")] Step);
    let read = Vec::<One>::deserialize(deserializer)?;
    Ok(read.into_iter().map(|One(step)| step).collect())
}

/// The most capability requests a run's record lists in `models`: the most one run
/// sends. One more is not sent ([`crate::capability::REQUESTS_LIMIT`]), so the list is
/// whole.
pub const MODELS_LIMIT: usize = crate::capability::REQUESTS_LIMIT;

/// The most characters a record keeps of a provider's id for an exchange.
pub const PROVIDER_ID_LIMIT: usize = 128;

/// How a capability request ended, as a word: `answered`, the kind of its fault (ADR
/// 0006), or `unwound` for a backend whose call panicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestEnded {
    Answered,
    Cancelled,
    Unsupported,
    Credential,
    Rejected,
    Unavailable,
    TimedOut,
    Malformed,
    TooLarge,
    Unwound,
}

impl RequestEnded {
    /// Every way, in the order above.
    pub const ALL: [RequestEnded; 10] = [
        RequestEnded::Answered,
        RequestEnded::Cancelled,
        RequestEnded::Unsupported,
        RequestEnded::Credential,
        RequestEnded::Rejected,
        RequestEnded::Unavailable,
        RequestEnded::TimedOut,
        RequestEnded::Malformed,
        RequestEnded::TooLarge,
        RequestEnded::Unwound,
    ];

    pub(super) fn of(ended: HowEnded) -> RequestEnded {
        match ended {
            HowEnded::Answered => RequestEnded::Answered,
            HowEnded::Unwound => RequestEnded::Unwound,
            HowEnded::Fault(kind) => match kind {
                FaultKind::Cancelled => RequestEnded::Cancelled,
                FaultKind::Unsupported => RequestEnded::Unsupported,
                FaultKind::Credential => RequestEnded::Credential,
                FaultKind::Rejected => RequestEnded::Rejected,
                FaultKind::Unavailable => RequestEnded::Unavailable,
                FaultKind::TimedOut => RequestEnded::TimedOut,
                FaultKind::Malformed => RequestEnded::Malformed,
                FaultKind::TooLarge => RequestEnded::TooLarge,
            },
        }
    }
}

/// One capability request the run sent (ADR 0009): which capability, and the backend,
/// the model and the version that answered, as the backend declares them. The requests
/// are in the order they were sent. `device` and `cached` are null: no response says
/// which device answered or whether a cache did (ADR 0006), so neither is known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub capability: Capability,
    pub backend: String,
    pub model: String,
    pub version: String,
    #[serde(deserialize_with = "present")]
    pub device: Option<Nothing>,
    #[serde(deserialize_with = "present")]
    pub cached: Option<Nothing>,
    pub ended: RequestEnded,
}

/// What a duration is of, as a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Timed {
    /// A capability request: `index` is its place in `models`.
    Request,
    /// A call of another action: `index` is its place in `steps`.
    Call,
}

/// How long one capability request, or one call of another action, took (ADR 0009):
/// when it began, in milliseconds since the Unix epoch, and how many milliseconds
/// later it was over, by the executor's clock. The requests come first, then the
/// calls, each in the order of its list. A call whose run was abandoned has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Duration {
    pub of: Timed,
    pub index: u64,
    pub start_ms: u64,
    pub ms: u64,
}

/// The tokens a judge's turn used, as the backend reported them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tokens {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

/// Why a judge's turn ended, as a word. For `other`, the provider's own word is in
/// the exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stop {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
    Other,
}

/// What a judging action concluded, as a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Pass,
    Fail,
    Unsure,
}

/// One entry of the `judge` group (ADR 0009), under the word that says which it is.
///
/// A `turn` is one request to a judge, in the order they were sent: a summary within a
/// bound. `request` is its place in `models`; `exchange` names the diagnostic of the
/// run that holds the whole of what was sent and what came back, or is null when it
/// could not be kept; `stop` is null when no turn came back, and the request's fault
/// is then in `models`; `excluded` says why the harness left the turn out of the
/// conversation that went on, or is null when it did not.
///
/// A `verdict` is what the action concluded: at most one, the last entry, of a run
/// that completed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "judge", rename_all = "snake_case", deny_unknown_fields)]
pub enum Judged {
    Turn {
        request: u64,
        #[serde(deserialize_with = "present")]
        exchange: Option<String>,
        max_output_tokens: u32,
        messages: u64,
        #[serde(deserialize_with = "present")]
        stop: Option<Stop>,
        tool_calls: u64,
        #[serde(deserialize_with = "present")]
        usage: Option<Tokens>,
        #[serde(deserialize_with = "present")]
        provider_id: Option<String>,
        #[serde(deserialize_with = "present")]
        excluded: Option<Excluded>,
    },
    Verdict {
        decision: Decision,
        why: String,
    },
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
    /// The caller's id for the request that made the run, or none when it gave none.
    /// One that is there is a request id ([`RequestId`]): a record with any other text
    /// here is refused.
    #[serde(deserialize_with = "present")]
    pub request: Option<String>,
    /// The run that called this one, from inside its action (ADR 0008, "Nested
    /// calls"). None for a run that nothing nested. One that is there is a run id
    /// ([`RunId`]), and not this run's.
    #[serde(deserialize_with = "present")]
    pub parent: Option<String>,
    /// The version of whizbang that ran it; for a run a dead process left, the
    /// version that ended it, with `version` among what is lost.
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

/// The most settings a record lists.
pub const SETTINGS_LIMIT: usize = 96;

/// The most characters of a setting's name, and of a setting's text.
pub const SETTING_NAME_LIMIT: usize = 128;
pub const SETTING_TEXT_LIMIT: usize = 256;

/// One resolved setting the run ran under (ADR 0013): a name and a value, written as
/// an argument is. A record lists them in the order of their names' bytes, each once.
///
/// It is written as JSON, and read from JSON only as part of a stored record.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Setting {
    name: String,
    value: ArgumentValue,
}

impl Setting {
    /// A setting, as a record keeps it. The name is 1 to [`SETTING_NAME_LIMIT`]
    /// printable ASCII characters with no space, no `"` and no `\`, and holds no
    /// secret: one that does is refused and not shown, as is one that is not a name.
    /// A text has the secrets taken out while it is whole, and is then cut at
    /// [`SETTING_TEXT_LIMIT`] characters. A number or an integer is written as it is:
    /// one that is not finite, or whose digits as they are written hold a secret,
    /// makes no setting.
    pub fn new(name: &str, value: &Value, secrets: &Secrets) -> Option<Setting> {
        if secrets.redact(name) != name || !is_setting_name(name) {
            return None;
        }
        // A number is written as it is, with nothing taken out of it: one that is not
        // finite cannot be written at all, and one whose digits hold a secret would
        // be written with it. Neither makes a setting.
        let written = |digits: String| secrets.redact(&digits) == digits;
        let value = match value {
            Value::Number(number) => {
                let digits = serde_json::to_string(number).ok()?;
                if !number.is_finite() || !written(digits) {
                    return None;
                }
                ArgumentValue::Number(*number)
            }
            Value::Integer(number) => {
                if !written(number.to_string()) {
                    return None;
                }
                ArgumentValue::Integer(*number)
            }
            Value::Boolean(boolean) => ArgumentValue::Boolean(*boolean),
            Value::Text(text) => {
                ArgumentValue::Text(super::engine::cut_to(secrets, text, SETTING_TEXT_LIMIT, ""))
            }
        };
        Some(Setting {
            name: name.to_owned(),
            value,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &ArgumentValue {
        &self.value
    }
}

fn is_setting_name(name: &str) -> bool {
    let plain = |b: u8| b.is_ascii_graphic() && b != b'"' && b != b'\\';
    !name.is_empty() && name.len() <= SETTING_NAME_LIMIT && name.bytes().all(plain)
}

#[derive(Deserialize)]
#[serde(remote = "Setting", deny_unknown_fields)]
struct SettingWire {
    name: String,
    value: ArgumentValue,
}

fn settings_wire<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Setting>, D::Error> {
    #[derive(Deserialize)]
    struct One(#[serde(with = "SettingWire")] Setting);
    let read = Vec::<One>::deserialize(deserializer)?;
    Ok(read.into_iter().map(|One(setting)| setting).collect())
}

/// What was asked: the action, its exact arguments, and the settings it ran under.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asked {
    /// The action. None only in the record of a run whose admission could not be
    /// read, where `admission` is among what is lost.
    #[serde(deserialize_with = "present")]
    pub action: Option<String>,
    /// The action's version, as its descriptor gave it. None as the action is.
    #[serde(deserialize_with = "present")]
    pub version: Option<String>,
    /// Every argument that had a value, in the descriptor's order.
    pub arguments: Vec<Argument>,
    /// The resolved settings, in the order of their names.
    #[serde(deserialize_with = "settings_wire")]
    pub settings: Vec<Setting>,
    /// The digest of the config the settings came from. None only in the record of a
    /// run a dead process left, where `settings` is among what is lost.
    #[serde(deserialize_with = "present")]
    pub config: Option<Digest>,
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
    /// What was refused inside the run, and why: each call the run's action made, to
    /// run another action, that was refused, in the order they were refused, the
    /// first [`REFUSALS_LIMIT`] of them.
    #[serde(deserialize_with = "refusals_wire")]
    pub refusals: Vec<Refused>,
    /// How many more refusals there were than the list holds.
    pub refusals_omitted: u64,
    /// What the record does not know (ADR 0013): nothing, for a run that ended by
    /// itself; every group but the admission, for a run a dead process left; and the
    /// admission too, when it could not be read. A `run/2` record has no such field,
    /// and none is written for one.
    pub lost: Vec<Lost>,
}

/// One group of a record that a run's record may not know (ADR 0013).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lost {
    /// `asked.arguments`.
    Arguments,
    /// `asked.settings`, and `asked.config`, which is then null.
    Settings,
    /// `inputs`.
    Inputs,
    /// `outcome.refusals` and `outcome.refusals_omitted`.
    Refusals,
    /// `time.durations`.
    Durations,
    /// `models`.
    Models,
    /// `spend.requests`.
    Requests,
    /// `egress`.
    Egress,
    /// `steps`.
    Steps,
    /// `judge`.
    Judge,
    /// `diagnostics`: the journal among them.
    Diagnostics,
    /// The version of whizbang that ran the run: `identity.whizbang` is the version
    /// that ended it.
    Version,
    /// What the admission gives: `identity.request`, `identity.parent`,
    /// `asked.action`, `asked.version`, `time.start_ms` and `spend.reserved_micros`,
    /// each then null.
    Admission,
}

impl Lost {
    /// Every group, in the one order a record lists them: what a record has lost
    /// when its admission could not be read.
    pub const ALL: [Lost; 13] = [
        Lost::Arguments,
        Lost::Settings,
        Lost::Inputs,
        Lost::Refusals,
        Lost::Durations,
        Lost::Models,
        Lost::Requests,
        Lost::Egress,
        Lost::Steps,
        Lost::Judge,
        Lost::Diagnostics,
        Lost::Version,
        Lost::Admission,
    ];

    /// What the record of a run a dead process left has lost when its admission was
    /// read: every group but the admission.
    pub fn but_the_admission() -> &'static [Lost] {
        &Lost::ALL[..12]
    }
}

/// The kind of the error a run a dead process left is ended with.
pub const INTERRUPTED: &str = "interrupted";

/// Time: the start and the end, in milliseconds since the Unix epoch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Time {
    /// When the run started. None only in the record of a run whose admission could
    /// not be read.
    #[serde(deserialize_with = "present")]
    pub start_ms: Option<u64>,
    pub end_ms: u64,
    /// The duration of each capability request and of each call of another action.
    pub durations: Vec<Duration>,
}

/// Spend: what was reserved for the run, and its paid requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spend {
    /// What was reserved for the run when it was admitted, in millionths of a US
    /// dollar: the amount of its reservation and of its admission (ADR 0007). None
    /// when the executor that admitted it had no ledger: nothing was reserved. It is
    /// never the largest amount there is, which is no amount.
    #[serde(deserialize_with = "present")]
    pub reserved_micros: Option<u64>,
    /// Every paid request, with what was reserved for it, what was settled and what is
    /// unresolved, in the order they were sent.
    #[serde(deserialize_with = "paid_wire")]
    pub requests: Vec<Paid>,
}

/// The most paid requests a record lists: the most requests one run sends.
pub const PAID_LIMIT: usize = MODELS_LIMIT;

/// One paid request of the run (ADR 0013), all amounts in millionths of a US dollar:
/// its place in `models`; what was held for it, which is its bound; what it was
/// settled at; how much of that no answer settled; the stage it reached; and whether
/// what it was settled at is over what was held.
///
/// It is written as JSON, and read from JSON only as part of a stored record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Paid {
    request: u64,
    reserved_micros: u64,
    settled_micros: u64,
    unresolved_micros: u64,
    #[serde(with = "stage_name")]
    stage: Stage,
    over_bound: bool,
}

impl Paid {
    /// A request that was not sent: nothing was charged.
    pub fn not_sent(request: usize, reserved_micros: u64) -> Option<Paid> {
        Paid::of(request, reserved_micros, 0, Stage::NotSent)
    }

    /// A request that was answered, by a response or by a fault, and was settled at
    /// this charge: the charge the answer gave, or the bound when it gave none.
    pub fn answered(request: usize, reserved_micros: u64, settled_micros: u64) -> Option<Paid> {
        Paid::of(request, reserved_micros, settled_micros, Stage::Answered)
    }

    /// A request that no answer settled: it is settled at what was held for it, all
    /// of it unresolved.
    pub fn unresolved(request: usize, reserved_micros: u64) -> Option<Paid> {
        Paid::of(request, reserved_micros, reserved_micros, Stage::Unresolved)
    }

    fn of(request: usize, reserved: u64, settled: u64, stage: Stage) -> Option<Paid> {
        let paid = Paid {
            request: request as u64,
            reserved_micros: reserved,
            settled_micros: settled,
            unresolved_micros: if stage == Stage::Unresolved {
                settled
            } else {
                0
            },
            stage,
            over_bound: settled > reserved,
        };
        paid.holds().then_some(paid)
    }

    /// What the shape alone does not hold of one paid request.
    fn holds(&self) -> bool {
        let amounts = [
            self.reserved_micros,
            self.settled_micros,
            self.unresolved_micros,
        ];
        let by_stage = match self.stage {
            Stage::NotSent => self.settled_micros == 0 && self.unresolved_micros == 0,
            Stage::Answered => self.unresolved_micros == 0,
            Stage::Unresolved => {
                self.unresolved_micros == self.settled_micros
                    && self.settled_micros == self.reserved_micros
            }
        };
        self.request < PAID_LIMIT as u64
            && amounts.iter().all(|amount| *amount != u64::MAX)
            && by_stage
            && self.over_bound == (self.settled_micros > self.reserved_micros)
    }

    /// The request's place in `models`.
    pub fn request(&self) -> u64 {
        self.request
    }

    pub fn reserved_micros(&self) -> u64 {
        self.reserved_micros
    }

    pub fn settled_micros(&self) -> u64 {
        self.settled_micros
    }

    pub fn unresolved_micros(&self) -> u64 {
        self.unresolved_micros
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn is_over_bound(&self) -> bool {
        self.over_bound
    }
}

#[derive(Deserialize)]
#[serde(remote = "Paid", deny_unknown_fields)]
struct PaidWire {
    request: u64,
    reserved_micros: u64,
    settled_micros: u64,
    unresolved_micros: u64,
    #[serde(with = "stage_name")]
    stage: Stage,
    over_bound: bool,
}

fn paid_wire<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Paid>, D::Error> {
    #[derive(Deserialize)]
    struct One(#[serde(with = "PaidWire")] Paid);
    let read = Vec::<One>::deserialize(deserializer)?;
    Ok(read.into_iter().map(|One(paid)| paid).collect())
}

/// The most entries a record lists of where data went.
pub const EGRESS_LIMIT: usize = 128;

/// Whether data was handed to a recipient, or the send was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Handed {
    Sent,
    Refused,
}

/// One entry of where the run's data went (ADR 0013): the origin that was reached, or
/// that a send was refused for; the account used with it, cut as it is always shown;
/// the classes of data; and whether it was sent or refused. A record lists each
/// origin, account and outcome once, in the order they were first noted.
///
/// It is written as JSON, and read from JSON only as part of a stored record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Went {
    origin: String,
    account: Option<String>,
    classes: Vec<Class>,
    outcome: Handed,
}

/// A class of data, as a record spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Class {
    SourceImages,
    DerivedImages,
    Prompts,
    Evidence,
}

impl Class {
    fn of(class: DataClass) -> Class {
        match class {
            DataClass::SourceImages => Class::SourceImages,
            DataClass::DerivedImages => Class::DerivedImages,
            DataClass::Prompts => Class::Prompts,
            DataClass::Evidence => Class::Evidence,
        }
    }

    fn class(self) -> DataClass {
        match self {
            Class::SourceImages => DataClass::SourceImages,
            Class::DerivedImages => DataClass::DerivedImages,
            Class::Prompts => DataClass::Prompts,
            Class::Evidence => DataClass::Evidence,
        }
    }
}

impl Went {
    /// An entry for this origin, under the recipient's account. The account is
    /// written cut ([`Account::shown`]). An account that holds a secret the executor
    /// was told of is not written at all, not even cut: the entry then names none.
    pub fn new(
        origin: &Origin,
        account: Option<&Account>,
        classes: Classes,
        outcome: Handed,
        secrets: &Secrets,
    ) -> Went {
        Went {
            origin: origin.to_string(),
            account: account.and_then(|account| account.shown_without(secrets)),
            classes: classes.iter().map(Class::of).collect(),
            outcome,
        }
    }

    /// An entry for the recipient's own origin.
    pub fn to(recipient: &Recipient, classes: Classes, outcome: Handed, secrets: &Secrets) -> Went {
        Went::new(
            recipient.origin(),
            recipient.account(),
            classes,
            outcome,
            secrets,
        )
    }

    /// The origin, as an origin is written.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The account, cut.
    pub fn account(&self) -> Option<&str> {
        self.account.as_deref()
    }

    /// The classes of data, as a set.
    pub fn classes(&self) -> Classes {
        let classes: Vec<DataClass> = self.classes.iter().map(|class| class.class()).collect();
        Classes::of(&classes)
    }

    pub fn outcome(&self) -> Handed {
        self.outcome
    }

    /// What the shape alone does not hold: the origin is one, written the one way,
    /// and the account is one that was cut.
    fn holds(&self) -> bool {
        let none = Secrets::none();
        let origin = Origin::of(&self.origin, &none);
        let written = origin.is_ok_and(|origin| origin.to_string() == self.origin);
        // By its length, which is how `Account::shown` writes one: 1 to 4 characters
        // is a whole account, whatever its characters, dots among them; exactly 7
        // that end in three dots is a cut one, whose first four are an account's; and
        // nothing else is either.
        let cut = self.account.as_ref().is_none_or(|shown| {
            let account = match shown.len() {
                1..=4 => Some(shown.as_str()),
                7 => shown.strip_suffix("..."),
                _ => None,
            };
            account.is_some_and(|account| Account::new(account, &none).is_ok())
        });
        // The classes in their one order, each once.
        let ordered: Vec<Class> = self.classes().iter().map(Class::of).collect();
        written && cut && ordered == self.classes
    }
}

#[derive(Deserialize)]
#[serde(remote = "Went", deny_unknown_fields)]
struct WentWire {
    origin: String,
    #[serde(deserialize_with = "present")]
    account: Option<String>,
    classes: Vec<Class>,
    outcome: Handed,
}

fn went_wire<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Went>, D::Error> {
    #[derive(Deserialize)]
    struct One(#[serde(with = "WentWire")] Went);
    let read = Vec::<One>::deserialize(deserializer)?;
    Ok(read.into_iter().map(|One(went)| went).collect())
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
        // A name is a key in the store, written as it is: one that holds a secret is
        // refused, and the refusal does not show it.
        if secrets.redact(name) != name {
            return Err(
                "the name of a diagnostic holds something that must not be written: a \
                 name is stored as it is, so it cannot hold a secret"
                    .to_owned(),
            );
        }
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
    models: Vec<Model>,
    spend: Spend,
    #[serde(deserialize_with = "went_wire")]
    egress: Vec<Went>,
    #[serde(deserialize_with = "steps_wire")]
    steps: Vec<Step>,
    judge: Vec<Judged>,
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
    models: Vec<Model>,
    spend: Spend,
    egress: Vec<Went>,
    steps: Vec<Step>,
    judge: Vec<Judged>,
    diagnostics: Vec<Diagnostic>,
}

/// A `run/2` record as it is stored, as the tag `m1a` froze it: what `run/3` added is
/// not in it, and what `run/3` lets be null is not null in it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frozen {
    record: String,
    identity: Identity,
    asked: FrozenAsked,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    outcome: FrozenEnded,
    time: FrozenTime,
    models: Vec<Model>,
    spend: FrozenSpend,
    egress: Vec<Nothing>,
    #[serde(deserialize_with = "steps_wire")]
    steps: Vec<Step>,
    judge: Vec<Judged>,
    #[serde(deserialize_with = "diagnostics_wire")]
    diagnostics: Vec<Diagnostic>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenAsked {
    action: String,
    version: String,
    arguments: Vec<Argument>,
    settings: Vec<Nothing>,
    config: Digest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenEnded {
    status: Status,
    #[serde(deserialize_with = "present")]
    error: Option<Failed>,
    #[serde(deserialize_with = "present")]
    invalid: Option<String>,
    #[serde(deserialize_with = "refusals_wire")]
    refusals: Vec<Refused>,
    refusals_omitted: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenTime {
    start_ms: u64,
    end_ms: u64,
    durations: Vec<Duration>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenSpend {
    #[serde(deserialize_with = "present")]
    reserved_micros: Option<u64>,
    requests: Vec<Nothing>,
}

impl Frozen {
    fn record(self) -> RunRecord {
        let (_, _, _): (Vec<Nothing>, Vec<Nothing>, Vec<Nothing>) =
            (self.asked.settings, self.spend.requests, self.egress);
        RunRecord {
            record: self.record,
            identity: self.identity,
            asked: Asked {
                action: Some(self.asked.action),
                version: Some(self.asked.version),
                arguments: self.asked.arguments,
                settings: Vec::new(),
                config: Some(self.asked.config),
            },
            inputs: self.inputs,
            outputs: self.outputs,
            outcome: Ended {
                status: self.outcome.status,
                error: self.outcome.error,
                invalid: self.outcome.invalid,
                refusals: self.outcome.refusals,
                refusals_omitted: self.outcome.refusals_omitted,
                lost: Vec::new(),
            },
            time: Time {
                start_ms: Some(self.time.start_ms),
                end_ms: self.time.end_ms,
                durations: self.time.durations,
            },
            models: self.models,
            spend: Spend {
                reserved_micros: self.spend.reserved_micros,
                requests: Vec::new(),
            },
            egress: Vec::new(),
            steps: self.steps,
            judge: self.judge,
            diagnostics: self.diagnostics,
        }
    }
}

/// A `run/2` record as it is written: the record, without the one field `run/2` has
/// not. Its other fields are written as `run/3` writes them, which for a record that
/// was read as `run/2` is as the tag wrote them.
#[derive(Serialize)]
struct FrozenOut<'a> {
    record: &'a str,
    identity: &'a Identity,
    asked: &'a Asked,
    inputs: &'a [Input],
    outputs: &'a [Output],
    outcome: FrozenEndedOut<'a>,
    time: &'a Time,
    models: &'a [Model],
    spend: &'a Spend,
    egress: &'a [Went],
    steps: &'a [Step],
    judge: &'a [Judged],
    diagnostics: &'a [Diagnostic],
}

#[derive(Serialize)]
struct FrozenEndedOut<'a> {
    status: Status,
    error: &'a Option<Failed>,
    invalid: &'a Option<String>,
    refusals: &'a [Refused],
    refusals_omitted: u64,
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
    /// The caller's id for the request that made the run, when it gave one.
    pub request: Option<&'a RequestId>,
    /// The version of whizbang.
    pub whizbang: &'a str,
    /// The call as it was prepared.
    pub prepared: &'a PreparedCall,
    /// What was reserved for the run when it was admitted, in millionths of a US
    /// dollar, when anything was.
    pub reserved_micros: Option<u64>,
    /// How the run ended.
    pub outcome: &'a Outcome,
    pub start_ms: u64,
    pub end_ms: u64,
    /// The run's diagnostics, as they are stored.
    pub diagnostics: Vec<Diagnostic>,
    /// What the run did inside itself: the run that called it, the calls it made, and
    /// those that were refused.
    pub inside: Inside,
    /// What was noted of the run from outside the executor: its settings, what its
    /// paid requests cost, and where its data went.
    pub facts: Facts,
}

/// What `run/3` records beside what the executor itself knows (ADR 0013): the
/// resolved settings, the paid requests and where data went. Each list is bounded
/// here, where it comes in: one more than a record lists is not taken, and the caller
/// is told.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    settings: Vec<Setting>,
    paid: Vec<Paid>,
    went: Vec<Went>,
}

impl Facts {
    /// No facts.
    pub fn none() -> Facts {
        Facts::default()
    }

    /// Notes a setting. False, and nothing is noted, when the record already lists
    /// [`SETTINGS_LIMIT`] of them or one of this name.
    pub fn setting(&mut self, setting: Setting) -> bool {
        let room = self.settings.len() < SETTINGS_LIMIT;
        let new = self.settings.iter().all(|other| other.name != setting.name);
        if room && new {
            self.settings.push(setting);
        }
        room && new
    }

    /// Notes a paid request. False, and nothing is noted, when the record already
    /// lists [`PAID_LIMIT`] of them, or one for this request or a later one: they are
    /// noted in the order the requests were sent.
    pub fn paid(&mut self, paid: Paid) -> bool {
        let room = self.paid.len() < PAID_LIMIT;
        let next = self
            .paid
            .last()
            .is_none_or(|last| last.request < paid.request);
        if room && next {
            self.paid.push(paid);
        }
        room && next
    }

    /// Notes where data went. An origin, account and outcome that is already listed
    /// gains the classes of this one, and is not listed twice. False, and nothing is
    /// noted, when it is new and the record already lists [`EGRESS_LIMIT`].
    pub fn went(&mut self, went: Went) -> bool {
        let same = |other: &&mut Went| {
            (&other.origin, &other.account, other.outcome)
                == (&went.origin, &went.account, went.outcome)
        };
        if let Some(listed) = self.went.iter_mut().find(same) {
            let both = went.classes().iter().fold(listed.classes(), Classes::with);
            listed.classes = both.iter().map(Class::of).collect();
            return true;
        }
        let room = self.went.len() < EGRESS_LIMIT;
        if room {
            self.went.push(went);
        }
        room
    }
}

/// What the record of a run that a dead process left is made from (ADR 0013): the
/// run's name, what its admission said when it could be read, and when it was ended.
pub struct Left<'a> {
    /// The run's id.
    pub run: &'a RunId,
    /// The version of whizbang that ends it.
    pub whizbang: &'a str,
    /// What the run's admission said, when it could be read.
    pub admitted: Option<Admitted<'a>>,
    /// When the run is ended, in milliseconds since the Unix epoch.
    pub end_ms: u64,
}

/// What a record takes from a run's admission.
pub struct Admitted<'a> {
    pub request: Option<&'a RequestId>,
    pub parent: Option<&'a RunId>,
    pub action: &'a str,
    pub version: &'a str,
    pub reserved_micros: Option<u64>,
    pub start_ms: u64,
}

/// What a record holds of nested calls (ADR 0008): the run that called this one, the
/// calls this one made that were admitted, in order, and those that were refused.
/// Only the executor fills it; anything else makes the empty one.
#[derive(Debug, Clone, Default)]
pub struct Inside {
    pub(super) parent: Option<RunId>,
    pub(super) steps: Vec<Step>,
    pub(super) refusals: Vec<Refused>,
    pub(super) refusals_omitted: u64,
    /// Every request the run's capability handles sent, in order.
    pub(super) asked: Vec<Logged>,
    /// When each step began and was over, for those whose run ended: the step's
    /// place, the start and the end.
    pub(super) timed: Vec<(usize, u64, u64)>,
    /// What the action concluded, when it handed back a verdict and completed.
    pub(super) verdict: Option<Verdict>,
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
        // What the run asked of its capabilities: each request with its model and how
        // long it took, and for a judge what the exchange came to. A text that came
        // from a backend (a model's names, a provider's id) has the secrets taken out.
        let asked = &ran.inside.asked;
        // Redacted while whole, and only then cut to what the reader takes: a name
        // that redaction lengthened is still within the bound.
        let named = |name: &str| super::engine::cut_to(secrets, name, MODEL_NAME_LIMIT, "");
        let models = asked.iter().map(|asked| Model {
            capability: asked.capability,
            backend: named(asked.model.backend()),
            model: named(asked.model.model()),
            version: named(asked.model.version()),
            device: None,
            cached: None,
            ended: RequestEnded::of(asked.ended),
        });
        let lasted = |of, index: usize, start: u64, end: u64| Duration {
            of,
            index: index as u64,
            start_ms: start,
            ms: end.saturating_sub(start),
        };
        let requests = asked.iter().enumerate();
        let requests = requests.map(|(at, a)| lasted(Timed::Request, at, a.start_ms, a.end_ms));
        let calls = ran.inside.timed.iter();
        let calls = calls.map(|(at, start, end)| lasted(Timed::Call, *at, *start, *end));
        let durations = requests.chain(calls).collect();
        // A request to a judge whose backend's call unwound kept nothing of its
        // exchange. It has its turn all the same, with nothing in it: the record lists
        // one turn for each request to a judge.
        let unanswered = crate::capability::Judged::unanswered();
        let turns = asked.iter().enumerate().filter_map(|(at, asked)| {
            if asked.capability != Capability::Judge {
                return None;
            }
            let judged = asked.judged.as_ref().unwrap_or(&unanswered);
            Some(Judged::Turn {
                request: at as u64,
                exchange: judged.exchange.clone(),
                max_output_tokens: judged.max_output_tokens,
                messages: judged.messages as u64,
                stop: judged.stop.map(|stop| match stop {
                    "end_turn" => Stop::EndTurn,
                    "tool_use" => Stop::ToolUse,
                    "max_tokens" => Stop::MaxTokens,
                    "refusal" => Stop::Refusal,
                    _ => Stop::Other,
                }),
                tool_calls: judged.tool_calls as u64,
                usage: judged.usage.map(|usage| Tokens {
                    input_tokens: usage.input_tokens(),
                    output_tokens: usage.output_tokens(),
                    cache_read_tokens: usage.cache_read_tokens(),
                    cache_write_tokens: usage.cache_write_tokens(),
                }),
                // Redacted, cleaned and cut by the handle that took the turn (ADR
                // 0009); and here once more, cut after the redaction.
                provider_id: judged
                    .provider_id
                    .as_deref()
                    .map(|id| super::engine::cut_to(secrets, id, PROVIDER_ID_LIMIT, "")),
                excluded: judged.excluded,
            })
        });
        // A verdict is of a run that completed, and of no other.
        let verdict = ran.inside.verdict.as_ref();
        let verdict = verdict.filter(|_| status == Status::Completed);
        let verdict = verdict.map(|verdict| Judged::Verdict {
            decision: verdict.decision(),
            why: super::engine::said(secrets, verdict.why()),
        });
        let judge = turns.chain(verdict).collect();
        let models: Vec<Model> = models.collect();
        // What was noted from outside the executor: the settings in the order of their
        // names, and no paid request that is not one the record lists.
        let Facts {
            mut settings,
            mut paid,
            went,
        } = ran.facts;
        settings.sort_by(|a, b| a.name.cmp(&b.name));
        paid.retain(|paid| paid.request < models.len() as u64);
        RunRecord {
            record: RUN_RECORD.to_owned(),
            identity: Identity {
                run: ran.run.to_owned(),
                request: ran.request.map(|id| id.as_str().to_owned()),
                parent: ran.inside.parent.map(|run| run.as_str().to_owned()),
                whizbang: ran.whizbang.to_owned(),
            },
            asked: Asked {
                action: Some(prepared.action().to_owned()),
                version: Some(prepared.version().to_owned()),
                arguments,
                settings,
                config: Some(prepared.config()),
            },
            inputs,
            outputs,
            outcome: Ended {
                status,
                error,
                invalid,
                refusals: ran.inside.refusals,
                refusals_omitted: ran.inside.refusals_omitted,
                lost: Vec::new(),
            },
            time: Time {
                start_ms: Some(ran.start_ms),
                // A clock that went back does not make a run end before it started.
                end_ms: ran.end_ms.max(ran.start_ms),
                durations,
            },
            models,
            spend: Spend {
                reserved_micros: ran.reserved_micros,
                requests: paid,
            },
            egress: went,
            steps: ran.inside.steps,
            judge,
            diagnostics,
        }
    }

    /// The record of a run that a dead process left (ADR 0013): an error of the kind
    /// `interrupted`, made from the run's admission alone, or from its name alone when
    /// the admission could not be read, with everything else named as lost. The end
    /// is never before the start.
    ///
    /// None when what it was given makes a record that the reader refuses: a version,
    /// an action or an action's version that is empty, a parent that is the run
    /// itself, or the largest amount as what was reserved, which is no amount.
    pub fn interrupted(left: Left<'_>) -> Option<RunRecord> {
        let admitted = left.admitted.as_ref();
        let lost = match admitted {
            Some(_) => Lost::but_the_admission().to_vec(),
            None => Lost::ALL.to_vec(),
        };
        let start_ms = admitted.map(|admitted| admitted.start_ms);
        let record = RunRecord {
            record: RUN_RECORD.to_owned(),
            identity: Identity {
                run: left.run.as_str().to_owned(),
                request: admitted.and_then(|a| a.request.map(|id| id.as_str().to_owned())),
                parent: admitted.and_then(|a| a.parent.map(|run| run.as_str().to_owned())),
                whizbang: left.whizbang.to_owned(),
            },
            asked: Asked {
                action: admitted.map(|admitted| admitted.action.to_owned()),
                version: admitted.map(|admitted| admitted.version.to_owned()),
                arguments: Vec::new(),
                settings: Vec::new(),
                config: None,
            },
            inputs: Vec::new(),
            outputs: Vec::new(),
            outcome: Ended {
                status: Status::Error,
                error: Some(Failed {
                    kind: INTERRUPTED.to_owned(),
                    message: "the process that ran it ended before the run did".to_owned(),
                }),
                invalid: None,
                refusals: Vec::new(),
                refusals_omitted: 0,
                lost,
            },
            time: Time {
                start_ms,
                end_ms: left.end_ms.max(start_ms.unwrap_or(0)),
                durations: Vec::new(),
            },
            models: Vec::new(),
            spend: Spend {
                reserved_micros: admitted.and_then(|admitted| admitted.reserved_micros),
                requests: Vec::new(),
            },
            egress: Vec::new(),
            steps: Vec::new(),
            judge: Vec::new(),
            diagnostics: Vec::new(),
        };
        // What the admission said was read from a store. A record its own reader
        // would refuse is not made of it.
        record.check().is_ok().then_some(record)
    }

    /// The record as it is stored: JSON on one line, then a newline.
    pub fn to_bytes(&self) -> Vec<u8> {
        // A record holds strings, whole numbers, finite numbers and lists of these;
        // writing one as JSON cannot fail. A record that was read as `run/2` is
        // written as `run/2` is, without the field it has not.
        let written = if self.record == RUN_2 {
            serde_json::to_vec(&FrozenOut {
                record: &self.record,
                identity: &self.identity,
                asked: &self.asked,
                inputs: &self.inputs,
                outputs: &self.outputs,
                outcome: FrozenEndedOut {
                    status: self.outcome.status,
                    error: &self.outcome.error,
                    invalid: &self.outcome.invalid,
                    refusals: &self.outcome.refusals,
                    refusals_omitted: self.outcome.refusals_omitted,
                },
                time: &self.time,
                models: &self.models,
                spend: &self.spend,
                egress: &self.egress,
                steps: &self.steps,
                judge: &self.judge,
                diagnostics: &self.diagnostics,
            })
        } else {
            serde_json::to_vec(self)
        };
        let mut bytes = written.unwrap_or_default();
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
        let frozen = match serde_json::from_slice::<Version>(bytes) {
            Ok(Version { record }) if record != RUN_RECORD && record != RUN_2 => {
                // What it says its version is can be anything: its first 64 characters
                // are kept.
                return Err(RecordError::OtherVersion {
                    found: record.chars().take(QUOTED_LIMIT).collect(),
                });
            }
            Ok(Version { record }) => record == RUN_2,
            Err(_) => false,
        };
        let malformed = |error: serde_json::Error| RecordError::Malformed(error.to_string());
        let mut reader = serde_json::Deserializer::from_slice(bytes);
        let record = if frozen {
            Frozen::deserialize(&mut reader).map(Frozen::record)
        } else {
            RunRecordWire::deserialize(&mut reader)
        };
        let record = record.map_err(malformed)?;
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
        let nothing = String::new();
        let named = [
            ("the run's id", &self.identity.run),
            ("the version of whizbang", &self.identity.whizbang),
            (
                "the action's name",
                self.asked.action.as_ref().unwrap_or(&nothing),
            ),
            (
                "the action's version",
                self.asked.version.as_ref().unwrap_or(&nothing),
            ),
        ];
        // What is lost, and what that says of the rest (ADR 0013).
        self.check_lost()?;
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
            let unknown = std::ptr::eq(text, &nothing);
            if text.is_empty() && !unknown {
                return refuse(&format!("{what} is empty"));
            }
        }
        // The request's id is written as it is, so it is held to the rule of one.
        if let Some(request) = &self.identity.request
            && RequestId::parse(request).is_err()
        {
            return refuse(
                "the request's id is not one: 1 to 128 of the ASCII letters, the digits, \
                 _, -, . and :",
            );
        }
        // The parent is written as it is, and is a run of its own.
        if let Some(parent) = &self.identity.parent
            && (RunId::parse(parent).is_err() || *parent == self.identity.run)
        {
            return refuse(
                "the parent is not another run's id: a lower-case letter or a digit, then \
                 lower-case letters, digits, _ or -, 64 characters at most",
            );
        }
        // A step names a run that this one called: a run id that is neither this run's
        // nor its parent's, once, with an action's name.
        if self.steps.len() > STEPS_LIMIT {
            return refuse("more steps than a run may have");
        }
        let mut called = std::collections::BTreeSet::new();
        for step in &self.steps {
            let own =
                step.run == self.identity.run || Some(&step.run) == self.identity.parent.as_ref();
            if RunId::parse(&step.run).is_err() || own || !called.insert(&step.run) {
                return refuse(
                    "a step does not name a run of its own: a run id, neither this run's \
                     nor its parent's, named once",
                );
            }
            if text::action_name(&step.action).is_err() {
                return refuse("a step's action is not the name of one");
            }
        }
        let refusals = &self.outcome.refusals;
        if refusals.len() > REFUSALS_LIMIT
            || (self.outcome.refusals_omitted > 0 && refusals.len() < REFUSALS_LIMIT)
        {
            return refuse(
                "more refusals than a record lists, or refusals left out of a list that \
                 is not full",
            );
        }
        for refused in refusals {
            let of_a_request = refused.kind.is_of_a_request();
            if of_a_request && (refused.action.is_some() || self.record == RUN_2) {
                return refuse(
                    "a refusal of a capability request names an action, or is in a record \
                     of a version that has none",
                );
            }
            let named = refused.action.as_ref();
            if named.is_some_and(|action| text::action_name(action).is_err()) {
                return refuse("a refusal's action is not the name of one");
            }
            if refused.why.is_empty() || refused.why.chars().count() > REFUSAL_WHY_LIMIT {
                return refuse("a refusal's sentence is empty, or longer than one is kept");
            }
        }
        // The requests: no more than a run sends, each with a duration, and the
        // durations in their one order, each of something the record lists.
        if self.models.len() > MODELS_LIMIT {
            return refuse("more capability requests than a run may send");
        }
        let names = self.models.iter();
        let mut names = names.flat_map(|model| [&model.backend, &model.model, &model.version]);
        if names.any(|name| name.len() > MODEL_NAME_LIMIT || !name.is_ascii()) {
            return refuse("a model's name is longer than one is kept, or is not ASCII");
        }
        let durations = &self.time.durations;
        let of_requests = durations.iter().filter(|d| d.of == Timed::Request);
        if !of_requests.map(|d| d.index).eq(0..self.models.len() as u64)
            || !durations.is_sorted_by(|a, b| (a.of, a.index) < (b.of, b.index))
            || durations
                .iter()
                .any(|d| d.of == Timed::Call && d.index >= self.steps.len() as u64)
        {
            return refuse(
                "the durations are not one for each capability request, in order, and \
                 then at most one for each step, in order",
            );
        }
        // The judge group: one turn for each request that was sent to a judge, in
        // order, each naming a diagnostic the record lists or none; then at most one
        // verdict, of a run that completed.
        let to_judges = self.models.iter().enumerate();
        let to_judges = to_judges.filter(|(_, model)| model.capability == Capability::Judge);
        let mut to_judges = to_judges.map(|(at, _)| at as u64);
        let mut verdicts = 0;
        for (at, entry) in self.judge.iter().enumerate() {
            match entry {
                Judged::Turn {
                    request,
                    exchange,
                    provider_id,
                    ..
                } => {
                    if verdicts > 0 || to_judges.next() != Some(*request) {
                        return refuse(
                            "the judge's turns are not one for each request to a judge, in \
                             order, before the verdict",
                        );
                    }
                    let kept = |name: &String| self.diagnostics.iter().any(|d| d.name == *name);
                    if exchange.as_ref().is_some_and(|name| !kept(name)) {
                        return refuse("a judge's turn names an exchange the run does not hold");
                    }
                    if provider_id
                        .as_ref()
                        .is_some_and(|id| id.chars().count() > PROVIDER_ID_LIMIT)
                    {
                        return refuse("a provider's id is longer than one is kept");
                    }
                }
                Judged::Verdict { why, .. } => {
                    verdicts += 1;
                    let last = at + 1 == self.judge.len();
                    if !last || self.outcome.status != Status::Completed {
                        return refuse(
                            "a verdict is the last entry of the judge group, of a run that \
                             completed",
                        );
                    }
                    if why.chars().count() > super::MESSAGE_LIMIT + 3 {
                        return refuse("a verdict's reason is longer than one is kept");
                    }
                }
            }
        }
        if to_judges.next().is_some() {
            return refuse("a request to a judge has no turn in the judge group");
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
        if self
            .time
            .start_ms
            .is_some_and(|start| self.time.end_ms < start)
        {
            return refuse("the run ends before it starts");
        }
        // The count of each list is looked at before anything is compared within it:
        // a list far over its bound is refused for its count, at once.
        let (settings, paid, went) = (&self.asked.settings, &self.spend.requests, &self.egress);
        if settings.len() > SETTINGS_LIMIT || paid.len() > PAID_LIMIT || went.len() > EGRESS_LIMIT {
            return refuse(
                "more than a record lists: 96 settings, 1024 paid requests, 128 entries \
                 of where data went",
            );
        }
        // The settings: in the order of their names, each once, each a name, with a
        // text no longer than one is kept.
        let long = |setting: &Setting| match &setting.value {
            ArgumentValue::Text(text) => text.chars().count() > SETTING_TEXT_LIMIT,
            _ => false,
        };
        if !settings.is_sorted_by(|a, b| a.name < b.name)
            || !settings
                .iter()
                .all(|setting| is_setting_name(&setting.name))
            || settings.iter().any(long)
        {
            return refuse(
                "the settings are not in the order of their names, each once, each \
                 under a name of 1 to 128 printable ASCII characters, with a text of 256 \
                 characters at most",
            );
        }
        // The paid requests: each one the record lists, in order, each once.
        if !paid.is_sorted_by(|a, b| a.request < b.request)
            || paid
                .iter()
                .any(|paid| paid.request >= self.models.len() as u64 || !paid.holds())
        {
            return refuse(
                "a paid request is not one the record lists, named once and in order, \
                 with amounts that agree with the stage it reached",
            );
        }
        // Where data went: each origin, account and outcome once.
        let twice = went.iter().enumerate().any(|(at, one)| {
            let same = |other: &Went| {
                (&other.origin, &other.account, other.outcome)
                    == (&one.origin, &one.account, one.outcome)
            };
            went[..at].iter().any(same)
        });
        if twice || !went.iter().all(Went::holds) {
            return refuse(
                "an entry of where data went is not an origin written the one way, a \
                 cut account and classes in their order, each named once",
            );
        }
        if self.spend.reserved_micros == Some(u64::MAX) {
            return refuse(super::request::NOT_AN_AMOUNT);
        }
        if !self.diagnostics.is_sorted_by(|a, b| a.name < b.name) {
            return refuse("the diagnostics are not in the order of their names, each once");
        }
        Ok(())
    }

    /// What is lost, and what that says of the other fields (ADR 0013, "What is
    /// lost").
    fn check_lost(&self) -> Result<(), RecordError> {
        let refuse = |why: &str| Err(RecordError::Inconsistent(why.to_owned()));
        let (asked, time, outcome) = (&self.asked, &self.time, &self.outcome);
        let lost = &outcome.lost;
        let admission = lost.as_slice() == Lost::ALL;
        if !lost.is_empty() && !admission && lost.as_slice() != Lost::but_the_admission() {
            return refuse(
                "what is lost is not nothing, every group but the admission, or every \
                 group, in their one order",
            );
        }
        if lost.is_empty() {
            let known = asked.action.is_some()
                && asked.version.is_some()
                && asked.config.is_some()
                && time.start_ms.is_some();
            return if known {
                Ok(())
            } else {
                refuse("a field is null that only a lost group leaves null, and nothing is lost")
            };
        }
        let interrupted = outcome.status == Status::Error
            && outcome
                .error
                .as_ref()
                .is_some_and(|e| e.kind == INTERRUPTED);
        if !interrupted {
            return refuse("only an error of the kind interrupted has lost anything");
        }
        let empty = asked.arguments.is_empty()
            && asked.settings.is_empty()
            && asked.config.is_none()
            && self.inputs.is_empty()
            && outcome.refusals.is_empty()
            && outcome.refusals_omitted == 0
            && time.durations.is_empty()
            && self.models.is_empty()
            && self.spend.requests.is_empty()
            && self.egress.is_empty()
            && self.steps.is_empty()
            && self.judge.is_empty()
            && self.diagnostics.is_empty();
        if !empty {
            return refuse("a field holds something though its group is lost");
        }
        let from_the_admission = [
            asked.action.is_some(),
            asked.version.is_some(),
            time.start_ms.is_some(),
        ];
        let rest_absent = self.identity.request.is_none()
            && self.identity.parent.is_none()
            && self.spend.reserved_micros.is_none();
        let agrees = if admission {
            from_the_admission == [false; 3] && rest_absent
        } else {
            from_the_admission == [true; 3]
        };
        if !agrees {
            return refuse(
                "what the admission gives is there though the admission is lost, or is \
                 not there though it is not",
            );
        }
        Ok(())
    }

    /// The record's kind and version: `run/3`, or `run/2` for a record that the tag
    /// `m1a` wrote.
    pub fn version(&self) -> &str {
        &self.record
    }

    /// Where the run's data went, in the order it was first noted.
    pub fn egress(&self) -> &[Went] {
        &self.egress
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

    /// The calls the run made of other actions that were admitted, in order.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Every capability request the run sent, in order.
    pub fn models(&self) -> &[Model] {
        &self.models
    }

    /// The judge's turns, in order, and then the verdict when there is one.
    pub fn judge(&self) -> &[Judged] {
        &self.judge
    }

    /// The verdict, when the run concluded one.
    pub fn verdict(&self) -> Option<(Decision, &str)> {
        match self.judge.last() {
            Some(Judged::Verdict { decision, why }) => Some((*decision, why)),
            _ => None,
        }
    }

    /// What was reserved for the run, and its paid requests.
    pub fn spend(&self) -> &Spend {
        &self.spend
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
                // What a record says its run is can be anything: 64 characters are kept.
                Err(StoredRecordError::OtherRun {
                    names: record.identity.run.chars().take(QUOTED_LIMIT).collect(),
                })
            }
        });
    Some(record)
}

/// The kind of reference a run's record and its diagnostics are stored as: they are
/// kept and readable, and never usable as an output.
pub const RECORD_KIND: Kind = Kind::Diagnostic;

/// A stage as a record spells it: its name.
mod stage_name {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::capability::Stage;

    pub fn serialize<S: Serializer>(stage: &Stage, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(stage.name())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Stage, D::Error> {
        let name = String::deserialize(d)?;
        Stage::ALL
            .into_iter()
            .find(|stage| stage.name() == name)
            .ok_or_else(|| serde::de::Error::custom("not the name of a stage"))
    }
}

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
