//! The static description: what a build registered, and the six artifact types.

use std::sync::Arc;

use whizbang_core::artifact::ArtifactType;
use whizbang_core::contract::{Action, ActionDescriptor, Kind, Registry, text};
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
    assert_eq!(description.description_version(), DESCRIPTION_VERSION);
    assert_eq!(DESCRIPTION_VERSION, 12);
    assert_eq!(description.name(), "whizbang");
    assert_eq!(description.version(), "1.2.3");
    let names: Vec<&str> = description.actions().iter().map(|a| a.name()).collect();
    assert_eq!(names, ["seam", "remove_prompt"]);
    assert_eq!(
        description.actions()[1],
        fixtures::remove_prompt().build().unwrap()
    );
    let value = serde_json::to_value(&description).unwrap();
    assert_eq!(value["actions"][1]["outputs"][2]["name"], "found");
    // Marks (version 7): only of actions it registers, in the order given; and (version
    // 12) of the tools it names as offered only when a run names them.
    assert!(description.marks().is_empty());
    let marks = [
        ("seam", "experimental"),
        ("nothing", "admitted"),
        ("a_tool_file", "experimental"),
    ];
    let marked = description.clone().with_marks(marks, &[]);
    let value = serde_json::to_value(&marked).unwrap();
    assert_eq!(
        value["marks"],
        serde_json::json!([{"tool": "seam", "mark": "experimental"}])
    );
    let marked = description.with_marks(marks, &["a_tool_file"]);
    let value = serde_json::to_value(&marked).unwrap();
    assert_eq!(
        value["marks"],
        serde_json::json!([
            {"tool": "seam", "mark": "experimental"},
            {"tool": "a_tool_file", "mark": "experimental"}
        ])
    );
}

#[test]
fn a_description_of_nothing_lists_no_actions_and_still_the_types() {
    let description = Description::new("0.0.1", &Registry::new());
    assert!(description.actions().is_empty());
    assert_eq!(description.artifact_types().len(), 6);
}

#[test]
fn a_description_says_of_each_artifact_type_what_it_is_and_how_it_travels() {
    let description = Description::new("0.0.1", &Registry::new());
    let names: Vec<&str> = description
        .artifact_types()
        .iter()
        .map(|summary| summary.name())
        .collect();
    let expected: Vec<&str> = ArtifactType::ALL.iter().map(|t| t.name()).collect();
    assert_eq!(names, expected);
    for summary in description.artifact_types() {
        // Printable as it is, like an action's purpose.
        text::purpose(summary.purpose()).unwrap_or_else(|why| panic!("{}: {why}", summary.name()));
        // ADR 0002: every type is one canonical file, and that file is how it travels.
        assert_eq!(summary.travels_as(), "canonical_file");
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

#[test]
fn a_description_lists_recipes_first_then_loops_then_primitives() {
    // Whatever order a door registered them in: recipes are what is advertised, and
    // primitives follow as building blocks. Within a kind, registration order is kept.
    let mut registry = Registry::new();
    for (name, kind) in [
        ("grow", Kind::Primitive),
        ("retry", Kind::Loop),
        ("seam", Kind::Recipe),
        ("view", Kind::Primitive),
        ("again", Kind::Loop),
        ("remove", Kind::Recipe),
    ] {
        let draft = ActionDescriptor::draft(name, "1.0.0", kind, "A fake action.");
        registry
            .register(Arc::new(Fake(draft.build().unwrap())))
            .unwrap();
    }
    let description = Description::new("0.0.1", &registry);
    let names: Vec<&str> = description.actions().iter().map(|a| a.name()).collect();
    assert_eq!(names, ["seam", "remove", "retry", "again", "grow", "view"]);
    // The registry itself keeps the order it was given.
    let registered: Vec<&str> = registry.descriptors().map(|d| d.name()).collect();
    assert_eq!(
        registered,
        ["grow", "retry", "seam", "view", "again", "remove"]
    );
}

#[test]
fn the_description_lists_findings_among_an_actions_results_and_which_actions_plan() {
    use whizbang_core::contract::{ActionDescriptor, Kind, ResultKind};
    // Description version 10 (plan m4, task K1, items 1 and 3).
    let draft = |name: &str| ActionDescriptor::draft(name, "1.0.0", Kind::Loop, "A test action.");
    let mut registry = registry();
    let finds = draft("finds")
        .results(ResultKind::Findings)
        .results(ResultKind::Verdict)
        .plans();
    for draft in [finds, draft("plain")] {
        registry
            .register(Arc::new(Fake(draft.build().unwrap())))
            .unwrap();
    }
    let description = Description::new("1.2.3", &registry);
    let value = serde_json::to_value(&description).unwrap();
    assert_eq!(value["description_version"], 12);
    let action = |name: &str| {
        let actions = value["actions"].as_array().unwrap();
        actions.iter().find(|a| a["name"] == name).unwrap().clone()
    };
    // Findings are listed among the results, in the order of the kinds; and an action
    // that plans says so.
    let finds = action("finds");
    assert_eq!(finds["results"], serde_json::json!(["verdict", "findings"]));
    assert_eq!(finds["plans"], true);
    assert_eq!(ResultKind::Findings.name(), "findings");
    // An action that plans nothing and hands back nothing says neither.
    for name in ["plain", "seam", "remove_prompt"] {
        let plain = action(name);
        let fields = plain.as_object().unwrap();
        assert!(!fields.contains_key("plans"), "{name}");
        assert!(!fields.contains_key("results"), "{name}");
    }
    let planners = description.actions().iter().filter(|a| a.plans());
    let planners: Vec<&str> = planners.map(|a| a.name()).collect();
    assert_eq!(planners, ["finds"]);
}
