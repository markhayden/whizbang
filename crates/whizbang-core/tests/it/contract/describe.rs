//! The static description: what a build registered, and the six artifact types.

use std::sync::Arc;

use whizbang_core::artifact::ArtifactType;
use whizbang_core::contract::{Action, ActionDescriptor, Registry, text};
use whizbang_core::description::{DESCRIPTION_VERSION, Description};

use super::fixtures;

struct Fake(ActionDescriptor);

impl Action for Fake {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }
}

fn registry() -> Registry {
    let mut registry = Registry::new();
    for draft in [fixtures::seam(), fixtures::remove_prompt()] {
        registry
            .register(Arc::new(Fake(draft.build().unwrap())))
            .unwrap();
    }
    registry
}

#[test]
fn a_description_lists_what_was_registered_in_order_and_in_full() {
    let description = Description::new("1.2.3", &registry());
    assert_eq!(description.description_version, DESCRIPTION_VERSION);
    assert_eq!(DESCRIPTION_VERSION, 2);
    assert_eq!(description.name, "whizbang");
    assert_eq!(description.version, "1.2.3");
    let names: Vec<&str> = description.actions.iter().map(|a| a.name()).collect();
    assert_eq!(names, ["seam", "remove_prompt"]);
    assert_eq!(
        description.actions[1],
        fixtures::remove_prompt().build().unwrap()
    );
    let value = serde_json::to_value(&description).unwrap();
    assert_eq!(value["actions"][1]["outputs"][2]["name"], "found");
}

#[test]
fn a_description_of_nothing_lists_no_actions_and_still_the_types() {
    let description = Description::new("0.0.1", &Registry::new());
    assert!(description.actions.is_empty());
    assert_eq!(description.artifact_types.len(), 6);
}

#[test]
fn a_description_says_of_each_artifact_type_what_it_is_and_how_it_travels() {
    let description = Description::new("0.0.1", &Registry::new());
    let names: Vec<&str> = description
        .artifact_types
        .iter()
        .map(|summary| summary.name.as_str())
        .collect();
    let expected: Vec<&str> = ArtifactType::ALL.iter().map(|t| t.name()).collect();
    assert_eq!(names, expected);
    for summary in &description.artifact_types {
        // Printable as it is, like an action's purpose.
        text::purpose(&summary.purpose).unwrap_or_else(|why| panic!("{}: {why}", summary.name));
        // ADR 0002: every type is one canonical file, and that file is how it travels.
        assert_eq!(summary.travels_as, "canonical_file");
    }
    let value = serde_json::to_value(&description).unwrap();
    assert_eq!(value["artifact_types"][0]["name"], "mask");
}

#[test]
fn a_description_is_the_same_bytes_every_time() {
    let first = serde_json::to_string_pretty(&Description::new("0.0.1", &registry())).unwrap();
    let second = serde_json::to_string_pretty(&Description::new("0.0.1", &registry())).unwrap();
    assert_eq!(first, second);
}
