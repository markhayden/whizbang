//! Recipes (`SPEC.md` section 3, "Primitives, recipes and loops"; ADR 0033): a named,
//! typed sequence of steps over the actions of the build, read from a file of the format
//! `recipe/1`, checked against the actions' descriptors when it is read, and run by one
//! runner in the core as an action of the kind [`Kind::Recipe`], each step a nested call
//! through the executor in the recipe's context.
//!
//! # The format, `recipe/1`
//!
//! Rascal's recipe file (`crates/rascal-core/src/recipe.rs` and `docs/recipes.md` at the
//! commit `docs/sources.toml` pins), read as rascal read it, with three keys more that a
//! recipe whizbang runs must have: `purpose`, what it is for, and `use`, when to use it,
//! each one line held to the rule of a purpose; and, at the top, nothing else new.
//!
//! ```toml
//! version = 1
//! name = "remove_prompt"
//! purpose = "Makes the mask to remove what a phrase names."
//! use = "To remove a thing you can name in words."
//! use_case = "remove"                     # rascal's; read and kept
//!
//! [inputs]                                # artifact types, or Text
//! image = { type = "Image" }
//! prompt = { type = "Text" }
//! protect = { type = "Mask", optional = true }
//!
//! [[step]]
//! id = "segment"
//! op = "segment"                          # an action of the build
//! inputs = { image = "inputs.image" }     # port = inputs.<name> or <step>.<output>
//! query = "inputs.prompt"                 # a text input, bound to a text argument
//! max_instances = 8                       # every other key is an argument
//!
//! [outputs]
//! mask = "feather.mask"
//! ```
//!
//! An input may say `many = true`: it is then every artifact a caller gives at its port,
//! in the order given, and binds only a port that takes several (squint's use cases take a
//! candidate, a source and up to eight references so; plan m3, task Q1).
//!
//! A step's argument written `"<step>.measurements"`, naming an earlier step whose action
//! hands back measurements (ADR 0035), is bound to what that step measured, as one JSON
//! text: a text argument only. A recipe whose last step answers with a verdict hands that
//! verdict back to its caller with the outcome, and may hand back no artifact.
//!
//! `constants = "<set>"` in a step names a constant set: the action's argument
//! `constants`, which an action that has sets declares as a choice of their names.
//! Rascal's `projection` is read and kept, and its `[outputs] render` is read and kept
//! and refused when the recipe is checked: whizbang renders with the action `render`, as
//! a step.

pub mod runner;

use std::collections::BTreeMap;

use crate::artifact::ArtifactType;
use crate::contract::{ActionDescriptor, Registry, ResultKind, Value, ValueType, text};

pub use runner::{FROM, RecipeAction, SET, UNTIL, from_port, until_port};

/// The version of the format this build reads: rascal's `version = 1`.
pub const FORMAT: i64 = 1;

/// The most steps a recipe has: the most steps of a recipe a run's record lists.
pub const STEPS_MAX: usize = crate::contract::RECIPE_STEPS_LIMIT;

/// The most arguments one step of a recipe sets.
pub const ARGUMENTS_MAX: usize = crate::contract::RECIPE_ARGUMENTS_LIMIT;

/// The most bytes a recipe file holds.
pub const FILE_MAX_BYTES: usize = 64 * 1024;

/// Why a recipe is not one this build runs, naming the recipe and the step, or `-` where
/// no step is to blame, as rascal named them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeError {
    pub recipe: String,
    pub step: String,
    pub why: String,
}

impl std::fmt::Display for RecipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the recipe {}, step {}: {}",
            self.recipe.escape_debug(),
            self.step.escape_debug(),
            self.why
        )
    }
}

impl std::error::Error for RecipeError {}

/// What a declared input is: an artifact of one type, bound to ports, or a text, bound to
/// a text argument by naming it as the argument's value (`query = "inputs.prompt"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declared {
    Artifact(ArtifactType),
    Text,
}

/// One declared input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub declared: Declared,
    /// The caller may leave it out: it is then absent, and binds only an optional port
    /// or an argument a call may leave out.
    pub optional: bool,
    /// It is every artifact the caller gives at its port, in order (`many = true`).
    pub many: bool,
}

/// Where a port's artifacts come from: a declared input, or an output of an earlier step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    Input(String),
    Step { step: String, port: String },
}

impl Binding {
    /// A binding as a recipe writes it: `inputs.<name>` or `<step>.<output>`.
    pub fn parse(text: &str) -> Option<Binding> {
        let (source, name) = text.split_once('.')?;
        if source.is_empty() || name.is_empty() || name.contains('.') {
            return None;
        }
        Some(if source == "inputs" {
            Binding::Input(name.to_owned())
        } else {
            Binding::Step {
                step: source.to_owned(),
                port: name.to_owned(),
            }
        })
    }
}

impl std::fmt::Display for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Binding::Input(name) => write!(f, "inputs.{name}"),
            Binding::Step { step, port } => write!(f, "{step}.{port}"),
        }
    }
}

/// A parameter of a step as the file writes it.
#[derive(Debug, Clone, PartialEq)]
pub enum Written {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Text(String),
}

/// One step as it was read.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub id: String,
    pub op: String,
    /// Each port bound, with its binding as written.
    pub inputs: BTreeMap<String, String>,
    pub constants: Option<String>,
    /// Every other key, as written.
    pub params: BTreeMap<String, Written>,
}

/// A rendering rascal's recipe wrote under its run directory: read and kept, never run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Render {
    pub profile: String,
    pub out: String,
}

/// A recipe file as it was read: what rascal read, with `purpose` and `use`.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub version: i64,
    pub name: String,
    pub purpose: Option<String>,
    pub when: Option<String>,
    pub use_case: Option<String>,
    pub projection: Option<String>,
    pub inputs: BTreeMap<String, Input>,
    pub steps: Vec<Step>,
    /// Each output's name with its binding, as written.
    pub outputs: BTreeMap<String, String>,
    pub render: Vec<Render>,
}

fn kind_of(value: &toml::Value) -> &'static str {
    match value {
        toml::Value::String(_) => "a string",
        toml::Value::Integer(_) => "an integer",
        toml::Value::Float(_) => "a float",
        toml::Value::Boolean(_) => "a boolean",
        toml::Value::Datetime(_) => "a date",
        toml::Value::Array(_) => "an array",
        toml::Value::Table(_) => "a table",
    }
}

impl Recipe {
    /// Reads a recipe's text. The text alone is read: nothing is checked against the
    /// build's actions until [`Recipe::check`].
    pub fn read(text: &str) -> Result<Recipe, RecipeError> {
        let refuse = |recipe: &str, step: &str, why: String| RecipeError {
            recipe: recipe.to_owned(),
            step: step.to_owned(),
            why,
        };
        if text.len() > FILE_MAX_BYTES {
            return Err(refuse(
                "-",
                "-",
                format!("the file is over {FILE_MAX_BYTES} bytes"),
            ));
        }
        let mut table: toml::Table = toml::from_str(text)
            .map_err(|error| refuse("-", "-", format!("not TOML: {}", error.message())))?;
        let name = match table.remove("name") {
            Some(toml::Value::String(name)) => name,
            _ => return Err(refuse("-", "-", "`name` must be a string".to_owned())),
        };
        let refuse = |step: &str, why: String| refuse(&name, step, why);
        let version = match table.remove("version") {
            Some(toml::Value::Integer(version)) => version,
            _ => return Err(refuse("-", "`version` must be an integer".to_owned())),
        };
        let mut text_of = |key: &str| match table.remove(key) {
            None => Ok(None),
            Some(toml::Value::String(text)) => Ok(Some(text)),
            Some(other) => Err(refuse(
                "-",
                format!("`{key}` must be a string, not {}", kind_of(&other)),
            )),
        };
        let purpose = text_of("purpose")?;
        let when = text_of("use")?;
        let use_case = text_of("use_case")?;
        let projection = text_of("projection")?;

        let mut inputs = BTreeMap::new();
        if let Some(value) = table.remove("inputs") {
            let toml::Value::Table(declared) = value else {
                return Err(refuse("inputs", "`inputs` must be a table".to_owned()));
            };
            for (input, value) in declared {
                inputs.insert(
                    input.clone(),
                    read_input(&input, value).map_err(|why| refuse("inputs", why))?,
                );
            }
        }

        let mut steps = Vec::new();
        if let Some(value) = table.remove("step") {
            let toml::Value::Array(written) = value else {
                return Err(refuse("-", "`step` must be an array of tables".to_owned()));
            };
            for value in written {
                let toml::Value::Table(step) = value else {
                    return Err(refuse("-", "each step must be a table".to_owned()));
                };
                steps.push(read_step(step).map_err(|(step, why)| refuse(&step, why))?);
            }
        }

        let mut outputs = BTreeMap::new();
        let mut render = Vec::new();
        if let Some(value) = table.remove("outputs") {
            let toml::Value::Table(written) = value else {
                return Err(refuse("outputs", "`outputs` must be a table".to_owned()));
            };
            for (output, value) in written {
                match (output.as_str(), value) {
                    ("render", toml::Value::Array(renders)) => {
                        for one in renders {
                            render.push(read_render(one).map_err(|why| refuse("outputs", why))?);
                        }
                    }
                    (_, toml::Value::String(binding)) => {
                        outputs.insert(output, binding);
                    }
                    (_, other) => {
                        return Err(refuse(
                            "outputs",
                            format!(
                                "output {output} must be a binding string, not {}",
                                kind_of(&other)
                            ),
                        ));
                    }
                }
            }
        }
        if let Some(key) = table.keys().next() {
            return Err(refuse("-", format!("`{key}` is not a key of a recipe")));
        }
        Ok(Recipe {
            version,
            name,
            purpose,
            when,
            use_case,
            projection,
            inputs,
            steps,
            outputs,
            render,
        })
    }

    /// Whether a binding names a declared text input.
    fn text_input(&self, binding: &Binding) -> Option<&Input> {
        match binding {
            Binding::Input(name) => self
                .inputs
                .get(name)
                .filter(|input| input.declared == Declared::Text),
            Binding::Step { .. } => None,
        }
    }

    /// The earlier step a parameter binds the measurements of (`<step>.measurements`).
    fn bound_measurements<'a>(
        &self,
        written: &'a Written,
        before: &[CheckedStep],
    ) -> Option<&'a str> {
        let Written::Text(text) = written else {
            return None;
        };
        let step = text.strip_suffix(MEASUREMENTS)?.strip_suffix('.')?;
        before.iter().any(|b| b.id == step).then_some(step)
    }

    /// The text input a parameter names as its value, with its name.
    fn bound_text<'a>(&'a self, written: &'a Written) -> Option<(&'a str, &'a Input)> {
        let Written::Text(text) = written else {
            return None;
        };
        let name = text.strip_prefix("inputs.")?;
        let input = self.inputs.get(name)?;
        (input.declared == Declared::Text).then_some((name, input))
    }

    /// Checks the recipe against the actions of this registry: everything is checked
    /// before anything runs, and every refusal names the recipe and the step. What comes
    /// back is the recipe as the runner runs it, with each value of the type its
    /// argument takes.
    pub fn check(&self, registry: &Registry) -> Result<Checked, RecipeError> {
        let refuse = |step: &str, why: String| RecipeError {
            recipe: self.name.clone(),
            step: step.to_owned(),
            why,
        };
        if self.version != FORMAT {
            return Err(refuse(
                "-",
                format!(
                    "version {} is not read here (recipe/1 is version 1)",
                    self.version
                ),
            ));
        }
        text::action_name(&self.name).map_err(|why| refuse("-", why))?;
        let purpose = self
            .purpose
            .as_deref()
            .ok_or_else(|| refuse("-", "it says no `purpose`: what it is for".to_owned()))?;
        text::plain_line("purpose", purpose).map_err(|why| refuse("-", why))?;
        let when = self
            .when
            .as_deref()
            .ok_or_else(|| refuse("-", "it says no `use`: when to use it".to_owned()))?;
        text::plain_line("use", when).map_err(|why| refuse("-", why))?;
        if self.steps.is_empty() || self.steps.len() > STEPS_MAX {
            return Err(refuse(
                "-",
                format!(
                    "a recipe has 1 to {STEPS_MAX} steps, and this has {}",
                    self.steps.len()
                ),
            ));
        }
        for name in self.inputs.keys() {
            text::field_name(name).map_err(|why| refuse("inputs", why))?;
            if [SET, UNTIL, FROM].contains(&name.as_str()) {
                return Err(refuse(
                    "inputs",
                    format!("{name} is the name of the runner's own argument"),
                ));
            }
            // What a run from a step is given goes in at ports of the runner's own,
            // `from_<step>_<port>`: no input is named as one of them could be.
            if name.starts_with(&format!("{FROM}_")) {
                return Err(refuse(
                    "inputs",
                    format!(
                        "{name} begins as the runner's own inputs do, which are named \
                         {FROM}_<step>_<port>"
                    ),
                ));
            }
        }
        if !self.render.is_empty() {
            return Err(refuse(
                "outputs",
                "render is rascal's: here a rendering is a step of the action render".to_owned(),
            ));
        }

        let mut checked: Vec<CheckedStep> = Vec::new();
        for (index, step) in self.steps.iter().enumerate() {
            let at = step.id.as_str();
            let refuse = |why: String| refuse(at, why);
            text::action_name(&step.id).map_err(|why| {
                refuse(format!("a step's id is the name of one, and is not: {why}"))
            })?;
            if self.steps[..index]
                .iter()
                .any(|before| before.id == step.id)
            {
                return Err(refuse("the step id is given twice".to_owned()));
            }
            let action = registry
                .get(&step.op)
                .ok_or_else(|| refuse(format!("{} is no action of this build", step.op)))?;
            let descriptor = action.descriptor();
            let op = &step.op;

            let mut inputs = Vec::new();
            for (port, written) in &step.inputs {
                let declared = descriptor
                    .inputs()
                    .iter()
                    .find(|p| p.name() == port)
                    .ok_or_else(|| refuse(format!("{op} has no input {port}")))?;
                let binding = Binding::parse(written).ok_or_else(|| {
                    refuse(format!(
                        "input {port} is bound to {written:?}, which is neither inputs.<name> \
                         nor <step>.<output>"
                    ))
                })?;
                if self.text_input(&binding).is_some() {
                    return Err(refuse(format!(
                        "input {port} is bound to {binding}, a text input: a text binds a text \
                         argument ({port} = \"{binding}\"), never a port"
                    )));
                }
                let (artifact, many, optional) =
                    self.source(&binding, &checked, registry).ok_or_else(|| {
                        refuse(format!(
                            "input {port} is bound to {binding}, which is neither an earlier \
                             step's output nor a declared input"
                        ))
                    })?;
                if artifact != declared.artifact() {
                    return Err(refuse(format!(
                        "input {port} takes {}, and {binding} is {}",
                        declared.artifact().with_article(),
                        artifact.with_article()
                    )));
                }
                if declared.count().min() > 0 && optional {
                    return Err(refuse(format!(
                        "input {port} is required by {op}, and {binding} is an optional input"
                    )));
                }
                let takes_many = declared.count().max() != Some(1);
                if takes_many != many {
                    return Err(refuse(if takes_many {
                        format!(
                            "input {port} takes every artifact of an output that returns \
                             several, and {binding} returns one"
                        )
                    } else {
                        format!("input {port} takes one artifact, and {binding} returns several")
                    }));
                }
                inputs.push((port.clone(), binding));
            }
            for port in descriptor.inputs() {
                if port.count().min() > 0 && !step.inputs.contains_key(port.name()) {
                    return Err(refuse(format!("input {} is required by {op}", port.name())));
                }
            }

            let mut arguments = Vec::new();
            let mut written: Vec<(String, Written)> = (step.params.iter())
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            if let Some(set) = &step.constants {
                written.push((CONSTANTS.to_owned(), Written::Text(set.clone())));
            }
            if written.len() > ARGUMENTS_MAX {
                return Err(refuse(format!(
                    "a step sets at most {ARGUMENTS_MAX} arguments, and this sets {}",
                    written.len()
                )));
            }
            for (name, value) in &written {
                let argument = descriptor.argument(name).ok_or_else(|| {
                    if name == CONSTANTS {
                        refuse(format!("{op} has no constant sets"))
                    } else {
                        refuse(format!("{op} takes no argument {name}"))
                    }
                })?;
                if let Some(from) = self.bound_measurements(value, &checked) {
                    let measures = (checked.iter().find(|b| b.id == from))
                        .and_then(|b| registry.get(&b.action))
                        .is_some_and(|a| a.descriptor().answers_with(ResultKind::Measurements));
                    if !measures {
                        return Err(refuse(format!(
                            "argument {name} is bound to {from}.measurements, and the step \
                             {from} hands back no measurements"
                        )));
                    }
                    if !matches!(argument.value_type(), ValueType::Text) {
                        return Err(refuse(format!(
                            "argument {name} is bound to {from}.measurements, and is not a text"
                        )));
                    }
                    arguments.push((name.clone(), Param::Measured(from.to_owned())));
                    continue;
                }
                if argument.takes_measurements() {
                    return Err(refuse(format!(
                        "argument {name} takes only an earlier step's measurements, bound as \
                         <step>.measurements, and is given another value"
                    )));
                }
                if let Some((input, declared)) = self.bound_text(value) {
                    if !matches!(
                        argument.value_type(),
                        ValueType::Text | ValueType::Choice { .. }
                    ) {
                        return Err(refuse(format!(
                            "argument {name} takes the text input inputs.{input}, and is not \
                             a text or a choice"
                        )));
                    }
                    if argument.is_required() && declared.optional {
                        return Err(refuse(format!(
                            "argument {name} is required by {op}, and inputs.{input} is an \
                             optional input"
                        )));
                    }
                    arguments.push((name.clone(), Param::Input(input.to_owned())));
                    continue;
                }
                let value = typed(argument.value_type(), value)
                    .map_err(|why| refuse(format!("{op}: argument {name}: {why}")))?;
                arguments.push((name.clone(), Param::Value(value)));
            }
            for argument in descriptor.arguments() {
                if argument.is_required() && !arguments.iter().any(|(n, _)| n == argument.name()) {
                    return Err(refuse(format!(
                        "argument {} is required by {op}",
                        argument.name()
                    )));
                }
            }
            checked.push(CheckedStep {
                id: step.id.clone(),
                action: step.op.clone(),
                inputs,
                arguments,
                constants: step.constants.clone(),
            });
        }

        let mut outputs = Vec::new();
        for (name, written) in &self.outputs {
            let refuse = |why: String| refuse("outputs", why);
            text::field_name(name).map_err(refuse)?;
            let binding = Binding::parse(written).ok_or_else(|| {
                refuse(format!(
                    "{name} = {written:?} is neither inputs.<name> nor <step>.<output>"
                ))
            })?;
            let (artifact, many, optional) =
                self.source(&binding, &checked, registry).ok_or_else(|| {
                    refuse(format!(
                        "{name} = {binding} names no step's output and no artifact input"
                    ))
                })?;
            if optional {
                return Err(refuse(format!(
                    "{name} = {binding} is an optional input, which a caller may leave out"
                )));
            }
            outputs.push(Output {
                name: name.clone(),
                binding,
                artifact,
                many,
            });
        }
        // A recipe whose last step answers with a verdict hands it back (ADR 0035).
        let verdict = (checked.last())
            .and_then(|last| registry.get(&last.action))
            .is_some_and(|last| last.descriptor().answers_with(ResultKind::Verdict));
        // And one whose last step declares findings hands those back with it.
        let findings = (checked.last())
            .and_then(|last| registry.get(&last.action))
            .is_some_and(|last| last.descriptor().answers_with(ResultKind::Findings));
        if outputs.is_empty() && !verdict {
            return Err(refuse(
                "outputs",
                "a recipe hands back at least one output, or its last step a verdict".to_owned(),
            ));
        }
        Ok(Checked {
            name: self.name.clone(),
            purpose: purpose.to_owned(),
            when: when.to_owned(),
            inputs: self.inputs.clone(),
            steps: checked,
            outputs,
            verdict,
            findings,
        })
    }

    /// What a binding resolves to before the step at hand: the artifact type, whether it
    /// is several artifacts, and whether it is an input a caller may leave out. None
    /// when it names no earlier step's output and no declared artifact input.
    fn source(
        &self,
        binding: &Binding,
        before: &[CheckedStep],
        registry: &Registry,
    ) -> Option<(ArtifactType, bool, bool)> {
        match binding {
            Binding::Input(name) => match self.inputs.get(name)? {
                Input {
                    declared: Declared::Artifact(artifact),
                    optional,
                    many,
                } => Some((*artifact, *many, *optional)),
                _ => None,
            },
            Binding::Step { step, port } => {
                let step = before.iter().find(|before| before.id == *step)?;
                let descriptor = registry.get(&step.action)?.descriptor();
                let output = descriptor.outputs().iter().find(|p| p.name() == port)?;
                Some((output.artifact(), output.count().max() != Some(1), false))
            }
        }
    }
}

/// The argument an action that has constant sets takes them by: a choice of their
/// names.
pub const CONSTANTS: &str = "constants";

/// What a step's argument names after an earlier step's id to be bound to what that step
/// measured (ADR 0035).
pub const MEASUREMENTS: &str = "measurements";

fn read_input(name: &str, value: toml::Value) -> Result<Input, String> {
    let toml::Value::Table(mut table) = value else {
        return Err(format!(
            "input {name} must be a table: {{ type = \"...\" }}"
        ));
    };
    let declared = match table.remove("type") {
        Some(toml::Value::String(kind)) => match kind.as_str() {
            "Image" => Declared::Artifact(ArtifactType::Image),
            "Location" => Declared::Artifact(ArtifactType::Location),
            "Mask" => Declared::Artifact(ArtifactType::Mask),
            "BandProfile" => Declared::Artifact(ArtifactType::BandProfile),
            "Series" => Declared::Artifact(ArtifactType::Series),
            "Plan" => Declared::Artifact(ArtifactType::Plan),
            "Text" => Declared::Text,
            other => {
                return Err(format!(
                    "input {name} has the type {other:?}: Image, Location, Mask, BandProfile, \
                     Series, Plan or Text"
                ));
            }
        },
        _ => return Err(format!("input {name} must say its type, a string")),
    };
    let optional = match table.remove("optional") {
        None => false,
        Some(toml::Value::Boolean(optional)) => optional,
        Some(_) => return Err(format!("input {name}: `optional` must be a boolean")),
    };
    let many = match table.remove("many") {
        None => false,
        Some(toml::Value::Boolean(many)) => many,
        Some(_) => return Err(format!("input {name}: `many` must be a boolean")),
    };
    if many && declared == Declared::Text {
        return Err(format!("input {name}: a text is one, and is never many"));
    }
    if let Some(key) = table.keys().next() {
        return Err(format!("input {name}: `{key}` is not a key of an input"));
    }
    Ok(Input {
        declared,
        optional,
        many,
    })
}

fn read_step(mut table: toml::Table) -> Result<Step, (String, String)> {
    let id = match table.remove("id") {
        Some(toml::Value::String(id)) => id,
        _ => return Err(("-".to_owned(), "a step's `id` must be a string".to_owned())),
    };
    let refuse = |why: String| (id.clone(), why);
    let op = match table.remove("op") {
        Some(toml::Value::String(op)) => op,
        _ => return Err(refuse("`op` must be a string".to_owned())),
    };
    let constants = match table.remove("constants") {
        None => None,
        Some(toml::Value::String(set)) => Some(set),
        Some(_) => return Err(refuse("`constants` must be a string".to_owned())),
    };
    let mut inputs = BTreeMap::new();
    match table.remove("inputs") {
        None => {}
        Some(toml::Value::Table(bound)) => {
            for (port, value) in bound {
                let toml::Value::String(binding) = value else {
                    return Err(refuse(format!("input {port} must be a binding string")));
                };
                inputs.insert(port, binding);
            }
        }
        Some(_) => return Err(refuse("`inputs` must be a table".to_owned())),
    }
    let mut params = BTreeMap::new();
    for (name, value) in table {
        let written = match value {
            toml::Value::Integer(number) => Written::Integer(number),
            toml::Value::Float(number) => Written::Float(number),
            toml::Value::Boolean(boolean) => Written::Boolean(boolean),
            toml::Value::String(text) => Written::Text(text),
            other => {
                return Err(refuse(format!(
                    "parameter {name} is {}, and an argument is a number, a boolean or a text",
                    kind_of(&other)
                )));
            }
        };
        params.insert(name, written);
    }
    Ok(Step {
        id,
        op,
        inputs,
        constants,
        params,
    })
}

fn read_render(value: toml::Value) -> Result<Render, String> {
    let toml::Value::Table(mut table) = value else {
        return Err("each render must be a table".to_owned());
    };
    let mut text = |key: &str| match table.remove(key) {
        Some(toml::Value::String(text)) => Ok(text),
        _ => Err(format!("a render's `{key}` must be a string")),
    };
    let profile = text("for")?;
    let out = text("out")?;
    if let Some(key) = table.keys().next() {
        return Err(format!("`{key}` is not a key of a render"));
    }
    Ok(Render { profile, out })
}

/// A value as written, as the type of its argument takes it: a whole number is a number
/// where the argument takes a number, as rascal resolved it, so the two spellings record
/// the same; a whole number where it takes one must be written as one.
pub fn typed(value_type: &ValueType, written: &Written) -> Result<Value, String> {
    let value = match (value_type, written) {
        (ValueType::Number { .. }, Written::Integer(number)) => Value::Number(*number as f64),
        (_, Written::Integer(number)) => Value::Integer(*number),
        (_, Written::Float(number)) => Value::Number(*number),
        (_, Written::Boolean(boolean)) => Value::Boolean(*boolean),
        (_, Written::Text(text)) => Value::Text(text.clone()),
    };
    value_type.accepts(&value)?;
    Ok(value)
}

/// A value given as a word on a command line (`--set <step>.<param>=<value>`), as the
/// type of its argument takes it.
pub fn typed_word(value_type: &ValueType, word: &str) -> Result<Value, String> {
    let written = match value_type {
        ValueType::Number { .. } => match word.parse::<f64>() {
            Ok(number) => Written::Float(number),
            Err(_) => return Err(format!("{word:?} is not a number")),
        },
        ValueType::Integer { .. } => match word.parse::<i64>() {
            Ok(number) => Written::Integer(number),
            Err(_) => return Err(format!("{word:?} is not a whole number")),
        },
        ValueType::Boolean => match word {
            "true" => Written::Boolean(true),
            "false" => Written::Boolean(false),
            _ => return Err(format!("{word:?} is not true or false")),
        },
        ValueType::Text | ValueType::Choice { .. } => Written::Text(word.to_owned()),
    };
    typed(value_type, &written)
}

/// An argument a checked step gives: a value, the text of a declared text input, or what
/// an earlier step measured, by its id.
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Value(Value),
    Input(String),
    Measured(String),
}

/// One step, checked.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckedStep {
    pub id: String,
    pub action: String,
    pub inputs: Vec<(String, Binding)>,
    /// Each argument the step sets, its constant set among them, under `constants`.
    pub arguments: Vec<(String, Param)>,
    pub constants: Option<String>,
}

/// One output, checked: its type, and whether it is several artifacts.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub name: String,
    pub binding: Binding,
    pub artifact: ArtifactType,
    pub many: bool,
}

/// A recipe checked against the actions of a registry: what the runner runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    pub name: String,
    pub purpose: String,
    pub when: String,
    pub inputs: BTreeMap<String, Input>,
    pub steps: Vec<CheckedStep>,
    pub outputs: Vec<Output>,
    /// Whether its last step answers with a verdict, which the recipe hands back.
    pub verdict: bool,
    /// Whether its last step declares findings, which the recipe hands back too.
    pub findings: bool,
}

impl Checked {
    /// The descriptor of the step's action in this registry.
    pub fn descriptor_of<'a>(
        &self,
        step: &CheckedStep,
        registry: &'a Registry,
    ) -> Option<&'a ActionDescriptor> {
        registry.get(&step.action).map(|action| action.descriptor())
    }
}
