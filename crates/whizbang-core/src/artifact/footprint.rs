//! Footprints on a panorama (plan mE, task P1, item 4): a shape carried onto a complete
//! panorama through a perspective view or a rotation is the polygon of its boundary there,
//! cut at the seam into one part for each side, or a band the full width of the panorama
//! when the shape holds a pole; and a shape on a panorama is carried into a perspective
//! view as the box or the polygon of its projected boundary, clipped to the view.
//!
//! Ported from squint, `crates/squint-core/src/frames/sphere.rs` (`map_through_view`,
//! `seam_parts_through_view`, `footprint`, `map_into_view`), at the commit
//! `docs/sources.toml` pins: in pixels, through the core's arithmetic of each mapping,
//! whose functions are `libm`'s, so a carried location has the same identity on every
//! machine. Squint cut a footprint that crossed the seam to the side holding the region's
//! centre and kept the other side apart; here both sides are parts of the one carry.

use crate::identity::{Binding, Direction, Mapping, Perspective, Point, Space};

use super::{Result, Shape};

/// The most points a boundary is sampled at, so that a footprint cut at the seam, which
/// adds at most two vertices to each part, stays within a polygon's 64.
const SAMPLES: usize = 60;

fn refuse<T>(why: impl Into<String>) -> Result<T> {
    Err(super::ArtifactError::Invalid {
        artifact: super::ArtifactType::Location,
        why: why.into(),
    })
}

/// The view of a perspective mapping.
pub(super) fn perspective_of(mapping: &Mapping) -> Option<Perspective> {
    match mapping {
        Mapping::Perspective { yaw, pitch, fov } => Some(Perspective {
            yaw: *yaw,
            pitch: *pitch,
            fov: *fov,
        }),
        _ => None,
    }
}

/// The boundary of a box, a polygon or the whole of a space, as a closed ring of points:
/// a box's four edges at [`SAMPLES`] / 4 points each, clockwise from its top left; a
/// polygon's edges divided evenly while the ring has at most [`SAMPLES`] points, or its
/// vertices alone when `divide` is not set.
fn ring(shape: &Shape, space: &Space, divide: bool) -> Option<Vec<Point>> {
    let corners = |left: f64, top: f64, right: f64, bottom: f64| {
        vec![
            Point::new(left, top),
            Point::new(right, top),
            Point::new(right, bottom),
            Point::new(left, bottom),
        ]
    };
    let (vertices, per_edge) = match shape {
        Shape::Box {
            left,
            top,
            right,
            bottom,
        } => (corners(*left, *top, *right, *bottom), SAMPLES / 4),
        Shape::WholeImage => {
            let (w, h) = (f64::from(space.width()), f64::from(space.height()));
            (corners(0.0, 0.0, w, h), SAMPLES / 4)
        }
        Shape::Polygon { points } if divide => (points.clone(), (SAMPLES / points.len()).max(1)),
        Shape::Polygon { points } => (points.clone(), 1),
        _ => return None,
    };
    let mut out = Vec::with_capacity(vertices.len() * per_edge);
    for (i, a) in vertices.iter().enumerate() {
        let b = vertices[(i + 1) % vertices.len()];
        for k in 0..per_edge {
            let t = k as f64 / per_edge as f64;
            out.push(Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)));
        }
    }
    Some(out)
}

/// Whether a point lies in a box, a polygon (by ray casting) or the whole of a space.
fn holds(shape: &Shape, space: &Space, p: Point) -> bool {
    match shape {
        Shape::Box {
            left,
            top,
            right,
            bottom,
        } => p.x >= *left && p.x <= *right && p.y >= *top && p.y <= *bottom,
        Shape::WholeImage => {
            (0.0..=f64::from(space.width())).contains(&p.x)
                && (0.0..=f64::from(space.height())).contains(&p.y)
        }
        Shape::Polygon { points } => {
            let mut inside = false;
            for (i, a) in points.iter().enumerate() {
                let b = points[(i + 1) % points.len()];
                if (a.y > p.y) != (b.y > p.y) {
                    let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                    if p.x < x {
                        inside = !inside;
                    }
                }
            }
            inside
        }
        _ => false,
    }
}

/// A closed ring of points on a complete panorama, each column from 0 to its width, as the
/// parts of a location: one polygon; two, cut at the seam, when the ring crosses it; or a
/// band the full width from the top (`north`) or to the bottom (`south`) as far as the
/// ring reaches, when the shape holds that pole.
fn on_panorama(ring: &[Point], north: bool, south: bool, into: &Space) -> Result<Vec<Shape>> {
    let (w, h) = (f64::from(into.width()), f64::from(into.height()));
    let band = |top: f64, bottom: f64| Shape::Box {
        left: 0.0,
        top,
        right: w,
        bottom,
    };
    if north {
        let lowest = ring.iter().map(|p| p.y).fold(0.0, f64::max);
        return Ok(vec![band(0.0, lowest.clamp(1e-6, h))]);
    }
    if south {
        let highest = ring.iter().map(|p| p.y).fold(h, f64::min);
        return Ok(vec![band(highest.clamp(0.0, h - 1e-6), h)]);
    }
    // Each point taken round so that it is within half the width of the one before.
    let mut unwrapped: Vec<Point> = Vec::with_capacity(ring.len());
    for p in ring {
        let x = match unwrapped.last() {
            Some(before) => {
                let turns = ((before.x - p.x) / w).round();
                p.x + turns * w
            }
            None => p.x,
        };
        unwrapped.push(Point::new(x, p.y));
    }
    let closing = match (unwrapped.first(), unwrapped.last()) {
        (Some(a), Some(b)) => (a.x - b.x).abs(),
        _ => 0.0,
    };
    if closing > w / 2.0 {
        return refuse("the shape goes round the panorama without holding a pole");
    }
    let lo = unwrapped.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let hi = unwrapped
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let shifted = |by: f64| -> Vec<Point> {
        unwrapped
            .iter()
            .map(|p| Point::new(p.x + by, p.y))
            .collect()
    };
    let rings = if lo >= 0.0 && hi <= w {
        vec![unwrapped.clone()]
    } else if lo < 0.0 {
        vec![shifted(w), unwrapped.clone()]
    } else {
        vec![unwrapped.clone(), shifted(-w)]
    };
    let whole = super::carry::Window {
        lo: 0.0,
        hi: w,
        ylo: 0.0,
        yhi: h,
    };
    let parts: Vec<Shape> = rings
        .iter()
        .map(|ring| super::carry::clip_to(ring, &whole))
        .filter(|part| part.len() >= 3)
        .map(|points| Shape::Polygon { points })
        .collect();
    if parts.is_empty() {
        return refuse("the shape has no area on the panorama");
    }
    Ok(parts)
}

/// A shape carried through a perspective view: from the view onto its panorama when
/// `forward`, or from the panorama into the view. The parts, and whether any was clipped.
pub(super) fn through_view(
    shape: &Shape,
    binding: &Binding,
    forward: bool,
    into: &Space,
) -> Result<(Vec<Shape>, bool)> {
    let Some(view) = perspective_of(binding.mapping()) else {
        return refuse("the binding is not a perspective view");
    };
    let basis = view.basis();
    // The view is the end the mapping's parameters run from.
    let (square, panorama) = match binding.direction() {
        Direction::SourceToDestination => (binding.source_space(), binding.destination_space()),
        Direction::DestinationToSource => (binding.destination_space(), binding.source_space()),
    };
    let _ = into;
    let side = f64::from(square.width());
    let (w, h) = (f64::from(panorama.width()), f64::from(panorama.height()));
    match shape {
        Shape::Absent => return Ok((vec![Shape::Absent], false)),
        Shape::View { .. } => return refuse("a view is not carried through a perspective view"),
        Shape::Line { .. } => return refuse("a line has no one width through a perspective view"),
        Shape::Stroke { .. } => {
            return refuse("a stroke has no one radius through a perspective view");
        }
        _ => {}
    }
    if forward {
        let Some(points) = ring(shape, &square, true) else {
            return refuse("the shape is not carried through a perspective view");
        };
        let mapped: Vec<Point> = points
            .iter()
            .map(|p| {
                let (x, y) = basis.to_panorama(side, w, h, p.x, p.y);
                Point::new(x, y)
            })
            .collect();
        // A pole is held when the view sees it, and the shape holds where it is seen.
        let pole = |y: f64| {
            basis
                .from_panorama(side, w, h, 0.0, y)
                .is_some_and(|(x, y)| holds(shape, &square, Point::new(x, y)))
        };
        let parts = on_panorama(&mapped, pole(0.0), pole(h), &panorama)?;
        return Ok((parts, false));
    }
    if matches!(shape, Shape::WholeImage) {
        // A view sees nothing but its panorama.
        return Ok((vec![Shape::WholeImage], false));
    }
    let divide = matches!(shape, Shape::Box { .. });
    let Some(points) = ring(shape, &panorama, divide) else {
        return refuse("the shape is not carried through a perspective view");
    };
    let seen: Vec<Point> = points
        .iter()
        .filter_map(|p| basis.from_panorama(side, w, h, p.x, p.y))
        .map(|(x, y)| Point::new(x, y))
        .collect();
    let inside = |p: &Point| (0.0..=side).contains(&p.x) && (0.0..=side).contains(&p.y);
    let partial = seen.len() < points.len() || !seen.iter().all(inside);
    let clamp = |p: &Point| Point::new(p.x.clamp(0.0, side), p.y.clamp(0.0, side));
    match shape {
        Shape::Box { .. } => {
            if seen.is_empty() {
                return refuse("the location lies wholly behind the view");
            }
            let (mut x0, mut y0, mut x1, mut y1) = (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            );
            for p in &seen {
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            }
            let (left, top) = (x0.clamp(0.0, side), y0.clamp(0.0, side));
            let (right, bottom) = (x1.clamp(0.0, side), y1.clamp(0.0, side));
            if right <= left || bottom <= top {
                return refuse("the location lies wholly outside the view");
            }
            Ok((
                vec![Shape::Box {
                    left,
                    top,
                    right,
                    bottom,
                }],
                partial,
            ))
        }
        _ => {
            let mut points: Vec<Point> = seen.iter().map(clamp).collect();
            points.dedup();
            if points.len() < 3 {
                return refuse("the location lies wholly outside the view");
            }
            Ok((vec![Shape::Polygon { points }], partial))
        }
    }
}

/// A shape carried onto a complete panorama through a rotation: a polygon vertex by vertex,
/// a box by its boundary, cut at the seam or made a band at a pole as through a view. The
/// whole image is the whole image.
pub(super) fn through_rotation(
    shape: &Shape,
    from: &Space,
    into: &Space,
    one: &dyn Fn(Point) -> std::result::Result<Point, crate::identity::IdentityError>,
    back: &dyn Fn(Point) -> std::result::Result<Point, crate::identity::IdentityError>,
) -> Result<Vec<Shape>> {
    let Some(points) = ring(shape, from, matches!(shape, Shape::Box { .. })) else {
        return refuse("the shape is not carried through a rotation");
    };
    let mapped = points
        .iter()
        .map(|p| one(*p))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let h = f64::from(into.height());
    // A pole of the panorama it lands on is held when the point it comes from is in it.
    let pole = |y: f64| back(Point::new(0.0, y)).is_ok_and(|p| holds(shape, from, p));
    on_panorama(&mapped, pole(0.0), pole(h), into)
}
