//! The execution contract (`SPEC.md` section 3), independent of any door.
//!
//! This module holds its static part (ADR 0003): the action [descriptor](ActionDescriptor),
//! the [rule](text) for the text a description prints, and the [`Registry`] a build's
//! actions are registered in. And the first half of a call's life (ADR 0004): the
//! [`Call`], [preparing](Executor::prepare) it, and the [`Executor`]'s interface with the
//! types of what an executing action is [handed](Context) and [hands back](Outcome).
//! And the second half, as far as it is built (ADR 0005): an action [runs](Action::run)
//! through [`Executor::submit`], and its run is written to the store as a [`RunRecord`]
//! with its journal and diagnostics, [redacted](Secrets). And request ids and
//! admission (ADR 0007): a [`Request`] may carry a [`RequestId`], which is
//! looked up before anything is prepared; a run is made durably, with its
//! [`Admission`], before its action runs; and the id's record outlives the run. Prepare
//! returns an [`Estimate`], and a request that comes with the preparation its caller
//! was shown is refused as [stale](Stale) when its call no longer prepares the same.
//! A run is admitted with a [`Reservation`] of that estimate, which a [`Ledger`] is
//! asked for in the admission's own step. And the commit step (ADR 0008): an action
//! hands back artifacts ([`Produced`]), and the executor publishes them with the run's
//! completed status in the one publication that ends the run; a panic in an action ends
//! its run as an error. And asking after a run by its id: [`Executor::start`],
//! [`Executor::status`] and [`Executor::wait`]; and cancelling it
//! ([`Executor::cancel`]), which is accepted before the run's commit step and never
//! after. And nested calls: the executor in an action's [`Context`] admits a call under
//! the action's run, with an allowance taken from that run's, and refuses a fifth level
//! and a call back; a cancellation reaches every run under the one it cancels. The
//! ledger itself and settlement are not here. And the executor's parts (ADR 0015): an
//! [`AdmissionPart`] is asked before each attempt at an admission's publication, may
//! refuse the call, and may add records among the run's names. And the one way to a
//! writer ([`Opening`]): a store is recovered, by a list of [parts](RecoveryPart),
//! before anyone is handed its writer.

mod call;
mod context;
mod descriptor;
mod engine;
mod facts;
mod findings;
mod interrupted;
mod journal;
pub mod ledger;
mod live;
mod nested;
mod outputs;
mod part;
mod plan;
mod prepare;
pub mod record;
mod recover;
mod redact;
mod registry;
pub mod request;
mod run;
mod sixth;
mod snapshot;
pub mod text;

pub use crate::capability::{
    Around, Ended, FillHandle, JudgeHandle, Nothing, Open, Place, Places, SegmentHandle, Stop,
    Wrapped,
};
pub use call::Call;
pub use context::{
    Allowance, Cancellation, Capabilities, Context, Diagnostics, Dispatched, Events, Handles,
    NoDiagnostics,
};
pub use descriptor::{
    ActionDescriptor, Argument, Capability, Count, DescriptorError, Draft, InputPort, Kind,
    OutputPort, PortRef, Requirement, ResultKind, Value, ValueType, When,
};
pub use engine::{
    BoundRefused, Engine, Executor, NOT_CONFIRMED_KIND, STARTED_RUNS_CEILING, STARTED_RUNS_DEFAULT,
    Spawn, WHIZBANG_VERSION,
};
pub use facts::RunFacts;
pub use findings::{
    Confidence, Disposition, FINDING_OBLIGATIONS_LIMIT, FINDING_PLACES_LIMIT,
    FINDING_POLYGON_LIMIT, FINDINGS, FINDINGS_LIMIT, FINDINGS_MAX_BYTES, FINDINGS_SCHEMA, Finding,
    FindingCount, Findings, Form, FoundAt, LeftOut, is_obligation_id,
};
pub use interrupted::Interrupted;
pub use journal::{
    Clock, EVENT_MAX_BYTES, EVENTS_KEPT_LIMIT, EXCHANGE_PREFIX, Event, Journal, JournalLine,
    MESSAGES_KEPT_LIMIT, MESSAGES_KEPT_MAX_BYTES, NotKept, SystemClock, journal_bytes,
    read_journal,
};
pub use ledger::{
    Closing, Declined, DispatchRefused, LEDGER_NAME_LIMIT, Ledger, MadeName, RESERVATION_RECORD,
    RESERVATIONS_PREFIX, Reread, Reservation, SPEND_PREFIX, Settled, Step, reservations,
};
#[doc(hidden)]
pub use nested::HeldOpen;
pub use nested::{DESCENDANTS_LIMIT, LEVELS_LIMIT, STARTED_LIMIT};
pub use part::{
    AdmissionPart, Asking, PART_RECORD_MAX_BYTES, PART_RECORDS_LIMIT, PART_RECORDS_PREFIX,
    PartDeclined, PartRefusal, PartStep, part_record_name,
};
pub use plan::{Able, Attempts, PLAN_ATTEMPTS_MAX, PLAN_WHY_LIMIT, Plan, Planning, Sends};
pub use prepare::{
    ARGUMENTS_TEXT_MAX_BYTES, Backends, CallError, Estimate, INPUTS_LIMIT, InputFault, Missing,
    MissingKind, ModelIdentity, Need, NoBackends, NotJudged, PreparedCall, Readiness, Refusal,
    ResolvedInput, Stale, StoreFailure, Unavailable,
};
pub use record::{
    DIAGNOSTIC_NAME_LIMIT, DIAGNOSTICS_LIMIT, RECIPE_ARGUMENTS_LIMIT, RECIPE_STEPS_LIMIT,
    RECIPE_TEXT_LIMIT, RECORD_MAX_BYTES, REFUSAL_WHY_LIMIT, REFUSALS_LIMIT, RUN_2, RUN_2_SCHEMA,
    RUN_3, RUN_3_SCHEMA, RUN_5, RUN_5_SCHEMA, RUN_6, RUN_6_SCHEMA, RUN_RECORD, RUN_RECORD_SCHEMA,
    RecordError, RefusalKind, RunRecord, STEPS_LIMIT, is_diagnostic_name,
};
pub use recover::{
    NoWriter, Opening, RECOVERY_WHY_LIMIT, Recovering, Recovery, RecoveryFailed, RecoveryPart,
    Remover,
};
pub use redact::{REDACTED, SECRET_MIN_BYTES, SecretRefused, Secrets};
pub use registry::{
    Action, Failure, MEASUREMENTS_MAX_BYTES, Produced, RegistrationError, Registry, Verdict,
};
pub use request::{
    ADMISSION_RECORD, ADMITTED_NAME, Admission, Answer, Cancel, ENDING_RECORD, Earlier, Ending,
    LookedUp, REQUEST_ID_LIMIT, REQUEST_RECORD_MAX_BYTES, REQUESTS_PREFIX, RESERVED_REQUEST_PREFIX,
    Request, RequestId, RequestRecordError, RunState, Watched, request_fingerprint,
};
pub use run::{Committed, Finished, MESSAGE_LIMIT, Outcome, RUN_ID_LIMIT, RunId};
pub use snapshot::{
    ConfigSource, Made, Offered, SECRET_ORIGIN_APPROVED, SECRETS_KEPT_LIMIT, SOURCE_WHY_LIMIT,
    SOURCE_WHY_WITHHELD, Settings, Snapshot, SourceRefused,
};
