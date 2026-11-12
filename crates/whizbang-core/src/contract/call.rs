//! A call: what a door hands the executor.

use std::collections::BTreeMap;

use crate::artifact::ArtifactId;

use super::Value;

/// One call of one action: its name, the values of its arguments, and the artifacts at
/// its input ports, each by identity.
///
/// That is all a call can hold. It carries no artifact itself, no binding and no
/// derivation: an input is something the store already holds, named by its identity, and
/// what it belongs to is read from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    action: String,
    arguments: BTreeMap<String, Value>,
    inputs: BTreeMap<String, Vec<ArtifactId>>,
}

impl Call {
    /// A call of the action of this name, with no argument and no input yet.
    pub fn new(action: &str) -> Call {
        Call {
            action: action.to_owned(),
            arguments: BTreeMap::new(),
            inputs: BTreeMap::new(),
        }
    }

    /// With this argument. Given twice, the later value stands.
    pub fn argument(mut self, name: &str, value: Value) -> Call {
        self.arguments.insert(name.to_owned(), value);
        self
    }

    /// With one more artifact at this input port, after those already given there.
    pub fn input(mut self, port: &str, artifact: ArtifactId) -> Call {
        self.inputs
            .entry(port.to_owned())
            .or_default()
            .push(artifact);
        self
    }

    pub fn action(&self) -> &str {
        &self.action
    }

    /// The arguments the call gives, by name.
    pub fn arguments(&self) -> &BTreeMap<String, Value> {
        &self.arguments
    }

    /// The artifacts the call gives, by port, in the order they were given.
    pub fn inputs(&self) -> &BTreeMap<String, Vec<ArtifactId>> {
        &self.inputs
    }
}
