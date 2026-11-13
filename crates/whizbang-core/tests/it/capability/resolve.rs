//! Feature declarations and resolution (`SPEC.md` section 4, ADR 0006, "Resolution"): an
//! explicit choice, then the default; a backend that lacks a needed feature is an error;
//! a model is never substituted.

use whizbang_core::capability::{Declaration, Roster, RosterError, Unresolved, feature};
use whizbang_core::contract::{Backends, Capability, ModelIdentity};

fn declared(model: &str, features: &[&str]) -> Declaration {
    Declaration::new(ModelIdentity::new("fake", model, "1"), features).unwrap()
}

fn needs(features: &[&str]) -> Vec<String> {
    features.iter().map(|f| (*f).to_owned()).collect()
}

/// Two segmenters, the first the default; two fillers, the second the default.
fn roster() -> Roster {
    Roster::new()
        .offer(
            Capability::Segment,
            "sam3",
            declared("sam3", &[feature::PHRASE, feature::POINT, feature::BOX]),
        )
        .unwrap()
        .offer(Capability::Segment, "tiny", declared("tiny", &["phrase"]))
        .unwrap()
        .offer(Capability::Fill, "bria", declared("bria-erase", &[]))
        .unwrap()
        .offer(
            Capability::Fill,
            "flux",
            declared("flux-fill", &[feature::PROMPT]),
        )
        .unwrap()
        .default_for(Capability::Segment, "sam3")
        .unwrap()
        .default_for(Capability::Fill, "flux")
        .unwrap()
}

#[test]
fn a_declaration_holds_a_model_and_its_features() {
    let declaration = declared("sam3", &["phrase", "box"]);
    assert_eq!(declaration.model().model(), "sam3");
    assert!(declaration.has("phrase"));
    assert!(declaration.has("box"));
    assert!(!declaration.has("point"));
    assert_eq!(
        declaration.features().collect::<Vec<_>>(),
        ["box", "phrase"]
    );
    // A feature is a name, and is declared once.
    let model = ModelIdentity::new("fake", "m", "1");
    assert!(Declaration::new(model.clone(), &["Phrase"]).is_err());
    assert!(Declaration::new(model.clone(), &[""]).is_err());
    assert!(Declaration::new(model, &["phrase", "phrase"]).is_err());
}

#[test]
fn the_features_a_request_can_need_are_named_once() {
    assert_eq!(
        [
            feature::PHRASE,
            feature::POINT,
            feature::BOX,
            feature::PROMPT,
            feature::IMAGE,
            feature::TOOL
        ],
        ["phrase", "point", "box", "prompt", "image", "tool"]
    );
}

#[test]
fn an_explicit_choice_is_taken_before_the_default() {
    let roster = roster();
    let chosen = roster
        .resolve(Capability::Segment, Some("tiny"), &needs(&["phrase"]))
        .unwrap();
    assert_eq!(chosen.name(), "tiny");
    assert_eq!(chosen.declaration().model().model(), "tiny");
    let chosen = roster.resolve(Capability::Fill, Some("bria"), &[]).unwrap();
    assert_eq!(chosen.declaration().model().model(), "bria-erase");
    // The default, chosen by name, is the default.
    let chosen = roster.resolve(Capability::Fill, Some("flux"), &[]).unwrap();
    assert_eq!(chosen.name(), "flux");
}

#[test]
fn without_a_choice_the_default_answers() {
    let roster = roster();
    let default = roster
        .resolve(Capability::Segment, None, &needs(&["phrase", "box"]))
        .unwrap();
    assert_eq!(default.name(), "sam3");
    let default = roster.resolve(Capability::Fill, None, &[]).unwrap();
    assert_eq!(default.name(), "flux");
}

#[test]
fn a_needed_feature_that_is_missing_is_an_error_and_nothing_is_substituted() {
    let roster = roster();
    // The chosen segmenter lacks boxes. The default has them, and is not used.
    assert_eq!(
        roster
            .resolve(
                Capability::Segment,
                Some("tiny"),
                &needs(&["phrase", "box"])
            )
            .unwrap_err(),
        Unresolved::MissingFeature {
            capability: Capability::Segment,
            name: "tiny".to_owned(),
            feature: "box".to_owned(),
        }
    );
    // The default filler takes a prompt; the other does not, and choosing it for a call
    // that needs one is an error though the default would do.
    let error = roster
        .resolve(Capability::Fill, Some("bria"), &needs(&["prompt"]))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "the fill backend bria does not declare the feature prompt"
    );
    // The default lacking a feature is the same error: no other backend is looked for.
    let roster = roster.default_for(Capability::Segment, "tiny").unwrap();
    assert_eq!(
        roster
            .resolve(Capability::Segment, None, &needs(&["point"]))
            .unwrap_err(),
        Unresolved::MissingFeature {
            capability: Capability::Segment,
            name: "tiny".to_owned(),
            feature: "point".to_owned(),
        }
    );
    // The first feature missing, in the order they were needed, is the one named.
    let error = roster
        .resolve(
            Capability::Segment,
            None,
            &needs(&["box", "phrase", "point"]),
        )
        .unwrap_err();
    assert!(matches!(error, Unresolved::MissingFeature { feature, .. } if feature == "box"));
}

#[test]
fn a_choice_that_is_not_offered_is_an_error_and_the_default_is_not_used() {
    let roster = roster();
    let error = roster
        .resolve(Capability::Fill, Some("lama"), &[])
        .unwrap_err();
    assert_eq!(
        error,
        Unresolved::UnknownChoice {
            capability: Capability::Fill,
            choice: "lama".to_owned(),
        }
    );
    assert_eq!(error.to_string(), "no fill backend is named \"lama\"");
    // A name offered for another capability is not a choice for this one.
    assert!(matches!(
        roster.resolve(Capability::Fill, Some("sam3"), &[]),
        Err(Unresolved::UnknownChoice { .. })
    ));
    // What is shown of a choice is escaped, and its first 64 characters at most.
    let long = format!("{}\n", "x".repeat(100));
    let shown = roster
        .resolve(Capability::Fill, Some(&long), &[])
        .unwrap_err()
        .to_string();
    assert_eq!(
        shown,
        format!(
            "no fill backend is named \"{}\"... (101 characters)",
            "x".repeat(64)
        )
    );
    let shown = roster
        .resolve(Capability::Fill, Some("a\nb"), &[])
        .unwrap_err()
        .to_string();
    assert_eq!(shown, "no fill backend is named \"a\\nb\"");
}

#[test]
fn without_a_choice_and_without_a_default_nothing_answers() {
    // A judge is not offered at all.
    let roster = roster();
    assert_eq!(
        roster.resolve(Capability::Judge, None, &[]).unwrap_err(),
        Unresolved::NoDefault {
            capability: Capability::Judge
        }
    );
    // One backend offered and no default set: it is still not the default.
    let one = Roster::new()
        .offer(Capability::Judge, "claude", declared("judge", &[]))
        .unwrap();
    let error = one.resolve(Capability::Judge, None, &[]).unwrap_err();
    assert_eq!(
        error.to_string(),
        "no default backend is configured for judge, and the call chose none"
    );
    one.resolve(Capability::Judge, Some("claude"), &[]).unwrap();
}

#[test]
fn a_roster_is_built_only_of_names_that_can_be_chosen() {
    let offered = || {
        Roster::new()
            .offer(Capability::Fill, "bria", declared("bria-erase", &[]))
            .unwrap()
    };
    assert_eq!(
        offered()
            .offer(Capability::Fill, "bria", declared("other", &[]))
            .unwrap_err(),
        RosterError::Twice {
            capability: Capability::Fill,
            name: "bria".to_owned()
        }
    );
    // The same name under another capability is another entry.
    offered()
        .offer(Capability::Segment, "bria", declared("other", &[]))
        .unwrap();
    assert!(matches!(
        offered().offer(Capability::Fill, "Not A Name", declared("other", &[])),
        Err(RosterError::Name { .. })
    ));
    assert_eq!(
        offered().default_for(Capability::Fill, "lama").unwrap_err(),
        RosterError::NotOffered {
            capability: Capability::Fill,
            name: "lama".to_owned()
        }
    );
    assert!(matches!(
        offered().default_for(Capability::Segment, "bria"),
        Err(RosterError::NotOffered { .. })
    ));
}

#[test]
fn a_roster_answers_prepare_with_the_model_or_the_reason() {
    let roster = roster();
    let backends: &dyn Backends = &roster;
    assert_eq!(
        backends
            .resolve(Capability::Fill, Some("bria"), &[])
            .unwrap(),
        ModelIdentity::new("fake", "bria-erase", "1")
    );
    assert_eq!(
        backends.resolve(Capability::Fill, None, &[]).unwrap(),
        ModelIdentity::new("fake", "flux-fill", "1")
    );
    let unavailable = backends
        .resolve(Capability::Fill, Some("bria"), &needs(&["prompt"]))
        .unwrap_err();
    assert_eq!(
        unavailable.why(),
        "the fill backend bria does not declare the feature prompt"
    );
}
