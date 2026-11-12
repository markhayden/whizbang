//! Wiring: the registry and the executor of this build, each made in one place.
//!
//! The binary calls [`registry`] and describes what it holds. A test calls the same
//! function and then registers its fakes, so what a test runs against is the shipping
//! registry and nothing the binary would not also have, plus what the test added. Fakes
//! live in `whizbang-testkit`, which this crate depends on for its tests only.

use std::sync::Arc;

use whizbang_core::artifact::Contents;
use whizbang_core::contract::{Engine, NoBackends, RegistrationError, Registry};
use whizbang_core::description::Description;
use whizbang_core::identity::Digest;

/// Every action this build ships, registered in the order a door lists them. Tools add
/// theirs here as they are ported.
pub fn registry() -> Result<Registry, RegistrationError> {
    Ok(Registry::new())
}

/// The executor of this build over a registry and a read of a store. The binary gives it
/// [`registry`]; a test gives it that registry with its fakes added.
///
/// Two things it is built with do not exist yet, and each has the one value that says
/// so. No backend is configured, so every capability is unavailable. And there is no
/// config, so its digest is the digest of no bytes.
pub fn executor(registry: Registry, store: Arc<dyn Contents + Send + Sync>) -> Engine {
    Engine::new(registry, Arc::new(NoBackends), store, Digest::of_bytes(b""))
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
