//! What the actions over a mask share: reading an argument, reading a mask from the
//! store, and the failure of a run that was told to stop.

use whizbang_core::artifact::{self, ArtifactId, ArtifactType, Mask};
use whizbang_core::contract::{Context, Failure, PreparedCall, Value};
use whizbang_core::identity::DecodeLimits;

/// The number of an argument, when the call has it.
pub(crate) fn number(call: &PreparedCall, name: &str) -> Option<f64> {
    call.arguments().iter().find_map(|(n, value)| match value {
        Value::Number(number) if n == name => Some(*number),
        _ => None,
    })
}

/// The text, or the choice, of an argument, when the call has it.
pub(crate) fn text<'a>(call: &'a PreparedCall, name: &str) -> Option<&'a str> {
    call.arguments().iter().find_map(|(n, value)| match value {
        Value::Text(text) if n == name => Some(text.as_str()),
        _ => None,
    })
}

/// The identities given at a port, in the order the call gave them.
pub(crate) fn given(call: &PreparedCall, port: &str) -> Vec<ArtifactId> {
    let at = call.inputs().iter().filter(|input| input.port() == port);
    at.map(|input| input.id()).collect()
}

/// The mask of this identity, from the run's store. The store's own words for a
/// failure name its files by their paths, and a message is kept in the run's record:
/// so the words without paths.
pub(crate) fn mask(context: &Context, id: ArtifactId) -> Result<Mask, Failure> {
    artifact::get(
        context.store(),
        ArtifactType::Mask,
        id,
        &DecodeLimits::default(),
    )
    .and_then(artifact::Artifact::into_mask)
    .map_err(|error| Failure::error("input", &error.without_paths()))
}

/// Whether the run was cancelled or its deadline, by the executor's clock, passed.
pub(crate) fn stopped(context: &Context) -> bool {
    context.cancellation().is_cancelled() || context.deadline_passed()
}

/// The failure of a run of `action` that stopped because it was told to.
pub(crate) fn stopped_failure(context: &Context, action: &str) -> Failure {
    if context.cancellation().is_cancelled() {
        Failure::error(
            "cancelled",
            &format!("{action} stopped: its run was cancelled"),
        )
    } else {
        Failure::error(
            "deadline",
            &format!("{action} stopped: its deadline passed"),
        )
    }
}

/// How many pixels of a plane are set.
pub(crate) fn set(plane: &[u8]) -> u64 {
    plane.iter().filter(|sample| **sample != 0).count() as u64
}

/// What one operation's command asks, as it was typed: the action it runs, its
/// arguments, and each input as its port, the flag that gave it and the identity typed.
/// Each operation's module has its own command's flags (`Args`) and says this of them, so
/// that the binary's command gains one line for each operation (plan m3, "Shared files").
pub trait Asked {
    /// The action the command runs.
    fn action(&self) -> &'static str;
    /// The arguments, each by its name.
    fn arguments(&self) -> Vec<(&'static str, Value)>;
    /// The inputs: the port, the flag that named it, and the identity as typed.
    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)>;
}
