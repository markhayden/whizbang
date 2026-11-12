//! Artifacts (`SPEC.md` section 3, ADR 0002): what actions take and return.
//!
//! There are six types: a [`Mask`], a [`Location`], an [`Image`], a [`BandProfile`], a
//! [`Series`] and a [`Plan`]. Each belongs to a binding (ADR 0001), has an identity that
//! depends on its content alone, records what produced it, and is written as one canonical
//! file: the same artifact gives the same bytes in every store, and a file that is not the
//! one way to write its artifact is refused.
//!
//! A type is never taken for another. A rendering of a mask is an [`Image`], and is
//! refused wherever a [`Mask`] is required: when a file is read, when an artifact is taken
//! from the store, and by the types themselves.

mod band;
mod location;
mod mask;
mod picture;
mod plan;
mod series;
mod stored;
mod wire;

use crate::identity::{DecodeLimits, Digest, IdentityError};
use crate::store;

pub use band::{BandProfile, Windows};
pub use location::{Line, Location, Shape};
pub use mask::{Mask, Prescription, Profile, Refinement, Source, UseCase, effective_weight};
pub use picture::{Image, ImageRole};
pub use plan::Plan;
pub use series::Series;
pub use stored::{Contents, get, holds, name, put};

use wire::{Reader, Writer};

/// The six types of artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArtifactType {
    Mask,
    Location,
    Image,
    BandProfile,
    Series,
    Plan,
}

impl ArtifactType {
    /// Every type, in the order ADR 0002 lists them.
    pub const ALL: [ArtifactType; 6] = [
        ArtifactType::Mask,
        ArtifactType::Location,
        ArtifactType::Image,
        ArtifactType::BandProfile,
        ArtifactType::Series,
        ArtifactType::Plan,
    ];

    /// The type's name, as a file's kind and a stored name spell it.
    pub fn name(self) -> &'static str {
        match self {
            ArtifactType::Mask => "mask",
            ArtifactType::Location => "location",
            ArtifactType::Image => "image",
            ArtifactType::BandProfile => "band_profile",
            ArtifactType::Series => "series",
            ArtifactType::Plan => "plan",
        }
    }

    /// The kind a file and an identity of this type start with: the name and the version
    /// of its fields.
    pub(crate) fn kind(self) -> &'static str {
        match self {
            ArtifactType::Mask => "mask/1",
            ArtifactType::Location => "location/1",
            ArtifactType::Image => "image/1",
            ArtifactType::BandProfile => "band_profile/1",
            ArtifactType::Series => "series/1",
            ArtifactType::Plan => "plan/1",
        }
    }
}

impl std::fmt::Display for ArtifactType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The identity of an artifact: a digest of its content, and of nothing local. Its origin
/// is not part of it, so the same artifact made twice, or held in two stores, is one
/// artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactId(pub Digest);

impl ArtifactId {
    /// What an artifact holds until its identity has been worked out, inside its own
    /// constructor and nowhere else.
    pub(crate) fn unset() -> ArtifactId {
        ArtifactId(Digest::from_bytes([0; 32]))
    }
}

impl std::fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Why an artifact could not be made, read, used or found.
#[derive(Debug)]
pub enum ArtifactError {
    /// The bytes are not a canonical artifact file.
    Format(String),
    /// An artifact of another type, or something that is no artifact, where this type is
    /// required.
    WrongType {
        expected: ArtifactType,
        found: String,
    },
    /// The parts do not make an artifact of this type.
    Invalid { artifact: ArtifactType, why: String },
    /// The artifact does not belong to the image it was asked to be used on.
    WrongImage(String),
    /// The rebind was refused: the binding does not show the new image to be derived
    /// from the artifact's own.
    Rebind(String),
    /// A binding, a space or an image inside the artifact was refused.
    Identity(IdentityError),
    /// The store does not hold the artifact.
    Missing {
        artifact: ArtifactType,
        id: ArtifactId,
    },
    /// The store failed.
    Store(store::Error),
}

impl std::fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactError::Format(why) => write!(f, "not a canonical artifact file: {why}"),
            ArtifactError::WrongType { expected, found } => {
                write!(f, "a {expected} is required, and this is {found}")
            }
            ArtifactError::Invalid { artifact, why } => write!(f, "not a {artifact}: {why}"),
            ArtifactError::WrongImage(why) => write!(f, "wrong image: {why}"),
            ArtifactError::Rebind(why) => write!(f, "rebind refused: {why}"),
            ArtifactError::Identity(error) => error.fmt(f),
            ArtifactError::Missing { artifact, id } => {
                write!(f, "the store holds no {artifact} {id}")
            }
            ArtifactError::Store(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ArtifactError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ArtifactError::Identity(error) => Some(error),
            ArtifactError::Store(error) => Some(error),
            _ => None,
        }
    }
}

impl From<IdentityError> for ArtifactError {
    fn from(error: IdentityError) -> ArtifactError {
        ArtifactError::Identity(error)
    }
}

impl From<store::Error> for ArtifactError {
    fn from(error: store::Error) -> ArtifactError {
        ArtifactError::Store(error)
    }
}

/// The result of this module's fallible functions.
pub type Result<T> = std::result::Result<T, ArtifactError>;

/// One step that made or changed an artifact: the action, its version, its arguments as
/// they were given, and the identities of its inputs by port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub action: String,
    pub version: String,
    /// Each argument's name and its value, as text.
    pub arguments: Vec<(String, String)>,
    /// Each input's port and its identity: an artifact's, a binding's or an image's.
    pub inputs: Vec<(String, Digest)>,
}

impl Step {
    pub fn new(action: &str, version: &str) -> Step {
        Step {
            action: action.to_owned(),
            version: version.to_owned(),
            arguments: Vec::new(),
            inputs: Vec::new(),
        }
    }

    pub fn argument(mut self, name: &str, value: &str) -> Step {
        self.arguments.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn input(mut self, port: &str, identity: Digest) -> Step {
        self.inputs.push((port.to_owned(), identity));
        self
    }
}

/// What produced an artifact: the steps that made it, oldest first. There is always at
/// least one, and each names its action, so no artifact is without an origin. The origin
/// travels in the artifact's file and is not part of its identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    steps: Vec<Step>,
}

impl Origin {
    /// The origin of something made by one step.
    pub fn new(step: Step) -> Result<Origin> {
        Origin::from_steps(vec![step])
    }

    pub(crate) fn from_steps(steps: Vec<Step>) -> Result<Origin> {
        if steps.is_empty() {
            return Err(ArtifactError::Format(
                "an artifact records what produced it: the origin has no step".to_owned(),
            ));
        }
        if steps.iter().any(|step| step.action.is_empty()) {
            return Err(ArtifactError::Format(
                "a step of the origin names no action".to_owned(),
            ));
        }
        Ok(Origin { steps })
    }

    /// This origin with one more step after it.
    pub fn then(mut self, step: Step) -> Result<Origin> {
        self.steps.push(step);
        Origin::from_steps(self.steps)
    }

    /// The steps, oldest first.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
}

/// What each type gives the code that writes, reads and identifies all six.
pub(crate) trait Record: Sized {
    const TYPE: ArtifactType;

    /// Writes the content, in the order ADR 0002 gives, and then the origin.
    fn write(&self, writer: &mut Writer);

    /// Reads back what `write` wrote, and checks the artifact as a new one is checked.
    fn read(reader: &mut Reader<'_>, limits: &DecodeLimits) -> Result<Self>;
}

/// The identity of an artifact: the hash of its kind and its content.
pub(crate) fn identity_of<R: Record>(record: &R) -> ArtifactId {
    let mut writer = Writer::new(R::TYPE.kind(), false);
    record.write(&mut writer);
    ArtifactId(writer.finish().1)
}

/// The canonical file of an artifact.
pub(crate) fn file_of<R: Record>(record: &R) -> Vec<u8> {
    let mut writer = Writer::new(R::TYPE.kind(), true);
    record.write(&mut writer);
    writer.finish().0.unwrap_or_default()
}

/// Reads something inside an artifact file that is itself an artifact of type `R`.
pub(crate) fn read_inside<R: Record>(reader: &mut Reader<'_>, limits: &DecodeLimits) -> Result<R> {
    reader.inside(R::TYPE.kind())?;
    R::read(reader, limits)
}

/// What a file of pictures is, in words for a refusal: a picture is never an artifact file.
fn picture(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("a PNG image, which is a picture and no artifact file")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("a JPEG image, which is a picture and no artifact file")
    } else {
        None
    }
}

/// The type whose file these bytes start as.
fn type_of(bytes: &[u8]) -> Result<(ArtifactType, Reader<'_>)> {
    let mut reader = Reader::new(bytes);
    let kind = reader.kind().map_err(|error| match picture(bytes) {
        Some(what) => ArtifactError::Format(format!("this is {what}")),
        None => error,
    })?;
    let found = ArtifactType::ALL
        .into_iter()
        .find(|artifact| artifact.kind() == kind)
        .ok_or_else(|| {
            ArtifactError::Format(format!(
                "its kind is {kind:?}, which this build does not read"
            ))
        })?;
    Ok((found, reader))
}

/// Reads the canonical file of an artifact of type `R`, and refuses anything else: a file
/// of another type, a picture, and a file that is not the one way to write its artifact.
pub(crate) fn parse<R: Record>(bytes: &[u8], limits: &DecodeLimits) -> Result<R> {
    let (found, mut reader) = type_of(bytes).map_err(|error| match picture(bytes) {
        Some(what) => ArtifactError::WrongType {
            expected: R::TYPE,
            found: what.to_owned(),
        },
        None => error,
    })?;
    if found != R::TYPE {
        return Err(ArtifactError::WrongType {
            expected: R::TYPE,
            found: format!("a {found}"),
        });
    }
    let record = R::read(&mut reader, limits)?;
    reader.end()?;
    if file_of(&record) != bytes {
        return Err(ArtifactError::Format(
            "it is not written the one way its artifact is written".to_owned(),
        ));
    }
    Ok(record)
}

/// An artifact of any type.
// A mask holds its profile whole, so it is the largest by far. Artifacts are passed one at
// a time, not held in bulk, and a box would only move the planes' three pointers.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Artifact {
    Mask(Mask),
    Location(Location),
    Image(Image),
    BandProfile(BandProfile),
    Series(Series),
    Plan(Plan),
}

impl Artifact {
    pub fn artifact_type(&self) -> ArtifactType {
        match self {
            Artifact::Mask(_) => ArtifactType::Mask,
            Artifact::Location(_) => ArtifactType::Location,
            Artifact::Image(_) => ArtifactType::Image,
            Artifact::BandProfile(_) => ArtifactType::BandProfile,
            Artifact::Series(_) => ArtifactType::Series,
            Artifact::Plan(_) => ArtifactType::Plan,
        }
    }

    pub fn id(&self) -> ArtifactId {
        match self {
            Artifact::Mask(mask) => mask.id(),
            Artifact::Location(location) => location.id(),
            Artifact::Image(image) => image.id(),
            Artifact::BandProfile(band) => band.id(),
            Artifact::Series(series) => series.id(),
            Artifact::Plan(plan) => plan.id(),
        }
    }

    /// What produced the artifact.
    pub fn origin(&self) -> &Origin {
        match self {
            Artifact::Mask(mask) => mask.origin(),
            Artifact::Location(location) => location.origin(),
            Artifact::Image(image) => image.origin(),
            Artifact::BandProfile(band) => band.origin(),
            Artifact::Series(series) => series.origin(),
            Artifact::Plan(plan) => plan.origin(),
        }
    }

    /// The artifact's canonical file.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Artifact::Mask(mask) => mask.to_bytes(),
            Artifact::Location(location) => location.to_bytes(),
            Artifact::Image(image) => image.to_bytes(),
            Artifact::BandProfile(band) => band.to_bytes(),
            Artifact::Series(series) => series.to_bytes(),
            Artifact::Plan(plan) => plan.to_bytes(),
        }
    }

    /// Reads the canonical file of an artifact of any type.
    pub fn from_bytes(bytes: &[u8], limits: &DecodeLimits) -> Result<Artifact> {
        let (found, _) = type_of(bytes)?;
        Artifact::from_bytes_as(found, bytes, limits)
    }

    /// Reads the canonical file of an artifact that must be of type `expected`.
    pub fn from_bytes_as(
        expected: ArtifactType,
        bytes: &[u8],
        limits: &DecodeLimits,
    ) -> Result<Artifact> {
        Ok(match expected {
            ArtifactType::Mask => Artifact::Mask(parse(bytes, limits)?),
            ArtifactType::Location => Artifact::Location(parse(bytes, limits)?),
            ArtifactType::Image => Artifact::Image(parse(bytes, limits)?),
            ArtifactType::BandProfile => Artifact::BandProfile(parse(bytes, limits)?),
            ArtifactType::Series => Artifact::Series(parse(bytes, limits)?),
            ArtifactType::Plan => Artifact::Plan(parse(bytes, limits)?),
        })
    }

    /// The mask this artifact is, or a refusal naming what it is instead. A rendering is
    /// an image, and is refused here.
    pub fn into_mask(self) -> Result<Mask> {
        match self {
            Artifact::Mask(mask) => Ok(mask),
            other => Err(ArtifactError::WrongType {
                expected: ArtifactType::Mask,
                found: match &other {
                    Artifact::Image(image) if image.role() == ImageRole::Rendering => {
                        "a rendering, which is an image".to_owned()
                    }
                    other => format!("a {}", other.artifact_type()),
                },
            }),
        }
    }
}

macro_rules! from_type {
    ($($variant:ident),*) => {$(
        impl From<$variant> for Artifact {
            fn from(artifact: $variant) -> Artifact {
                Artifact::$variant(artifact)
            }
        }
    )*};
}

from_type!(Mask, Location, Image, BandProfile, Series, Plan);
