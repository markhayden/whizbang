//! The execution contract (`SPEC.md` section 3), independent of any door.
//!
//! This module holds its static part (ADR 0003): the action [descriptor](ActionDescriptor),
//! the [rule](text) for the text a description prints, and the [`Registry`] a build's
//! actions are registered in. Preparing, admitting and executing a call, and the run
//! record, are not here yet.

mod descriptor;
mod registry;
pub mod text;

pub use descriptor::{
    ActionDescriptor, Argument, Capability, Count, DescriptorError, Draft, InputPort, Kind,
    OutputPort, PortRef, Requirement, Value, ValueType, When,
};
pub use registry::{Action, RegistrationError, Registry};
