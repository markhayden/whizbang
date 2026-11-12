//! The execution contract (`SPEC.md` section 3), independent of any door.
//!
//! This module holds its static part (ADR 0003): the action [descriptor](ActionDescriptor),
//! the [rule](text) for the text a description prints, and the [`Registry`] a build's
//! actions are registered in. And the first half of a call's life (ADR 0004): the
//! [`Call`], [preparing](Executor::prepare) it, and the [`Executor`]'s interface with the
//! types of what an executing action is [handed](Context) and [hands back](Outcome).
//! And the second half, as far as it is built (ADR 0005): an action [runs](Action::run)
//! through [`Executor::submit`], and its run is written to the store as a [`RunRecord`]
//! with its journal and diagnostics, [redacted](Secrets). Admission, nested calls,
//! cancellation and an action's outputs are not here yet.

mod call;
mod context;
mod descriptor;
mod engine;
mod journal;
mod prepare;
pub mod record;
mod redact;
mod registry;
mod run;
pub mod text;

pub use call::Call;
pub use context::{
    Allowance, Cancellation, Capabilities, Context, Diagnostics, Events, FillHandle, JudgeHandle,
    NoDiagnostics, SegmentHandle,
};
pub use descriptor::{
    ActionDescriptor, Argument, Capability, Count, DescriptorError, Draft, InputPort, Kind,
    OutputPort, PortRef, Requirement, Value, ValueType, When,
};
pub use engine::{Engine, Executor, WHIZBANG_VERSION};
pub use journal::{Clock, JournalLine, SystemClock, journal_bytes, read_journal};
pub use prepare::{
    Backends, CallError, InputFault, Missing, ModelIdentity, Need, NoBackends, PreparedCall,
    Readiness, Refusal, ResolvedInput, StoreFailure, Unavailable,
};
pub use record::{
    DIAGNOSTIC_NAME_LIMIT, RECORD_MAX_BYTES, RUN_RECORD, RUN_RECORD_SCHEMA, RecordError, RunRecord,
    is_diagnostic_name,
};
pub use redact::{REDACTED, SECRET_MIN_BYTES, SecretRefused, Secrets};
pub use registry::{Action, Failure, Produced, RegistrationError, Registry};
pub use run::{Committed, Finished, Outcome, RUN_ID_LIMIT, RunId};
