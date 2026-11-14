//! Request ids and admission (`SPEC.md` section 3, "Request identity"; ADR 0007).
//!
//! A caller may give a request an id. The id is bound to a fingerprint of the request as
//! it was submitted, and admission records the id, the fingerprint and the run in the
//! one publication that makes the run, with the reservation ([`super::ledger`]). The record is published under a name that
//! belongs to no run, so it is still there when the run is deleted and pruned, and it
//! is what answers the same request sent again.
//!
//! Two records, each written one way and read strictly:
//!
//! - the [`Admission`], kind `admission/1`: which run was made, for which request, and
//!   when. It is stored twice under one content: among the run's names, as `admitted`,
//!   and, when the caller gave an id, outside runs, as `requests/<id>/admitted`;
//! - the [`Ending`], kind `ending/1`: how a run that had a request id ended, as a word.
//!   It is stored outside runs, as `requests/<id>/ended`, by the publication that
//!   writes the run's record.

use serde::{Deserialize, Serialize};

use crate::artifact::{ArtifactId, Contents};
use crate::capability::Shown;
use crate::identity::{Digest, Encoder, Fields};
use crate::store::{Entry, Generation};

use super::record::{self, QUOTED_LIMIT, RECORD_NAME, RunRecord, Status, StoredRecordError};
use std::time::Instant;

use super::{
    Call, CallError, Cancellation, Committed, JournalLine, Outcome, PreparedCall, RunId,
    StoreFailure, Value, text,
};

/// The longest request id, in characters.
pub const REQUEST_ID_LIMIT: usize = 128;

/// The most bytes a stored admission or ending may be. The longest one this code writes
/// is well under it: every text in one is held to a rule with a length.
pub const REQUEST_RECORD_MAX_BYTES: u64 = 1024;

/// The kind of an admission record, with its version.
pub const ADMISSION_RECORD: &str = "admission/1";

/// The kind of an ending record, with its version.
pub const ENDING_RECORD: &str = "ending/1";

/// The name, among a run's names in the store, of its admission.
pub const ADMITTED_NAME: &str = "admitted";

/// What the name of every record of a request id starts with, outside runs.
pub const REQUESTS_PREFIX: &str = "requests/";

/// A caller's id for a request: one to 128 of the ASCII letters, the digits, `_`, `-`,
/// `.` and `:`.
///
/// It is text a caller chose, and it becomes part of a name in the store and a field of
/// two records, written as it is. So it is held to an alphabet in which nothing needs
/// escaping, in a name, on a command line or in JSON, and to a length. A UUID, a ULID
/// and a hash in hex are all ids. Letter case counts: `A` and `a` are two ids.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(String);

impl RequestId {
    /// The id this text is, or why it is none. The refusal shows the text escaped, its
    /// first 64 characters at most, and its length when it was longer.
    pub fn parse(id: &str) -> Result<RequestId, String> {
        let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':');
        if id.is_empty() || id.len() > REQUEST_ID_LIMIT || !id.chars().all(allowed) {
            return Err(format!(
                "{} is no request id: a request id is 1 to {REQUEST_ID_LIMIT} of the ASCII \
                 letters, the digits, _, -, . and :",
                Shown(id)
            ));
        }
        Ok(RequestId(id.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The name, outside runs, of the admission recorded for this id.
    pub fn admission_name(&self) -> String {
        format!("{REQUESTS_PREFIX}{}/admitted", self.0)
    }

    /// The name, outside runs, of the ending recorded for this id.
    pub fn ending_name(&self) -> String {
        format!("{REQUESTS_PREFIX}{}/ended", self.0)
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What is submitted to an executor to run: a call, the id the caller gives the
/// request, if it gives one, and the preparation the caller was shown, if it hands one
/// over.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    call: Call,
    id: Option<RequestId>,
    expecting: Option<PreparedCall>,
    cancellation: Option<Cancellation>,
    deadline: Option<Instant>,
}

impl Request {
    /// A request to run this call, with no id.
    pub fn new(call: Call) -> Request {
        Request {
            call,
            id: None,
            expecting: None,
            cancellation: None,
            deadline: None,
        }
    }

    /// The same request, cancelled by this signal, which its caller keeps (ADR 0008,
    /// "Cancellation"). Set before the request is admitted, it ends the admission with
    /// nothing published: that is how an admission, which has no run yet, is
    /// cancelled. From the admission on it is the run's own signal. It is no part of
    /// the request's fingerprint.
    pub fn cancelled_by(mut self, signal: Cancellation) -> Request {
        self.cancellation = Some(signal);
        self
    }

    /// The signal the request is cancelled by, when it came with one.
    pub fn cancellation(&self) -> Option<&Cancellation> {
        self.cancellation.as_ref()
    }

    /// The same request, to be over by this time: an admission that has not been
    /// published by then is given up, and the run is handed the deadline, with its
    /// capability handles. It is no part of the request's fingerprint.
    pub fn until(mut self, deadline: Instant) -> Request {
        self.deadline = Some(deadline);
        self
    }

    /// When the request must be over, when it has a limit.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// The same request, to be admitted only if its call still prepares as `prepared`:
    /// what the caller was shown and decided on. Admission prepares the call again and
    /// refuses the request as stale when the two differ ([`super::Stale`]).
    pub fn expecting(mut self, prepared: &PreparedCall) -> Request {
        self.expecting = Some(prepared.clone());
        self
    }

    /// The preparation the request must still match, when it came with one.
    pub fn expected(&self) -> Option<&PreparedCall> {
        self.expecting.as_ref()
    }

    /// The same request under this id.
    pub fn with_id(mut self, id: RequestId) -> Request {
        self.id = Some(id);
        self
    }

    pub fn call(&self) -> &Call {
        &self.call
    }

    pub fn id(&self) -> Option<&RequestId> {
        self.id.as_ref()
    }

    /// The fingerprint of the request: see [`request_fingerprint`].
    pub fn fingerprint(&self) -> Digest {
        request_fingerprint(&self.call)
    }
}

/// The fingerprint of a request as it was submitted, kind `request/1`, in the encoding
/// ADR 0001 gives every identity: the action's name; the count of the arguments the call
/// gives, and each, in the order of their names as bytes, as its name and its value; the
/// count of the ports the call gives artifacts at, and each, in the order of their names
/// as bytes, as its name, the count of its artifacts, and each artifact's identity in
/// the order the call gave them. A value is written as the fingerprint of a prepared
/// call writes it (ADR 0004), negative zero as zero.
///
/// It is of the call alone: no descriptor, no store, no config and no model is looked
/// at, so it is had before anything is prepared, and it is the same whatever changed
/// since the request was first sent. It follows that a default given by hand and the
/// same default left out are two requests, as are a backend named and the same backend
/// left to the default.
pub fn request_fingerprint(call: &Call) -> Digest {
    let mut fields = Encoder::new("request/1");
    fields.bytes(call.action().as_bytes());
    fields.unsigned(call.arguments().len() as u64);
    for (name, value) in call.arguments() {
        fields.bytes(name.as_bytes());
        match value {
            Value::Number(number) => {
                fields.unsigned(0);
                fields.float(if *number == 0.0 { 0.0 } else { *number });
            }
            Value::Integer(number) => {
                fields.unsigned(1);
                fields.unsigned(u64::from_be_bytes(number.to_be_bytes()));
            }
            Value::Boolean(boolean) => {
                fields.unsigned(2);
                fields.unsigned(u64::from(*boolean));
            }
            Value::Text(text) => {
                fields.unsigned(3);
                fields.bytes(text.as_bytes());
            }
        }
    }
    fields.unsigned(call.inputs().len() as u64);
    for (port, artifacts) in call.inputs() {
        fields.bytes(port.as_bytes());
        fields.unsigned(artifacts.len() as u64);
        for artifact in artifacts {
            fields.digest(&artifact.0);
        }
    }
    fields.finish()
}

/// Why bytes are not a record of a request that this build reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestRecordError {
    /// More bytes than such a record may be ([`REQUEST_RECORD_MAX_BYTES`]). They were
    /// not read.
    TooLarge { bytes: u64 },
    /// A record of another kind or version. A store written by other code is not
    /// migrated.
    OtherVersion { found: String },
    /// Not a record: not JSON, or not of the record's shape.
    Malformed(String),
    /// A record of the right shape that says what none can.
    Inconsistent(String),
    /// A record that is not written the one way a record is written.
    NotCanonical,
}

impl std::fmt::Display for RequestRecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestRecordError::TooLarge { bytes } => write!(
                f,
                "{bytes} bytes, and such a record is at most {REQUEST_RECORD_MAX_BYTES}"
            ),
            RequestRecordError::OtherVersion { found } => write!(
                f,
                "a record of another kind or version: {}",
                found.escape_debug()
            ),
            // The reason can quote what was read: it is shown escaped and cut.
            RequestRecordError::Malformed(why) => f.write_str(&record::quoted(why)),
            RequestRecordError::Inconsistent(why) => f.write_str(why),
            RequestRecordError::NotCanonical => {
                f.write_str("it is not written the one way a record is written")
            }
        }
    }
}

impl std::error::Error for RequestRecordError {}

/// Why the largest amount there is is not one a record holds.
pub(super) const NOT_AN_AMOUNT: &str = "what was reserved is the largest amount there is, which is no amount: nothing is \
     reserved for a cost without a bound";

/// The kind field of a stored record, looked at first and by itself.
pub(super) fn kind_of(bytes: &[u8], reads: &str) -> Result<(), RequestRecordError> {
    if bytes.len() as u64 > REQUEST_RECORD_MAX_BYTES {
        return Err(RequestRecordError::TooLarge {
            bytes: bytes.len() as u64,
        });
    }
    #[derive(Deserialize)]
    struct Kind {
        record: String,
    }
    match serde_json::from_slice::<Kind>(bytes) {
        Ok(Kind { record }) if record != reads => Err(RequestRecordError::OtherVersion {
            found: record.chars().take(QUOTED_LIMIT).collect(),
        }),
        _ => Ok(()),
    }
}

/// One line of JSON and a newline. Such a record holds strings and whole numbers;
/// writing one cannot fail.
pub(super) fn line_of<T: Serialize>(record: &T) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(record).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

pub(super) fn malformed(error: serde_json::Error) -> RequestRecordError {
    RequestRecordError::Malformed(error.to_string())
}

/// An admission as it is stored. Private: an admission is read from JSON by
/// [`Admission::from_bytes`] and by nothing else, so none is read without being checked.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdmissionWire {
    record: String,
    run: String,
    #[serde(deserialize_with = "present")]
    request: Option<String>,
    #[serde(deserialize_with = "present")]
    parent: Option<String>,
    fingerprint: Digest,
    prepared: Digest,
    #[serde(deserialize_with = "present")]
    reserved_micros: Option<u64>,
    action: String,
    version: String,
    start_ms: u64,
}

/// A field that may be null and may not be left out.
pub(super) fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

/// The record of an admission: a run was made for a request.
///
/// It is made by the executor when it admits a call, and by reading a stored one, which
/// checks it; in no other way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admission {
    run: RunId,
    request: Option<RequestId>,
    parent: Option<RunId>,
    fingerprint: Digest,
    prepared: Digest,
    reserved_micros: Option<u64>,
    action: String,
    version: String,
    start_ms: u64,
}

impl Admission {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        run: RunId,
        request: Option<RequestId>,
        fingerprint: Digest,
        prepared: Digest,
        reserved_micros: Option<u64>,
        action: &str,
        version: &str,
        start_ms: u64,
    ) -> Admission {
        Admission {
            run,
            request,
            parent: None,
            fingerprint,
            prepared,
            reserved_micros,
            action: action.to_owned(),
            version: version.to_owned(),
            start_ms,
        }
    }

    /// The same admission, of a run that this run called from inside its action.
    pub(super) fn under(mut self, parent: Option<RunId>) -> Admission {
        self.parent = parent;
        self
    }

    /// The run that called this one from inside its action, when one did (ADR 0008,
    /// "Nested calls"). Such a run has no request id and no reservation: what
    /// [`Admission::reserved_micros`] says of it was taken from its parent's
    /// allowance, and the ledger was not asked.
    pub fn parent(&self) -> Option<&RunId> {
        self.parent.as_ref()
    }

    /// The run that was made.
    pub fn run(&self) -> &RunId {
        &self.run
    }

    /// The caller's id for the request, when it gave one.
    pub fn request(&self) -> Option<&RequestId> {
        self.request.as_ref()
    }

    /// The fingerprint of the request as it was submitted ([`request_fingerprint`]).
    pub fn fingerprint(&self) -> Digest {
        self.fingerprint
    }

    /// The fingerprint of the call as it was prepared when it was admitted (ADR 0004).
    pub fn prepared(&self) -> Digest {
        self.prepared
    }

    /// What was reserved for the run when it was admitted, in millionths of a US
    /// dollar: the amount of its reservation ([`super::Reservation`]), which the same
    /// publication made; or, for a run with a parent, what was taken for it from its
    /// parent's allowance, with no reservation. None when the executor that admitted
    /// the run had no ledger: nothing was reserved, and the run has no reservation.
    pub fn reserved_micros(&self) -> Option<u64> {
        self.reserved_micros
    }

    /// The action's name.
    pub fn action(&self) -> &str {
        &self.action
    }

    /// The action's version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// When the run was admitted, in milliseconds since the Unix epoch: the run's start.
    pub fn start_ms(&self) -> u64 {
        self.start_ms
    }

    /// The admission as it is stored: JSON on one line, then a newline.
    pub fn to_bytes(&self) -> Vec<u8> {
        line_of(&AdmissionWire {
            record: ADMISSION_RECORD.to_owned(),
            run: self.run.as_str().to_owned(),
            request: self.request.as_ref().map(|id| id.as_str().to_owned()),
            parent: self.parent.as_ref().map(|run| run.as_str().to_owned()),
            fingerprint: self.fingerprint,
            prepared: self.prepared,
            reserved_micros: self.reserved_micros,
            action: self.action.clone(),
            version: self.version.clone(),
            start_ms: self.start_ms,
        })
    }

    /// Reads a stored admission. Refused, in this order: more bytes than one may be;
    /// another kind or version; anything not of the shape, an unknown field and a field
    /// left out among them; a run that is no run id, a request that is no request id, an
    /// action or a version that is not one; bytes that are not the bytes this code
    /// writes for what they hold.
    pub fn from_bytes(bytes: &[u8]) -> Result<Admission, RequestRecordError> {
        kind_of(bytes, ADMISSION_RECORD)?;
        let mut reader = serde_json::Deserializer::from_slice(bytes);
        let wire = AdmissionWire::deserialize(&mut reader).map_err(malformed)?;
        reader.end().map_err(malformed)?;
        let inconsistent = |why: String| RequestRecordError::Inconsistent(why);
        let run = RunId::parse(&wire.run).map_err(inconsistent)?;
        let request = match &wire.request {
            Some(id) => Some(RequestId::parse(id).map_err(inconsistent)?),
            None => None,
        };
        text::action_name(&wire.action)
            .and_then(|()| text::version(&wire.version))
            // The rule's sentence shows what it refuses escaped already: it is cut, and
            // not escaped a second time.
            .map_err(|why| inconsistent(why.chars().take(record::REASON_LIMIT).collect()))?;
        let parent = match &wire.parent {
            Some(parent) => Some(RunId::parse(parent).map_err(inconsistent)?),
            None => None,
        };
        // A run another run called is not its own parent, and carries no request id.
        if parent.as_ref() == Some(&run) || (parent.is_some() && request.is_some()) {
            return Err(inconsistent(
                "the parent is the run itself, or a run with a parent has a request id".to_owned(),
            ));
        }
        let admission = Admission {
            run,
            request,
            parent,
            fingerprint: wire.fingerprint,
            prepared: wire.prepared,
            reserved_micros: wire.reserved_micros,
            action: wire.action,
            version: wire.version,
            start_ms: wire.start_ms,
        };
        if admission.reserved_micros == Some(u64::MAX) {
            return Err(inconsistent(NOT_AN_AMOUNT.to_owned()));
        }
        if admission.to_bytes() != bytes {
            return Err(RequestRecordError::NotCanonical);
        }
        Ok(admission)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EndingWire {
    record: String,
    run: String,
    request: String,
    status: Status,
    end_ms: u64,
}

/// The record of how a run that had a request id ended: the word, and when. It says no
/// more than the word: an error's message and a reason go with the run's own record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ending {
    run: RunId,
    request: RequestId,
    status: Status,
    end_ms: u64,
}

impl Ending {
    pub(super) fn new(run: RunId, request: RequestId, status: Status, end_ms: u64) -> Ending {
        Ending {
            run,
            request,
            status,
            end_ms,
        }
    }

    pub fn run(&self) -> &RunId {
        &self.run
    }

    pub fn request(&self) -> &RequestId {
        &self.request
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn end_ms(&self) -> u64 {
        self.end_ms
    }

    /// The ending as it is stored: JSON on one line, then a newline.
    pub fn to_bytes(&self) -> Vec<u8> {
        line_of(&EndingWire {
            record: ENDING_RECORD.to_owned(),
            run: self.run.as_str().to_owned(),
            request: self.request.as_str().to_owned(),
            status: self.status,
            end_ms: self.end_ms,
        })
    }

    /// Reads a stored ending, as strictly as an admission is read.
    pub fn from_bytes(bytes: &[u8]) -> Result<Ending, RequestRecordError> {
        kind_of(bytes, ENDING_RECORD)?;
        let mut reader = serde_json::Deserializer::from_slice(bytes);
        let wire = EndingWire::deserialize(&mut reader).map_err(malformed)?;
        reader.end().map_err(malformed)?;
        let ending = Ending {
            run: RunId::parse(&wire.run).map_err(RequestRecordError::Inconsistent)?,
            request: RequestId::parse(&wire.request).map_err(RequestRecordError::Inconsistent)?,
            status: wire.status,
            end_ms: wire.end_ms,
        };
        if ending.to_bytes() != bytes {
            return Err(RequestRecordError::NotCanonical);
        }
        Ok(ending)
    }
}

/// Where the run of an earlier request is now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunState {
    /// The run was admitted and has not ended. `here` says whether the executor that
    /// answers writes the store and made the run since it opened it: then the run is
    /// executing in this process, or its end could not be written. Otherwise, for an
    /// executor that writes the store, the process that made the run is gone and the
    /// run will never end; an executor that only reads the store cannot tell.
    NotEnded { here: bool },
    /// The run was admitted, has not ended, and was abandoned by the process that
    /// answers, which made it: nothing in this process will end it. Its admission
    /// became visible and was not confirmed, so its action never started; or its end
    /// was refused by the store; or the executor's own code panicked. `failure` is the
    /// store's failure, when one was the cause. Only the process that abandoned the
    /// run says so: to any other it is `NotEnded { here: false }`.
    Abandoned { failure: Option<StoreFailure> },
    /// The run ended, and its record is in the store: how it ended, as the first
    /// answer said it.
    Ended(Outcome),
    /// The run is no longer in the store: it was deleted. `ended` is how it had ended,
    /// from the ending recorded for the request, or none when it was deleted before it
    /// ended.
    Removed { ended: Option<Status> },
}

impl RunState {
    /// The same state, the store's failure in it made without paths
    /// ([`StoreFailure::pathless`]): what an action is told of a run.
    pub(super) fn pathless(self) -> RunState {
        match self {
            RunState::Abandoned { failure } => RunState::Abandoned {
                failure: failure.map(|failure| failure.pathless()),
            },
            other => other,
        }
    }
}

impl Answer {
    /// The same answer, as an action is given it ([`RunState::pathless`]).
    pub(super) fn pathless(self) -> Answer {
        match self {
            Answer::Repeat(earlier) => {
                let state = earlier.state.clone().pathless();
                Answer::Repeat(earlier.in_state(state))
            }
            other => other,
        }
    }
}

impl Cancel {
    /// The same answer, as an action is given it ([`RunState::pathless`]).
    pub(super) fn pathless(self) -> Cancel {
        match self {
            Cancel::NotAccepted(state) => Cancel::NotAccepted(state.pathless()),
            Cancel::Accepted => Cancel::Accepted,
        }
    }
}

impl Watched {
    /// The same answer, as an action is given it ([`RunState::pathless`]).
    pub(super) fn pathless(mut self) -> Watched {
        self.state = self.state.pathless();
        self
    }
}

/// The answer to a request that was sent before under the same id: the run that the
/// first one made, and where that run is now. Nothing was prepared and nothing ran to
/// give it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Earlier {
    run: RunId,
    start_ms: u64,
    state: RunState,
}

impl Earlier {
    /// The run the first request made.
    pub fn run(&self) -> &RunId {
        &self.run
    }

    /// When that run was admitted, in milliseconds since the Unix epoch.
    pub fn start_ms(&self) -> u64 {
        self.start_ms
    }

    pub fn state(&self) -> &RunState {
        &self.state
    }

    /// The same answer, with the run in this state.
    pub(super) fn in_state(mut self, state: RunState) -> Earlier {
        self.state = state;
        self
    }
}

/// What a request that was taken is answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// This request made a run, and the run ended.
    Ran(super::Finished),
    /// The request was sent before under this id: the run it made then.
    Repeat(Earlier),
    /// This request made a run, which is executing: it was started, not waited for
    /// ([`super::Executor::start`]).
    Started(RunId),
}

impl Answer {
    /// The run: the one this request made, or the one the first request made.
    pub fn run(&self) -> &RunId {
        match self {
            Answer::Ran(finished) => finished.run(),
            Answer::Repeat(earlier) => earlier.run(),
            Answer::Started(run) => run,
        }
    }
}

/// The answer to a cancellation of a run (ADR 0008, "Cancellation").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cancel {
    /// The cancellation was accepted: it arrived before the run entered its commit
    /// step. Nothing the run made is committed, and the run ends as cancelled. The
    /// answer does not wait for that end: the action is told, and comes back when it
    /// does ([`super::Executor::wait`]).
    Accepted,
    /// The cancellation was not accepted, and where the run is says why. `Ended`: the
    /// run had ended, or was in its commit step, which was waited for until the
    /// executor confirmed it; this is how it ended. `Abandoned`: this process made the
    /// run and will not end it: its end was refused by the store or not confirmed, its
    /// admission was not confirmed, or the executor's own code panicked.
    /// `NotEnded { here: true }`: other code of this process made the run.
    /// `NotEnded { here: false }`: nothing in this process works on the run, so
    /// nothing can be told to stop.
    NotAccepted(RunState),
}

/// The answer to a question after a run by its id: where the run is now, and, while it
/// executes in this process, the progress events it has reported so far, in order, the
/// secrets taken out. For a run that ended, the events are in its journal, in the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watched {
    run: RunId,
    state: RunState,
    events: Vec<JournalLine>,
}

impl Watched {
    pub(super) fn new(run: RunId, state: RunState, events: Vec<JournalLine>) -> Watched {
        Watched { run, state, events }
    }

    pub fn run(&self) -> &RunId {
        &self.run
    }

    pub fn state(&self) -> &RunState {
        &self.state
    }

    /// The progress events of a run that is executing here; none of any other.
    pub fn events(&self) -> &[JournalLine] {
        &self.events
    }
}

/// What looking a request id up found.
pub(super) enum Found {
    /// The id was never admitted.
    Nothing,
    /// The id was admitted for this request.
    Same(Earlier),
    /// The id was admitted for another request, which made this run.
    Other { run: RunId },
}

fn unread(what: &'static str, why: impl std::fmt::Display) -> CallError {
    CallError::Record {
        what,
        why: why.to_string(),
    }
}

const THE_REQUEST: &str = "the request id's record";
const THE_RUN: &str = "the record of the request's run";

/// The content of an entry that should hold a record of a request, bounded before it
/// is read.
fn bounded(contents: &dyn Contents, entry: &Entry) -> Result<Vec<u8>, CallError> {
    if entry.bytes > REQUEST_RECORD_MAX_BYTES {
        return Err(unread(
            THE_REQUEST,
            RequestRecordError::TooLarge { bytes: entry.bytes },
        ));
    }
    contents
        .read_record(entry)
        .map_err(|error| CallError::Store(StoreFailure::new(error)))
}

/// Looks a request id up in the current generation: what the id was admitted for, and
/// where its run is now. `made_here` says whether this executor's store made a run.
///
/// What is read is not trusted to be what this build wrote: each record is bounded
/// before it is read, read strictly, and held to name the id and the run it is filed
/// under.
pub(super) fn look_up(
    contents: &dyn Contents,
    id: &RequestId,
    fingerprint: Digest,
    made_here: impl Fn(&str) -> bool,
) -> Result<Found, CallError> {
    let generation = contents
        .snapshot()
        .map_err(|error| CallError::Store(StoreFailure::new(error)))?;
    let Some(entry) = generation.get(&id.admission_name()) else {
        // An ending under an id that has no admission is not what this code writes. The
        // id is not free: admitted now, its run would execute and its end be refused.
        if generation.get(&id.ending_name()).is_some() {
            return Err(unread(THE_REQUEST, "the id has an ending and no admission"));
        }
        return Ok(Found::Nothing);
    };
    let admission = Admission::from_bytes(&bounded(contents, entry)?)
        .map_err(|error| unread(THE_REQUEST, error))?;
    if admission.request() != Some(id) {
        return Err(unread(THE_REQUEST, "it is the record of another request"));
    }
    let run = admission.run().clone();
    if admission.fingerprint() != fingerprint {
        return Ok(Found::Other { run });
    }
    // A run's id is free again once the run is deleted, and can be given to another
    // run. The run the store holds under the id is this request's only when it holds,
    // as its own admission, the very content of the id's record: the two were
    // published as the same bytes.
    let own = generation.run(run.as_str()).filter(|held| {
        held.get(ADMITTED_NAME)
            .is_some_and(|admitted| admitted.digest == entry.digest)
    });
    let state = match own {
        None => RunState::Removed {
            ended: ending_of(contents, &generation, id, &run)?,
        },
        Some(held) if held.get(RECORD_NAME).is_none() => RunState::NotEnded {
            here: made_here(run.as_str()),
        },
        Some(_) => {
            let read = |entry: &Entry| contents.read_record(entry);
            // The run is in this generation and has a record: `stored` answers.
            let stored = record::stored(&generation, run.as_str(), read)
                .unwrap_or(Err(StoredRecordError::NoRecord));
            match stored {
                Ok(record) => RunState::Ended(outcome_of(&record)),
                Err(StoredRecordError::Store(failure)) => return Err(CallError::Store(failure)),
                Err(error) => return Err(unread(THE_RUN, error)),
            }
        }
    };
    Ok(Found::Same(Earlier {
        run,
        start_ms: admission.start_ms(),
        state,
    }))
}

/// How the run of a request ended, from the ending recorded for the id, when there is
/// one.
fn ending_of(
    contents: &dyn Contents,
    generation: &Generation,
    id: &RequestId,
    run: &RunId,
) -> Result<Option<Status>, CallError> {
    let Some(entry) = generation.get(&id.ending_name()) else {
        return Ok(None);
    };
    let ending = Ending::from_bytes(&bounded(contents, entry)?)
        .map_err(|error| unread(THE_REQUEST, error))?;
    if ending.request() != id || ending.run() != run {
        return Err(unread(
            THE_REQUEST,
            "its ending is of another request or another run",
        ));
    }
    Ok(Some(ending.status()))
}

/// The outcome a run's record holds: what the run's first answer said, since the
/// secrets were taken out of both alike.
pub(super) fn outcome_of(record: &RunRecord) -> Outcome {
    let ended = record.outcome();
    match ended.status {
        Status::Completed => Outcome::Completed {
            outputs: record
                .outputs()
                .iter()
                .map(|o| Committed::new(&o.port, o.artifact_type, ArtifactId(o.artifact)))
                .collect(),
        },
        Status::Cancelled => Outcome::Cancelled,
        Status::Error => {
            let failed = ended.error.clone();
            let (kind, message) = failed.map(|f| (f.kind, f.message)).unwrap_or_default();
            Outcome::Error { kind, message }
        }
        Status::Invalid => Outcome::Invalid {
            why: ended.invalid.clone().unwrap_or_default(),
        },
    }
}

/// Why a run's admission could not be had from a store.
#[derive(Debug)]
pub enum StoredAdmissionError {
    /// The run has no name `admitted`: something other than an executor of this build
    /// made it.
    NoAdmission,
    /// The store failed to give the content. Its text holds no path.
    Store(StoreFailure),
    /// What the run holds under `admitted` is not an admission this build reads.
    Record(RequestRecordError),
    /// The admission names another run than the one it is stored in.
    OtherRun,
}

impl std::fmt::Display for StoredAdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoredAdmissionError::NoAdmission => f.write_str("the run has no admission"),
            StoredAdmissionError::Store(error) => error.fmt(f),
            StoredAdmissionError::Record(error) => write!(f, "not an admission: {error}"),
            StoredAdmissionError::OtherRun => f.write_str("the admission names another run"),
        }
    }
}

impl std::error::Error for StoredAdmissionError {}

/// The admission of the run `id` in this generation, read with `read`, which gives the
/// content of an entry of either kind. `id` is whatever the store holds a run under.
/// `None` when the generation holds no such run.
pub fn admitted(
    generation: &Generation,
    id: &str,
    read: impl Fn(&Entry) -> crate::store::Result<Vec<u8>>,
) -> Option<Result<Admission, StoredAdmissionError>> {
    let run = generation.run(id)?;
    let Some(entry) = run.get(ADMITTED_NAME) else {
        return Some(Err(StoredAdmissionError::NoAdmission));
    };
    if entry.bytes > REQUEST_RECORD_MAX_BYTES {
        let error = RequestRecordError::TooLarge { bytes: entry.bytes };
        return Some(Err(StoredAdmissionError::Record(error)));
    }
    let admission = read(entry)
        .map_err(|error| StoredAdmissionError::Store(StoreFailure::new(error)))
        .and_then(|bytes| Admission::from_bytes(&bytes).map_err(StoredAdmissionError::Record))
        .and_then(|admission| {
            if admission.run().as_str() == id {
                Ok(admission)
            } else {
                Err(StoredAdmissionError::OtherRun)
            }
        });
    Some(admission)
}
