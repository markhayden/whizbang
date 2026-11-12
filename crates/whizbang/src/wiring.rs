//! Wiring: the registry of this build, made in one place.
//!
//! The binary calls [`registry`] and describes what it holds. A test calls the same
//! function and then registers its fakes, so what a test runs against is the shipping
//! registry and nothing the binary would not also have, plus what the test added. Fakes
//! live in `whizbang-testkit`, which this crate depends on for its tests only.

use whizbang_core::contract::{RegistrationError, Registry};
use whizbang_core::description::Description;

/// Every action this build ships, registered in the order a door lists them. Tools add
/// theirs here as they are ported.
pub fn registry() -> Result<Registry, RegistrationError> {
    Ok(Registry::new())
}

/// The static description of a build of this crate's version that holds this registry.
pub fn description(registry: &Registry) -> Description {
    Description::new(env!("CARGO_PKG_VERSION"), registry)
}

/// The description as `whizbang describe` prints it for a person: the build, then each
/// action's name and purpose.
pub fn describe_text(description: &Description) -> String {
    let mut text = format!("{} {}\n", description.name(), description.version());
    if description.actions().is_empty() {
        text.push_str("no actions yet\n");
    }
    for action in description.actions() {
        text.push_str(&format!("  {}  {}\n", action.name(), action.purpose()));
    }
    text
}
