//! The registry: the actions a build can run, by name, in the order they were registered.
//!
//! The core names no tool. The binary builds the registry from the tools it links and
//! hands it on; a test builds the same registry and adds its fakes.

use std::sync::Arc;

use super::{ActionDescriptor, Context, PreparedCall};

/// What an action hands back when it finished. It holds nothing yet: an action cannot
/// hand back an artifact until there is a commit step to publish it in (the plan's task
/// for nested calls, cancellation and publication). It cannot be made outside this crate
/// except as nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Produced {
    _contents: (),
}

impl Produced {
    /// The action finished, and made no artifact.
    pub fn nothing() -> Produced {
        Produced { _contents: () }
    }
}

/// How an action says it did not finish. The executor makes the run's outcome from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The action failed: the type of the failure, as a short word, and what it said.
    Error { kind: String, message: String },
    /// The action found that what it was given cannot be worked on.
    Invalid { why: String },
}

impl Failure {
    pub fn error(kind: &str, message: &str) -> Failure {
        Failure::Error {
            kind: kind.to_owned(),
            message: message.to_owned(),
        }
    }

    pub fn invalid(why: &str) -> Failure {
        Failure::Invalid {
            why: why.to_owned(),
        }
    }
}

/// One action: something a registry holds and an executor runs.
pub trait Action: Send + Sync {
    /// What the action is, takes, returns and needs. The same value on every call.
    fn descriptor(&self) -> &ActionDescriptor;

    /// Runs the action on a call that was prepared and found ready, with what the
    /// executor hands it. It returns when the action is over.
    ///
    /// An action that only describes itself, as a fixture does, has nothing to run, and
    /// fails saying so.
    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let _ = context;
        Err(Failure::error(
            "not_runnable",
            &format!("the action {} has nothing to run", call.action()),
        ))
    }
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
