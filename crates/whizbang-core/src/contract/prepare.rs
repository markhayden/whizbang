//! Prepare (`SPEC.md` section 3, ADR 0004): can this exact call run now?
//!
//! Preparing a call checks it against its action's descriptor, finds its inputs in the
//! store, and asks which model would answer each capability it needs. It answers with a
//! [`PreparedCall`], which says whether the call is ready and carries a fingerprint of
//! everything that was seen, or with a [`Refusal`].
//!
//! It has no side effects and spends nothing, and this is so by what it is given: a read
//! of the store ([`Contents`], which has no way to write), and [`Backends`], which
//! answers what is configured and has no way to send a request or to reserve.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::artifact::{self, Artifact, ArtifactError, ArtifactId, ArtifactType, Contents};
use crate::identity::{Binding, BindingId, DecodeLimits, Digest, Encoder, Fields};

use crate::store;

use super::{ActionDescriptor, Call, Capability, Count, InputPort, Value};

/// Which model answers a capability: the backend, the model, and the model's version or
/// reported identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelIdentity {
    backend: String,
    model: String,
    version: String,
}

impl ModelIdentity {
    pub fn new(backend: &str, model: &str, version: &str) -> ModelIdentity {
        ModelIdentity {
            backend: backend.to_owned(),
            model: model.to_owned(),
            version: version.to_owned(),
        }
    }

    pub fn backend(&self) -> &str {
        &self.backend
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

/// Why nothing answers a capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable {
    why: String,
}

impl Unavailable {
    pub fn new(why: &str) -> Unavailable {
        Unavailable {
            why: why.to_owned(),
        }
    }

    pub fn why(&self) -> &str {
        &self.why
    }
}

/// What prepare may ask about capabilities: which model would answer one, with these
/// features. It is answered from what is configured, with no network; nothing is sent to
/// a model and nothing is reserved. How a choice and a default are resolved is the
/// capability interfaces' to define.
pub trait Backends: Send + Sync {
    fn resolve(
        &self,
        capability: Capability,
        features: &[String],
    ) -> Result<ModelIdentity, Unavailable>;
}

/// No backend at all: what a build has until one is configured.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoBackends;

impl Backends for NoBackends {
    fn resolve(&self, capability: Capability, _: &[String]) -> Result<ModelIdentity, Unavailable> {
        Err(Unavailable::new(&format!(
            "no backend is configured for {}",
            capability.name()
        )))
    }
}

/// One capability a call needs, and the model that would answer it, if one would.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Need {
    capability: Capability,
    features: Vec<String>,
    model: Option<ModelIdentity>,
}

impl Need {
    pub fn capability(&self) -> Capability {
        self.capability
    }

    pub fn features(&self) -> &[String] {
        &self.features
    }

    /// The model that would answer, or none when nothing does.
    pub fn model(&self) -> Option<&ModelIdentity> {
        self.model.as_ref()
    }
}

/// One capability a call needs and nothing answers, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    capability: Capability,
    features: Vec<String>,
    why: String,
}

impl Missing {
    pub fn capability(&self) -> Capability {
        self.capability
    }

    pub fn features(&self) -> &[String] {
        &self.features
    }

    pub fn why(&self) -> &str {
        &self.why
    }
}

/// Whether a call can run now, with these exact arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    /// It cannot: each capability it needs that nothing answers. Never empty.
    NotReady(Vec<Missing>),
}

/// One input of a prepared call: the port, the type the port takes, the artifact, and
/// the binding the artifact belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInput {
    port: String,
    artifact: ArtifactType,
    id: ArtifactId,
    binding: BindingId,
}

impl ResolvedInput {
    pub fn port(&self) -> &str {
        &self.port
    }

    pub fn artifact(&self) -> ArtifactType {
        self.artifact
    }

    pub fn id(&self) -> ArtifactId {
        self.id
    }

    /// The identity of the binding the artifact belongs to, as it was read from the
    /// artifact. The artifact's identity names the artifact, so the fingerprint does not
    /// hash this beside it.
    pub fn binding(&self) -> BindingId {
        self.binding
    }
}

/// A call that was checked and resolved, and what was seen while doing it.
///
/// It is made by preparing a call and in no other way.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedCall {
    action: String,
    version: String,
    arguments: Vec<(String, Value)>,
    inputs: Vec<ResolvedInput>,
    needs: Vec<Need>,
    outputs: Vec<String>,
    readiness: Readiness,
    config: Digest,
    fingerprint: Digest,
}

impl PreparedCall {
    /// The action's name.
    pub fn action(&self) -> &str {
        &self.action
    }

    /// The action's version, as its descriptor gives it.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Every argument that has a value, in the descriptor's order: what the call gave,
    /// and the default of what it left out. An argument left out that has no default is
    /// not here.
    pub fn arguments(&self) -> &[(String, Value)] {
        &self.arguments
    }

    /// Every input, by port in the descriptor's order, and within a port in the order
    /// the call gave them. Each was found in the store as the type its port takes.
    pub fn inputs(&self) -> &[ResolvedInput] {
        &self.inputs
    }

    /// The capabilities this call needs, in the descriptor's order.
    pub fn needs(&self) -> &[Need] {
        &self.needs
    }

    /// The output ports that return something for this call, in the descriptor's order.
    pub fn outputs(&self) -> &[String] {
        &self.outputs
    }

    pub fn readiness(&self) -> &Readiness {
        &self.readiness
    }

    /// The digest of the config the preparation was made under.
    pub fn config(&self) -> Digest {
        self.config
    }

    /// The digest of everything this preparation saw: the action and its version, every
    /// argument's value, every input's identity, the config, and for each capability
    /// needed the model that would answer or that none would. Two preparations with one
    /// fingerprint saw the same things; a preparation whose fingerprint is no longer what
    /// preparing the call again gives is stale.
    pub fn fingerprint(&self) -> Digest {
        self.fingerprint
    }
}

/// What is wrong with an input, so that a door can tell the cases apart without reading
/// a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputFault {
    /// The store holds no artifact of the port's type with this identity.
    Missing,
    /// The store holds this identity as another type: `found` names it with its article,
    /// as in "a mask".
    WrongType { found: String },
    /// The artifact belongs to another image, or to another space, than the image at the
    /// port it must belong to.
    WrongImage { why: String },
    /// The store holds something under the artifact's name that is not that artifact:
    /// not a canonical file of the type, one over the limits it is read within, or one
    /// with another identity.
    NotTheArtifact { why: String },
}

impl std::fmt::Display for InputFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputFault::Missing => f.write_str("the store does not hold it"),
            InputFault::WrongType { found } => write!(f, "this is {found}"),
            InputFault::WrongImage { why } => write!(f, "wrong image: {why}"),
            InputFault::NotTheArtifact { why } => f.write_str(why),
        }
    }
}

/// The store failed while a call was being answered. This says nothing about the call:
/// the same call may be answered once the store is sound.
///
/// Its text holds no path, so it can be handed to a caller who is not on this machine:
/// a file of the store is named by its last part, a digest or a generation's number.
/// The whole failure, paths included, is [`StoreFailure::cause`].
#[derive(Debug, Clone)]
pub struct StoreFailure {
    what: String,
    cause: Arc<store::Error>,
}

impl StoreFailure {
    pub(super) fn new(cause: store::Error) -> StoreFailure {
        StoreFailure {
            what: cause.without_paths(),
            cause: Arc::new(cause),
        }
    }

    /// The store's own error, with the paths it names: for whoever runs the store, and
    /// never for a caller elsewhere.
    pub fn cause(&self) -> &store::Error {
        &self.cause
    }
}

/// Two failures are equal when they say the same thing.
impl PartialEq for StoreFailure {
    fn eq(&self, other: &StoreFailure) -> bool {
        self.what == other.what
    }
}

impl Eq for StoreFailure {}

impl std::fmt::Display for StoreFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.what)
    }
}

/// Why a call got no answer: it was refused, which is a judgement of the call, or the
/// store failed, which is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    Refused(Refusal),
    Store(StoreFailure),
}

impl CallError {
    /// The refusal, when the call was refused.
    pub fn refusal(&self) -> Option<&Refusal> {
        match self {
            CallError::Refused(refusal) => Some(refusal),
            CallError::Store(_) => None,
        }
    }
}

impl From<Refusal> for CallError {
    fn from(refusal: Refusal) -> CallError {
        CallError::Refused(refusal)
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Refused(refusal) => refusal.fmt(f),
            CallError::Store(failure) => write!(f, "the store failed: {failure}"),
        }
    }
}

impl std::error::Error for CallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CallError::Refused(refusal) => Some(refusal),
            CallError::Store(failure) => Some(failure.cause()),
        }
    }
}

/// Why a call was turned away before any run existed. A refusal is not an outcome
/// (`SPEC.md` section 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No action of this name is registered.
    UnknownAction { name: String },
    /// The call is not one the action's descriptor allows.
    Invalid { action: String, why: String },
    /// An input is not what its port takes: which artifact, of the type the port takes,
    /// and what is wrong with it.
    Input {
        action: String,
        port: String,
        artifact: ArtifactType,
        id: ArtifactId,
        fault: InputFault,
    },
    /// The call is well formed and cannot run now: what is missing.
    NotReady {
        action: String,
        missing: Vec<Missing>,
    },
    /// The call is ready, and this build cannot run an action yet.
    NothingRunsYet,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::UnknownAction { name } => write!(f, "no action named {name} is registered"),
            Refusal::Invalid { action, why } => write!(f, "not a call of {action}: {why}"),
            Refusal::Input {
                action,
                port,
                artifact,
                id,
                fault,
            } => {
                write!(f, "{action}: the input {port}: ")?;
                match fault {
                    InputFault::Missing => write!(f, "the store holds no {artifact} {id}"),
                    InputFault::WrongType { found } => write!(
                        f,
                        "{} is required, and this is {found}",
                        artifact.with_article()
                    ),
                    InputFault::WrongImage { .. } | InputFault::NotTheArtifact { .. } => {
                        fault.fmt(f)
                    }
                }
            }
            Refusal::NotReady { action, missing } => {
                write!(f, "{action} cannot run now:")?;
                for (index, one) in missing.iter().enumerate() {
                    let comma = if index == 0 { "" } else { ";" };
                    write!(f, "{comma} it needs {}", one.capability.name())?;
                    if !one.features.is_empty() {
                        write!(f, " with {}", one.features.join(", "))?;
                    }
                    write!(f, ", and {}", one.why)?;
                }
                Ok(())
            }
            Refusal::NothingRunsYet => {
                write!(
                    f,
                    "the call is ready, and this build cannot run an action yet"
                )
            }
        }
    }
}

impl std::error::Error for Refusal {}

/// The binding an artifact belongs to.
fn binding_of(artifact: &Artifact) -> Binding {
    match artifact {
        Artifact::Mask(mask) => mask.binding().clone(),
        Artifact::Location(location) => location.binding().clone(),
        Artifact::Image(image) => image.binding(),
        Artifact::BandProfile(band) => band.binding().clone(),
        Artifact::Series(series) => series.binding().clone(),
        Artifact::Plan(plan) => plan.binding().clone(),
    }
}

/// How many a port takes, in words.
fn takes(count: Count) -> String {
    match (count.min(), count.max()) {
        (min, Some(max)) if min == max => format!("{min}"),
        (min, Some(max)) => format!("{min} to {max}"),
        (min, None) => format!("{min} or more"),
    }
}

/// Prepares `call` against `descriptor`: the whole of prepare once the action is found.
pub(super) fn prepare(
    descriptor: &ActionDescriptor,
    call: &Call,
    contents: &(dyn Contents + Send + Sync),
    backends: &dyn Backends,
    config: Digest,
    limits: &DecodeLimits,
) -> Result<PreparedCall, CallError> {
    let action = descriptor.name();
    let invalid = |why: String| {
        CallError::Refused(Refusal::Invalid {
            action: action.to_owned(),
            why,
        })
    };
    // Negative zero is zero: the fingerprint hashes it so, and what is prepared is what
    // is bound.
    let settled = |value: &Value| match value {
        Value::Number(number) if *number == 0.0 => Value::Number(0.0),
        other => other.clone(),
    };

    // The arguments: only declared ones, every required one, each a value of its type.
    if let Some(name) = call
        .arguments()
        .keys()
        .find(|name| descriptor.argument(name).is_none())
    {
        return Err(invalid(format!("{name} is no argument of it")));
    }
    let mut arguments = Vec::new();
    let mut given = BTreeSet::new();
    for argument in descriptor.arguments() {
        let name = argument.name();
        match call.arguments().get(name) {
            Some(value) => {
                argument
                    .value_type()
                    .accepts(value)
                    .map_err(|why| invalid(format!("the argument {name}: {why}")))?;
                given.insert(name.to_owned());
                arguments.push((name.to_owned(), settled(value)));
            }
            None if argument.is_required() => {
                return Err(invalid(format!("the argument {name} is required")));
            }
            None => {
                if let Some(default) = argument.default() {
                    arguments.push((name.to_owned(), settled(default)));
                }
            }
        }
    }

    // The inputs: only declared ports, each with as many artifacts as it takes.
    if let Some(port) = call
        .inputs()
        .keys()
        .find(|port| descriptor.input(port).is_none())
    {
        return Err(invalid(format!("{port} is no input of it")));
    }
    let given_at = |port: &InputPort| -> &[ArtifactId] {
        call.inputs()
            .get(port.name())
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    for port in descriptor.inputs() {
        let count = given_at(port).len() as u64;
        let (min, max) = (port.count().min(), port.count().max());
        if count < u64::from(min) || max.is_some_and(|max| count > u64::from(max)) {
            return Err(invalid(format!(
                "the input {} takes {}, and was given {count}",
                port.name(),
                takes(port.count())
            )));
        }
    }

    // Each input, read from the store as the type its port takes.
    let mut inputs = Vec::new();
    let mut found: Vec<(&InputPort, Artifact)> = Vec::new();
    for port in descriptor.inputs() {
        for id in given_at(port) {
            let refuse = |fault: InputFault| {
                CallError::Refused(Refusal::Input {
                    action: action.to_owned(),
                    port: port.name().to_owned(),
                    artifact: port.artifact(),
                    id: *id,
                    fault,
                })
            };
            let artifact = artifact::get(contents, port.artifact(), *id, limits).map_err(
                |error| match error {
                    // The store failing is not a fault of the call.
                    ArtifactError::Store(error) => CallError::Store(StoreFailure::new(error)),
                    ArtifactError::Missing { .. } => refuse(InputFault::Missing),
                    ArtifactError::WrongType { found, .. } => {
                        refuse(InputFault::WrongType { found })
                    }
                    other => refuse(InputFault::NotTheArtifact {
                        why: other.to_string(),
                    }),
                },
            )?;
            inputs.push(ResolvedInput {
                port: port.name().to_owned(),
                artifact: port.artifact(),
                id: *id,
                binding: binding_of(&artifact).id(),
            });
            found.push((port, artifact));
        }
    }

    // What belongs to an image belongs to the image this call gives, and to no other:
    // the same pixels in the same space. A mask on another image of the same size is
    // refused here.
    for (port, artifact) in &found {
        let Some(on) = port.belongs_to() else {
            continue;
        };
        let Some(Artifact::Image(image)) = found
            .iter()
            .find(|(other, _)| other.name() == on)
            .map(|(_, artifact)| artifact)
        else {
            continue;
        };
        let (pixels, space) = (image.identity().pixels, image.space());
        let refuse = |why: String| {
            CallError::Refused(Refusal::Input {
                action: action.to_owned(),
                port: port.name().to_owned(),
                artifact: port.artifact(),
                id: artifact.id(),
                fault: InputFault::WrongImage { why },
            })
        };
        match artifact {
            Artifact::Mask(mask) => {
                mask.require_image(pixels, space)
                    .map_err(|error| match error {
                        ArtifactError::WrongImage(why) => refuse(why),
                        other => refuse(other.to_string()),
                    })?
            }
            other => {
                let binding = binding_of(other);
                if binding.source() != pixels {
                    return Err(refuse(format!(
                        "the {} belongs to the image {}, not to {pixels}, the image at {on}",
                        other.artifact_type(),
                        binding.source()
                    )));
                }
                if binding.source_space() != space {
                    return Err(refuse(format!(
                        "the {}'s image is in {:?}, not in {space:?}",
                        other.artifact_type(),
                        binding.source_space()
                    )));
                }
            }
        }
    }

    // What this call needs, and which model would answer each need.
    let mut needs = Vec::new();
    let mut missing = Vec::new();
    for requirement in descriptor.needs_for(&given) {
        let (capability, features) = (requirement.capability(), requirement.features());
        let model = match backends.resolve(capability, features) {
            Ok(model) => Some(model),
            Err(unavailable) => {
                missing.push(Missing {
                    capability,
                    features: features.to_vec(),
                    why: unavailable.why,
                });
                None
            }
        };
        needs.push(Need {
            capability,
            features: features.to_vec(),
            model,
        });
    }
    let readiness = if missing.is_empty() {
        Readiness::Ready
    } else {
        Readiness::NotReady(missing)
    };
    let outputs = descriptor
        .outputs_for(&given)
        .iter()
        .map(|port| port.name().to_owned())
        .collect();

    let mut prepared = PreparedCall {
        action: action.to_owned(),
        version: descriptor.version().to_owned(),
        arguments,
        inputs,
        needs,
        outputs,
        readiness,
        config,
        fingerprint: config,
    };
    prepared.fingerprint = fingerprint(&prepared);
    Ok(prepared)
}

/// The fingerprint, kind `prepared/1`, in the encoding ADR 0001 gives every identity: the
/// action and its version; the count of arguments, and each as its name and its value;
/// the count of inputs, and each as its port and its identity; the config's digest; the
/// count of needs, and each as its capability, the count of its features and each
/// feature, then 1 and the backend, the model and the version of the model that would
/// answer, or 0 where none would. A value is its kind (0 a number, 1 a whole number, 2 a
/// boolean, 3 a text) and then itself.
fn fingerprint(prepared: &PreparedCall) -> Digest {
    let mut fields = Encoder::new("prepared/1");
    fields.bytes(prepared.action.as_bytes());
    fields.bytes(prepared.version.as_bytes());
    fields.unsigned(prepared.arguments.len() as u64);
    for (name, value) in &prepared.arguments {
        fields.bytes(name.as_bytes());
        match value {
            Value::Number(number) => {
                fields.unsigned(0);
                fields.float(*number);
            }
            Value::Integer(number) => {
                fields.unsigned(1);
                fields.unsigned(u64::from_be_bytes(number.to_be_bytes()));
            }
            Value::Boolean(boolean) => {
                fields.unsigned(2);
                fields.unsigned(u64::from(*boolean));
            }
            Value::Text(text) => {
                fields.unsigned(3);
                fields.bytes(text.as_bytes());
            }
        }
    }
    fields.unsigned(prepared.inputs.len() as u64);
    for input in &prepared.inputs {
        fields.bytes(input.port.as_bytes());
        fields.digest(&input.id.0);
    }
    fields.digest(&prepared.config);
    fields.unsigned(prepared.needs.len() as u64);
    for need in &prepared.needs {
        fields.bytes(need.capability.name().as_bytes());
        fields.unsigned(need.features.len() as u64);
        for feature in &need.features {
            fields.bytes(feature.as_bytes());
        }
        match &need.model {
            Some(model) => {
                fields.unsigned(1);
                fields.bytes(model.backend.as_bytes());
                fields.bytes(model.model.as_bytes());
                fields.bytes(model.version.as_bytes());
            }
            None => fields.unsigned(0),
        }
    }
    fields.finish()
}
