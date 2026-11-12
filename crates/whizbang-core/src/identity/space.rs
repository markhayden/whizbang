//! Coordinate spaces: dimensions, projection and topology, with no content (ADR 0001).
//!
//! Coordinates are in pixels and continuous: `x` runs from 0 at the left edge to the width at
//! the right, `y` from 0 at the top to the height at the bottom, so pixel `(i, j)` covers
//! `[i, i + 1) × [j, j + 1)` and its centre is `(i + 0.5, j + 0.5)`.

use serde::{Deserialize, Serialize};

use super::digest::{FieldSource, Fields};
use super::{IdentityError, Result};

/// How an image's pixels map onto the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    /// An ordinary picture.
    Flat,
    /// Longitude across and latitude down: the left edge is longitude -180 degrees, the
    /// right edge 180, the top latitude 90 and the bottom -90.
    Equirectangular,
}

/// Whether the left and right edges meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topology {
    /// The edges are edges.
    Bounded,
    /// The left and right edges are the same column: a complete turn.
    Wrap,
}

/// A point of a space, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }
}

/// A coordinate space. Two spaces are equal only when their dimensions, projection and
/// topology all match; no image is involved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "SpaceRecord", into = "SpaceRecord")]
pub struct Space {
    width: u32,
    height: u32,
    projection: Projection,
    topology: Topology,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpaceRecord {
    width: u32,
    height: u32,
    projection: Projection,
    topology: Topology,
}

impl TryFrom<SpaceRecord> for Space {
    type Error = IdentityError;

    fn try_from(record: SpaceRecord) -> Result<Space> {
        Space::new(
            record.width,
            record.height,
            record.projection,
            record.topology,
        )
    }
}

impl From<Space> for SpaceRecord {
    fn from(space: Space) -> SpaceRecord {
        SpaceRecord {
            width: space.width,
            height: space.height,
            projection: space.projection,
            topology: space.topology,
        }
    }
}

impl Space {
    /// A space of these dimensions. Both are at least 1, and a flat space does not wrap.
    pub fn new(
        width: u32,
        height: u32,
        projection: Projection,
        topology: Topology,
    ) -> Result<Space> {
        if width == 0 || height == 0 {
            return Err(IdentityError::Space(format!(
                "{width}x{height} has no area"
            )));
        }
        if projection == Projection::Flat && topology == Topology::Wrap {
            return Err(IdentityError::Space(
                "a flat space does not wrap".to_owned(),
            ));
        }
        Ok(Space {
            width,
            height,
            projection,
            topology,
        })
    }

    /// The space of an ordinary picture.
    pub fn flat(width: u32, height: u32) -> Result<Space> {
        Space::new(width, height, Projection::Flat, Topology::Bounded)
    }

    /// The space of a complete equirectangular panorama, whose left and right edges meet.
    pub fn panorama(width: u32, height: u32) -> Result<Space> {
        Space::new(width, height, Projection::Equirectangular, Topology::Wrap)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn projection(&self) -> Projection {
        self.projection
    }

    pub fn topology(&self) -> Topology {
        self.topology
    }

    /// True for a complete panorama: the only space a rotation on the sphere applies to.
    pub(super) fn is_sphere(&self) -> bool {
        self.projection == Projection::Equirectangular && self.topology == Topology::Wrap
    }

    /// True for an ordinary picture.
    pub(super) fn is_flat(&self) -> bool {
        self.projection == Projection::Flat
    }

    /// The point's place in this space, or a refusal when it has none. A bounded space
    /// holds its edges and nothing past them. A wrapped space takes any column round to
    /// `[0, width)`; rows never wrap.
    pub fn place(&self, point: Point) -> Result<Point> {
        let (width, height) = (f64::from(self.width), f64::from(self.height));
        let refuse = || {
            IdentityError::Outside(format!(
                "({}, {}) is outside the {}x{} space",
                point.x, point.y, self.width, self.height
            ))
        };
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(refuse());
        }
        if !(0.0..=height).contains(&point.y) {
            return Err(refuse());
        }
        match self.topology {
            Topology::Bounded if (0.0..=width).contains(&point.x) => Ok(point),
            Topology::Bounded => Err(refuse()),
            Topology::Wrap => {
                let x = point.x.rem_euclid(width);
                // A column a hair left of the seam can round up to the width itself.
                Ok(Point::new(if x >= width { 0.0 } else { x }, point.y))
            }
        }
    }

    /// Like [`Space::place`], for a point a mapping computed: rounding may have carried it
    /// a hair past an edge, so it is given `SLACK` pixels and brought back onto the edge.
    pub(super) fn land(&self, point: Point) -> Result<Point> {
        const SLACK: f64 = 1e-9;
        let (width, height) = (f64::from(self.width), f64::from(self.height));
        let snap = |value: f64, limit: f64| {
            if (-SLACK..0.0).contains(&value) {
                0.0
            } else if value > limit && value <= limit + SLACK {
                limit
            } else {
                value
            }
        };
        let x = match self.topology {
            Topology::Bounded => snap(point.x, width),
            Topology::Wrap => point.x,
        };
        self.place(Point::new(x, snap(point.y, height)))
    }

    /// Writes the space into an identity.
    pub(crate) fn encode(&self, encoder: &mut dyn Fields) {
        encoder.unsigned(u64::from(self.width));
        encoder.unsigned(u64::from(self.height));
        encoder.unsigned(match self.projection {
            Projection::Flat => 0,
            Projection::Equirectangular => 1,
        });
        encoder.unsigned(match self.topology {
            Topology::Bounded => 0,
            Topology::Wrap => 1,
        });
    }

    /// Reads back what [`Space::encode`] wrote, and checks it as a new space is checked.
    pub(crate) fn decode(source: &mut dyn FieldSource) -> Result<Space> {
        let mut side = |what: &str| -> Result<u32> {
            let value = source.unsigned()?;
            u32::try_from(value)
                .map_err(|_| IdentityError::Space(format!("a {what} of {value} is too large")))
        };
        let (width, height) = (side("width")?, side("height")?);
        let projection = match source.unsigned()? {
            0 => Projection::Flat,
            1 => Projection::Equirectangular,
            other => {
                return Err(IdentityError::Space(format!("{other} is not a projection")));
            }
        };
        let topology = match source.unsigned()? {
            0 => Topology::Bounded,
            1 => Topology::Wrap,
            other => return Err(IdentityError::Space(format!("{other} is not a topology"))),
        };
        Space::new(width, height, projection, topology)
    }
}
