//! A location: where something is on an image (ADR 0002). Coordinates are pixels of the
//! source space of the binding the location belongs to, as ADR 0001 defines them.

use crate::identity::{Binding, DecodeLimits, Fields, Point, Projection, Space, Topology};

use super::wire::{Reader, Writer};
use super::{ArtifactError, ArtifactId, ArtifactType, Origin, Record, Result, identity_of};

/// The most vertices a polygon may have.
const MAX_VERTICES: usize = 64;

/// The line a band follows.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// A column, from the top of the image to the bottom.
    Column(f64),
    /// A path of two or more points.
    Path(Vec<Point>),
}

/// The shape of a location.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A box with area.
    Box {
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
    },
    /// A polygon of three to sixty-four vertices.
    Polygon { points: Vec<Point> },
    /// A direction on a complete panorama, in degrees: yaw from -180 to 180, pitch from -90
    /// to 90, and a field of view from 10 to 120.
    View { yaw: f64, pitch: f64, fov: f64 },
    /// The whole image.
    WholeImage,
    /// Something that is missing from the image.
    Absent,
    /// A line with a full width in pixels.
    Line { line: Line, width: f64 },
    /// A rough mark along a path of two or more points, with a radius in pixels.
    Stroke { path: Vec<Point>, radius: f64 },
}

/// A shape on the source image of a binding.
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    binding: Binding,
    shape: Shape,
    origin: Origin,
    id: ArtifactId,
}

impl Location {
    /// A location on the binding's source image. Every coordinate is finite and inside the
    /// image; a width or a radius is positive and no longer than the image's diagonal; a
    /// view needs a complete panorama.
    pub fn new(binding: Binding, shape: Shape, origin: Origin) -> Result<Location> {
        check(&shape, &binding.source_space()).map_err(|why| ArtifactError::Invalid {
            artifact: ArtifactType::Location,
            why,
        })?;
        let mut location = Location {
            binding,
            shape,
            origin,
            id: ArtifactId::unset(),
        };
        location.id = identity_of(&location);
        Ok(location)
    }

    pub fn id(&self) -> ArtifactId {
        self.id
    }

    /// The binding whose source image the location is on.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The location's canonical file.
    pub fn to_bytes(&self) -> Vec<u8> {
        super::file_of(self)
    }

    /// Reads a location's canonical file.
    pub fn from_bytes(bytes: &[u8], limits: &DecodeLimits) -> Result<Location> {
        super::parse(bytes, limits)
    }
}

fn check(shape: &Shape, space: &Space) -> std::result::Result<(), String> {
    let (width, height) = (f64::from(space.width()), f64::from(space.height()));
    let inside =
        |point: &Point| (0.0..=width).contains(&point.x) && (0.0..=height).contains(&point.y);
    let path = |points: &[Point], least: usize, what: &str| {
        if points.len() < least {
            return Err(format!(
                "{what} needs at least {least} points, and has {}",
                points.len()
            ));
        }
        match points.iter().find(|point| !inside(point)) {
            Some(point) => Err(format!(
                "({}, {}) is not inside the {}x{} image",
                point.x,
                point.y,
                space.width(),
                space.height()
            )),
            None => Ok(()),
        }
    };
    // Beyond the diagonal a width means nothing.
    let reach = |value: f64, what: &str| {
        let diagonal = (width * width + height * height).sqrt();
        if value > 0.0 && value <= diagonal {
            Ok(())
        } else {
            Err(format!(
                "{what} {value} is not positive and at most the image's diagonal"
            ))
        }
    };
    match shape {
        Shape::Box {
            left,
            top,
            right,
            bottom,
        } => {
            // The comparisons refuse a number that is not finite.
            let ordered = left < right && top < bottom;
            if !ordered || *left < 0.0 || *top < 0.0 || *right > width || *bottom > height {
                return Err(format!(
                    "the box ({left}, {top}) to ({right}, {bottom}) has no area, or is not \
                     inside the {}x{} image",
                    space.width(),
                    space.height()
                ));
            }
        }
        Shape::Polygon { points } => {
            path(points, 3, "a polygon")?;
            if points.len() > MAX_VERTICES {
                return Err(format!(
                    "a polygon has at most {MAX_VERTICES} vertices, and this has {}",
                    points.len()
                ));
            }
        }
        Shape::View { yaw, pitch, fov } => {
            if (space.projection(), space.topology())
                != (Projection::Equirectangular, Topology::Wrap)
            {
                return Err("a view needs a complete panorama".to_owned());
            }
            if !(-180.0..=180.0).contains(yaw)
                || !(-90.0..=90.0).contains(pitch)
                || !(10.0..=120.0).contains(fov)
            {
                return Err(format!(
                    "a view has yaw from -180 to 180, pitch from -90 to 90 and a field of \
                     view from 10 to 120, not {yaw}, {pitch} and {fov}"
                ));
            }
        }
        Shape::WholeImage | Shape::Absent => {}
        Shape::Line {
            line,
            width: across,
        } => {
            reach(*across, "a line's width of")?;
            match line {
                Line::Column(column) if (0.0..=width).contains(column) => {}
                Line::Column(column) => {
                    return Err(format!(
                        "the column {column} is not inside an image {} wide",
                        space.width()
                    ));
                }
                Line::Path(points) => path(points, 2, "a line's path")?,
            }
        }
        Shape::Stroke {
            path: points,
            radius,
        } => {
            reach(*radius, "a stroke's radius of")?;
            path(points, 2, "a stroke's path")?;
        }
    }
    Ok(())
}

fn write_points(writer: &mut Writer, points: &[Point]) {
    writer.count(points.len());
    for point in points {
        writer.float(point.x);
        writer.float(point.y);
    }
}

fn read_points(reader: &mut Reader<'_>) -> Result<Vec<Point>> {
    let mut points = Vec::new();
    for _ in 0..reader.count()? {
        points.push(Point::new(reader.float()?, reader.float()?));
    }
    Ok(points)
}

impl Record for Location {
    const TYPE: ArtifactType = ArtifactType::Location;

    fn write(&self, writer: &mut Writer) {
        writer.binding(&self.binding);
        match &self.shape {
            Shape::Box {
                left,
                top,
                right,
                bottom,
            } => {
                writer.unsigned(0);
                for value in [left, top, right, bottom] {
                    writer.float(*value);
                }
            }
            Shape::Polygon { points } => {
                writer.unsigned(1);
                write_points(writer, points);
            }
            Shape::View { yaw, pitch, fov } => {
                writer.unsigned(2);
                for value in [yaw, pitch, fov] {
                    writer.float(*value);
                }
            }
            Shape::WholeImage => writer.unsigned(3),
            Shape::Absent => writer.unsigned(4),
            Shape::Line { line, width } => {
                writer.unsigned(5);
                match line {
                    Line::Column(column) => {
                        writer.unsigned(0);
                        writer.float(*column);
                    }
                    Line::Path(points) => {
                        writer.unsigned(1);
                        write_points(writer, points);
                    }
                }
                writer.float(*width);
            }
            Shape::Stroke { path, radius } => {
                writer.unsigned(6);
                write_points(writer, path);
                writer.float(*radius);
            }
        }
        writer.origin(&self.origin);
    }

    fn read(reader: &mut Reader<'_>, _limits: &DecodeLimits) -> Result<Location> {
        let binding = reader.binding()?;
        let not = |what: &str, number: u64| {
            ArtifactError::Format(format!("{number} is not a kind of {what}"))
        };
        let shape = match reader.unsigned()? {
            0 => Shape::Box {
                left: reader.float()?,
                top: reader.float()?,
                right: reader.float()?,
                bottom: reader.float()?,
            },
            1 => Shape::Polygon {
                points: read_points(reader)?,
            },
            2 => Shape::View {
                yaw: reader.float()?,
                pitch: reader.float()?,
                fov: reader.float()?,
            },
            3 => Shape::WholeImage,
            4 => Shape::Absent,
            5 => {
                let line = match reader.unsigned()? {
                    0 => Line::Column(reader.float()?),
                    1 => Line::Path(read_points(reader)?),
                    other => return Err(not("line", other)),
                };
                Shape::Line {
                    line,
                    width: reader.float()?,
                }
            }
            6 => Shape::Stroke {
                path: read_points(reader)?,
                radius: reader.float()?,
            },
            other => return Err(not("shape", other)),
        };
        let origin = reader.origin()?;
        Location::new(binding, shape, origin)
    }
}
