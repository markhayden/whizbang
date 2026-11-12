//! What a descriptor must be before it exists: nothing reaches a registry, a description
//! or the reference without passing every rule here.

use whizbang_core::artifact::ArtifactType;
use whizbang_core::contract::{
    ActionDescriptor, Argument, Capability, Count, Draft, InputPort, Kind, OutputPort, PortRef,
    Requirement, Value, ValueType, When,
};

fn draft() -> Draft {
    ActionDescriptor::draft("fake", "1.0.0", Kind::Primitive, "A fake action.")
}

/// A draft with one image in, one optional argument and one mask out.
fn base() -> Draft {
    draft()
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image.",
        ))
        .argument(Argument::optional("filler", "A filler.", ValueType::Text))
        .output(
            OutputPort::one("mask", ArtifactType::Mask, "The mask.").on(PortRef::input("image")),
        )
}

/// The refusal of a draft, which must name the action and say `what`.
#[track_caller]
fn refused(draft: Draft, what: &str) {
    let message = match draft.build() {
        Ok(_) => panic!("accepted, where a refusal naming {what:?} was expected"),
        Err(error) => error.to_string(),
    };
    assert!(message.contains(what), "{message}");
}

#[test]
fn the_smallest_descriptor_has_a_name_a_version_a_kind_and_a_purpose() {
    let fake = draft().build().unwrap();
    assert_eq!(fake.name(), "fake");
    assert_eq!(fake.version(), "1.0.0");
    assert_eq!(fake.kind(), Kind::Primitive);
    assert_eq!(fake.purpose(), "A fake action.");
    assert!(fake.arguments().is_empty());
    assert!(fake.inputs().is_empty());
    assert!(fake.outputs().is_empty());
    assert!(fake.needs().is_empty());
    // An action does not run beside itself unless it says it may.
    assert!(!fake.concurrent());
    base().build().unwrap();
}

#[test]
fn a_name_that_breaks_the_rule_is_no_descriptor() {
    for name in ["", "Zebra", "9lives", "two words", "tick`", "new\nline"] {
        let draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A fake action.");
        assert!(draft.build().is_err(), "{name:?} was accepted");
    }
}

#[test]
fn a_purpose_that_breaks_the_rule_is_no_descriptor() {
    for purpose in [
        "",
        "# A heading",
        "A `tick`.",
        "Two\nlines.",
        "See www.example.com.",
    ] {
        let draft = ActionDescriptor::draft("fake", "1.0.0", Kind::Primitive, purpose);
        refused(draft, "purpose");
    }
    // The purpose of an argument and of a port is printed too, and held to the same rule.
    refused(
        draft().argument(Argument::optional("px", "A | bar.", ValueType::Text)),
        "px",
    );
    refused(
        draft().input(InputPort::required("image", ArtifactType::Image, "<b>")),
        "image",
    );
    refused(
        draft().output(OutputPort::one("mask", ArtifactType::Mask, "")),
        "mask",
    );
}

#[test]
fn a_version_that_is_not_three_numbers_is_refused() {
    let draft = ActionDescriptor::draft("fake", "1.2", Kind::Primitive, "A fake action.");
    refused(draft, "version");
}

#[test]
fn a_refusal_names_the_action() {
    let draft = ActionDescriptor::draft("fake", "one", Kind::Primitive, "A fake action.");
    refused(draft, "fake");
}

#[test]
fn names_are_unique_among_what_a_call_gives_and_among_what_it_returns() {
    refused(
        base().argument(Argument::optional("filler", "Again.", ValueType::Text)),
        "filler",
    );
    // An argument and an input port are given by one call under one set of names.
    refused(
        base().argument(Argument::optional("image", "A name.", ValueType::Text)),
        "image",
    );
    refused(
        base().input(InputPort::required("image", ArtifactType::Image, "Again.")),
        "image",
    );
    refused(
        base().output(OutputPort::one("mask", ArtifactType::Mask, "Again.")),
        "mask",
    );
    // An output may share a name with an input: they are never given together.
    base()
        .output(OutputPort::one("image", ArtifactType::Image, "A copy."))
        .build()
        .unwrap();
}

#[test]
fn a_field_name_holds_no_dash_and_no_capital() {
    refused(
        draft().argument(Argument::optional("down-px", "A radius.", ValueType::Text)),
        "down-px",
    );
    refused(
        draft().input(InputPort::required(
            "Image",
            ArtifactType::Image,
            "The image.",
        )),
        "Image",
    );
    refused(
        draft().output(OutputPort::one("the mask", ArtifactType::Mask, "The mask.")),
        "the mask",
    );
}

#[test]
fn an_argument_is_required_or_has_a_default_or_neither_and_never_both() {
    let number = ValueType::number(Some(0.0), Some(64.0));
    let px = Argument::with_default("px", "A radius.", number.clone(), Value::Number(8.0));
    let fake = draft().argument(px).build().unwrap();
    let px = fake.argument("px").unwrap();
    assert!(!px.is_required());
    assert_eq!(px.default(), Some(&Value::Number(8.0)));
    let required = Argument::required("px", "A radius.", number.clone());
    let fake = draft().argument(required).build().unwrap();
    assert!(fake.argument("px").unwrap().is_required());
    assert_eq!(fake.argument("px").unwrap().default(), None);
    // A default its own type refuses.
    for wrong in [
        Value::Number(65.0),
        Value::Number(-1.0),
        Value::Number(f64::NAN),
        Value::Integer(8),
        Value::text("8"),
    ] {
        refused(
            draft().argument(Argument::with_default(
                "px",
                "A radius.",
                number.clone(),
                wrong,
            )),
            "px",
        );
    }
}

#[test]
fn a_value_is_accepted_by_its_type_alone() {
    let integer = ValueType::integer(Some(1), Some(8));
    assert!(integer.accepts(&Value::Integer(1)).is_ok());
    assert!(integer.accepts(&Value::Integer(8)).is_ok());
    assert!(integer.accepts(&Value::Integer(9)).is_err());
    assert!(integer.accepts(&Value::Number(2.0)).is_err());
    let open = ValueType::number(None, None);
    assert!(open.accepts(&Value::Number(-1e300)).is_ok());
    assert!(open.accepts(&Value::Number(f64::INFINITY)).is_err());
    assert!(ValueType::Boolean.accepts(&Value::Boolean(true)).is_ok());
    assert!(ValueType::Boolean.accepts(&Value::text("true")).is_err());
    assert!(ValueType::Text.accepts(&Value::text("a phrase")).is_ok());
    assert!(ValueType::Text.accepts(&Value::Boolean(true)).is_err());
    let choice = ValueType::choice(&["flat", "equirectangular"]);
    assert!(choice.accepts(&Value::text("flat")).is_ok());
    assert!(choice.accepts(&Value::text("fisheye")).is_err());
}

#[test]
fn a_type_that_accepts_nothing_is_refused() {
    let argument =
        |value: ValueType| draft().argument(Argument::optional("px", "A radius.", value));
    refused(argument(ValueType::number(Some(2.0), Some(1.0))), "px");
    refused(argument(ValueType::number(Some(f64::NAN), None)), "px");
    refused(argument(ValueType::number(None, Some(f64::INFINITY))), "px");
    refused(argument(ValueType::integer(Some(2), Some(1))), "px");
    refused(argument(ValueType::choice(&[])), "px");
    refused(argument(ValueType::choice(&["flat", "flat"])), "px");
    refused(argument(ValueType::choice(&["Not a name"])), "px");
}

#[test]
fn an_input_is_required_optional_or_variadic() {
    let fake = draft()
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image.",
        ))
        .input(InputPort::optional("protect", ArtifactType::Mask, "What to keep.").on("image"))
        .input(InputPort::variadic(
            "plane",
            ArtifactType::Mask,
            "Each instance.",
            Count::at_least(1),
        ))
        .build()
        .unwrap();
    assert_eq!(fake.input("image").unwrap().count(), Count::exactly(1));
    assert_eq!(fake.input("protect").unwrap().count(), Count::between(0, 1));
    let plane = fake.input("plane").unwrap().count();
    assert_eq!((plane.min(), plane.max()), (1, None));
    // A port that can never be given, or needs more than it allows.
    refused(
        draft().input(InputPort::variadic(
            "plane",
            ArtifactType::Mask,
            "Each instance.",
            Count::between(0, 0),
        )),
        "plane",
    );
    refused(
        draft().input(InputPort::variadic(
            "plane",
            ArtifactType::Mask,
            "Each instance.",
            Count::between(3, 2),
        )),
        "plane",
    );
}

#[test]
fn an_input_belongs_to_an_image_the_same_call_gives() {
    let protect = || InputPort::optional("protect", ArtifactType::Mask, "What to keep.");
    refused(draft().input(protect().on("image")), "image");
    refused(draft().input(protect().on("protect")), "protect");
    // Not to a mask, and not to one image among several.
    refused(
        draft()
            .input(InputPort::required("other", ArtifactType::Mask, "A mask."))
            .input(protect().on("other")),
        "other",
    );
    refused(
        draft()
            .input(InputPort::variadic(
                "frames",
                ArtifactType::Image,
                "Images.",
                Count::at_least(1),
            ))
            .input(protect().on("frames")),
        "frames",
    );
}

#[test]
fn every_output_has_a_bound() {
    refused(
        base().output(OutputPort::counted(
            "found",
            ArtifactType::Location,
            "Each one.",
            Count::at_least(0),
        )),
        "found",
    );
    refused(
        base().output(OutputPort::counted(
            "found",
            ArtifactType::Location,
            "Each one.",
            Count::between(0, 0),
        )),
        "found",
    );
    refused(
        base().output(OutputPort::counted(
            "found",
            ArtifactType::Location,
            "Each one.",
            Count::between(9, 8),
        )),
        "found",
    );
}

#[test]
fn a_condition_names_an_argument_that_a_call_may_leave_out() {
    let result = |when: When| {
        base().output(OutputPort::one("result", ArtifactType::Image, "The result.").when(when))
    };
    result(When::given("filler")).build().unwrap();
    result(When::not_given("filler")).build().unwrap();
    refused(result(When::given("nothing")), "nothing");
    // An input port is not an argument.
    refused(result(When::given("image")), "image");
    // A required argument is always given, and one with a default always has a value.
    refused(
        result(When::given("prompt")).argument(Argument::required(
            "prompt",
            "Words.",
            ValueType::Text,
        )),
        "prompt",
    );
    refused(
        result(When::given("strict")).argument(Argument::with_default(
            "strict",
            "Strictly.",
            ValueType::Boolean,
            Value::Boolean(false),
        )),
        "strict",
    );
    refused(
        base().needs(Requirement::new(Capability::Fill).when(When::given("nothing"))),
        "nothing",
    );
}

#[test]
fn an_output_belongs_to_an_image_the_action_takes_or_returns() {
    let mask = |on: PortRef| {
        draft()
            .input(InputPort::required(
                "image",
                ArtifactType::Image,
                "The image.",
            ))
            .input(InputPort::required("old", ArtifactType::Mask, "A mask."))
            .output(OutputPort::one("canvas", ArtifactType::Image, "A canvas."))
            .output(OutputPort::one("plan", ArtifactType::Plan, "A plan."))
            .output(OutputPort::one("mask", ArtifactType::Mask, "The mask.").on(on))
    };
    mask(PortRef::input("image")).build().unwrap();
    mask(PortRef::output("canvas")).build().unwrap();
    refused(mask(PortRef::input("nothing")), "nothing");
    refused(mask(PortRef::output("nothing")), "nothing");
    refused(mask(PortRef::input("old")), "old");
    refused(mask(PortRef::output("plan")), "plan");
    refused(mask(PortRef::output("mask")), "mask");
}

#[test]
fn only_an_image_is_derived_and_from_another_image() {
    let result = |port: OutputPort| base().output(port);
    let image = || OutputPort::one("result", ArtifactType::Image, "The result.");
    result(image().derived_from(PortRef::input("image")))
        .build()
        .unwrap();
    refused(
        result(image().derived_from(PortRef::input("nothing"))),
        "nothing",
    );
    refused(
        result(image().derived_from(PortRef::output("mask"))),
        "mask",
    );
    refused(
        result(image().derived_from(PortRef::output("result"))),
        "result",
    );
    refused(
        result(
            OutputPort::one("plan", ArtifactType::Plan, "A plan.")
                .derived_from(PortRef::input("image")),
        ),
        "plan",
    );
    // Two images each derived from the other were derived from nothing.
    refused(
        result(image().derived_from(PortRef::output("again"))).output(
            OutputPort::one("again", ArtifactType::Image, "Again.")
                .derived_from(PortRef::output("result")),
        ),
        "itself",
    );
}

#[test]
fn a_mask_is_rebound_only_onto_an_image_derived_from_its_own() {
    let draft = |mask: OutputPort, result: OutputPort| {
        draft()
            .input(InputPort::required(
                "image",
                ArtifactType::Image,
                "The image.",
            ))
            .input(InputPort::required(
                "other",
                ArtifactType::Image,
                "Another.",
            ))
            .output(mask)
            .output(result)
    };
    let mask = || OutputPort::one("mask", ArtifactType::Mask, "The mask.");
    let result = || OutputPort::one("result", ArtifactType::Image, "The result.");
    let derived = || result().derived_from(PortRef::input("image"));
    draft(
        mask().on(PortRef::input("image")).rebound_onto("result"),
        derived(),
    )
    .build()
    .unwrap();
    // Onto nothing; onto an image that is no derivation; onto one derived from another
    // image; and a mask that does not say what it is on.
    refused(
        draft(
            mask().on(PortRef::input("image")).rebound_onto("nothing"),
            derived(),
        ),
        "nothing",
    );
    refused(
        draft(
            mask().on(PortRef::input("image")).rebound_onto("result"),
            result(),
        ),
        "result",
    );
    refused(
        draft(
            mask().on(PortRef::input("other")).rebound_onto("result"),
            derived(),
        ),
        "result",
    );
    refused(draft(mask().rebound_onto("result"), derived()), "mask");
    // Only a mask is rebound.
    refused(
        draft(
            OutputPort::one("where", ArtifactType::Location, "A place.")
                .on(PortRef::input("image"))
                .rebound_onto("result"),
            derived(),
        ),
        "where",
    );
}

#[test]
fn a_requirement_is_stated_once_and_its_features_are_names() {
    base()
        .needs(
            Requirement::new(Capability::Segment)
                .feature("phrase")
                .feature("box"),
        )
        .needs(Requirement::new(Capability::Fill).when(When::given("filler")))
        .needs(Requirement::new(Capability::Judge).when(When::not_given("filler")))
        .build()
        .unwrap();
    refused(
        base()
            .needs(Requirement::new(Capability::Fill))
            .needs(Requirement::new(Capability::Fill)),
        "fill",
    );
    refused(
        base().needs(
            Requirement::new(Capability::Segment)
                .feature("phrase")
                .feature("phrase"),
        ),
        "phrase",
    );
    refused(
        base().needs(Requirement::new(Capability::Segment).feature("A phrase")),
        "A phrase",
    );
}

#[test]
fn a_descriptor_as_data_holds_every_field_in_a_fixed_order() {
    let fake = base()
        .argument(Argument::with_default(
            "px",
            "A radius.",
            ValueType::number(Some(0.0), None),
            Value::Number(8.0),
        ))
        .needs(Requirement::new(Capability::Fill).when(When::given("filler")))
        .concurrent(true)
        .build()
        .unwrap();
    assert_eq!(
        serde_json::to_string(&fake).unwrap(),
        concat!(
            r#"{"name":"fake","version":"1.0.0","kind":"primitive","purpose":"A fake action.","#,
            r#""arguments":[{"name":"filler","purpose":"A filler.","type":{"type":"text"},"required":false,"default":null},"#,
            r#"{"name":"px","purpose":"A radius.","type":{"type":"number","min":0.0,"max":null},"required":false,"default":8.0}],"#,
            r#""inputs":[{"name":"image","artifact":"image","purpose":"The image.","count":{"min":1,"max":1},"belongs_to":null}],"#,
            r#""outputs":[{"name":"mask","artifact":"mask","purpose":"The mask.","count":{"min":1,"max":1},"when":"always","#,
            r#""belongs_to":{"input":"image"},"derived_from":null,"rebound_onto":null}],"#,
            r#""needs":[{"capability":"fill","features":[],"when":{"given":"filler"}}],"#,
            r#""concurrent":true}"#
        )
    );
}
