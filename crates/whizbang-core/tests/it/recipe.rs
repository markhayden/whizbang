//! The format of a recipe, `recipe/1` (plan m3, task R1, item 1; ADR 0033): read as rascal
//! read its files, and checked against the build's actions before anything runs, every
//! refusal naming the recipe and the step. The actions here are fakes over masks, which
//! the tests of the runner (`runner`) run too.

use std::sync::Arc;

use whizbang_core::artifact::{self, ArtifactType, Mask, Step as Made};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Capability, Context, Count, Failure, InputPort, Kind,
    OutputPort, PreparedCall, Produced, Registry, Requirement, Value, ValueType, When,
};
use whizbang_core::identity::DecodeLimits;
use whizbang_core::recipe::{Binding, Declared, Param, Recipe, RecipeAction, RecipeError, Written};

pub type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

/// An action whose run is what the test says.
pub struct Fake(pub ActionDescriptor, pub Body);

impl Action for Fake {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(call, context)
    }
}

/// The mask given at a port, read from the run's store.
pub fn mask_at(call: &PreparedCall, context: &Context, port: &str) -> Option<Mask> {
    let input = call.inputs().iter().find(|input| input.port() == port)?;
    let read = artifact::get(
        context.store(),
        ArtifactType::Mask,
        input.id(),
        &DecodeLimits::default(),
    );
    Some(read.unwrap().into_mask().unwrap())
}

fn integer(call: &PreparedCall, name: &str) -> Option<i64> {
    call.arguments().iter().find_map(|(n, value)| match value {
        Value::Integer(number) if n == name => Some(*number),
        _ => None,
    })
}

/// `mark`: sets the core's pixel `x` of the mask at `mask`, and protects the core of the
/// mask at `other` when one is given. It takes a note, and a constant set.
pub fn mark() -> Arc<dyn Action> {
    let descriptor = ActionDescriptor::draft("mark", "1.0.0", Kind::Primitive, "Marks a pixel.")
        .argument(Argument::required(
            "x",
            "The pixel.",
            ValueType::integer(Some(0), Some(47)),
        ))
        .argument(Argument::optional("note", "A note.", ValueType::Text))
        .argument(Argument::with_default(
            "constants",
            "The constant set.",
            ValueType::choice(&["mark-v1", "mark-v2"]),
            Value::text("mark-v1"),
        ))
        .input(InputPort::required("mask", ArtifactType::Mask, "The mask."))
        .input(InputPort::optional(
            "other",
            ArtifactType::Mask,
            "A mask to protect.",
        ))
        .output(OutputPort::one(
            "mask",
            ArtifactType::Mask,
            "The marked mask.",
        ))
        .concurrent(true)
        .build()
        .unwrap();
    Arc::new(Fake(
        descriptor,
        Box::new(|call, context| {
            let mut mask = mask_at(call, context, "mask").unwrap();
            let x = integer(call, "x").unwrap() as usize;
            let mut core = mask.core().to_vec();
            core[x] = 255;
            mask.replace_core(core).unwrap();
            if let Some(other) = mask_at(call, context, "other") {
                mask.protect_union(other.core()).unwrap();
            }
            mask.record(Made::new("mark", "1.0.0")).unwrap();
            Ok(Produced::nothing().with("mask", mask))
        }),
    ))
}

/// `split`: two masks of the mask it is given, one with pixel 0 set and one with pixel 1.
pub fn split() -> Arc<dyn Action> {
    let descriptor = ActionDescriptor::draft("split", "1.0.0", Kind::Primitive, "Splits a mask.")
        .input(InputPort::required("mask", ArtifactType::Mask, "The mask."))
        .output(OutputPort::counted(
            "mask",
            ArtifactType::Mask,
            "Two masks.",
            Count::between(0, 4),
        ))
        .concurrent(true)
        .build()
        .unwrap();
    Arc::new(Fake(
        descriptor,
        Box::new(|call, context| {
            let mask = mask_at(call, context, "mask").unwrap();
            let mut produced = Produced::nothing();
            for at in 0..2 {
                let mut one = mask.clone();
                let mut core = vec![0; one.core().len()];
                core[at] = 255;
                one.replace_core(core).unwrap();
                produced = produced.with("mask", one);
            }
            Ok(produced)
        }),
    ))
}

/// `join`: the union of the cores of the masks it is given.
pub fn join() -> Arc<dyn Action> {
    let descriptor = ActionDescriptor::draft("join", "1.0.0", Kind::Primitive, "Joins masks.")
        .input(InputPort::variadic(
            "mask",
            ArtifactType::Mask,
            "The masks.",
            Count::between(1, 4),
        ))
        .output(OutputPort::one("mask", ArtifactType::Mask, "Their union."))
        .concurrent(true)
        .build()
        .unwrap();
    Arc::new(Fake(
        descriptor,
        Box::new(|call, context| {
            let mut union: Option<Mask> = None;
            for input in call.inputs() {
                let read = artifact::get(
                    context.store(),
                    ArtifactType::Mask,
                    input.id(),
                    &DecodeLimits::default(),
                );
                let mask = read.unwrap().into_mask().unwrap();
                union = Some(match union {
                    None => mask,
                    Some(mut held) => {
                        let core: Vec<u8> = (held.core().iter().zip(mask.core()))
                            .map(|(a, b)| *a | *b)
                            .collect();
                        held.replace_core(core).unwrap();
                        held
                    }
                });
            }
            Ok(Produced::nothing().with("mask", union.unwrap()))
        }),
    ))
}

/// `paid`: hands its mask back, and needs a filler, so that a call of it may cost.
pub fn paid() -> Arc<dyn Action> {
    let descriptor = ActionDescriptor::draft("paid", "1.0.0", Kind::Primitive, "Costs.")
        .input(InputPort::required("mask", ArtifactType::Mask, "The mask."))
        .output(OutputPort::one(
            "mask",
            ArtifactType::Mask,
            "The same mask.",
        ))
        .needs(Requirement::new(Capability::Fill))
        .concurrent(true)
        .build()
        .unwrap();
    Arc::new(Fake(
        descriptor,
        Box::new(|call, context| {
            let mask = mask_at(call, context, "mask").unwrap();
            Ok(Produced::nothing().with("mask", mask))
        }),
    ))
}

/// A registry of the fakes, and of any more the test gives.
pub fn fakes(more: Vec<Arc<dyn Action>>) -> Registry {
    let mut registry = Registry::new();
    for action in [mark(), split(), join(), paid()].into_iter().chain(more) {
        registry.register(action).unwrap();
    }
    registry
}

/// A recipe of these steps, with an input `seed` (a mask), an optional `protect` (a
/// mask), an optional `note` (a text), and the output `mask` bound to `out`.
pub fn recipe(steps: &str, out: &str) -> String {
    format!(
        r#"version = 1
name = "marks"
purpose = "Marks a mask."
use = "To mark a mask in a test."

[inputs]
seed = {{ type = "Mask" }}
protect = {{ type = "Mask", optional = true }}
note = {{ type = "Text", optional = true }}

{steps}

[outputs]
mask = "{out}"
"#
    )
}

pub const TWO_MARKS: &str = r#"
[[step]]
id = "first"
op = "mark"
inputs = { mask = "inputs.seed" }
x = 0
note = "inputs.note"

[[step]]
id = "second"
op = "mark"
inputs = { mask = "first.mask", other = "inputs.protect" }
x = 1
constants = "mark-v2"
"#;

/// The recipe's text, checked against the fakes, as an action.
pub fn action_of(text: &str, registry: &Registry) -> Result<RecipeAction, RecipeError> {
    let checked = Recipe::read(text)?.check(registry)?;
    RecipeAction::new(checked, text, registry)
}

fn refused(text: &str) -> RecipeError {
    match action_of(text, &fakes(Vec::new())) {
        Ok(_) => panic!("the recipe was taken"),
        Err(error) => error,
    }
}

#[test]
fn a_recipe_whose_binding_reaches_no_earlier_step_is_refused_when_read() {
    // The second step's output, bound by the first: a later step is not an earlier one.
    let later = TWO_MARKS.replacen("inputs.seed", "second.mask", 1);
    let error = refused(&recipe(&later, "second.mask"));
    assert_eq!(
        (error.recipe.as_str(), error.step.as_str()),
        ("marks", "first")
    );
    assert!(
        error
            .why
            .contains("second.mask, which is neither an earlier step's output"),
        "{error}"
    );
    // A step of no such name, an output the step does not have, an input not declared.
    for (from, to) in [
        ("first.mask", "nowhere.mask"),
        ("first.mask", "first.plane"),
        ("first.mask", "inputs.nothing"),
    ] {
        let error = refused(&recipe(&TWO_MARKS.replacen(from, to, 1), "second.mask"));
        assert_eq!(error.step, "second", "{error}");
        assert!(error.why.contains(to), "{error}");
    }
    // An output bound past every step, or to nothing.
    let error = refused(&recipe(TWO_MARKS, "third.mask"));
    assert_eq!(error.step, "outputs");
    // A step's own output, bound by itself.
    let error = refused(&recipe(
        &TWO_MARKS.replacen("first.mask", "second.mask", 1),
        "second.mask",
    ));
    assert_eq!(error.step, "second");
    // The same recipe, bound in order, is taken.
    action_of(&recipe(TWO_MARKS, "second.mask"), &fakes(Vec::new())).unwrap();
}

#[test]
fn a_parameter_the_action_does_not_take_is_refused_naming_the_step() {
    let error = refused(&recipe(
        &TWO_MARKS.replacen("x = 1", "x = 1\nradius = 3", 1),
        "second.mask",
    ));
    assert_eq!(error.step, "second");
    assert_eq!(error.why, "mark takes no argument radius");
    assert_eq!(
        error.to_string(),
        "the recipe marks, step second: mark takes no argument radius"
    );
    // A value its type refuses; a whole number where it takes a text; one it requires,
    // left out; a constant set it does not have; one of an action that has none.
    for (from, to, why) in [
        ("x = 1", "x = 99", "mark: argument x: 99 is outside 0 to 47"),
        (
            "x = 1",
            "x = 1.5",
            "mark: argument x: a number where a whole number is required",
        ),
        (
            "x = 1",
            "x = 1\nnote = 3",
            "mark: argument note: a whole number where a text is required",
        ),
        ("x = 1", "", "argument x is required by mark"),
        (
            "mark-v2",
            "mark-v9",
            "mark: argument constants: \"mark-v9\" is not one of mark-v1, mark-v2",
        ),
    ] {
        let error = refused(&recipe(&TWO_MARKS.replacen(from, to, 1), "second.mask"));
        assert_eq!((error.step.as_str(), error.why.as_str()), ("second", why));
    }
    let split = r#"
[[step]]
id = "halves"
op = "split"
inputs = { mask = "inputs.seed" }
constants = "split-v1"
"#;
    let error = refused(&recipe(split, "halves.mask"));
    assert_eq!(
        (error.step.as_str(), error.why.as_str()),
        ("halves", "split has no constant sets")
    );
    // An action of no such name.
    let error = refused(&recipe(
        &TWO_MARKS.replacen("op = \"mark\"", "op = \"heal\"", 1),
        "second.mask",
    ));
    assert_eq!(
        (error.step.as_str(), error.why.as_str()),
        ("first", "heal is no action of this build")
    );
}

#[test]
fn what_a_recipe_binds_is_held_to_the_ports_types_and_counts() {
    let error = refused(&recipe(
        &TWO_MARKS.replacen("first.mask", "inputs.note", 1),
        "second.mask",
    ));
    assert!(
        error
            .why
            .contains("a text input: a text binds a text argument"),
        "{error}"
    );
    // A required port bound to an optional input.
    let error = refused(&recipe(
        &TWO_MARKS.replacen("inputs.seed", "inputs.protect", 1),
        "second.mask",
    ));
    assert_eq!(
        error.why,
        "input mask is required by mark, and inputs.protect is an optional input"
    );
    // Several to one, and one to several.
    let many = r#"
[[step]]
id = "halves"
op = "split"
inputs = { mask = "inputs.seed" }

[[step]]
id = "marked"
op = "mark"
inputs = { mask = "halves.mask" }
x = 2
"#;
    let error = refused(&recipe(many, "marked.mask"));
    assert_eq!(
        error.why,
        "input mask takes one artifact, and halves.mask returns several"
    );
    let joined = many
        .replacen("op = \"mark\"", "op = \"join\"", 1)
        .replacen("x = 2", "", 1);
    action_of(&recipe(&joined, "marked.mask"), &fakes(Vec::new())).unwrap();
    let one = joined.replacen("halves.mask", "inputs.seed", 1);
    let error = refused(&recipe(&one, "marked.mask"));
    assert!(error.why.contains("returns one"), "{error}");
    // Two steps of one id; an id that is not a name; no purpose; rascal's render.
    let twice = TWO_MARKS.replacen("id = \"second\"", "id = \"first\"", 1);
    assert_eq!(
        refused(&recipe(&twice, "first.mask")).why,
        "the step id is given twice"
    );
    let dotted = TWO_MARKS.replacen("id = \"second\"", "id = \"sec.ond\"", 1);
    assert_eq!(refused(&recipe(&dotted, "second.mask")).step, "sec.ond");
    let bare = recipe(TWO_MARKS, "second.mask").replacen("purpose = \"Marks a mask.\"\n", "", 1);
    assert_eq!(refused(&bare).why, "it says no `purpose`: what it is for");
    let rendered =
        recipe(TWO_MARKS, "second.mask") + "render = [{ for = \"grey\", out = \"g.png\" }]\n";
    assert_eq!(refused(&rendered).step, "outputs");
}

/// Rascal's `remove-prompt.toml`, as its file is at the commit `docs/sources.toml` pins.
const RASCAL_REMOVE_PROMPT: &str = r#"# Remove what a prompt names: a configured pack's `segment` finds the instances, `select` takes
# every plane as the core, then the same shaping as `remove`: grown a little and further downward
# for the shadow, closed, small holes filled, kept off anything protected, feathered. Needs
# --config with a [packs.*] entry that serves segment; `--prompt` binds inputs.prompt.
# `model` picks the segmenter (SPEC.md "Choosable segmenters"): `--model` or the call's `model`,
# else `[segment] model`, else the pack's default; rascal fills it, so no step sets it itself.
version = 1
name = "remove-prompt"
use_case = "remove"

[inputs]
image = { type = "Image" }
prompt = { type = "Text" }
model = { type = "Text", optional = true }
protect = { type = "Mask", optional = true }

[[step]]
id = "segment"
op = "segment"
inputs = { image = "inputs.image" }
prompt = "inputs.prompt"
model = "inputs.model"
max_instances = 8

[[step]]
id = "select"
op = "select"
inputs = { plane = "segment.plane" }

[[step]]
id = "grow"
op = "grow"
inputs = { mask = "select.mask" }
px = 4.0
down_px = 12.0

[[step]]
id = "close"
op = "close"
inputs = { mask = "grow.mask" }
radius_px = 3.0

[[step]]
id = "plug"
op = "plug"
inputs = { mask = "close.mask" }
max_share = 0.1

[[step]]
id = "subtract"
op = "subtract"
inputs = { mask = "plug.mask", subtract = "inputs.protect" }
grow_px = 2.0

[[step]]
id = "feather"
op = "feather"
inputs = { mask = "subtract.mask" }
sigma_px = 4.0

[outputs]
mask = "feather.mask"
image = "inputs.image"
render = [{ for = "grey", out = "remove-prompt-grey.png" }]
"#;

/// The shipped `remove_prompt`, as `crates/whizbang-rascal/recipes/remove_prompt.toml` has it.
const SHIPPED_REMOVE_PROMPT: &str =
    include_str!("../../../whizbang-rascal/recipes/remove_prompt.toml");

#[test]
fn the_ported_recipes_of_rascal_read_as_rascal_read_them() {
    // What rascal's reader gave for its file (`Recipe::parse`, its `Step` and `Outputs`):
    // the name, the use case, the four inputs, each step's id, op, bindings and
    // parameters, and the outputs and the rendering.
    let read = Recipe::read(RASCAL_REMOVE_PROMPT).unwrap();
    assert_eq!((read.version, read.name.as_str()), (1, "remove-prompt"));
    assert_eq!(read.use_case.as_deref(), Some("remove"));
    assert_eq!((read.purpose.as_ref(), read.when.as_ref()), (None, None));
    let declared: Vec<(&str, Declared, bool)> = (read.inputs.iter())
        .map(|(name, input)| (name.as_str(), input.declared, input.optional))
        .collect();
    assert_eq!(
        declared,
        [
            ("image", Declared::Artifact(ArtifactType::Image), false),
            ("model", Declared::Text, true),
            ("prompt", Declared::Text, false),
            ("protect", Declared::Artifact(ArtifactType::Mask), true),
        ]
    );
    let steps: Vec<(&str, &str)> = (read.steps.iter())
        .map(|step| (step.id.as_str(), step.op.as_str()))
        .collect();
    let rascal = [
        ("segment", "segment"),
        ("select", "select"),
        ("grow", "grow"),
        ("close", "close"),
        ("plug", "plug"),
        ("subtract", "subtract"),
        ("feather", "feather"),
    ];
    assert_eq!(steps, rascal);
    let bound = |at: usize| -> Vec<(&str, &str)> {
        (read.steps[at].inputs.iter())
            .map(|(port, binding)| (port.as_str(), binding.as_str()))
            .collect()
    };
    assert_eq!(bound(0), [("image", "inputs.image")]);
    assert_eq!(bound(1), [("plane", "segment.plane")]);
    assert_eq!(
        bound(5),
        [("mask", "plug.mask"), ("subtract", "inputs.protect")]
    );
    let params = |at: usize| -> Vec<(&str, &Written)> {
        (read.steps[at].params.iter())
            .map(|(name, value)| (name.as_str(), value))
            .collect()
    };
    let text = |text: &str| Written::Text(text.to_owned());
    assert_eq!(
        params(0),
        [
            ("max_instances", &Written::Integer(8)),
            ("model", &text("inputs.model")),
            ("prompt", &text("inputs.prompt")),
        ]
    );
    assert_eq!(
        params(2),
        [
            ("down_px", &Written::Float(12.0)),
            ("px", &Written::Float(4.0))
        ]
    );
    assert_eq!(params(3), [("radius_px", &Written::Float(3.0))]);
    assert_eq!(params(4), [("max_share", &Written::Float(0.1))]);
    assert_eq!(params(5), [("grow_px", &Written::Float(2.0))]);
    assert_eq!(params(6), [("sigma_px", &Written::Float(4.0))]);
    assert!(read.steps.iter().all(|step| step.constants.is_none()));
    let outputs: Vec<(&str, &str)> = (read.outputs.iter())
        .map(|(name, binding)| (name.as_str(), binding.as_str()))
        .collect();
    assert_eq!(
        outputs,
        [("image", "inputs.image"), ("mask", "feather.mask")]
    );
    assert_eq!(read.render.len(), 1);
    assert_eq!(
        (read.render[0].profile.as_str(), read.render[0].out.as_str()),
        ("grey", "remove-prompt-grey.png")
    );

    // The port: the same steps in the same order, each with rascal's numbers, under
    // this build's names for what differs (ADR 0033): the phrase is `segment`'s `query`
    // and the segmenter its `backend`; `select` takes masks; no rendering. Then one step
    // more, `fill` (plan m3, task F1, item 6), whose image and mask are the recipe's
    // outputs.
    let port = Recipe::read(SHIPPED_REMOVE_PROMPT).unwrap();
    let ported: Vec<(&str, &str)> = (port.steps.iter())
        .map(|step| (step.id.as_str(), step.op.as_str()))
        .collect();
    assert_eq!(ported[..7], rascal);
    assert_eq!(ported[7..], [("fill", "fill")]);
    assert_eq!(port.steps[7].inputs["image"], "inputs.image");
    assert_eq!(port.steps[7].inputs["mask"], "feather.mask");
    assert_eq!(port.steps[7].params["backend"], text("inputs.filler"));
    assert_eq!(
        port.outputs.get("image").map(String::as_str),
        Some("fill.image")
    );
    for at in 2..7 {
        assert_eq!(
            port.steps[at].params, read.steps[at].params,
            "{}",
            port.steps[at].id
        );
        assert_eq!(
            port.steps[at].inputs, read.steps[at].inputs,
            "{}",
            port.steps[at].id
        );
    }
    assert_eq!(port.steps[0].params["max_instances"], Written::Integer(8));
    assert_eq!(port.steps[0].params["query"], text("inputs.prompt"));
    assert_eq!(port.steps[0].params["backend"], text("inputs.backend"));
    assert_eq!(port.steps[1].inputs["mask"], "segment.mask");
    assert_eq!(
        port.outputs.get("mask").map(String::as_str),
        Some("fill.mask")
    );
    assert!(port.render.is_empty());
    assert_eq!(port.name, "remove_prompt");

    // A binding reads as rascal's did: `inputs.<name>` or `<step>.<output>`.
    assert_eq!(
        Binding::parse("inputs.image"),
        Some(Binding::Input("image".to_owned()))
    );
    assert_eq!(
        Binding::parse("grow.mask"),
        Some(Binding::Step {
            step: "grow".to_owned(),
            port: "mask".to_owned()
        })
    );
    for not in ["grow", ".mask", "grow.", "a.b.c"] {
        assert_eq!(Binding::parse(not), None, "{not}");
    }
}

#[test]
fn a_checked_recipe_gives_each_value_its_arguments_type_and_states_what_its_steps_need() {
    let text = recipe(
        &format!(
            "{TWO_MARKS}\n[[step]]\nid = \"cost\"\nop = \"paid\"\ninputs = {{ mask = \"second.mask\" }}\n"
        ),
        "cost.mask",
    );
    let registry = fakes(Vec::new());
    let action = action_of(&text, &registry).unwrap();
    let checked = action.recipe();
    assert_eq!(
        checked.steps[0].arguments,
        [
            ("note".to_owned(), Param::Input("note".to_owned())),
            ("x".to_owned(), Param::Value(Value::Integer(0))),
        ]
    );
    assert_eq!(checked.steps[1].constants.as_deref(), Some("mark-v2"));
    let descriptor = action.descriptor();
    assert_eq!(descriptor.kind(), Kind::Recipe);
    assert_eq!(descriptor.use_when(), Some("To mark a mask in a test."));
    assert_eq!(descriptor.needs().len(), 1);
    assert_eq!(descriptor.needs()[0].capability(), Capability::Fill);
    let names: Vec<&str> = descriptor.arguments().iter().map(Argument::name).collect();
    assert_eq!(names, ["note", "set", "from", "until"]);
    // With `from`, an input for each output of each step before the last, which may be
    // given in its place (plan m4, task K1, item 2); and the recipe plans such a call.
    let ports: Vec<&str> = descriptor.inputs().iter().map(InputPort::name).collect();
    assert_eq!(
        ports,
        ["protect", "seed", "from_first_mask", "from_second_mask"]
    );
    assert!(descriptor.plans());
    // `until` names a step before the last, and what only the last step needs is needed
    // only without it (plan m3, task F1, item 6).
    let until = descriptor.argument("until").unwrap().value_type();
    assert_eq!(*until, ValueType::choice(&["first", "second"]));
    assert_eq!(*descriptor.needs()[0].applies(), When::not_given("until"));
    // A need an earlier step states too is needed whatever `until` says.
    let earlier = recipe(
        &format!(
            "[[step]]\nid = \"early\"\nop = \"paid\"\ninputs = {{ mask = \"inputs.seed\" }}\n\n{TWO_MARKS}\n[[step]]\nid = \"cost\"\nop = \"paid\"\ninputs = {{ mask = \"second.mask\" }}\n"
        ),
        "cost.mask",
    );
    let action = action_of(&earlier, &registry).unwrap();
    assert_eq!(*action.descriptor().needs()[0].applies(), When::Always);
    // A recipe of one step has no `until`.
    let one = recipe(
        "[[step]]\nid = \"cost\"\nop = \"paid\"\ninputs = { mask = \"inputs.seed\" }\n",
        "cost.mask",
    );
    let action = action_of(&one, &registry).unwrap();
    assert!(action.descriptor().argument("until").is_none());
    assert_eq!(*action.descriptor().needs()[0].applies(), When::Always);
}

#[test]
fn an_input_named_as_the_runners_from_or_one_of_its_ports_is_refused_when_read() {
    // Plan m4, task K1: `from` is the runner's own argument, and what a run from a step
    // is given goes in at `from_<step>_<port>`.
    for name in ["from", "from_first_mask", "from_anything"] {
        let text = recipe(TWO_MARKS, "second.mask")
            .replacen("note = {", &format!("{name} = {{"), 1)
            .replacen(
                "note = \"inputs.note\"",
                &format!("note = \"inputs.{name}\""),
                1,
            );
        let error = refused(&text);
        assert_eq!(error.step, "inputs", "{name}");
        let said = match name {
            "from" => "from is the name of the runner's own argument".to_owned(),
            _ => format!("{name} begins as the runner's own inputs do"),
        };
        assert!(error.why.contains(&said), "{name}: {}", error.why);
    }
    // A name that only starts with the same letters is an input like any other.
    let text = recipe(TWO_MARKS, "second.mask")
        .replacen("note = {", "fromage = {", 1)
        .replacen("inputs.note", "inputs.fromage", 1);
    action_of(&text, &fakes(Vec::new())).unwrap();
}
