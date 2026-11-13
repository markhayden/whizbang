//! The action descriptor (`SPEC.md` section 3, ADR 0003): what an action is, takes,
//! returns and needs, said once, statically.
//!
//! A descriptor holds no runtime state: no installed model, no configured backend, no
//! reachability. It is the same on every machine for the same build, and it is what
//! `whizbang describe` prints and the reference is generated from.
//!
//! A descriptor is made through a [`Draft`] and exists only once [`Draft::build`] has
//! checked it. There is no other way to make one: its fields are private and it is never
//! read from data. So everything that holds a descriptor (a registry, a description, the
//! reference) holds one that keeps every rule this module states.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize, Serializer};

use crate::artifact::ArtifactType;

use super::text;

/// The level an action is at (`SPEC.md`, "Primitives, recipes and loops").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// One step: a building block.
    Primitive,
    /// A named, typed sequence of steps. Recipes are what is advertised first.
    Recipe,
    /// A loop across tools.
    Loop,
}

/// What a model does for an action. The request and the response of each are the
/// capability interfaces; a descriptor only names which an action needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Segment,
    Fill,
    Judge,
}

impl Capability {
    /// The capability's name, as a description spells it.
    pub fn name(self) -> &'static str {
        match self {
            Capability::Segment => "segment",
            Capability::Fill => "fill",
            Capability::Judge => "judge",
        }
    }
}

/// The value of an argument.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Value {
    Number(f64),
    Integer(i64),
    Boolean(bool),
    Text(String),
}

impl Value {
    pub fn text(text: &str) -> Value {
        Value::Text(text.to_owned())
    }
}

/// The type of an argument: what values it accepts.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ValueType {
    /// A finite number, within the bounds, each inclusive, where there is one.
    Number {
        min: Option<f64>,
        max: Option<f64>,
    },
    /// A whole number, within the bounds, each inclusive, where there is one.
    Integer {
        min: Option<i64>,
        max: Option<i64>,
    },
    Boolean,
    Text,
    /// One of these, each of them a name.
    Choice {
        choices: Vec<String>,
    },
}

impl ValueType {
    pub fn number(min: Option<f64>, max: Option<f64>) -> ValueType {
        ValueType::Number { min, max }
    }

    pub fn integer(min: Option<i64>, max: Option<i64>) -> ValueType {
        ValueType::Integer { min, max }
    }

    pub fn choice(choices: &[&str]) -> ValueType {
        ValueType::Choice {
            choices: choices.iter().map(|choice| (*choice).to_owned()).collect(),
        }
    }

    /// Whether the type accepts the value. A value is accepted by its own type alone: a
    /// whole number is not a number here, and a text is never a boolean. A door that
    /// reads looser input decides what it meant before it makes a value.
    pub fn accepts(&self, value: &Value) -> Result<(), String> {
        match (self, value) {
            (ValueType::Number { min, max }, Value::Number(number)) => {
                if !number.is_finite() {
                    return Err(format!("{number} is not a finite number"));
                }
                if min.is_some_and(|min| *number < min) || max.is_some_and(|max| *number > max) {
                    return Err(format!("{number} is outside {}", bounds(min, max)));
                }
                Ok(())
            }
            (ValueType::Integer { min, max }, Value::Integer(number)) => {
                if min.is_some_and(|min| *number < min) || max.is_some_and(|max| *number > max) {
                    return Err(format!("{number} is outside {}", bounds(min, max)));
                }
                Ok(())
            }
            (ValueType::Boolean, Value::Boolean(_)) | (ValueType::Text, Value::Text(_)) => Ok(()),
            (ValueType::Choice { choices }, Value::Text(choice)) => {
                if choices.contains(choice) {
                    Ok(())
                } else {
                    Err(format!("{choice:?} is not one of {}", choices.join(", ")))
                }
            }
            (expected, found) => Err(format!(
                "{} where {} is required",
                found.kind(),
                expected.kind()
            )),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            ValueType::Number { .. } => "a number",
            ValueType::Integer { .. } => "a whole number",
            ValueType::Boolean => "a boolean",
            ValueType::Text => "a text",
            ValueType::Choice { .. } => "a choice",
        }
    }

    /// Whether the type accepts anything at all.
    fn check(&self) -> Result<(), String> {
        match self {
            ValueType::Number { min, max } => {
                if [min, max].into_iter().flatten().any(|b| !b.is_finite()) {
                    return Err("a bound is not a finite number".to_owned());
                }
                if let (Some(min), Some(max)) = (min, max)
                    && min > max
                {
                    return Err(format!(
                        "its lower bound {min} is above its upper bound {max}"
                    ));
                }
            }
            ValueType::Integer { min, max } => {
                if let (Some(min), Some(max)) = (min, max)
                    && min > max
                {
                    return Err(format!(
                        "its lower bound {min} is above its upper bound {max}"
                    ));
                }
            }
            ValueType::Boolean | ValueType::Text => {}
            ValueType::Choice { choices } => {
                if choices.is_empty() {
                    return Err("it offers no choice".to_owned());
                }
                let mut seen = BTreeSet::new();
                for choice in choices {
                    text::action_name(choice)?;
                    if !seen.insert(choice) {
                        return Err(format!("it offers {choice} twice"));
                    }
                }
            }
        }
        Ok(())
    }
}

impl Value {
    fn kind(&self) -> &'static str {
        match self {
            Value::Number(_) => "a number",
            Value::Integer(_) => "a whole number",
            Value::Boolean(_) => "a boolean",
            Value::Text(_) => "a text",
        }
    }
}

fn bounds<T: std::fmt::Display>(min: &Option<T>, max: &Option<T>) -> String {
    match (min, max) {
        (Some(min), Some(max)) => format!("{min} to {max}"),
        (Some(min), None) => format!("{min} and above"),
        (None, Some(max)) => format!("{max} and below"),
        (None, None) => "every value".to_owned(),
    }
}

/// One argument of an action: a named value that is not an artifact.
///
/// It is required, or it has a default, or it is neither: an argument a call may leave
/// out and that then has no value. Only the last kind can be what a [`When`] names.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Argument {
    name: String,
    purpose: String,
    #[serde(rename = "type")]
    value_type: ValueType,
    required: bool,
    default: Option<Value>,
}

impl Argument {
    /// An argument every call gives.
    pub fn required(name: &str, purpose: &str, value_type: ValueType) -> Argument {
        Argument {
            name: name.to_owned(),
            purpose: purpose.to_owned(),
            value_type,
            required: true,
            default: None,
        }
    }

    /// An argument a call may leave out, and that then has no value.
    pub fn optional(name: &str, purpose: &str, value_type: ValueType) -> Argument {
        Argument {
            required: false,
            ..Argument::required(name, purpose, value_type)
        }
    }

    /// An argument a call may leave out, and that then has this value.
    pub fn with_default(
        name: &str,
        purpose: &str,
        value_type: ValueType,
        default: Value,
    ) -> Argument {
        Argument {
            default: Some(default),
            ..Argument::optional(name, purpose, value_type)
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn value_type(&self) -> &ValueType {
        &self.value_type
    }

    pub fn is_required(&self) -> bool {
        self.required
    }

    pub fn default(&self) -> Option<&Value> {
        self.default.as_ref()
    }

    /// Whether a call may leave the argument out and so leave it without a value.
    fn may_be_absent(&self) -> bool {
        !self.required && self.default.is_none()
    }
}

/// How many artifacts a port takes or returns: at least `min`, and at most `max` where
/// there is one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Count {
    min: u32,
    max: Option<u32>,
}

impl Count {
    pub fn exactly(count: u32) -> Count {
        Count::between(count, count)
    }

    pub fn between(min: u32, max: u32) -> Count {
        Count {
            min,
            max: Some(max),
        }
    }

    /// At least this many, with no bound above. For an input only: every output is
    /// bounded.
    pub fn at_least(min: u32) -> Count {
        Count { min, max: None }
    }

    pub fn min(self) -> u32 {
        self.min
    }

    pub fn max(self) -> Option<u32> {
        self.max
    }

    /// A count something can meet: at least one may be given, and no more are needed
    /// than are allowed.
    fn check(self) -> Result<(), String> {
        match self.max {
            Some(0) => Err("it can never hold anything".to_owned()),
            Some(max) if self.min > max => Err(format!("it needs {} and allows {max}", self.min)),
            _ => Ok(()),
        }
    }
}

/// When something an action declares applies: always, or by whether a call gives one of
/// the action's arguments. It is how a descriptor says that a capability is needed, or an
/// output returned, only for some arguments, and it is decided from the call alone.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    Always,
    /// The call gives this argument.
    Given(String),
    /// The call leaves this argument out.
    NotGiven(String),
}

impl When {
    pub fn given(argument: &str) -> When {
        When::Given(argument.to_owned())
    }

    pub fn not_given(argument: &str) -> When {
        When::NotGiven(argument.to_owned())
    }

    /// Whether it applies to a call that gives these arguments.
    pub fn holds(&self, given: &BTreeSet<String>) -> bool {
        match self {
            When::Always => true,
            When::Given(argument) => given.contains(argument),
            When::NotGiven(argument) => !given.contains(argument),
        }
    }

    fn argument(&self) -> Option<&str> {
        match self {
            When::Always => None,
            When::Given(argument) | When::NotGiven(argument) => Some(argument),
        }
    }
}

/// A port of the same action, by name: one a call gives, or one the action returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortRef {
    Input(String),
    Output(String),
}

impl PortRef {
    pub fn input(port: &str) -> PortRef {
        PortRef::Input(port.to_owned())
    }

    pub fn output(port: &str) -> PortRef {
        PortRef::Output(port.to_owned())
    }
}

impl std::fmt::Display for PortRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortRef::Input(port) => write!(f, "the input {port}"),
            PortRef::Output(port) => write!(f, "the output {port}"),
        }
    }
}

fn artifact_name<S: Serializer>(artifact: &ArtifactType, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(artifact.name())
}

/// A typed artifact port a call gives: required, optional or variadic.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputPort {
    name: String,
    #[serde(serialize_with = "artifact_name")]
    artifact: ArtifactType,
    purpose: String,
    count: Count,
    belongs_to: Option<String>,
}

impl InputPort {
    /// A port that takes exactly one artifact.
    pub fn required(name: &str, artifact: ArtifactType, purpose: &str) -> InputPort {
        InputPort::variadic(name, artifact, purpose, Count::exactly(1))
    }

    /// A port that takes one artifact or none.
    pub fn optional(name: &str, artifact: ArtifactType, purpose: &str) -> InputPort {
        InputPort::variadic(name, artifact, purpose, Count::between(0, 1))
    }

    /// A port that takes as many artifacts as the count allows.
    pub fn variadic(name: &str, artifact: ArtifactType, purpose: &str, count: Count) -> InputPort {
        InputPort {
            name: name.to_owned(),
            artifact,
            purpose: purpose.to_owned(),
            count,
            belongs_to: None,
        }
    }

    /// The artifact given here belongs to the image given at this other input port: a
    /// mask to protect, on the image being worked on.
    pub fn on(mut self, image: &str) -> InputPort {
        self.belongs_to = Some(image.to_owned());
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn artifact(&self) -> ArtifactType {
        self.artifact
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn count(&self) -> Count {
        self.count
    }

    /// The input port whose image the artifact given here must belong to, if it says.
    pub fn belongs_to(&self) -> Option<&str> {
        self.belongs_to.as_deref()
    }

    /// A port that takes exactly one image: what another port can belong to or be
    /// derived from.
    fn is_one_image(&self) -> bool {
        self.artifact == ArtifactType::Image && self.count == Count::exactly(1)
    }
}

/// A typed artifact port an action returns, with its bound.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutputPort {
    name: String,
    #[serde(serialize_with = "artifact_name")]
    artifact: ArtifactType,
    purpose: String,
    count: Count,
    when: When,
    belongs_to: Option<PortRef>,
    derived_from: Option<PortRef>,
    rebound_onto: Option<String>,
}

impl OutputPort {
    /// A port that returns exactly one artifact.
    pub fn one(name: &str, artifact: ArtifactType, purpose: &str) -> OutputPort {
        OutputPort::counted(name, artifact, purpose, Count::exactly(1))
    }

    /// A port that returns as many artifacts as the count allows. The count has an upper
    /// bound: an action never returns an unbounded number of anything.
    pub fn counted(name: &str, artifact: ArtifactType, purpose: &str, count: Count) -> OutputPort {
        OutputPort {
            name: name.to_owned(),
            artifact,
            purpose: purpose.to_owned(),
            count,
            when: When::Always,
            belongs_to: None,
            derived_from: None,
            rebound_onto: None,
        }
    }

    /// The port returns something only when this holds for the call.
    pub fn when(mut self, when: When) -> OutputPort {
        self.when = when;
        self
    }

    /// What is returned here belongs to the image at this port: one the call gave, or
    /// one the action returns.
    pub fn on(mut self, image: PortRef) -> OutputPort {
        self.belongs_to = Some(image);
        self
    }

    /// The image returned here was made from the image at this port, pixel for pixel in
    /// place: the composite a fill makes (ADR 0002, "the checked rebind").
    pub fn derived_from(mut self, image: PortRef) -> OutputPort {
        self.derived_from = Some(image);
        self
    }

    /// When the action also returns the image at this output port, which it derived from
    /// the image this mask is on, the mask is returned rebound onto that image.
    pub fn rebound_onto(mut self, image: &str) -> OutputPort {
        self.rebound_onto = Some(image.to_owned());
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn artifact(&self) -> ArtifactType {
        self.artifact
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn count(&self) -> Count {
        self.count
    }

    /// When the port returns something.
    pub fn applies(&self) -> &When {
        &self.when
    }

    /// The port whose image what is returned here belongs to, if it says.
    pub fn belongs_to(&self) -> Option<&PortRef> {
        self.belongs_to.as_ref()
    }

    /// For an image: the port whose image it was derived from, pixel for pixel in place.
    pub fn parent(&self) -> Option<&PortRef> {
        self.derived_from.as_ref()
    }

    /// For a mask: the derived image, among the outputs, that it is rebound onto when the
    /// action returns that image.
    pub fn rebinds_onto(&self) -> Option<&str> {
        self.rebound_onto.as_deref()
    }
}

/// A capability an action needs, the features it needs of it, when, and which argument
/// of a call, if any, chooses the backend that answers (ADR 0006, "Resolution").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Requirement {
    capability: Capability,
    features: Vec<String>,
    when: When,
    chosen_by: Option<String>,
}

impl Requirement {
    /// The capability, needed by every call, with no feature asked of it.
    pub fn new(capability: Capability) -> Requirement {
        Requirement {
            capability,
            features: Vec::new(),
            when: When::Always,
            chosen_by: None,
        }
    }

    /// With a feature the capability must have, by name.
    pub fn feature(mut self, feature: &str) -> Requirement {
        self.features.push(feature.to_owned());
        self
    }

    /// Needed only when this holds for the call.
    pub fn when(mut self, when: When) -> Requirement {
        self.when = when;
        self
    }

    pub fn capability(&self) -> Capability {
        self.capability
    }

    pub fn features(&self) -> &[String] {
        &self.features
    }

    /// When the capability is needed.
    pub fn applies(&self) -> &When {
        &self.when
    }

    /// The backend is the one a call names in this argument, a text the call may leave
    /// out; a call that leaves it out gets the default. Without this, the default
    /// always answers.
    pub fn chosen_by(mut self, argument: &str) -> Requirement {
        self.chosen_by = Some(argument.to_owned());
        self
    }

    /// The argument that chooses the backend, when there is one.
    pub fn chooser(&self) -> Option<&str> {
        self.chosen_by.as_deref()
    }
}

/// What an action is, takes, returns and needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionDescriptor {
    name: String,
    version: String,
    kind: Kind,
    purpose: String,
    arguments: Vec<Argument>,
    inputs: Vec<InputPort>,
    outputs: Vec<OutputPort>,
    needs: Vec<Requirement>,
    concurrent: bool,
}

/// Why a draft is not a descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptorError {
    /// The action's name, as the draft gave it.
    pub action: String,
    pub why: String,
}

impl std::fmt::Display for DescriptorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "not an action descriptor ({:?}): {}",
            self.action, self.why
        )
    }
}

impl std::error::Error for DescriptorError {}

/// A descriptor being written. Nothing is checked until [`Draft::build`].
#[derive(Debug, Clone)]
pub struct Draft(ActionDescriptor);

impl Draft {
    /// An argument, listed after those already given.
    pub fn argument(mut self, argument: Argument) -> Draft {
        self.0.arguments.push(argument);
        self
    }

    /// An input port, listed after those already given.
    pub fn input(mut self, port: InputPort) -> Draft {
        self.0.inputs.push(port);
        self
    }

    /// An output port, listed after those already given.
    pub fn output(mut self, port: OutputPort) -> Draft {
        self.0.outputs.push(port);
        self
    }

    /// A capability the action needs.
    pub fn needs(mut self, requirement: Requirement) -> Draft {
        self.0.needs.push(requirement);
        self
    }

    /// Whether two runs of the action may execute at the same time. Unless it says so,
    /// they may not.
    pub fn concurrent(mut self, concurrent: bool) -> Draft {
        self.0.concurrent = concurrent;
        self
    }

    /// The descriptor, if the draft keeps every rule; otherwise the first rule it breaks.
    pub fn build(self) -> Result<ActionDescriptor, DescriptorError> {
        match self.0.check() {
            Ok(()) => Ok(self.0),
            Err(why) => Err(DescriptorError {
                action: self.0.name,
                why,
            }),
        }
    }
}

impl ActionDescriptor {
    /// Starts a descriptor: the action's name, its version, its level and what it is for.
    pub fn draft(name: &str, version: &str, kind: Kind, purpose: &str) -> Draft {
        Draft(ActionDescriptor {
            name: name.to_owned(),
            version: version.to_owned(),
            kind,
            purpose: purpose.to_owned(),
            arguments: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            needs: Vec::new(),
            concurrent: false,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The version of the action itself, which an artifact's origin records with its name.
    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    /// The arguments, in the order they were declared.
    pub fn arguments(&self) -> &[Argument] {
        &self.arguments
    }

    pub fn argument(&self, name: &str) -> Option<&Argument> {
        self.arguments.iter().find(|argument| argument.name == name)
    }

    /// The input ports, in the order they were declared.
    pub fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    pub fn input(&self, name: &str) -> Option<&InputPort> {
        self.inputs.iter().find(|port| port.name == name)
    }

    /// The output ports, in the order they were declared.
    pub fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    pub fn output(&self, name: &str) -> Option<&OutputPort> {
        self.outputs.iter().find(|port| port.name == name)
    }

    /// Every capability the action can need, whatever the call.
    pub fn needs(&self) -> &[Requirement] {
        &self.needs
    }

    /// Whether two runs of the action may execute at the same time.
    pub fn concurrent(&self) -> bool {
        self.concurrent
    }

    /// The capabilities a call needs that gives exactly these arguments, by name, in the
    /// order they were declared. No capability is listed twice.
    pub fn needs_for(&self, given: &BTreeSet<String>) -> Vec<&Requirement> {
        self.needs
            .iter()
            .filter(|requirement| requirement.when.holds(given))
            .collect()
    }

    /// The output ports that return something for a call that gives exactly these
    /// arguments, by name, in the order they were declared.
    pub fn outputs_for(&self, given: &BTreeSet<String>) -> Vec<&OutputPort> {
        self.outputs
            .iter()
            .filter(|port| port.when.holds(given))
            .collect()
    }

    /// Every rule a descriptor keeps. The first one broken is the answer.
    fn check(&self) -> Result<(), String> {
        text::action_name(&self.name)?;
        text::version(&self.version)?;
        text::purpose(&self.purpose)?;

        // What a call gives, arguments and input ports, is given under one set of names.
        let mut given = BTreeSet::new();
        for argument in &self.arguments {
            let name = &argument.name;
            let of = |why: String| format!("the argument {name}: {why}");
            text::field_name(name).map_err(of)?;
            text::purpose(&argument.purpose).map_err(of)?;
            argument.value_type.check().map_err(of)?;
            if let Some(default) = &argument.default {
                if argument.required {
                    return Err(of("it is required and has a default".to_owned()));
                }
                argument
                    .value_type
                    .accepts(default)
                    .map_err(|why| of(format!("its default is refused by its type: {why}")))?;
                // A default is printed with the description, so a text is held as a
                // purpose is. A choice is a name already.
                if let (ValueType::Text, Value::Text(line)) = (&argument.value_type, default) {
                    text::plain_line("default", line)
                        .map_err(|why| of(format!("its default is no plain line: {why}")))?;
                }
            }
            if !given.insert(name.as_str()) {
                return Err(format!("the name {name} is declared twice"));
            }
        }
        for port in &self.inputs {
            let name = &port.name;
            let of = |why: String| format!("the input {name}: {why}");
            text::field_name(name).map_err(of)?;
            text::purpose(&port.purpose).map_err(of)?;
            port.count.check().map_err(of)?;
            if !given.insert(name.as_str()) {
                return Err(format!(
                    "the name {name} is declared twice among the arguments and the inputs"
                ));
            }
        }
        for port in &self.inputs {
            if let Some(image) = &port.belongs_to {
                let name = &port.name;
                if port.artifact == ArtifactType::Image {
                    return Err(format!(
                        "the input {name} is an image, and an image belongs to no other"
                    ));
                }
                if self.one_image_in(image).is_none() {
                    return Err(format!(
                        "the input {name} belongs to {image}, which is no input that takes \
                         exactly one image"
                    ));
                }
            }
        }

        let mut returned = BTreeSet::new();
        for port in &self.outputs {
            let name = &port.name;
            let of = |why: String| format!("the output {name}: {why}");
            text::field_name(name).map_err(of)?;
            text::purpose(&port.purpose).map_err(of)?;
            port.count.check().map_err(of)?;
            if port.count.max.is_none() {
                return Err(of("it has no upper bound".to_owned()));
            }
            if !returned.insert(name.as_str()) {
                return Err(format!("the output {name} is declared twice"));
            }
            self.check_when(&port.when)
                .map_err(|why| format!("the output {name} {why}"))?;
        }
        for port in &self.outputs {
            let name = &port.name;
            if let Some(image) = &port.belongs_to {
                if port.artifact == ArtifactType::Image {
                    return Err(format!(
                        "the output {name} is an image, and an image belongs to no other"
                    ));
                }
                self.the_image(image, &port.when)
                    .map_err(|why| format!("the output {name} belongs to {image}, {why}"))?;
            }
            if let Some(parent) = &port.derived_from {
                if port.artifact != ArtifactType::Image {
                    return Err(format!(
                        "the output {name} is {} and only an image is derived from another",
                        port.artifact.with_article()
                    ));
                }
                self.check_no_circle(port)?;
                self.the_image(parent, &port.when)
                    .map_err(|why| format!("the output {name} is derived from {parent}, {why}"))?;
            }
            if let Some(onto) = &port.rebound_onto {
                if port.artifact != ArtifactType::Mask {
                    return Err(format!(
                        "the output {name} is {} and only a mask is rebound",
                        port.artifact.with_article()
                    ));
                }
                let Some(on) = &port.belongs_to else {
                    return Err(format!(
                        "the output {name} is rebound onto {onto} and does not say which \
                         image it is on"
                    ));
                };
                let target = self
                    .output(onto)
                    .filter(|image| image.artifact == ArtifactType::Image);
                if target.and_then(|image| image.derived_from.as_ref()) != Some(on) {
                    return Err(format!(
                        "the output {name} is rebound onto {onto}, which is no output image \
                         derived from {on}"
                    ));
                }
                if target.is_some_and(|image| image.count != Count::exactly(1)) {
                    return Err(format!(
                        "the output {name} is rebound onto {onto}, which does not return \
                         exactly one image"
                    ));
                }
            }
        }

        for (index, requirement) in self.needs.iter().enumerate() {
            let capability = requirement.capability.name();
            self.check_when(&requirement.when)
                .map_err(|why| format!("the need for {capability} {why}"))?;
            if let Some(name) = &requirement.chosen_by {
                let chooses = |why: &str| {
                    Err(format!(
                        "the need for {capability} is chosen by {name}, which {why}"
                    ))
                };
                if requirement.when == When::NotGiven(name.clone()) {
                    return chooses("it is needed only without");
                }
                match self.argument(name) {
                    None => return chooses("is no argument"),
                    Some(argument)
                        if argument.value_type != ValueType::Text || !argument.may_be_absent() =>
                    {
                        return chooses("is not a text that a call may leave out");
                    }
                    Some(_) => {}
                }
            }
            let mut features = BTreeSet::new();
            for feature in &requirement.features {
                text::action_name(feature)
                    .map_err(|why| format!("a feature of {capability}: {why}"))?;
                if !features.insert(feature) {
                    return Err(format!(
                        "the need for {capability} names the feature {feature} twice"
                    ));
                }
            }
            // No call needs a capability twice: two needs for one capability are stated
            // only for calls that can never be the same call.
            let apart = |other: &Requirement| {
                matches!(
                    (&requirement.when, &other.when),
                    (When::Given(a), When::NotGiven(b)) | (When::NotGiven(a), When::Given(b))
                        if a == b
                )
            };
            if self.needs[..index]
                .iter()
                .any(|other| other.capability == requirement.capability && !apart(other))
            {
                return Err(format!(
                    "the need for {capability} is stated more than once for calls that can \
                     be the same"
                ));
            }
        }
        Ok(())
    }

    /// The input port of this name, when it takes exactly one image.
    fn one_image_in(&self, name: &str) -> Option<&InputPort> {
        self.input(name).filter(|port| port.is_one_image())
    }

    /// Whether the reference names the image something belongs to or is derived from:
    /// an input that takes exactly one image, or an output that returns exactly one
    /// image for every call that returns the thing itself (`when` is its condition).
    /// Otherwise, why not, as the end of a sentence.
    fn the_image(&self, image: &PortRef, when: &When) -> Result<(), &'static str> {
        match image {
            PortRef::Input(port) => match self.one_image_in(port) {
                Some(_) => Ok(()),
                None => Err("which is no input that takes exactly one image"),
            },
            PortRef::Output(port) => {
                let Some(output) = self
                    .output(port)
                    .filter(|output| output.artifact == ArtifactType::Image)
                else {
                    return Err("which is no output that returns an image");
                };
                if output.count != Count::exactly(1) {
                    return Err("which does not return exactly one image");
                }
                if output.when != When::Always && output.when != *when {
                    return Err("which is not returned for every call that this is");
                }
                Ok(())
            }
        }
    }

    /// A derivation ends at an input or at an output that is derived from nothing: it
    /// never comes back to where it started.
    fn check_no_circle(&self, start: &OutputPort) -> Result<(), String> {
        let mut at = start;
        for _ in 0..self.outputs.len() {
            match at.derived_from.as_ref() {
                Some(PortRef::Output(next)) => match self.output(next) {
                    Some(next) => at = next,
                    None => return Ok(()),
                },
                _ => return Ok(()),
            }
        }
        Err(format!(
            "the output {} is derived, step by step, from itself",
            start.name
        ))
    }

    /// A condition names an argument of this action that a call may leave without a
    /// value: one that is not required and has no default.
    fn check_when(&self, when: &When) -> Result<(), String> {
        let Some(name) = when.argument() else {
            return Ok(());
        };
        match self.argument(name) {
            None => Err(format!("depends on {name}, which is no argument")),
            Some(argument) if !argument.may_be_absent() => Err(format!(
                "depends on {name}, which every call has a value for"
            )),
            Some(_) => Ok(()),
        }
    }
}
