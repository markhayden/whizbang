//! The registry: the actions a build can run, by name, in the order they were registered.
//!
//! The core names no tool. The binary builds the registry from the tools it links and
//! hands it on; a test builds the same registry and adds its fakes.

use std::sync::Arc;

use super::ActionDescriptor;

/// One action: something a registry holds and an executor runs.
///
/// Today an action says what it is, and that is all this trait asks. How an action is
/// prepared and executed is not decided yet.
pub trait Action: Send + Sync {
    /// What the action is, takes, returns and needs. The same value on every call.
    fn descriptor(&self) -> &ActionDescriptor;
}

/// Why an action was not registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationError {
    /// An action of this name is registered already.
    Duplicate { name: String },
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistrationError::Duplicate { name } => {
                write!(f, "an action named {name} is registered already")
            }
        }
    }
}

impl std::error::Error for RegistrationError {}

/// The actions of one build.
#[derive(Clone, Default)]
pub struct Registry {
    actions: Vec<Arc<dyn Action>>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    /// Adds an action after those already registered. A name is registered once: a second
    /// action of the same name is refused, and the registry is as it was.
    ///
    /// Nothing else is checked here because nothing else can be wrong: a descriptor
    /// exists only when it keeps every rule, the rule for a name and a purpose among them
    /// ([`Draft::build`](super::Draft::build)).
    pub fn register(&mut self, action: Arc<dyn Action>) -> Result<(), RegistrationError> {
        let name = action.descriptor().name();
        if self.get(name).is_some() {
            return Err(RegistrationError::Duplicate {
                name: name.to_owned(),
            });
        }
        self.actions.push(action);
        Ok(())
    }

    /// The action of this name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Action>> {
        self.actions
            .iter()
            .find(|action| action.descriptor().name() == name)
    }

    /// Every action's descriptor, in the order the actions were registered.
    pub fn descriptors(&self) -> impl Iterator<Item = &ActionDescriptor> {
        self.actions.iter().map(|action| action.descriptor())
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}
