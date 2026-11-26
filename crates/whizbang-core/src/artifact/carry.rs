//! Carrying a location through a binding: the same place, on the other image (ADR 0001,
//! ADR 0002).
//!
//! A location is on its binding's source image. Carried toward a binding's destination it
//! comes out on the destination image, in that image's own space; carried toward the
//! source, a location on the destination image comes out on the source image, bound by
//! that binding, so it still says where its image sits.
//!
//! **Crops and wrapped cuts** are affine on each of their regions, as rascal's frames were
//! (`crates/rascal-core/src/mapping.rs` at the commit `docs/sources.toml` pins), so a
//! location is carried region by region: a shape that crosses from one region to another,
//! the middle column of a cut's canvas, is split into one part for each; a shape that
//! reaches outside every region is clipped and the result says it is `partial`; a shape
//! wholly outside is refused. Boxes are cut to each region, polygons by Sutherland and
//! Hodgman, paths by Liang and Barsky; a column falls in one region; the whole image is a
//! box for each region; a view is not carried, as rascal did not. A line's width and a
//! stroke's radius are scaled by the region's scale, which must then be the same across
//! and down. Rascal worked on coordinates divided by the frame's size; here they are
//! pixels, so what is hashed is the pixels the arithmetic gives, and no division by a
//! size comes between. Rascal clipped a path down to the frame's height alone; here a
//! path is clipped to the region's rows as a polygon is, so a path carried into a crop
//! stays inside it.
//!
//! **Rotations and homographies** move each vertex through the binding's own mapping,
//! whose sine, cosine and arc functions are `libm`'s, so a carried location has the same
//! identity on every machine. A polygon, a line's path and a stroke's path are carried
//! vertex by vertex; a box through a homography is the polygon of its four corners; the
//! whole image through a rotation is the whole image. Nothing is clipped there: a vertex
//! that lands outside is refused, and so is every shape whose width has no one image there
//! (a column, a line or a stroke through a rotation, a line or a stroke through a
//! homography).
//!
//! **Through a perspective view** (ADR 0051) a box, a polygon or the whole view lands on
//! the panorama as the polygon of its boundary, cut at the seam into a part for each side,
//! or as a band the full width of the panorama when it holds a pole; and a box or a polygon
//! on the panorama lands in the view as the box or the polygon of what the view sees of
//! it, clipped to the view and said to be partial when anything was left out. A rotation
//! carries a polygon or a box onto a panorama the same way (`footprint.rs`). Lines,
//! strokes and views are refused through either.
//!
//! An absent thing is absent on the other image too.

use crate::identity::{Binding, Destination, Direction, Mapping, Point, Space, Topology};

use super::{ArtifactError, ArtifactType, Line, Location, Result, Shape, Step, footprint};

/// The action and version a carried location's origin names.
const STEP: (&str, &str) = ("carry", "1");

/// Which end of the binding a location is carried to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toward {
    /// From the binding's source image onto its destination image.
    Destination,
    /// From the binding's destination image onto its source image.
    Source,
}

/// A location carried through a binding.
#[derive(Debug, Clone, PartialEq)]
pub struct Carried {
    /// One location for each region the shape reached, in the order of the regions.
    pub parts: Vec<Location>,
    /// Whether part of the shape lay outside every region and was clipped away.
    pub partial: bool,
}

fn refuse<T>(why: impl Into<String>) -> Result<T> {
    Err(ArtifactError::Invalid {
        artifact: ArtifactType::Location,
        why: why.into(),
    })
}

/// Carries `location` through `binding`, toward its destination or its source. The
/// location must be on the image at the end it leaves, in that image's space; a binding
/// to the root carries nothing.
pub fn carry(location: &Location, binding: &Binding, toward: Toward) -> Result<Carried> {
    let Destination::Image(destination) = binding.destination() else {
        return refuse("a binding to the root places its image onto nothing");
    };
    let on = location.binding();
    let (from, into, landing) = match toward {
        Toward::Destination => (
            (binding.source(), binding.source_space()),
            binding.destination_space(),
            Binding::root(destination, binding.destination_space()),
        ),
        Toward::Source => (
            (destination, binding.destination_space()),
            binding.source_space(),
            binding.clone(),
        ),
    };
    if (on.source(), on.source_space()) != from {
        return refuse(format!(
            "the location is on {} in {:?}, not on the image the binding carries it from",
            on.source(),
            on.source_space()
        ));
    }
    // Whether the mapping's parameters run the way the location is carried.
    let forward = matches!(
        (binding.direction(), toward),
        (Direction::SourceToDestination, Toward::Destination)
            | (Direction::DestinationToSource, Toward::Source)
    );
    let (shapes, partial) = match regions(binding.mapping(), binding, forward) {
        Some(regions) => by_regions(location.shape(), &regions, &into)?,
        None if *binding.mapping() == Mapping::Identity => (vec![location.shape().clone()], false),
        None if matches!(binding.mapping(), Mapping::Perspective { .. }) => {
            footprint::through_view(location.shape(), binding, forward)?
        }
        None => (by_points(location.shape(), binding, toward, &into)?, false),
    };
    let step = Step::new(STEP.0, STEP.1)
        .input("location", location.id().0)
        .input("binding", binding.id().0);
    let origin = location.origin().clone().then(step)?;
    let parts = shapes
        .into_iter()
        .map(|shape| Location::new(landing.clone(), shape, origin.clone()))
        .collect::<Result<Vec<_>>>()?;
    Ok(Carried { parts, partial })
}

/// One region of a mapping: `x' = ax x + bx` and `y' = ay y + by`, on the rectangle from
/// `lo` to `hi` across and `ylo` to `yhi` down, in pixels.
#[derive(Debug, Clone, Copy)]
struct Region {
    lo: f64,
    hi: f64,
    ylo: f64,
    yhi: f64,
    ax: f64,
    bx: f64,
    ay: f64,
    by: f64,
}

impl Region {
    fn point(&self, p: Point) -> Point {
        Point::new(self.ax * p.x + self.bx, self.ay * p.y + self.by)
    }

    /// The inverse map, on this region's image.
    fn inverse(&self) -> Region {
        let (lo, hi) = order(self.ax * self.lo + self.bx, self.ax * self.hi + self.bx);
        let (ylo, yhi) = order(self.ay * self.ylo + self.by, self.ay * self.yhi + self.by);
        Region {
            lo,
            hi,
            ylo,
            yhi,
            ax: 1.0 / self.ax,
            bx: -self.bx / self.ax,
            ay: 1.0 / self.ay,
            by: -self.by / self.ay,
        }
    }

    fn contains(&self, p: Point) -> bool {
        p.x >= self.lo - 1e-12
            && p.x <= self.hi + 1e-12
            && p.y >= self.ylo - 1e-12
            && p.y <= self.yhi + 1e-12
    }
}

/// The regions of a crop or a wrapped cut, the way the location is carried: `None` for
/// any other mapping.
fn regions(mapping: &Mapping, binding: &Binding, forward: bool) -> Option<Vec<Region>> {
    // The space the parameters run from.
    let from = match binding.direction() {
        Direction::SourceToDestination => binding.source_space(),
        Direction::DestinationToSource => binding.destination_space(),
    };
    let (fw, fh) = (f64::from(from.width()), f64::from(from.height()));
    let stated = match mapping {
        Mapping::Crop {
            left,
            top,
            right,
            bottom,
        } => vec![Region {
            lo: 0.0,
            hi: fw,
            ylo: 0.0,
            yhi: fh,
            ax: (right - left) / fw,
            bx: *left,
            ay: (bottom - top) / fh,
            by: *top,
        }],
        Mapping::Wrap { strip } => {
            // The canvas's left half is the panorama's last strip, its right half the
            // first.
            let to = match binding.direction() {
                Direction::SourceToDestination => binding.destination_space(),
                Direction::DestinationToSource => binding.source_space(),
            };
            let (s, w) = (f64::from(*strip), f64::from(to.width()));
            let half = |lo: f64, hi: f64, bx: f64| Region {
                lo,
                hi,
                ylo: 0.0,
                yhi: fh,
                ax: 1.0,
                bx,
                ay: 1.0,
                by: 0.0,
            };
            vec![half(0.0, s, w - s), half(s, 2.0 * s, -s)]
        }
        _ => return None,
    };
    Some(if forward {
        stated
    } else {
        stated.iter().map(Region::inverse).collect()
    })
}

fn order(a: f64, b: f64) -> (f64, f64) {
    if a <= b { (a, b) } else { (b, a) }
}

/// A point brought inside `space`, against the last bits of the arithmetic.
fn inside(p: Point, space: &Space) -> Point {
    let (w, h) = (f64::from(space.width()), f64::from(space.height()));
    Point::new(p.x.clamp(0.0, w), p.y.clamp(0.0, h))
}

/// A width or a radius carried through a region: scaled by the region's scale, which is
/// the same across and down.
fn scaled(value: f64, region: &Region, what: &str) -> Result<f64> {
    if region.ax != region.ay {
        return refuse(format!(
            "{what} is not carried through a mapping of one scale across and another down"
        ));
    }
    Ok(value * region.ax.abs())
}

/// The shape carried region by region: the parts, and whether any was clipped.
fn by_regions(shape: &Shape, regions: &[Region], into: &Space) -> Result<(Vec<Shape>, bool)> {
    let wraps = into.topology() == Topology::Wrap;
    let mut parts = Vec::new();
    let mut clipped = false;
    let place = |region: &Region, p: Point| inside(region.point(p), into);
    match shape {
        Shape::Absent => parts.push(Shape::Absent),
        Shape::View { .. } => {
            return refuse("a view is on a complete panorama alone, and is not carried");
        }
        Shape::WholeImage => {
            for region in regions {
                let a = place(region, Point::new(region.lo, region.ylo));
                let b = place(region, Point::new(region.hi, region.yhi));
                parts.push(boxed(a, b));
            }
        }
        Shape::Box {
            left,
            top,
            right,
            bottom,
        } => {
            let mut covered = 0.0;
            let mut cut_down = false;
            for region in regions {
                let (lo, hi) = (left.max(region.lo), right.min(region.hi));
                let (ylo, yhi) = (top.max(region.ylo), bottom.min(region.yhi));
                if hi <= lo || yhi <= ylo {
                    continue;
                }
                covered += hi - lo;
                cut_down |= ylo > *top || yhi < *bottom;
                let a = place(region, Point::new(lo, ylo));
                let b = place(region, Point::new(hi, yhi));
                parts.push(boxed(a, b));
            }
            clipped = covered + 1e-12 < (right - left) || cut_down;
        }
        Shape::Polygon { points } => {
            for region in regions {
                let piece = clip_polygon(points, region);
                if piece.len() >= 3 {
                    let points = piece.iter().map(|p| place(region, *p)).collect();
                    parts.push(Shape::Polygon { points });
                }
            }
            clipped = points
                .iter()
                .any(|p| !regions.iter().any(|r| r.contains(*p)));
        }
        Shape::Line {
            line: Line::Column(column),
            width,
        } => {
            if let Some(region) = regions.iter().find(|r| *column >= r.lo && *column <= r.hi) {
                let x = place(region, Point::new(*column, region.ylo)).x;
                // In a wrapped space the right edge is column 0.
                let x = if wraps && x == f64::from(into.width()) {
                    0.0
                } else {
                    x
                };
                parts.push(Shape::Line {
                    line: Line::Column(x),
                    width: scaled(*width, region, "a line's width")?,
                });
            }
        }
        Shape::Line {
            line: Line::Path(path),
            width,
        } => {
            let runs = split_polyline(path, regions);
            clipped = runs.clipped;
            for (region, points) in runs.pieces {
                parts.push(Shape::Line {
                    line: Line::Path(points.iter().map(|p| place(&region, *p)).collect()),
                    width: scaled(*width, &region, "a line's width")?,
                });
            }
        }
        Shape::Stroke { path, radius } => {
            let runs = split_polyline(path, regions);
            clipped = runs.clipped;
            for (region, points) in runs.pieces {
                parts.push(Shape::Stroke {
                    path: points.iter().map(|p| place(&region, *p)).collect(),
                    radius: scaled(*radius, &region, "a stroke's radius")?,
                });
            }
        }
    }
    if parts.is_empty() {
        return refuse("the location lies wholly outside the image it is carried to");
    }
    Ok((parts, clipped))
}

fn boxed(a: Point, b: Point) -> Shape {
    let (left, right) = order(a.x, b.x);
    let (top, bottom) = order(a.y, b.y);
    Shape::Box {
        left,
        top,
        right,
        bottom,
    }
}

/// The shape carried vertex by vertex through a homography, or through a rotation, where
/// a polygon or a box is its footprint, cut at the seam or a band at a pole
/// ([`footprint::through_rotation`]).
fn by_points(shape: &Shape, binding: &Binding, toward: Toward, into: &Space) -> Result<Vec<Shape>> {
    let rotation = matches!(binding.mapping(), Mapping::Rotation { .. });
    let one = |p: Point| match toward {
        Toward::Destination => binding.to_destination(p),
        Toward::Source => binding.to_source(p),
    };
    let back = |p: Point| match toward {
        Toward::Destination => binding.to_source(p),
        Toward::Source => binding.to_destination(p),
    };
    let from = match toward {
        Toward::Destination => binding.source_space(),
        Toward::Source => binding.destination_space(),
    };
    if rotation && matches!(shape, Shape::Polygon { .. } | Shape::Box { .. }) {
        return footprint::through_rotation(shape, &from, into, &one, &back);
    }
    let many = |points: &[Point]| -> Result<Vec<Point>> {
        Ok(points
            .iter()
            .map(|p| one(*p))
            .collect::<std::result::Result<Vec<_>, _>>()?)
    };
    let not = |what: &str| {
        refuse(format!(
            "{what} is not carried through a rotation or a homography"
        ))
    };
    let shape = match shape {
        Shape::Absent => Shape::Absent,
        Shape::WholeImage if rotation => Shape::WholeImage,
        Shape::Polygon { points } => Shape::Polygon {
            points: many(points)?,
        },
        Shape::WholeImage | Shape::Box { .. } if !rotation => {
            let (left, top, right, bottom) = match shape {
                Shape::Box {
                    left,
                    top,
                    right,
                    bottom,
                } => (*left, *top, *right, *bottom),
                _ => (0.0, 0.0, f64::from(from.width()), f64::from(from.height())),
            };
            let corners = [
                Point::new(left, top),
                Point::new(right, top),
                Point::new(right, bottom),
                Point::new(left, bottom),
            ];
            Shape::Polygon {
                points: many(&corners)?,
            }
        }
        Shape::Box { .. } | Shape::WholeImage => return not("a box"),
        Shape::View { .. } => return not("a view"),
        Shape::Line { .. } => return not("a line"),
        Shape::Stroke { .. } => return not("a stroke"),
    };
    Ok(vec![shape])
}

/// A rectangle to clip a polygon to, in pixels.
pub(super) struct Window {
    pub lo: f64,
    pub hi: f64,
    pub ylo: f64,
    pub yhi: f64,
}

/// A polygon clipped to a rectangle, by [`clip_polygon`].
pub(super) fn clip_to(points: &[Point], window: &Window) -> Vec<Point> {
    let region = Region {
        lo: window.lo,
        hi: window.hi,
        ylo: window.ylo,
        yhi: window.yhi,
        ax: 1.0,
        bx: 0.0,
        ay: 1.0,
        by: 0.0,
    };
    clip_polygon(points, &region)
}

/// Sutherland and Hodgman, against the region's four edges.
fn clip_polygon(points: &[Point], region: &Region) -> Vec<Point> {
    type Inside = fn(Point, f64) -> bool;
    type Cross = fn(Point, Point, f64) -> Point;
    let planes: [(Inside, Cross, f64); 4] = [
        (|p, v| p.x >= v, cross_x, region.lo),
        (|p, v| p.x <= v, cross_x, region.hi),
        (|p, v| p.y >= v, cross_y, region.ylo),
        (|p, v| p.y <= v, cross_y, region.yhi),
    ];
    let mut output: Vec<Point> = points.to_vec();
    for (inside, cross, value) in planes {
        let input = std::mem::take(&mut output);
        let Some(last) = input.last() else {
            break;
        };
        let mut prev = *last;
        for &current in &input {
            let (cur_in, prev_in) = (inside(current, value), inside(prev, value));
            if cur_in {
                if !prev_in {
                    output.push(cross(prev, current, value));
                }
                output.push(current);
            } else if prev_in {
                output.push(cross(prev, current, value));
            }
            prev = current;
        }
    }
    let same = |a: &Point, b: &Point| (a.x - b.x).abs() < 1e-12 && (a.y - b.y).abs() < 1e-12;
    output.dedup_by(|a, b| same(a, b));
    if output.len() > 1 && same(&output[0], &output[output.len() - 1]) {
        output.pop();
    }
    output
}

fn cross_x(a: Point, b: Point, x: f64) -> Point {
    let t = (x - a.x) / (b.x - a.x);
    Point::new(x, a.y + t * (b.y - a.y))
}

fn cross_y(a: Point, b: Point, y: f64) -> Point {
    let t = (y - a.y) / (b.y - a.y);
    Point::new(a.x + t * (b.x - a.x), y)
}

struct Runs {
    pieces: Vec<(Region, Vec<Point>)>,
    clipped: bool,
}

/// A path split into runs, one for each region it crosses, each segment cut where it
/// leaves a region; what lies outside every region is dropped and said to be clipped.
fn split_polyline(path: &[Point], regions: &[Region]) -> Runs {
    let mut pieces: Vec<(Region, Vec<Point>)> = Vec::new();
    let mut clipped = false;
    for region in regions {
        let mut current: Vec<Point> = Vec::new();
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            match clip_segment(a, b, region) {
                None => {
                    if !region.contains(a) || !region.contains(b) {
                        clipped = true;
                    }
                    if current.len() >= 2 {
                        pieces.push((*region, std::mem::take(&mut current)));
                    } else {
                        current.clear();
                    }
                }
                Some((p, q)) => {
                    if p != a || q != b {
                        clipped = true;
                    }
                    if current.last() != Some(&p) {
                        if current.len() >= 2 {
                            pieces.push((*region, std::mem::take(&mut current)));
                        } else {
                            current.clear();
                        }
                        current.push(p);
                    }
                    current.push(q);
                }
            }
        }
        if current.len() >= 2 {
            pieces.push((*region, current));
        }
    }
    // A split exactly where two regions meet is no clipping: both halves were kept.
    if clipped {
        clipped = path.iter().any(|p| !regions.iter().any(|r| r.contains(*p)));
    }
    Runs { pieces, clipped }
}

/// The part of the segment from `a` to `b` inside the region (Liang and Barsky).
fn clip_segment(a: Point, b: Point, region: &Region) -> Option<(Point, Point)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut t0 = 0.0_f64;
    let mut t1 = 1.0_f64;
    for (p, q) in [
        (-dx, a.x - region.lo),
        (dx, region.hi - a.x),
        (-dy, a.y - region.ylo),
        (dy, region.yhi - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    let point = |t: f64| {
        if t == 0.0 {
            a
        } else if t == 1.0 {
            b
        } else {
            Point::new(a.x + t * dx, a.y + t * dy)
        }
    };
    Some((point(t0), point(t1)))
}
