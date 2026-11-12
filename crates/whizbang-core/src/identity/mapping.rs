//! Mappings between two spaces (ADR 0001). A mapping's parameters are stated one way, from
//! a space called `from` here to one called `to`; a [`super::Binding`] says which of its two
//! ends is which. Every mapping can be run both ways, and is checked against the two spaces
//! before it is used.

use std::f64::consts::{FRAC_PI_2, PI};

use serde::{Deserialize, Serialize};

use super::digest::Encoder;
use super::space::{Point, Space, Topology};
use super::{IdentityError, Result};

/// A 3 by 3 matrix, row-major.
pub(super) type Matrix = [f64; 9];

pub(super) const UNIT: Matrix = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// How the points of one space sit in another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Mapping {
    /// The two spaces are one space.
    Identity,
    /// The whole of `from` covers this box of `to`, in `to`'s pixels. The box lies inside
    /// `to` and may be at another scale than `from`. `from` is flat, unless the box is the
    /// whole of `to` (a resize), when it has `to`'s projection and topology.
    Crop {
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
    },
    /// The row-major matrix that takes `from`'s pixels to `to`'s, in homogeneous
    /// coordinates. Both spaces are flat. It can be inverted.
    Homography { matrix: [f64; 9] },
    /// The row-major rotation that takes `from`'s directions to `to`'s. Both spaces are
    /// complete panoramas. The direction of longitude `lon` and latitude `lat` is
    /// `(cos lat cos lon, cos lat sin lon, sin lat)`.
    Rotation { matrix: [f64; 9] },
    /// A wrapped cut. `to` is a complete panorama; `from` is a flat canvas, `2 * strip`
    /// wide and as tall, that holds the panorama's last `strip` columns and then its first
    /// `strip`, so the seam is the canvas's middle column.
    Wrap { strip: u32 },
}

impl Mapping {
    /// Checks the mapping against the two spaces its parameters run between.
    pub fn check(&self, from: &Space, to: &Space) -> Result<()> {
        let refuse = |why: String| Err(IdentityError::Binding(why));
        match self {
            Mapping::Identity => {
                if from != to {
                    return refuse(format!(
                        "the identity needs one space, not {from:?} and {to:?}"
                    ));
                }
            }
            Mapping::Crop {
                left,
                top,
                right,
                bottom,
            } => {
                let (width, height) = (f64::from(to.width()), f64::from(to.height()));
                let finite = [left, top, right, bottom].iter().all(|v| v.is_finite());
                if !finite || left >= right || top >= bottom {
                    return refuse(format!(
                        "the crop ({left}, {top}) to ({right}, {bottom}) has no area"
                    ));
                }
                if *left < 0.0 || *top < 0.0 || *right > width || *bottom > height {
                    return refuse(format!(
                        "the crop ({left}, {top}) to ({right}, {bottom}) is not inside {}x{}",
                        to.width(),
                        to.height()
                    ));
                }
                let whole = *left == 0.0 && *top == 0.0 && *right == width && *bottom == height;
                if whole {
                    if (from.projection(), from.topology()) != (to.projection(), to.topology()) {
                        return refuse(
                            "a crop of the whole of a space keeps its projection and topology"
                                .to_owned(),
                        );
                    }
                } else if !from.is_flat() {
                    return refuse("a crop of part of a space is flat".to_owned());
                }
            }
            Mapping::Homography { matrix } => {
                if !from.is_flat() || !to.is_flat() {
                    return refuse("a homography needs two flat spaces".to_owned());
                }
                if singular(&normalised(matrix, from, to)) {
                    return refuse(format!("the homography {matrix:?} is singular"));
                }
            }
            Mapping::Rotation { matrix } => {
                if !from.is_sphere() || !to.is_sphere() {
                    return refuse("a rotation needs two complete panoramas".to_owned());
                }
                if !proper_rotation(matrix) {
                    return refuse(format!("{matrix:?} is not a rotation"));
                }
            }
            Mapping::Wrap { strip } => {
                if !to.is_sphere() {
                    return refuse("a wrapped cut needs a complete panorama".to_owned());
                }
                let fits =
                    *strip >= 2 && strip % 2 == 0 && 2 * u64::from(*strip) <= u64::from(to.width());
                if !fits {
                    return refuse(format!(
                        "strip {strip} does not fit a panorama {} wide: it is even, at least 2, \
                         and at most half the width",
                        to.width()
                    ));
                }
                if !from.is_flat()
                    || u64::from(from.width()) != 2 * u64::from(*strip)
                    || from.height() != to.height()
                {
                    return refuse(format!(
                        "the canvas of a cut of strip {strip} is flat, {} wide and {} tall",
                        2 * u64::from(*strip),
                        to.height()
                    ));
                }
            }
        }
        Ok(())
    }

    /// Maps a point of `from` to its place in `to`. The point must lie in `from`, and what
    /// it maps to must lie in `to`. The mapping has passed [`Mapping::check`] for these two
    /// spaces.
    pub fn forward(&self, from: &Space, to: &Space, point: Point) -> Result<Point> {
        let point = from.place(point)?;
        let mapped = match self {
            Mapping::Identity => point,
            Mapping::Crop {
                left,
                top,
                right,
                bottom,
            } => Point::new(
                between(*left, *right, point.x, f64::from(from.width())),
                between(*top, *bottom, point.y, f64::from(from.height())),
            ),
            Mapping::Homography { matrix } => project(matrix, point)?,
            Mapping::Rotation { matrix } => rotate(matrix, from, to, point),
            Mapping::Wrap { strip } => Point::new(point.x - f64::from(*strip), point.y),
        };
        to.land(mapped)
    }

    /// Maps a point of `to` back to its place in `from`: the inverse of
    /// [`Mapping::forward`].
    pub fn backward(&self, from: &Space, to: &Space, point: Point) -> Result<Point> {
        let point = to.place(point)?;
        let mapped = match self {
            Mapping::Identity => point,
            Mapping::Crop {
                left,
                top,
                right,
                bottom,
            } => {
                // In a wrapped space the right edge is column 0. A box that reaches the
                // right edge holds that column as its own right edge.
                let width = f64::from(to.width());
                let wrapped = to.topology() == Topology::Wrap;
                let x = if wrapped && point.x < *left && point.x + width <= *right {
                    point.x + width
                } else {
                    point.x
                };
                Point::new(
                    within(*left, *right, x, f64::from(from.width())),
                    within(*top, *bottom, point.y, f64::from(from.height())),
                )
            }
            Mapping::Homography { matrix } => project(&inverse(matrix)?, point)?,
            Mapping::Rotation { matrix } => rotate(&transpose(matrix), to, from, point),
            Mapping::Wrap { strip } => {
                let (strip, width) = (f64::from(*strip), f64::from(to.width()));
                if point.x <= strip {
                    Point::new(point.x + strip, point.y)
                } else if point.x >= width - strip {
                    Point::new(point.x - (width - strip), point.y)
                } else {
                    return Err(IdentityError::Outside(format!(
                        "column {} is in neither strip of the cut",
                        point.x
                    )));
                }
            }
        };
        from.land(mapped)
    }

    /// The mapping as a matrix on pixels, from `from` to `to`, when it is one: the
    /// identity, a crop or a homography.
    pub(super) fn planar(&self, from: &Space) -> Option<Matrix> {
        match self {
            Mapping::Identity => Some(UNIT),
            Mapping::Crop {
                left,
                top,
                right,
                bottom,
            } => Some([
                (right - left) / f64::from(from.width()),
                0.0,
                *left,
                0.0,
                (bottom - top) / f64::from(from.height()),
                *top,
                0.0,
                0.0,
                1.0,
            ]),
            Mapping::Homography { matrix } => Some(*matrix),
            Mapping::Rotation { .. } | Mapping::Wrap { .. } => None,
        }
    }

    /// Writes the mapping into an identity: its kind, then its parameters as given.
    pub(super) fn encode(&self, encoder: &mut Encoder) {
        match self {
            Mapping::Identity => encoder.unsigned(0),
            Mapping::Crop {
                left,
                top,
                right,
                bottom,
            } => {
                encoder.unsigned(1);
                for value in [left, top, right, bottom] {
                    encoder.float(*value);
                }
            }
            Mapping::Homography { matrix } => {
                encoder.unsigned(2);
                for value in matrix {
                    encoder.float(*value);
                }
            }
            Mapping::Rotation { matrix } => {
                encoder.unsigned(3);
                for value in matrix {
                    encoder.float(*value);
                }
            }
            Mapping::Wrap { strip } => {
                encoder.unsigned(4);
                encoder.unsigned(u64::from(*strip));
            }
        }
    }
}

pub(super) fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [0.0; 9];
    for row in 0..3 {
        for column in 0..3 {
            out[3 * row + column] = (0..3).map(|k| a[3 * row + k] * b[3 * k + column]).sum();
        }
    }
    out
}

pub(super) fn transpose(m: &Matrix) -> Matrix {
    [m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]]
}

fn determinant(m: &Matrix) -> f64 {
    m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6])
}

/// The matrix between the two spaces' unit squares: what the homography is once each
/// space's pixels are divided by its width and height. Its entries are then of one scale,
/// which a matrix on pixels never is.
fn normalised(m: &Matrix, from: &Space, to: &Space) -> Matrix {
    let (fw, fh) = (f64::from(from.width()), f64::from(from.height()));
    let (tw, th) = (f64::from(to.width()), f64::from(to.height()));
    [
        m[0] * fw / tw,
        m[1] * fh / tw,
        m[2] / tw,
        m[3] * fw / th,
        m[4] * fh / th,
        m[5] / th,
        m[6] * fw,
        m[7] * fh,
        m[8],
    ]
}

/// True when the matrix cannot be inverted: an entry is not finite, or its determinant is
/// at most `1e-12` of the cube of its Frobenius norm, which no scaling of the matrix
/// changes.
fn singular(m: &Matrix) -> bool {
    if m.iter().any(|v| !v.is_finite()) {
        return true;
    }
    let norm = m.iter().map(|v| v * v).sum::<f64>().sqrt();
    let det = determinant(m);
    let bound = 1e-12 * norm * norm * norm;
    !norm.is_finite() || !det.is_finite() || !bound.is_finite() || det.abs() <= bound
}

/// The inverse of a matrix on pixels.
pub(super) fn inverse(m: &Matrix) -> Result<Matrix> {
    let det = determinant(m);
    if det == 0.0 || !det.is_finite() {
        return Err(IdentityError::Binding(format!(
            "the matrix {m:?} is singular"
        )));
    }
    Ok([
        (m[4] * m[8] - m[5] * m[7]) / det,
        (m[2] * m[7] - m[1] * m[8]) / det,
        (m[1] * m[5] - m[2] * m[4]) / det,
        (m[5] * m[6] - m[3] * m[8]) / det,
        (m[0] * m[8] - m[2] * m[6]) / det,
        (m[2] * m[3] - m[0] * m[5]) / det,
        (m[3] * m[7] - m[4] * m[6]) / det,
        (m[1] * m[6] - m[0] * m[7]) / det,
        (m[0] * m[4] - m[1] * m[3]) / det,
    ])
}

/// Applies a matrix on pixels to a point. A point on the matrix's horizon, where the
/// denominator is zero, has no image.
fn project(m: &Matrix, point: Point) -> Result<Point> {
    let d = m[6] * point.x + m[7] * point.y + m[8];
    let x = (m[0] * point.x + m[1] * point.y + m[2]) / d;
    let y = (m[3] * point.x + m[4] * point.y + m[5]) / d;
    let scale = m[6].abs().max(m[7].abs()).max(m[8].abs());
    if d.abs() <= 1e-12 * scale || !x.is_finite() || !y.is_finite() {
        return Err(IdentityError::Outside(format!(
            "({}, {}) is on the horizon of the homography",
            point.x, point.y
        )));
    }
    Ok(Point::new(x, y))
}

/// True when the matrix is a rotation: its rows are unit vectors at right angles, to within
/// `1e-9`, and its determinant is 1 and not -1.
fn proper_rotation(m: &Matrix) -> bool {
    if m.iter().any(|v| !v.is_finite()) {
        return false;
    }
    let product = multiply(m, &transpose(m));
    let orthonormal = product
        .iter()
        .zip(UNIT)
        .all(|(got, want)| (got - want).abs() <= 1e-9);
    orthonormal && (determinant(m) - 1.0).abs() <= 1e-9
}

/// Turns a point of one panorama into a direction, rotates it, and returns its point in
/// the other panorama.
fn rotate(m: &Matrix, from: &Space, to: &Space, point: Point) -> Point {
    debug_assert!(from.topology() == Topology::Wrap && to.topology() == Topology::Wrap);
    let lon = point.x / f64::from(from.width()) * 2.0 * PI - PI;
    let lat = FRAC_PI_2 - point.y / f64::from(from.height()) * PI;
    let d = [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()];
    let r = [
        m[0] * d[0] + m[1] * d[1] + m[2] * d[2],
        m[3] * d[0] + m[4] * d[1] + m[5] * d[2],
        m[6] * d[0] + m[7] * d[1] + m[8] * d[2],
    ];
    let lon = r[1].atan2(r[0]);
    let lat = r[2].clamp(-1.0, 1.0).asin();
    Point::new(
        (lon + PI) / (2.0 * PI) * f64::from(to.width()),
        (FRAC_PI_2 - lat) / PI * f64::from(to.height()),
    )
}

/// The place between `low` and `high` that `at` has between 0 and `size`, measured from
/// the nearer end: `low + at (high - low) / size` in the first half, and
/// `high - (size - at) (high - low) / size` in the second. At 0 it is `low` and at `size`
/// it is `high`, exactly, and it is never outside the two.
pub(super) fn between(low: f64, high: f64, at: f64, size: f64) -> f64 {
    if at <= size / 2.0 {
        low + at * (high - low) / size
    } else {
        high - (size - at) * (high - low) / size
    }
}

/// The inverse of [`between`]: the place between 0 and `size` that `value` has between
/// `low` and `high`, measured from the nearer end. At `low` it is 0 and at `high` it is
/// `size`, exactly.
fn within(low: f64, high: f64, value: f64, size: f64) -> f64 {
    if value - low <= high - value {
        (value - low) * size / (high - low)
    } else {
        size - (high - value) * size / (high - low)
    }
}
