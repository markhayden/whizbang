//! Wiring: the registry and the executor of this build, each made in one place.
//!
//! The binary calls [`registry`] and describes what it holds. A test calls the same
//! function and then registers its fakes, so what a test runs against is the shipping
//! registry and nothing the binary would not also have, plus what the test added. Fakes
//! live in `whizbang-testkit`, which this crate depends on for its tests only.

use std::sync::Arc;

use whizbang_core::artifact::Contents;
use whizbang_core::contract::{Action, Engine, NoBackends, Registry};
use whizbang_core::description::Description;
use whizbang_core::identity::Digest;
use whizbang_core::store::Store;

/// Every action this build ships, registered in the order a door lists them: the
/// recipe first, then the primitives. Tools add theirs here as they are ported. What
/// comes back otherwise is why an action could not be registered, in a sentence.
///
/// `check` and `view` are squint's (ADR 0009). They are described, and no command runs
/// them yet: `whizbang squint check` is milestone 2's. `check` needs a judge, and the
/// executors below are built with no backend, so on this build it is prepared as not
/// ready; `view` needs none, and is what `check` calls.
pub fn registry() -> Result<Registry, String> {
    let said = |error: &dyn std::fmt::Display| error.to_string();
    let mut registry = Registry::new();
    let toolbox = whizbang_squint::check::Toolbox::standard();
    let check = whizbang_squint::check::Check::new(toolbox).map_err(|e| said(&e))?;
    let grow = whizbang_rascal::grow::Grow::new();
    let view = whizbang_squint::view::View::new().map_err(|e| said(&e))?;
    let actions: [Arc<dyn Action>; 3] = [Arc::new(check), Arc::new(grow), Arc::new(view)];
    for action in actions {
        registry.register(action).map_err(|e| said(&e))?;
    }
    Ok(registry)
}

/// The executor of this build over a registry and a read of a store: it prepares calls
/// and runs none. The binary gives it [`registry`]; a test gives it that registry with
/// its fakes added.
///
/// Two things it is built with do not exist yet, and each has the one value that says
/// so. No backend is configured, so every capability is unavailable. And there is no
/// config, so its digest is the digest of no bytes.
pub fn executor(registry: Registry, store: Arc<dyn Contents + Send + Sync>) -> Engine {
    Engine::new(registry, Arc::new(NoBackends), store, Digest::of_bytes(b""))
}

/// The executor of this build that runs actions: over a registry and a store opened to
/// write, which it reads and writes its runs to. It is built as [`executor`] is, with no
/// backend, no config and no secret to redact: nothing reads a key yet. `whizbang rascal
/// grow` runs its action on it, and the tests run theirs, with their fakes registered.
pub fn runner(registry: Registry, store: Arc<Store>) -> Engine {
    Engine::over(registry, Arc::new(NoBackends), store, Digest::of_bytes(b""))
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
