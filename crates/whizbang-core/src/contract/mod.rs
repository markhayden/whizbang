//! The execution contract (`SPEC.md` section 3), independent of any door.
//!
//! This module holds its static part (ADR 0003): the action [descriptor](ActionDescriptor),
//! the [rule](text) for the text a description prints, and the [`Registry`] a build's
//! actions are registered in. And the first half of a call's life (ADR 0004): the
//! [`Call`], [preparing](Executor::prepare) it, and the [`Executor`]'s interface with the
//! types of what an executing action is [handed](Context) and [hands back](Outcome).
//! Admitting and executing a call, and the run record, are not here yet.

mod call;
mod context;
mod descriptor;
mod engine;
mod prepare;
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
pub use prepare::{
    Backends, Missing, ModelIdentity, Need, NoBackends, PreparedCall, Readiness, Refusal,
    ResolvedInput, Unavailable,
};
pub use registry::{Action, RegistrationError, Registry};
pub use run::{Committed, Finished, Outcome, RUN_ID_LIMIT, RunId};
