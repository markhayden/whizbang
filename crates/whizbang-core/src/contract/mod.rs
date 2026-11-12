//! The execution contract (`SPEC.md` section 3), independent of any door.
//!
//! This module holds its static part (ADR 0003): the action [descriptor](ActionDescriptor),
//! the [rule](text) for the text a description prints, and the [`Registry`] a build's
//! actions are registered in. And the first half of a call's life (ADR 0004): the
//! [`Call`], [preparing](Executor::prepare) it, and the [`Executor`]'s interface with the
//! types of what an executing action is [handed](Context) and [hands back](Outcome).
//! And the account of a run (ADR 0005): the [`RunRecord`], its journal, and the
//! [redaction](Secrets) of what is written. Admitting and executing a call, and writing
//! its record, are not here yet.

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
    Allowance, Cancellation, Capabilities, Context, Events, FillHandle, JudgeHandle, SegmentHandle,
};
pub use descriptor::{
    ActionDescriptor, Argument, Capability, Count, DescriptorError, Draft, InputPort, Kind,
    OutputPort, PortRef, Requirement, Value, ValueType, When,
};
pub use engine::{Engine, Executor};
pub use journal::{JournalLine, journal_bytes, read_journal};
pub use prepare::{
    Backends, CallError, InputFault, Missing, ModelIdentity, Need, NoBackends, PreparedCall,
    Readiness, Refusal, ResolvedInput, StoreFailure, Unavailable,
};
pub use record::{RUN_RECORD, RUN_RECORD_SCHEMA, RecordError, RunRecord};
pub use redact::{REDACTED, SECRET_MIN_BYTES, SecretRefused, Secrets};
pub use registry::{Action, RegistrationError, Registry};
pub use run::{Committed, Finished, Outcome, RUN_ID_LIMIT, RunId};
