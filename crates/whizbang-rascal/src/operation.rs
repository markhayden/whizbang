//! What the actions over a mask share: reading an argument, reading a mask from the
//! store, and the failure of a run that was told to stop.

use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType, Image, Location, Mask};
use whizbang_core::contract::{Context, Failure, PreparedCall, Value};
use whizbang_core::identity::{DecodeLimits, Point};

/// The number of an argument, when the call has it.
pub(crate) fn number(call: &PreparedCall, name: &str) -> Option<f64> {
    call.arguments().iter().find_map(|(n, value)| match value {
        Value::Number(number) if n == name => Some(*number),
        _ => None,
    })
}

/// The text, or the choice, of an argument, when the call has it.
pub(crate) fn text<'a>(call: &'a PreparedCall, name: &str) -> Option<&'a str> {
    call.arguments().iter().find_map(|(n, value)| match value {
        Value::Text(text) if n == name => Some(text.as_str()),
        _ => None,
    })
}

/// The identities given at a port, in the order the call gave them.
pub(crate) fn given(call: &PreparedCall, port: &str) -> Vec<ArtifactId> {
    let at = call.inputs().iter().filter(|input| input.port() == port);
    at.map(|input| input.id()).collect()
}

/// The mask of this identity, from the run's store. The store's own words for a
/// failure name its files by their paths, and a message is kept in the run's record:
/// so the words without paths.
pub(crate) fn mask(context: &Context, id: ArtifactId) -> Result<Mask, Failure> {
    artifact::get(
        context.store(),
        ArtifactType::Mask,
        id,
        &DecodeLimits::default(),
    )
    .and_then(artifact::Artifact::into_mask)
    .map_err(|error| Failure::error("input", &error.without_paths()))
}

/// The image of this identity, from the run's store, in the words of [`mask`].
pub(crate) fn image(context: &Context, id: ArtifactId) -> Result<Image, Failure> {
    let read = artifact::get(
        context.store(),
        ArtifactType::Image,
        id,
        &DecodeLimits::default(),
    );
    match read.map_err(|error| Failure::error("input", &error.without_paths()))? {
        Artifact::Image(image) => Ok(image),
        _ => Err(Failure::error("input", "what was read is not an image")),
    }
}

/// The location of this identity, from the run's store, in the words of [`mask`].
pub(crate) fn location(context: &Context, id: ArtifactId) -> Result<Location, Failure> {
    let read = artifact::get(
        context.store(),
        ArtifactType::Location,
        id,
        &DecodeLimits::default(),
    );
    match read.map_err(|error| Failure::error("input", &error.without_paths()))? {
        Artifact::Location(location) => Ok(location),
        _ => Err(Failure::error("input", "what was read is not a location")),
    }
}

/// The one identity given at a port, or the failure of a call that gives none.
pub(crate) fn one(call: &PreparedCall, port: &str) -> Result<ArtifactId, Failure> {
    given(call, port)
        .first()
        .copied()
        .ok_or_else(|| Failure::error("input", &format!("the call gives no {port}")))
}

/// Reads points in pixels of an image: points apart by `;`, each two finite numbers, X
/// and Y, apart by spaces or commas. `what` names the argument in a refusal.
pub fn points(text: &str, what: &str) -> Result<Vec<Point>, String> {
    let mut points = Vec::new();
    for part in text.split(';').filter(|part| !part.trim().is_empty()) {
        let read: Option<Vec<f64>> = part
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|word| !word.is_empty())
            .map(|word| word.parse::<f64>().ok().filter(|n| n.is_finite()))
            .collect();
        match read.as_deref() {
            Some(&[x, y]) => points.push(Point::new(x, y)),
            _ => {
                return Err(format!(
                    "{what} is points apart by ;, each two finite numbers, X and Y, in pixels"
                ));
            }
        }
    }
    Ok(points)
}

/// Whether the run was cancelled or its deadline, by the executor's clock, passed.
pub(crate) fn stopped(context: &Context) -> bool {
    context.cancellation().is_cancelled() || context.deadline_passed()
}

/// The failure of a run of `action` that stopped because it was told to.
pub(crate) fn stopped_failure(context: &Context, action: &str) -> Failure {
    if context.cancellation().is_cancelled() {
        Failure::error(
            "cancelled",
            &format!("{action} stopped: its run was cancelled"),
        )
    } else {
        Failure::error(
            "deadline",
            &format!("{action} stopped: its deadline passed"),
        )
    }
}

/// How many pixels of a plane are set.
pub(crate) fn set(plane: &[u8]) -> u64 {
    plane.iter().filter(|sample| **sample != 0).count() as u64
}

/// What one operation's command asks, as it was typed: the action it runs, its
/// arguments, and each input as its port, the flag that gave it and the identity typed.
/// Each operation's module has its own command's flags (`Args`) and says this of them, so
/// that the binary's command gains one line for each operation (plan m3, "Shared files").
pub trait Asked {
    /// The action the command runs.
    fn action(&self) -> &'static str;
    /// The arguments, each by its name.
    fn arguments(&self) -> Vec<(&'static str, Value)>;
    /// The inputs: the port, the flag that named it, and the identity as typed.
    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)>;
}

/// What `line`, `rect` and `stroke` share: the image given at `image`, read; the shape
/// `shape` makes of the image's size, as a location on the image's binding, whose origin
/// is `step` with the image as its input. A shape the image cannot hold, or arguments
/// `shape` refuses, end the run as invalid, in the words of the refusal.
pub(crate) fn located(
    call: &PreparedCall,
    context: &Context,
    step: artifact::Step,
    shape: impl FnOnce(whizbang_core::identity::Space) -> Result<artifact::Shape, String>,
) -> Result<Location, Failure> {
    let id = one(call, "image")?;
    let image = self::image(context, id)?;
    let shape = shape(image.space()).map_err(|why| Failure::invalid(&why))?;
    let origin = artifact::Origin::new(step.input("image", id.0))
        .map_err(|error| Failure::error("location", &error.without_paths()))?;
    Location::new(image.binding(), shape, origin).map_err(|error| match error {
        artifact::ArtifactError::Invalid { why, .. } => Failure::invalid(&why),
        other => Failure::error("location", &other.without_paths()),
    })
}
