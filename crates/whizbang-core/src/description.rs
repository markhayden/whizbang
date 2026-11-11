//! The static description: what this build can do.
//!
//! It holds no runtime state (no installed models, credentials or reachability), so it is
//! identical on every machine for the same build. The reference documentation is generated
//! from it.

use serde::Serialize;

/// The version of the description's own shape.
pub const DESCRIPTION_VERSION: u32 = 1;

/// Everything a door can offer, as one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Description {
    pub description_version: u32,
    pub name: String,
    pub version: String,
    pub actions: Vec<ActionSummary>,
}

/// One action, as listed. The full descriptor arrives with the execution contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionSummary {
    pub name: String,
    pub purpose: String,
}

impl Description {
    /// The description of a build that registers the given actions, in the order given.
    pub fn new(version: &str, actions: Vec<ActionSummary>) -> Self {
        Self {
            description_version: DESCRIPTION_VERSION,
            name: "whizbang".to_owned(),
            version: version.to_owned(),
            actions,
        }
    }
}
