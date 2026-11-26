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

use crate::identity::{Binding, Direction, Mapping, Perspective, Point, Space, panorama_direction};

use super::{Result, Shape};

/// The most points a boundary is sampled at, to tell which way round the panorama each step
/// goes: a box's edge at 60 points, a polygon's edges evenly at a multiple of [`KEEP`].
const SAMPLES: usize = 240;

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
        Shape::Polygon { points } if divide => (
            points.clone(),
            KEEP * (SAMPLES / KEEP / points.len()).max(1),
        ),
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

/// One sample in this many of a ring taken round the seam is kept for the outline, a box's
/// corners and a polygon's vertices among them: the dense samples tell which way round each
/// step goes, and the outline of a sixtieth of the edge keeps the polygon simple and within
/// its 64 vertices.
const KEEP: usize = 4;

/// Every [`KEEP`]th point of a ring, from its first.
fn coarse(ring: &[Point]) -> Vec<Point> {
    ring.iter().step_by(KEEP).copied().collect()
}

/// The most vertices a polygon holds (`Location`'s rule).
const MAX_VERTICES: usize = 64;

/// A part cut from a ring, thinned evenly to at most [`MAX_VERTICES`], its first kept.
fn thinned(points: Vec<Point>) -> Vec<Point> {
    if points.len() <= MAX_VERTICES {
        return points;
    }
    let n = points.len();
    (0..MAX_VERTICES)
        .map(|i| points[i * n / MAX_VERTICES])
        .collect()
}

/// Each point of a ring on a panorama `w` wide taken round so that it is within half the
/// width of the one before. A step that is still over a quarter of the width is refused:
/// which way round it went is not known.
fn unwrapped(ring: &[Point], w: f64) -> Result<Vec<Point>> {
    let mut out: Vec<Point> = Vec::with_capacity(ring.len());
    for p in ring {
        let x = match out.last() {
            Some(before) => {
                let x = p.x + ((before.x - p.x) / w).round() * w;
                if (x - before.x).abs() > w / 4.0 {
                    return refuse(
                        "the shape's edge moves too far between two samples to know which way \
                         round the panorama it goes",
                    );
                }
                x
            }
            None => p.x,
        };
        out.push(Point::new(x, p.y));
    }
    Ok(out)
}

/// The pieces of a simple ring that lie between the columns 0 and `w`, each a simple
/// polygon. Sutherland and Hodgman would join the pieces into one polygon by edges along
/// the two columns, which is not simple; here the ring is cut where it crosses either
/// column into chains that lie between them, and the chains are joined along a column only
/// to the next chain that starts there, in the way the ring turns, as Weiler and Atherton
/// join them.
fn strip(ring: &[Point], w: f64) -> Vec<Vec<Point>> {
    let inside = |p: &Point| p.x >= 0.0 && p.x <= w;
    if ring.iter().all(inside) {
        return vec![ring.to_vec()];
    }
    // Which column a point outside is beyond, and where an edge crosses it.
    let beyond = |p: &Point| if p.x < 0.0 { 0.0 } else { w };
    let cross = |a: Point, b: Point, x: f64| {
        let t = (x - a.x) / (b.x - a.x);
        Point::new(x, a.y + t * (b.y - a.y))
    };
    // The chains inside, each from where it comes in to where it goes out.
    let n = ring.len();
    let Some(start) = (0..n).find(|i| !inside(&ring[*i])) else {
        return vec![ring.to_vec()];
    };
    let mut chains: Vec<Vec<Point>> = Vec::new();
    let mut current: Option<Vec<Point>> = None;
    for step in 0..n {
        let a = ring[(start + step) % n];
        let b = ring[(start + step + 1) % n];
        match (inside(&a), inside(&b)) {
            (false, true) => current = Some(vec![cross(a, b, beyond(&a)), b]),
            (true, true) => {
                if let Some(chain) = current.as_mut() {
                    chain.push(b);
                }
            }
            (true, false) => {
                if let Some(mut chain) = current.take() {
                    chain.push(cross(a, b, beyond(&b)));
                    chains.push(chain);
                }
            }
            (false, false) => {
                // An edge from beyond one column to beyond the other passes between them.
                if beyond(&a) != beyond(&b) {
                    let (p, q) = (cross(a, b, beyond(&a)), cross(a, b, beyond(&b)));
                    chains.push(vec![p, q]);
                }
            }
        }
    }
    // Along the right column the ring's inside lies one way and along the left the other,
    // by the way the ring turns.
    let turning: f64 = (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    let along = |x: f64| -> f64 {
        let on_right = x == w;
        if (turning > 0.0) == on_right {
            1.0
        } else {
            -1.0
        }
    };
    let mut used = vec![false; chains.len()];
    let mut pieces = Vec::new();
    for first in 0..chains.len() {
        if used[first] {
            continue;
        }
        let mut piece: Vec<Point> = Vec::new();
        let mut at = first;
        loop {
            used[at] = true;
            piece.extend_from_slice(&chains[at]);
            let Some(end) = chains[at].last().copied() else {
                break;
            };
            let way = along(end.x);
            // The nearest chain that starts on this column, ahead of where this one ends.
            let next = (0..chains.len())
                .filter(|i| !used[*i] || *i == first)
                .filter(|i| chains[*i][0].x == end.x)
                .map(|i| (i, (chains[i][0].y - end.y) * way))
                .filter(|(_, ahead)| *ahead >= 0.0)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            match next {
                Some((i, _)) if i != first => at = i,
                _ => break,
            }
        }
        let same = |a: &Point, b: &Point| (a.x - b.x).abs() < 1e-12 && (a.y - b.y).abs() < 1e-12;
        piece.dedup_by(|a, b| same(a, b));
        if piece.len() > 1 && same(&piece[0], &piece[piece.len() - 1]) {
            piece.pop();
        }
        pieces.push(piece);
    }
    pieces
}

/// A closed ring of points on a complete panorama, as the parts of a location: the ring
/// taken round the seam ([`unwrapped`]) and clipped to the panorama once for each whole
/// turn it reaches into, so a part on either side of the seam, or on both, is kept; or a
/// band the full width from the top (`north`) or to the bottom (`south`) as far as the
/// ring reaches, when the shape holds that pole, and the whole panorama when it holds
/// both. `taken` is the ring already taken round, when the caller did that itself.
fn on_panorama(
    ring: &[Point],
    north: bool,
    south: bool,
    into: &Space,
    taken: Option<Vec<Point>>,
) -> Result<Vec<Shape>> {
    let (w, h) = (f64::from(into.width()), f64::from(into.height()));
    let band = |top: f64, bottom: f64| Shape::Box {
        left: 0.0,
        top,
        right: w,
        bottom,
    };
    match (north, south) {
        (true, true) => return Ok(vec![band(0.0, h)]),
        (true, false) => {
            let lowest = ring.iter().map(|p| p.y).fold(0.0, f64::max);
            return Ok(vec![band(0.0, lowest.clamp(1e-6, h))]);
        }
        (false, true) => {
            let highest = ring.iter().map(|p| p.y).fold(h, f64::min);
            return Ok(vec![band(highest.clamp(0.0, h - 1e-6), h)]);
        }
        (false, false) => {}
    }
    let unwrapped = match taken {
        Some(taken) => taken,
        None => coarse(&unwrapped(ring, w)?),
    };
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
    // Every whole turn `k` for which the ring, shifted by `k` widths, meets the panorama.
    let first = (-hi / w).floor() as i64;
    let last = (-lo / w).ceil() as i64;
    let mut parts = Vec::new();
    for k in first..=last {
        let by = k as f64 * w;
        if hi + by <= 0.0 || lo + by >= w {
            continue;
        }
        let shifted: Vec<Point> = unwrapped
            .iter()
            .map(|p| Point::new(p.x + by, p.y))
            .collect();
        for part in strip(&shifted, w) {
            if part.len() >= 3 {
                parts.push(Shape::Polygon {
                    points: thinned(part),
                });
            }
        }
    }
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
        let parts = on_panorama(&mapped, pole(0.0), pole(h), &panorama, None)?;
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
    // The ring's directions, clipped to what lies in front of the view (Sutherland and
    // Hodgman against the plane of depth `FRONT`), then seen and clipped to the view.
    const FRONT: f64 = 1e-6;
    let directions: Vec<[f64; 3]> = points
        .iter()
        .map(|p| panorama_direction(w, h, p.x, p.y))
        .collect();
    let mut front: Vec<[f64; 3]> = Vec::with_capacity(directions.len() + 2);
    for (i, a) in directions.iter().enumerate() {
        let b = directions[(i + 1) % directions.len()];
        let (da, db) = (basis.depth(*a), basis.depth(b));
        if da >= FRONT {
            front.push(*a);
        }
        if (da >= FRONT) != (db >= FRONT) {
            let t = (FRONT - da) / (db - da);
            front.push([
                a[0] + t * (b[0] - a[0]),
                a[1] + t * (b[1] - a[1]),
                a[2] + t * (b[2] - a[2]),
            ]);
        }
    }
    if front.len() < 3 {
        return refuse("the location lies wholly behind the view");
    }
    let seen: Vec<Point> = front
        .iter()
        .map(|d| {
            let (x, y) = basis.seen(side, *d);
            Point::new(x, y)
        })
        .collect();
    let window = super::carry::Window {
        lo: 0.0,
        hi: side,
        ylo: 0.0,
        yhi: side,
    };
    let clipped = super::carry::clip_to(&seen, &window);
    if clipped.len() < 3 {
        return refuse("the location lies wholly outside the view");
    }
    let partial = front.len() != points.len() || clipped != seen;
    match shape {
        Shape::Box { .. } => {
            let (mut x0, mut y0, mut x1, mut y1) = (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            );
            for p in &clipped {
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            }
            if x1 <= x0 || y1 <= y0 {
                return refuse("the location lies wholly outside the view");
            }
            Ok((
                vec![Shape::Box {
                    left: x0,
                    top: y0,
                    right: x1,
                    bottom: y1,
                }],
                partial,
            ))
        }
        _ => Ok((
            vec![Shape::Polygon {
                points: thinned(clipped),
            }],
            partial,
        )),
    }
}

/// A shape carried onto a complete panorama through a rotation, cut at the seam or made a
/// band at a pole as through a view. A box is sampled along its boundary. A polygon keeps
/// its vertices, each mapped as before, but which way round the panorama each lies is read
/// from its edges sampled as a box's are: a polygon wider than half the panorama is then
/// taken round the right way, and one whose edge leaps too far between two samples is
/// refused. The whole image is the whole image.
pub(super) fn through_rotation(
    shape: &Shape,
    from: &Space,
    into: &Space,
    one: &dyn Fn(Point) -> std::result::Result<Point, crate::identity::IdentityError>,
    back: &dyn Fn(Point) -> std::result::Result<Point, crate::identity::IdentityError>,
) -> Result<Vec<Shape>> {
    let (w, h) = (f64::from(into.width()), f64::from(into.height()));
    let Some(dense) = ring(shape, from, true) else {
        return refuse("the shape is not carried through a rotation");
    };
    let mapped = dense
        .iter()
        .map(|p| one(*p))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    // A pole of the panorama it lands on is held when the point it comes from is in it.
    let pole = |y: f64| back(Point::new(0.0, y)).is_ok_and(|p| holds(shape, from, p));
    let (north, south) = (pole(0.0), pole(h));
    if north || south {
        return on_panorama(&mapped, north, south, into, None);
    }
    let taken = unwrapped(&mapped, w)?;
    let taken = match shape {
        // Each vertex is the sample at the start of its edge, mapped as it always was.
        Shape::Polygon { points } => {
            let per_edge = dense.len() / points.len();
            (0..points.len()).map(|i| taken[i * per_edge]).collect()
        }
        _ => coarse(&taken),
    };
    let landed: Vec<Point> = taken
        .iter()
        .map(|p| Point::new(p.x.rem_euclid(w), p.y))
        .collect();
    on_panorama(&landed, north, south, into, Some(taken))
}
