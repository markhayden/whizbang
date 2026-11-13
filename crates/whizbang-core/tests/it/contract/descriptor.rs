//! What a descriptor must be before it exists: nothing reaches a registry, a description
//! or the reference without passing every rule here.

use std::collections::BTreeSet;

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

/// The refusal of a draft, which must say every one of `what`: the action, the argument
/// or the port at fault, and the reason.
#[track_caller]
fn refused(draft: Draft, what: &[&str]) {
    let message = match draft.build() {
        Ok(_) => panic!("accepted, where a refusal saying {what:?} was expected"),
        Err(error) => error.to_string(),
    };
    for part in what {
        assert!(message.contains(part), "{part:?} is not in: {message}");
    }
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
        refused(draft, &["the name", "must be a lower-case letter"]);
    }
}

#[test]
fn a_purpose_that_breaks_the_rule_is_no_descriptor() {
    for (purpose, why) in [
        ("", "must start with a letter"),
        ("# A heading", "must start with a letter"),
        ("A `tick`.", "holds '`'"),
        ("Two\nlines.", "holds '\\n'"),
        ("See www.example.com.", "a link"),
    ] {
        let draft = ActionDescriptor::draft("fake", "1.0.0", Kind::Primitive, purpose);
        refused(draft, &["the purpose", why]);
    }
    // The purpose of an argument and of a port is printed too, and held to the same rule.
    refused(
        draft().argument(Argument::optional("px", "A | bar.", ValueType::Text)),
        &["the argument px", "the purpose", "holds '|'"],
    );
    refused(
        draft().input(InputPort::required("image", ArtifactType::Image, "<b>")),
        &["the input image", "the purpose", "must start with a letter"],
    );
    refused(
        draft().output(OutputPort::one("mask", ArtifactType::Mask, "")),
        &["the output mask", "the purpose", "must start with a letter"],
    );
}

#[test]
fn a_version_that_is_not_three_numbers_is_refused() {
    let draft = ActionDescriptor::draft("fake", "1.2", Kind::Primitive, "A fake action.");
    refused(draft, &["the version", "three numbers"]);
    let draft = ActionDescriptor::draft("fake", "01.2.3", Kind::Primitive, "A fake action.");
    refused(draft, &["the version", "three numbers"]);
}

#[test]
fn a_refusal_names_the_action() {
    let draft = ActionDescriptor::draft("fake", "one", Kind::Primitive, "A fake action.");
    refused(draft, &["not an action descriptor (\"fake\")"]);
}

#[test]
fn names_are_unique_among_what_a_call_gives_and_among_what_it_returns() {
    refused(
        base().argument(Argument::optional("filler", "Again.", ValueType::Text)),
        &["the name filler", "declared twice"],
    );
    // An argument and an input port are given by one call under one set of names.
    refused(
        base().argument(Argument::optional("image", "A name.", ValueType::Text)),
        &["the name image", "declared twice"],
    );
    refused(
        base().input(InputPort::required("image", ArtifactType::Image, "Again.")),
        &["the name image", "declared twice"],
    );
    refused(
        base().output(OutputPort::one("mask", ArtifactType::Mask, "Again.")),
        &["the output mask", "declared twice"],
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
        &["the argument down-px", "must be a lower-case letter"],
    );
    refused(
        draft().input(InputPort::required(
            "Image",
            ArtifactType::Image,
            "The image.",
        )),
        &["the input Image", "must be a lower-case letter"],
    );
    refused(
        draft().output(OutputPort::one("the mask", ArtifactType::Mask, "The mask.")),
        &["the output the mask", "must be a lower-case letter"],
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
    // A default at either bound is inside the bounds.
    for edge in [0.0, 64.0] {
        draft()
            .argument(Argument::with_default(
                "px",
                "A radius.",
                number.clone(),
                Value::Number(edge),
            ))
            .build()
            .unwrap();
    }
    // A default its own type refuses.
    for (wrong, why) in [
        (Value::Number(65.0), "is outside 0 to 64"),
        (Value::Number(-1.0), "is outside 0 to 64"),
        (Value::Number(f64::NAN), "is not a finite number"),
        (
            Value::Integer(8),
            "a whole number where a number is required",
        ),
        (Value::text("8"), "a text where a number is required"),
    ] {
        refused(
            draft().argument(Argument::with_default(
                "px",
                "A radius.",
                number.clone(),
                wrong,
            )),
            &["the argument px", "its default is refused by its type", why],
        );
    }
}

#[test]
fn a_text_default_is_one_plain_line() {
    let text = |default: &str| {
        draft().argument(Argument::with_default(
            "prompt",
            "Words.",
            ValueType::Text,
            Value::text(default),
        ))
    };
    text("A person, a dog; 2 cars.").build().unwrap();
    text(&"A".repeat(200)).build().unwrap();
    for (default, why) in [
        ("", "must start with a letter"),
        ("Two\nlines.", "holds '\\n'"),
        ("A <b>tag</b>.", "holds '<'"),
        ("See https://example.com.", "a link"),
    ] {
        refused(
            text(default),
            &["the argument prompt", "its default is no plain line", why],
        );
    }
    refused(
        text(&"A".repeat(100_000)),
        &[
            "the argument prompt",
            "its default is no plain line",
            "longer than 200",
        ],
    );
}

#[test]
fn a_value_is_accepted_by_its_type_alone() {
    let integer = ValueType::integer(Some(1), Some(8));
    assert!(integer.accepts(&Value::Integer(1)).is_ok());
    assert!(integer.accepts(&Value::Integer(8)).is_ok());
    assert!(integer.accepts(&Value::Integer(0)).is_err());
    assert!(integer.accepts(&Value::Integer(9)).is_err());
    assert!(integer.accepts(&Value::Number(2.0)).is_err());
    // A bound is inclusive: the number exactly at it is accepted, and the next is not.
    let number = ValueType::number(Some(0.5), Some(2.5));
    assert!(number.accepts(&Value::Number(0.5)).is_ok());
    assert!(number.accepts(&Value::Number(2.5)).is_ok());
    assert!(number.accepts(&Value::Number(0.499)).is_err());
    assert!(number.accepts(&Value::Number(2.501)).is_err());
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
    let above = "its lower bound";
    refused(
        argument(ValueType::number(Some(2.0), Some(1.0))),
        &["the argument px", above],
    );
    refused(
        argument(ValueType::number(Some(f64::NAN), None)),
        &["the argument px", "a bound is not a finite number"],
    );
    refused(
        argument(ValueType::number(None, Some(f64::INFINITY))),
        &["the argument px", "a bound is not a finite number"],
    );
    refused(
        argument(ValueType::integer(Some(2), Some(1))),
        &["the argument px", above],
    );
    refused(
        argument(ValueType::choice(&[])),
        &["the argument px", "offers no choice"],
    );
    refused(
        argument(ValueType::choice(&["flat", "flat"])),
        &["the argument px", "offers flat twice"],
    );
    refused(
        argument(ValueType::choice(&["Not a name"])),
        &["the argument px", "must be a lower-case letter"],
    );
    // A type whose bounds meet accepts one value, and that is something.
    argument(ValueType::number(Some(1.0), Some(1.0)))
        .build()
        .unwrap();
    argument(ValueType::integer(Some(1), Some(1)))
        .build()
        .unwrap();
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
    let plane = |count: Count| {
        draft().input(InputPort::variadic(
            "plane",
            ArtifactType::Mask,
            "Each instance.",
            count,
        ))
    };
    refused(
        plane(Count::between(0, 0)),
        &["the input plane", "can never hold anything"],
    );
    refused(
        plane(Count::between(3, 2)),
        &["the input plane", "needs 3 and allows 2"],
    );
}

#[test]
fn an_input_belongs_to_an_image_the_same_call_gives() {
    let protect = || InputPort::optional("protect", ArtifactType::Mask, "What to keep.");
    let no_image = "which is no input that takes exactly one image";
    refused(
        draft().input(protect().on("image")),
        &["the input protect", "image", no_image],
    );
    // Not to itself, not to a mask, and not to one image among several.
    refused(
        draft().input(protect().on("protect")),
        &["the input protect", no_image],
    );
    refused(
        draft()
            .input(InputPort::required("other", ArtifactType::Mask, "A mask."))
            .input(protect().on("other")),
        &["the input protect", "other", no_image],
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
        &["the input protect", "frames", no_image],
    );
}

#[test]
fn an_image_belongs_to_no_other_image() {
    // An image is what the others belong to (ADR 0002). So no port belongs to itself, and
    // no two ports belong to each other.
    let image = |name: &str| InputPort::required(name, ArtifactType::Image, "An image.");
    let why = "is an image, and an image belongs to no other";
    refused(
        draft().input(image("left").on("left")),
        &["the input left", why],
    );
    refused(
        draft()
            .input(image("left").on("right"))
            .input(image("right").on("left")),
        &["the input left", why],
    );
    let picture = |name: &str| OutputPort::one(name, ArtifactType::Image, "An image.");
    refused(
        draft().output(picture("left").on(PortRef::output("left"))),
        &["the output left", why],
    );
    refused(
        draft()
            .output(picture("left").on(PortRef::output("right")))
            .output(picture("right").on(PortRef::output("left"))),
        &["the output left", why],
    );
    refused(
        base().output(picture("copy").on(PortRef::input("image"))),
        &["the output copy", why],
    );
}

#[test]
fn every_output_has_a_bound() {
    let found = |count: Count| {
        base().output(OutputPort::counted(
            "found",
            ArtifactType::Location,
            "Each one.",
            count,
        ))
    };
    refused(
        found(Count::at_least(0)),
        &["the output found", "has no upper bound"],
    );
    refused(
        found(Count::between(0, 0)),
        &["the output found", "can never hold anything"],
    );
    refused(
        found(Count::between(9, 8)),
        &["the output found", "needs 9 and allows 8"],
    );
}

#[test]
fn a_condition_names_an_argument_that_a_call_may_leave_out() {
    let result = |when: When| {
        base().output(OutputPort::one("result", ArtifactType::Image, "The result.").when(when))
    };
    result(When::given("filler")).build().unwrap();
    result(When::not_given("filler")).build().unwrap();
    let none = "which is no argument";
    refused(
        result(When::given("nothing")),
        &["the output result", "nothing", none],
    );
    // An input port is not an argument.
    refused(
        result(When::given("image")),
        &["the output result", "image", none],
    );
    // A required argument is always given, and one with a default always has a value.
    let always = "which every call has a value for";
    refused(
        result(When::given("prompt")).argument(Argument::required(
            "prompt",
            "Words.",
            ValueType::Text,
        )),
        &["the output result", "prompt", always],
    );
    refused(
        result(When::given("strict")).argument(Argument::with_default(
            "strict",
            "Strictly.",
            ValueType::Boolean,
            Value::Boolean(false),
        )),
        &["the output result", "strict", always],
    );
    refused(
        base().needs(Requirement::new(Capability::Fill).when(When::given("nothing"))),
        &["the need for fill", "nothing", none],
    );
}

#[test]
fn a_condition_holds_by_the_arguments_a_call_gives() {
    let given: BTreeSet<String> = ["filler".to_owned()].into();
    let nothing = BTreeSet::new();
    assert!(When::Always.holds(&given) && When::Always.holds(&nothing));
    assert!(When::given("filler").holds(&given));
    assert!(!When::given("filler").holds(&nothing));
    assert!(!When::not_given("filler").holds(&given));
    assert!(When::not_given("filler").holds(&nothing));
    // And so do the outputs and the needs that depend on one.
    let fake = base()
        .output(
            OutputPort::one("rendering", ArtifactType::Image, "A picture of the mask.")
                .when(When::not_given("filler")),
        )
        .needs(Requirement::new(Capability::Judge).when(When::not_given("filler")))
        .build()
        .unwrap();
    let names = |given: &BTreeSet<String>| -> Vec<&str> {
        fake.outputs_for(given)
            .iter()
            .map(|port| port.name())
            .collect()
    };
    assert_eq!(names(&nothing), ["mask", "rendering"]);
    assert_eq!(names(&given), ["mask"]);
    assert_eq!(fake.needs_for(&nothing).len(), 1);
    assert!(fake.needs_for(&given).is_empty());
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
    let input = "which is no input that takes exactly one image";
    let output = "which is no output that returns an image";
    refused(
        mask(PortRef::input("nothing")),
        &["the output mask", "nothing", input],
    );
    refused(
        mask(PortRef::output("nothing")),
        &["the output mask", "nothing", output],
    );
    refused(
        mask(PortRef::input("old")),
        &["the output mask", "the input old", input],
    );
    refused(
        mask(PortRef::output("plan")),
        &["the output mask", "the output plan", output],
    );
    // Not to itself.
    refused(
        mask(PortRef::output("mask")),
        &["the output mask", "the output mask", output],
    );
}

#[test]
fn the_image_an_output_names_is_one_image_returned_whenever_the_output_is() {
    let mask = || OutputPort::one("mask", ArtifactType::Mask, "The mask.");
    // One of several images is not "the image".
    refused(
        base()
            .output(OutputPort::counted(
                "tiles",
                ArtifactType::Image,
                "Tiles.",
                Count::between(0, 8),
            ))
            .output(
                OutputPort::one("where", ArtifactType::Location, "A place.")
                    .on(PortRef::output("tiles")),
            ),
        &[
            "the output where",
            "the output tiles",
            "which does not return exactly one image",
        ],
    );
    // An image returned only for some calls, under an output returned for all of them.
    let result = || {
        OutputPort::one("result", ArtifactType::Image, "The result.").when(When::given("filler"))
    };
    let sometimes = "which is not returned for every call that this is";
    refused(
        base().output(result()).output(
            OutputPort::one("where", ArtifactType::Location, "A place.")
                .on(PortRef::output("result")),
        ),
        &["the output where", "the output result", sometimes],
    );
    refused(
        base().output(result()).output(
            OutputPort::one("again", ArtifactType::Image, "Again.")
                .derived_from(PortRef::output("result")),
        ),
        &["the output again", "the output result", sometimes],
    );
    // Under the same condition, the image is there whenever what names it is.
    base()
        .output(result())
        .output(
            OutputPort::one("where", ArtifactType::Location, "A place.")
                .on(PortRef::output("result"))
                .when(When::given("filler")),
        )
        .build()
        .unwrap();
    // An image returned for every call is there for an output returned only for some.
    base()
        .output(OutputPort::one("canvas", ArtifactType::Image, "A canvas."))
        .output(
            OutputPort::one("where", ArtifactType::Location, "A place.")
                .on(PortRef::output("canvas"))
                .when(When::given("filler")),
        )
        .build()
        .unwrap();
    // A rebind is the one that waits for its image: it happens when the image is returned.
    draft()
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image.",
        ))
        .argument(Argument::optional("filler", "A filler.", ValueType::Text))
        .output(mask().on(PortRef::input("image")).rebound_onto("result"))
        .output(result().derived_from(PortRef::input("image")))
        .build()
        .unwrap();
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
        &[
            "the output result",
            "nothing",
            "which is no input that takes exactly one image",
        ],
    );
    refused(
        result(image().derived_from(PortRef::output("mask"))),
        &[
            "the output result",
            "the output mask",
            "which is no output that returns an image",
        ],
    );
    refused(
        result(image().derived_from(PortRef::output("result"))),
        &["the output result", "from itself"],
    );
    refused(
        result(
            OutputPort::one("plan", ArtifactType::Plan, "A plan.")
                .derived_from(PortRef::input("image")),
        ),
        &["the output plan", "only an image is derived from another"],
    );
    // Two images each derived from the other were derived from nothing.
    refused(
        result(image().derived_from(PortRef::output("again"))).output(
            OutputPort::one("again", ArtifactType::Image, "Again.")
                .derived_from(PortRef::output("result")),
        ),
        &["the output result", "from itself"],
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
    let no_derivation = "which is no output image derived from the input";
    refused(
        draft(
            mask().on(PortRef::input("image")).rebound_onto("nothing"),
            derived(),
        ),
        &["the output mask", "onto nothing", no_derivation],
    );
    refused(
        draft(
            mask().on(PortRef::input("image")).rebound_onto("result"),
            result(),
        ),
        &["the output mask", "onto result", no_derivation],
    );
    refused(
        draft(
            mask().on(PortRef::input("other")).rebound_onto("result"),
            derived(),
        ),
        &[
            "the output mask",
            "onto result",
            "derived from the input other",
        ],
    );
    refused(
        draft(mask().rebound_onto("result"), derived()),
        &["the output mask", "does not say which image it is on"],
    );
    // Only a mask is rebound.
    refused(
        draft(
            OutputPort::one("where", ArtifactType::Location, "A place.")
                .on(PortRef::input("image"))
                .rebound_onto("result"),
            derived(),
        ),
        &["the output where", "only a mask is rebound"],
    );
    // And onto one image, not one of several.
    refused(
        draft(
            mask().on(PortRef::input("image")).rebound_onto("result"),
            OutputPort::counted(
                "result",
                ArtifactType::Image,
                "Results.",
                Count::between(1, 2),
            )
            .derived_from(PortRef::input("image")),
        ),
        &[
            "the output mask",
            "onto result",
            "which does not return exactly one image",
        ],
    );
}

#[test]
fn a_capability_is_needed_once_by_any_one_call() {
    base()
        .needs(
            Requirement::new(Capability::Segment)
                .feature("phrase")
                .feature("box"),
        )
        .needs(Requirement::new(Capability::Fill).when(When::given("filler")))
        .build()
        .unwrap();
    // Given and not given are never both true, so the two may ask for different features.
    base()
        .needs(
            Requirement::new(Capability::Judge)
                .feature("image")
                .when(When::given("filler")),
        )
        .needs(Requirement::new(Capability::Judge).when(When::not_given("filler")))
        .build()
        .unwrap();
    let twice = "is stated more than once for calls that can be the same";
    refused(
        base()
            .needs(Requirement::new(Capability::Fill))
            .needs(Requirement::new(Capability::Fill)),
        &["the need for fill", twice],
    );
    // Always, and also when an argument is given: a call that gives it needs it twice.
    refused(
        base()
            .needs(Requirement::new(Capability::Fill))
            .needs(Requirement::new(Capability::Fill).when(When::given("filler"))),
        &["the need for fill", twice],
    );
    // Two different arguments can both be given.
    refused(
        base()
            .argument(Argument::optional("model", "A model.", ValueType::Text))
            .needs(Requirement::new(Capability::Fill).when(When::given("filler")))
            .needs(Requirement::new(Capability::Fill).when(When::given("model"))),
        &["the need for fill", twice],
    );
    refused(
        base()
            .needs(Requirement::new(Capability::Fill).when(When::given("filler")))
            .needs(Requirement::new(Capability::Fill).when(When::given("filler"))),
        &["the need for fill", twice],
    );
    // One given and another left out can both be true of one call, too.
    refused(
        base()
            .argument(Argument::optional("model", "A model.", ValueType::Text))
            .needs(Requirement::new(Capability::Fill).when(When::given("filler")))
            .needs(Requirement::new(Capability::Fill).when(When::not_given("model"))),
        &["the need for fill", twice],
    );
    refused(
        base().needs(
            Requirement::new(Capability::Segment)
                .feature("phrase")
                .feature("phrase"),
        ),
        &["the need for segment", "names the feature phrase twice"],
    );
    refused(
        base().needs(Requirement::new(Capability::Segment).feature("A phrase")),
        &["a feature of segment", "must be a lower-case letter"],
    );
}

#[test]
fn a_need_can_name_the_argument_that_chooses_its_backend() {
    // The backend of a capability is the call's choice, or else the default (ADR 0006).
    // A need says which argument is that choice; without one, the default always
    // answers.
    let fake = base()
        .needs(
            Requirement::new(Capability::Fill)
                .when(When::given("filler"))
                .chosen_by("filler"),
        )
        .needs(Requirement::new(Capability::Segment))
        .build()
        .unwrap();
    assert_eq!(fake.needs()[0].chooser(), Some("filler"));
    assert_eq!(fake.needs()[1].chooser(), None);
    // The need may hold whether or not the choice is given: then the default answers.
    base()
        .needs(Requirement::new(Capability::Fill).chosen_by("filler"))
        .build()
        .unwrap();

    refused(
        base().needs(Requirement::new(Capability::Fill).chosen_by("nothing")),
        &[
            "the need for fill",
            "is chosen by nothing, which is no argument",
        ],
    );
    // A choice of backend is a text a call may leave out. What is on offer is the
    // machine's state, so it is not a choice among names the descriptor lists.
    let not_a_choice = "which is not a text that a call may leave out";
    let with = |argument: Argument| {
        base()
            .argument(argument)
            .needs(Requirement::new(Capability::Fill).chosen_by("which"))
    };
    for argument in [
        Argument::optional("which", "Which.", ValueType::number(None, None)),
        Argument::optional("which", "Which.", ValueType::Boolean),
        Argument::optional("which", "Which.", ValueType::choice(&["a", "b"])),
        Argument::required("which", "Which.", ValueType::Text),
        Argument::with_default("which", "Which.", ValueType::Text, Value::text("a")),
    ] {
        refused(
            with(argument),
            &["the need for fill", "is chosen by which", not_a_choice],
        );
    }
    with(Argument::optional("which", "Which.", ValueType::Text))
        .build()
        .unwrap();
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
        .needs(
            Requirement::new(Capability::Fill)
                .when(When::given("filler"))
                .chosen_by("filler"),
        )
        .needs(Requirement::new(Capability::Segment).feature("phrase"))
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
            r#""needs":[{"capability":"fill","features":[],"when":{"given":"filler"},"chosen_by":"filler"},"#,
            r#"{"capability":"segment","features":["phrase"],"when":"always","chosen_by":null}],"#,
            r#""concurrent":true}"#
        )
    );
}
