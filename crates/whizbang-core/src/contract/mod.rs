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
//! its run as an error. The ledger itself and settlement are not here, and neither are
//! nested calls and cancellation.

mod call;
mod context;
mod descriptor;
mod engine;
mod journal;
pub mod ledger;
mod prepare;
pub mod record;
mod redact;
mod registry;
pub mod request;
mod run;
pub mod text;

pub use crate::capability::{FillHandle, JudgeHandle, SegmentHandle};
pub use call::Call;
pub use context::{
    Allowance, Cancellation, Capabilities, Context, Diagnostics, Events, NoDiagnostics,
};
pub use descriptor::{
    ActionDescriptor, Argument, Capability, Count, DescriptorError, Draft, InputPort, Kind,
    OutputPort, PortRef, Requirement, Value, ValueType, When,
};
pub use engine::{ConfigSource, Engine, Executor, WHIZBANG_VERSION};
pub use journal::{Clock, JournalLine, SystemClock, journal_bytes, read_journal};
pub use ledger::{
    Declined, LEDGER_NAME_LIMIT, Ledger, RESERVATION_RECORD, RESERVATIONS_PREFIX, Reservation,
    SPEND_PREFIX, Step, reservations,
};
pub use prepare::{
    Backends, CallError, Estimate, InputFault, Missing, ModelIdentity, Need, NoBackends,
    PreparedCall, Readiness, Refusal, ResolvedInput, Stale, StoreFailure, Unavailable,
};
pub use record::{
    DIAGNOSTIC_NAME_LIMIT, RECORD_MAX_BYTES, RUN_RECORD, RUN_RECORD_SCHEMA, RecordError, RunRecord,
    is_diagnostic_name,
};
pub use redact::{REDACTED, SECRET_MIN_BYTES, SecretRefused, Secrets};
pub use registry::{Action, Failure, Produced, RegistrationError, Registry};
pub use request::{
    ADMISSION_RECORD, ADMITTED_NAME, Admission, Answer, ENDING_RECORD, Earlier, Ending,
    REQUEST_ID_LIMIT, REQUEST_RECORD_MAX_BYTES, REQUESTS_PREFIX, Request, RequestId,
    RequestRecordError, RunState, request_fingerprint,
};
pub use run::{Committed, Finished, MESSAGE_LIMIT, Outcome, RUN_ID_LIMIT, RunId};
