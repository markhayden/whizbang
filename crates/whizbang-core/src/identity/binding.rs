//! Bindings: a source image placed onto a destination through a mapping with a stated
//! direction (ADR 0001). A mask or a piece of evidence belongs to a binding, never to a
//! bare space.

use serde::{Deserialize, Serialize};

use super::digest::{Digest, Encoder, PixelDigest};
use super::mapping::{Mapping, Matrix, inverse, multiply, transpose};
use super::space::{Point, Space};
use super::{IdentityError, Result};

/// What a source image is placed onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    /// Nothing else: the source image in its own space.
    Root,
    /// Another image, named by its canonical pixel digest.
    Image(PixelDigest),
}

/// Which way a binding's mapping parameters run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// The parameters take the source's points to the destination's.
    SourceToDestination,
    /// The parameters take the destination's points to the source's.
    DestinationToSource,
}

impl Direction {
    fn flipped(self) -> Direction {
        match self {
            Direction::SourceToDestination => Direction::DestinationToSource,
            Direction::DestinationToSource => Direction::SourceToDestination,
        }
    }
}

/// The identity of a binding: a digest of everything it holds, and of nothing local.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BindingId(pub Digest);

impl std::fmt::Display for BindingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A source image placed onto a destination. It names the source image, the destination
/// image or the root, both coordinate spaces, the direction and the mapping. Images are
/// named by their canonical pixel digests, so a binding holds for every file that stores
/// the same picture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "BindingRecord", into = "BindingRecord")]
pub struct Binding {
    source: PixelDigest,
    source_space: Space,
    destination: Destination,
    destination_space: Space,
    direction: Direction,
    mapping: Mapping,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingRecord {
    source: PixelDigest,
    source_space: Space,
    destination: Destination,
    destination_space: Space,
    direction: Direction,
    mapping: Mapping,
}

impl TryFrom<BindingRecord> for Binding {
    type Error = IdentityError;

    fn try_from(record: BindingRecord) -> Result<Binding> {
        let binding = Binding {
            source: record.source,
            source_space: record.source_space,
            destination: record.destination,
            destination_space: record.destination_space,
            direction: record.direction,
            mapping: record.mapping,
        };
        binding.check()?;
        Ok(binding)
    }
}

impl From<Binding> for BindingRecord {
    fn from(binding: Binding) -> BindingRecord {
        BindingRecord {
            source: binding.source,
            source_space: binding.source_space,
            destination: binding.destination,
            destination_space: binding.destination_space,
            direction: binding.direction,
            mapping: binding.mapping,
        }
    }
}

impl Binding {
    /// The binding of an image in its own space: onto the root, by the identity.
    pub fn root(image: PixelDigest, space: Space) -> Binding {
        Binding {
            source: image,
            source_space: space,
            destination: Destination::Root,
            destination_space: space,
            direction: Direction::SourceToDestination,
            mapping: Mapping::Identity,
        }
    }

    /// The binding of a source image onto a destination image. The mapping's parameters
    /// run in `direction`, and the mapping is checked against the two spaces in that
    /// direction: a mapping that does not fit them, or that cannot be inverted, is refused
    /// here.
    pub fn new(
        source: PixelDigest,
        source_space: Space,
        destination: PixelDigest,
        destination_space: Space,
        direction: Direction,
        mapping: Mapping,
    ) -> Result<Binding> {
        let binding = Binding {
            source,
            source_space,
            destination: Destination::Image(destination),
            destination_space,
            direction,
            mapping,
        };
        binding.check()?;
        Ok(binding)
    }

    fn check(&self) -> Result<()> {
        if self.destination == Destination::Root
            && (self.mapping != Mapping::Identity
                || self.direction != Direction::SourceToDestination)
        {
            return Err(IdentityError::Binding(
                "a binding to the root is the identity, from the source".to_owned(),
            ));
        }
        let (from, to) = self.ends();
        self.mapping.check(from, to)
    }

    /// The two spaces in the order the mapping's parameters run.
    fn ends(&self) -> (&Space, &Space) {
        match self.direction {
            Direction::SourceToDestination => (&self.source_space, &self.destination_space),
            Direction::DestinationToSource => (&self.destination_space, &self.source_space),
        }
    }

    pub fn source(&self) -> PixelDigest {
        self.source
    }

    pub fn source_space(&self) -> Space {
        self.source_space
    }

    pub fn destination(&self) -> Destination {
        self.destination
    }

    pub fn destination_space(&self) -> Space {
        self.destination_space
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    pub fn mapping(&self) -> &Mapping {
        &self.mapping
    }

    /// The binding's identity (ADR 0001): the source's pixel digest, the source space, the
    /// destination (the root, or an image's pixel digest), the destination space, the
    /// direction, and the mapping's kind and parameters.
    pub fn id(&self) -> BindingId {
        let mut encoder = Encoder::new("binding/1");
        encoder.digest(&self.source.0);
        self.source_space.encode(&mut encoder);
        match &self.destination {
            Destination::Root => encoder.unsigned(0),
            Destination::Image(image) => {
                encoder.unsigned(1);
                encoder.digest(&image.0);
            }
        }
        self.destination_space.encode(&mut encoder);
        encoder.unsigned(match self.direction {
            Direction::SourceToDestination => 0,
            Direction::DestinationToSource => 1,
        });
        self.mapping.encode(&mut encoder);
        BindingId(encoder.finish())
    }

    /// Maps a point of the source to its place on the destination. A point outside the
    /// source space, or one that lands outside the destination space, is refused.
    pub fn to_destination(&self, point: Point) -> Result<Point> {
        let (from, to) = self.ends();
        match self.direction {
            Direction::SourceToDestination => self.mapping.forward(from, to, point),
            Direction::DestinationToSource => self.mapping.backward(from, to, point),
        }
    }

    /// Maps a point of the destination back to its place in the source.
    pub fn to_source(&self, point: Point) -> Result<Point> {
        let (from, to) = self.ends();
        match self.direction {
            Direction::SourceToDestination => self.mapping.backward(from, to, point),
            Direction::DestinationToSource => self.mapping.forward(from, to, point),
        }
    }

    /// The binding the other way round: the destination image placed onto the source. The
    /// parameters are kept as they are and their direction is flipped, so nothing is
    /// recomputed and the reverse of the reverse is this binding. A binding to the root has
    /// no reverse.
    pub fn reversed(&self) -> Result<Binding> {
        let Destination::Image(destination) = self.destination else {
            return Err(IdentityError::Binding(
                "a binding to the root has no reverse".to_owned(),
            ));
        };
        Ok(Binding {
            source: destination,
            source_space: self.destination_space,
            destination: Destination::Image(self.source),
            destination_space: self.source_space,
            direction: self.direction.flipped(),
            mapping: self.mapping.clone(),
        })
    }

    /// This binding followed by `next`, as one binding from this source onto `next`'s
    /// destination. `next` must start where this one ends: the same image in the same
    /// space. Two crops, each stated from its source, compose into a crop; any other pair
    /// of crops and homographies into a homography; rotations into a rotation. Any other pair does not make one mapping and is refused:
    /// map its points through each binding in turn.
    pub fn then(&self, next: &Binding) -> Result<Binding> {
        let refuse = |why: &str| Err(IdentityError::Binding(why.to_owned()));
        if self.destination != Destination::Image(next.source)
            || self.destination_space != next.source_space
        {
            return refuse("the second binding does not start where the first ends");
        }
        let Destination::Image(destination) = next.destination else {
            return refuse("a binding to the root is not a step to compose");
        };
        let build = |direction, mapping| {
            Binding::new(
                self.source,
                self.source_space,
                destination,
                next.destination_space,
                direction,
                mapping,
            )
        };
        if self.mapping == Mapping::Identity {
            return build(next.direction, next.mapping.clone());
        }
        if next.mapping == Mapping::Identity {
            return build(self.direction, self.mapping.clone());
        }
        if let (Some(first), Some(second)) = (self.rotation(), next.rotation()) {
            return build(
                Direction::SourceToDestination,
                Mapping::Rotation {
                    matrix: multiply(&second, &first),
                },
            );
        }
        // Two crops, each stated from its source: the inner box's edges, placed between
        // the outer box's edges. An edge the inner crop shares with the outer one comes out
        // as the outer one's edge exactly, and no edge can pass it.
        if let (
            Mapping::Crop {
                left,
                top,
                right,
                bottom,
            },
            Direction::SourceToDestination,
            Mapping::Crop {
                left: outer_left,
                top: outer_top,
                right: outer_right,
                bottom: outer_bottom,
            },
            Direction::SourceToDestination,
        ) = (&self.mapping, self.direction, &next.mapping, next.direction)
        {
            let width = f64::from(next.source_space.width());
            let height = f64::from(next.source_space.height());
            let mapping = Mapping::Crop {
                left: between(*outer_left, *outer_right, *left, width),
                top: between(*outer_top, *outer_bottom, *top, height),
                right: between(*outer_left, *outer_right, *right, width),
                bottom: between(*outer_top, *outer_bottom, *bottom, height),
            };
            return build(Direction::SourceToDestination, mapping);
        }
        let (Some(first), Some(second)) = (self.planar()?, next.planar()?) else {
            return refuse("these two mappings do not compose into one");
        };
        let mapping = Mapping::Homography {
            matrix: multiply(&second, &first),
        };
        build(Direction::SourceToDestination, mapping)
    }

    /// The rotation from the source's directions to the destination's, for a rotation.
    fn rotation(&self) -> Option<Matrix> {
        let Mapping::Rotation { matrix } = &self.mapping else {
            return None;
        };
        Some(match self.direction {
            Direction::SourceToDestination => *matrix,
            Direction::DestinationToSource => transpose(matrix),
        })
    }

    /// The matrix from the source's pixels to the destination's, for the identity, a crop
    /// or a homography.
    fn planar(&self) -> Result<Option<Matrix>> {
        let (from, _) = self.ends();
        let Some(matrix) = self.mapping.planar(from) else {
            return Ok(None);
        };
        Ok(Some(match self.direction {
            Direction::SourceToDestination => matrix,
            Direction::DestinationToSource => inverse(&matrix)?,
        }))
    }
}

/// The place between `low` and `high` that `at` has between 0 and `size`, measured from
/// the nearer end: `low + at (high - low) / size` in the first half, and
/// `high - (size - at) (high - low) / size` in the second. At 0 it is `low` and at `size`
/// it is `high`, exactly, and it is never outside the two.
fn between(low: f64, high: f64, at: f64, size: f64) -> f64 {
    if at <= size / 2.0 {
        low + at * (high - low) / size
    } else {
        high - (size - at) * (high - low) / size
    }
}
