//! The two hard cases, on paper: descriptors for rascal's `seam` and `remove_prompt`,
//! written from their recipes (`crates/rascal-mask/recipes/seam.toml` and
//! `remove-prompt.toml`) and from the properties every rascal action took beside its
//! recipe's inputs (`crates/rascal-mask/src/describe.rs`, `common_properties`), at the
//! commit `docs/sources.toml` pins. Neither recipe is ported here: the fixtures show that
//! the descriptor can say what they need.
//!
//! What the fixtures leave out of the recipes and the task descriptions, each with its
//! reason and its row on `docs/carry-over.md`:
//!
//! - `set`, the step overrides, and `max_spend_usd`, two of the five common properties:
//!   the first belongs to the recipe runner and the second to the call's spend cap.
//! - A recipe output that passes an input through (`image = "inputs.image"`): an action's
//!   outputs are what it commits.
//! - The `render` output of both recipes, the grey rendering of the mask.
//! - `remove_prompt`'s `check` argument, a boolean that defaults to true, and the verdict
//!   the action then answers with. With it, a judge is needed when the argument's value
//!   is true and a checker is configured, which a condition here cannot say: a condition
//!   is "this argument is given, or it is not".
//! - The minimum length of 1 on `prompt`: a text has no bounds.
//! - `model` as a choice among the installed segmenters: what is installed is the
//!   machine's state, and a descriptor holds none. It is a text here.

use std::collections::BTreeSet;

use whizbang_core::artifact::ArtifactType;
use whizbang_core::contract::{
    ActionDescriptor, Argument, Capability, Count, Draft, InputPort, Kind, OutputPort, PortRef,
    Requirement, Value, ValueType, When,
};

/// The three common properties that are arguments: the optional filler, the projection
/// with the recipe's own default, and `strict`.
fn common(draft: Draft, projection: Option<&str>) -> Draft {
    let projections = ValueType::choice(&["flat", "equirectangular"]);
    draft
        .argument(Argument::optional(
            "filler",
            "A filler to paint the mask; without one the run returns the mask alone.",
            ValueType::Text,
        ))
        .argument(match projection {
            Some(declared) => Argument::with_default(
                "projection",
                "The projection of the image.",
                projections,
                Value::text(declared),
            ),
            None => Argument::optional(
                "projection",
                "The projection of the image; without it, the one the image states.",
                projections,
            ),
        })
        .argument(Argument::with_default(
            "strict",
            "Fail when the recipe stops at a step that is not applicable.",
            ValueType::Boolean,
            Value::Boolean(false),
        ))
}

pub fn seam() -> Draft {
    let draft = ActionDescriptor::draft(
        "seam",
        "1.0.0",
        Kind::Recipe,
        "Masks the wrap edge of a panorama as a band with a tone ramp across it; with a filler, heals it.",
    )
    .input(InputPort::required(
        "image",
        ArtifactType::Image,
        "The panorama whose wrap edge is masked.",
    ));
    common(draft, Some("equirectangular"))
        .output(OutputPort::one(
            "image",
            ArtifactType::Image,
            "The canvas cut from the panorama, with the wrap edge at its centre column.",
        ))
        .output(
            OutputPort::one("mask", ArtifactType::Mask, "The band over the wrap edge.")
                .on(PortRef::output("image")),
        )
        .output(OutputPort::one(
            "plan",
            ArtifactType::Plan,
            "How the canvas maps back onto the panorama.",
        ))
        .output(
            OutputPort::one(
                "result",
                ArtifactType::Image,
                "The panorama with its edge healed.",
            )
            .when(When::given("filler")),
        )
        .needs(Requirement::new(Capability::Fill).when(When::given("filler")))
        .concurrent(true)
}

pub fn remove_prompt() -> Draft {
    let draft = ActionDescriptor::draft(
        "remove_prompt",
        "1.0.0",
        Kind::Recipe,
        "Finds the objects a text prompt names and masks them with their shadows; with a filler, removes them.",
    )
    .input(InputPort::required(
        "image",
        ArtifactType::Image,
        "The image the objects are in.",
    ))
    .input(
        InputPort::optional(
            "protect",
            ArtifactType::Mask,
            "What the mask is kept off.",
        )
        .on("image"),
    )
    .argument(Argument::required(
        "prompt",
        "What to find, in words.",
        ValueType::Text,
    ))
    .argument(Argument::optional(
        "model",
        "The segmenter to use; without it, the configured default.",
        ValueType::Text,
    ));
    common(draft, None)
        .output(
            OutputPort::one("mask", ArtifactType::Mask, "The mask over what was found.")
                .on(PortRef::input("image"))
                .rebound_onto("result"),
        )
        .output(
            OutputPort::one(
                "result",
                ArtifactType::Image,
                "The image with what was found painted out.",
            )
            .when(When::given("filler"))
            .derived_from(PortRef::input("image")),
        )
        .output(
            OutputPort::counted(
                "found",
                ArtifactType::Location,
                "Where each instance the segmenter found is.",
                Count::between(0, 8),
            )
            .on(PortRef::input("image")),
        )
        .needs(Requirement::new(Capability::Segment).feature("phrase"))
        .needs(Requirement::new(Capability::Fill).when(When::given("filler")))
        .concurrent(true)
}

fn given(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn both_fixtures_validate() {
    seam().build().unwrap();
    remove_prompt().build().unwrap();
}

#[test]
fn seam_has_several_outputs_of_different_types() {
    let seam = seam().build().unwrap();
    let types: Vec<(&str, ArtifactType)> = seam
        .outputs()
        .iter()
        .map(|port| (port.name(), port.artifact()))
        .collect();
    assert_eq!(
        types,
        [
            ("image", ArtifactType::Image),
            ("mask", ArtifactType::Mask),
            ("plan", ArtifactType::Plan),
            ("result", ArtifactType::Image),
        ]
    );
    // The projection the recipe declares is the argument's default.
    let projection = seam.argument("projection").unwrap();
    assert_eq!(projection.default(), Some(&Value::text("equirectangular")));
}

#[test]
fn remove_prompt_has_an_optional_protect_input() {
    let remove = remove_prompt().build().unwrap();
    let protect = remove.input("protect").unwrap();
    assert_eq!(protect.artifact(), ArtifactType::Mask);
    assert_eq!(protect.count(), Count::between(0, 1));
    assert_eq!(protect.belongs_to(), Some("image"));
    assert_eq!(remove.input("image").unwrap().count(), Count::exactly(1));
}

#[test]
fn remove_prompt_returns_a_bounded_number_of_instances() {
    let remove = remove_prompt().build().unwrap();
    let found = remove.output("found").unwrap();
    assert_eq!(found.artifact(), ArtifactType::Location);
    assert_eq!(found.count(), Count::between(0, 8));
}

#[test]
fn remove_prompt_needs_a_filler_only_when_one_is_chosen() {
    let remove = remove_prompt().build().unwrap();
    let capabilities = |arguments: &[&str]| -> Vec<Capability> {
        remove
            .needs_for(&given(arguments))
            .iter()
            .map(|requirement| requirement.capability())
            .collect()
    };
    assert_eq!(capabilities(&["prompt"]), [Capability::Segment]);
    assert_eq!(
        capabilities(&["prompt", "filler"]),
        [Capability::Segment, Capability::Fill]
    );
    // And the features travel with the capability.
    let needs = remove.needs_for(&given(&["prompt"]));
    assert_eq!(needs[0].features(), ["phrase".to_owned()]);
    // The mask alone: no composite is returned. With a filler it is.
    let outputs = |arguments: &[&str]| -> Vec<&str> {
        remove
            .outputs_for(&given(arguments))
            .iter()
            .map(|port| port.name())
            .collect()
    };
    assert_eq!(outputs(&["prompt"]), ["mask", "found"]);
    assert_eq!(outputs(&["prompt", "filler"]), ["mask", "result", "found"]);
}

#[test]
fn seam_needs_no_capability_for_the_mask_alone() {
    let seam = seam().build().unwrap();
    assert!(seam.needs_for(&given(&[])).is_empty());
    assert_eq!(seam.needs_for(&given(&["filler"])).len(), 1);
}

#[test]
fn remove_prompt_rebinds_its_mask_onto_the_composite() {
    let remove = remove_prompt().build().unwrap();
    let mask = remove.output("mask").unwrap();
    assert_eq!(mask.belongs_to(), Some(&PortRef::input("image")));
    assert_eq!(mask.rebinds_onto(), Some("result"));
    // The composite says what it was derived from: the image the mask was made on.
    let result = remove.output("result").unwrap();
    assert_eq!(result.parent(), Some(&PortRef::input("image")));
    assert_eq!(result.applies(), &When::given("filler"));
}

#[test]
fn a_fixture_reads_the_same_as_data_every_time() {
    let first = serde_json::to_string(&remove_prompt().build().unwrap()).unwrap();
    let second = serde_json::to_string(&remove_prompt().build().unwrap()).unwrap();
    assert_eq!(first, second);
    let value: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert_eq!(value["name"], "remove_prompt");
    assert_eq!(value["kind"], "recipe");
    assert_eq!(value["needs"][1]["capability"], "fill");
    assert_eq!(value["needs"][1]["when"]["given"], "filler");
    assert_eq!(value["needs"][0]["when"], "always");
    assert_eq!(value["outputs"][2]["count"]["max"], 8);
    assert_eq!(value["outputs"][0]["rebound_onto"], "result");
    assert_eq!(value["outputs"][1]["derived_from"]["input"], "image");
    assert_eq!(value["inputs"][1]["artifact"], "mask");
}
