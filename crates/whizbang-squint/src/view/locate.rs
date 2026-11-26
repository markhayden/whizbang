//! A box on a crop, mapped back to the image the crop was cut from, through the crop's
//! binding and the bindings of the crops it was cut from in turn (ADR 0001).
//!
//! Ported from squint, `crates/squint-core/src/frames/mod.rs` (`to_root` and
//! `map_through_bbox`), for boxes through crops; and `frames/aligned.rs`
//! (`map_through_homography`, `map_into_homography`), for a box on an aligned frame and back.
//! A box on a perspective view, or a crop of one, lands on its panorama through `carry`
//! (`box_onto_panorama`); other shapes are carried by `whizbang_core::artifact::carry`.

use whizbang_core::artifact::{Location, Origin, Shape, Step, Toward, carry};
use whizbang_core::identity::{Binding, Direction, IdentityError, Mapping, Point};

use super::{ViewError, invalid};

/// A box in the pixels of the image a chain of crops ends on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Located {
    /// Left, top, right and bottom.
    pub bbox: [f64; 4],
    /// True when the box reached past the crop it was stated on and was clipped to it.
    pub partial: bool,
}

/// Maps a box stated in the pixels of a crop to the pixels of the image the chain ends
/// on. `chain` starts with the crop's own binding; each binding after it is that of the
/// image the one before was cut from, so the last one's destination is the root image.
///
/// A box that reaches past the crop is clipped to it and flagged. A box with no area, or
/// one that lies wholly outside the crop, is the caller's mistake. Each edge of the crop
/// maps onto its edge of the cut exactly, however many crops the chain holds (ADR 0001).
///
/// The result may enter hashed bytes, as a location's coordinates do (ADR 0002): it is
/// worked out by additions, multiplications and divisions alone, which every machine
/// rounds alike, and by no function of a mathematics library.
pub fn box_to_root(bbox: [f64; 4], chain: &[Binding]) -> Result<Located, ViewError> {
    let refuse = |why: String| Err(ViewError::from(IdentityError::Binding(why)));
    let Some((first, rest)) = chain.split_first() else {
        return refuse("a box maps back through at least one binding".to_owned());
    };
    for binding in chain {
        let crop = matches!(binding.mapping(), Mapping::Crop { .. })
            && binding.direction() == Direction::SourceToDestination;
        if !crop {
            return refuse(format!(
                "a box maps back through crops stated from their source, and {:?} is not one",
                binding.mapping()
            ));
        }
    }
    if bbox.iter().any(|v| !v.is_finite()) || bbox[0] >= bbox[2] || bbox[1] >= bbox[3] {
        return invalid(format!(
            "box {bbox:?} must have left < right and top < bottom"
        ));
    }
    let space = first.source_space();
    let (width, height) = (f64::from(space.width()), f64::from(space.height()));
    let clipped = [
        bbox[0].max(0.0),
        bbox[1].max(0.0),
        bbox[2].min(width),
        bbox[3].min(height),
    ];
    if clipped[0] >= clipped[2] || clipped[1] >= clipped[3] {
        return invalid(format!(
            "box {bbox:?} lies wholly outside the {}x{} crop",
            space.width(),
            space.height()
        ));
    }
    let mut whole = first.clone();
    for next in rest {
        whole = whole.then(next)?;
    }
    let low = whole.to_destination(Point::new(clipped[0], clipped[1]))?;
    let high = whole.to_destination(Point::new(clipped[2], clipped[3]))?;
    Ok(Located {
        bbox: [low.x, low.y, high.x, high.y],
        partial: clipped != bbox,
    })
}

/// A box on an aligned frame, traced onto the image the frame was warped from.
#[derive(Debug, Clone, PartialEq)]
pub struct Traced {
    /// The box's four corners, left-top, right-top, right-bottom and left-bottom, in the
    /// pixels of the image: a polygon, since a homography does not keep a box a box.
    pub points: Vec<Point>,
    /// True when a corner landed outside the image and was clamped into it.
    pub partial: bool,
}

/// The matrix of an aligned frame's binding onto the image it was warped from (frame pixels
/// to image pixels), or the refusal.
fn homography(binding: &Binding) -> Result<[f64; 9], ViewError> {
    if let Mapping::Homography { matrix } = binding.mapping()
        && binding.direction() == Direction::SourceToDestination
    {
        return Ok(*matrix);
    }
    Err(ViewError::from(IdentityError::Binding(format!(
        "a box maps through an aligned frame's homography stated from the frame, and {:?} is \
         not one",
        binding.mapping()
    ))))
}

/// Maps a box stated in the pixels of an aligned frame to the image the frame was warped
/// from, through the frame's binding (`crate::align`). As squint's `map_through_homography`:
/// the box becomes the polygon of its corners; a corner outside the image is clamped into it
/// and the result flagged; a box none of whose corners lands inside the image, or one whose
/// corner is on the homography's horizon, is refused. A box that reaches past the frame is
/// first clipped to it and flagged, as [`box_to_root`] clips one to its crop.
///
/// A homography is worked out by additions, multiplications and divisions alone, so the
/// result is the same on every machine (ADR 0001).
pub fn box_through_homography(bbox: [f64; 4], binding: &Binding) -> Result<Traced, ViewError> {
    let matrix = homography(binding)?;
    if bbox.iter().any(|v| !v.is_finite()) || bbox[0] >= bbox[2] || bbox[1] >= bbox[3] {
        return invalid(format!(
            "box {bbox:?} must have left < right and top < bottom"
        ));
    }
    // Clipped to the frame first, as `box_to_root` clips a box to its crop: a binding maps
    // the points of its own space alone.
    let frame = binding.source_space();
    let (fw, fh) = (f64::from(frame.width()), f64::from(frame.height()));
    let clipped = [
        bbox[0].max(0.0),
        bbox[1].max(0.0),
        bbox[2].min(fw),
        bbox[3].min(fh),
    ];
    if clipped[0] >= clipped[2] || clipped[1] >= clipped[3] {
        return invalid(format!(
            "box {bbox:?} lies wholly outside the {}x{} frame",
            frame.width(),
            frame.height()
        ));
    }
    let corners = [
        Point::new(clipped[0], clipped[1]),
        Point::new(clipped[2], clipped[1]),
        Point::new(clipped[2], clipped[3]),
        Point::new(clipped[0], clipped[3]),
    ];
    let mut mapped = Vec::new();
    for corner in corners {
        let Some(point) = project(&matrix, corner) else {
            return invalid(format!("box {bbox:?} maps onto the horizon of the image"));
        };
        mapped.push(point);
    }
    let image = binding.destination_space();
    let (width, height) = (f64::from(image.width()), f64::from(image.height()));
    let inside = |p: &Point| (0.0..=width).contains(&p.x) && (0.0..=height).contains(&p.y);
    if !mapped.iter().any(inside) {
        return invalid(format!(
            "box {bbox:?} maps wholly outside the {}x{} image",
            image.width(),
            image.height()
        ));
    }
    let clamped: Vec<Point> = mapped
        .iter()
        .map(|p| Point::new(p.x.clamp(0.0, width), p.y.clamp(0.0, height)))
        .collect();
    let partial = clamped != mapped || clipped != bbox;
    Ok(Traced {
        points: clamped,
        partial,
    })
}

/// Maps points stated in the pixels of the image an aligned frame was warped from into the
/// frame, through the frame's binding: squint's `map_into_homography`, without the clamp.
pub fn points_into_homography(
    points: &[Point],
    binding: &Binding,
) -> Result<Vec<Point>, ViewError> {
    let matrix = homography(binding)?;
    let Some(back) = invert(&matrix) else {
        return invalid(format!("the homography {matrix:?} is singular"));
    };
    let mut mapped = Vec::new();
    for point in points {
        let Some(into) = project(&back, *point) else {
            return invalid(format!("{point:?} maps onto the horizon of the frame"));
        };
        mapped.push(into);
    }
    Ok(mapped)
}

/// A point through a matrix on pixels; none on its horizon, where the point has no finite
/// image (squint's `apply`).
fn project(h: &[f64; 9], p: Point) -> Option<Point> {
    let d = h[6] * p.x + h[7] * p.y + h[8];
    if d.abs() < 1e-12 {
        return None;
    }
    let q = Point::new(
        (h[0] * p.x + h[1] * p.y + h[2]) / d,
        (h[3] * p.x + h[4] * p.y + h[5]) / d,
    );
    (q.x.is_finite() && q.y.is_finite()).then_some(q)
}

/// Adjugate over determinant; none when singular (squint's `aligned.rs`, `invert`).
fn invert(m: &[f64; 9]) -> Option<[f64; 9]> {
    let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6]);
    if det.abs() < 1e-300 || !det.is_finite() {
        return None;
    }
    Some([
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

/// A box on a perspective view, or on a crop of one, on the panorama the view looks into:
/// the parts of its footprint there (plan mE, task P1, item 4).
#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    /// The polygon of the box's boundary on the panorama; two, cut at the seam, when it
    /// crosses it; or a band the panorama's full width when the box holds a pole.
    pub parts: Vec<Shape>,
    /// True when the box reached past the crop it was stated on and was clipped to it.
    pub partial: bool,
}

/// Maps a box stated in the pixels of a perspective view, or of a chain of crops cut from
/// one, onto the panorama the view looks into. `chain` is as [`box_to_root`] takes it,
/// and its last binding is the view's, a perspective mapping stated from the view: the
/// crops before it bring the box onto the view as [`box_to_root`] does, and the view's
/// binding carries it onto the panorama ([`whizbang_core::artifact::carry`]).
pub fn box_onto_panorama(bbox: [f64; 4], chain: &[Binding]) -> Result<Footprint, ViewError> {
    let refuse = |why: String| Err(ViewError::from(IdentityError::Binding(why)));
    let Some((view, crops)) = chain.split_last() else {
        return refuse("a box maps onto a panorama through at least one binding".to_owned());
    };
    let perspective = matches!(view.mapping(), Mapping::Perspective { .. })
        && view.direction() == Direction::SourceToDestination;
    if !perspective {
        return refuse(format!(
            "a box maps onto a panorama through a perspective view stated from the view, and \
             {:?} is not one",
            view.mapping()
        ));
    }
    let (on_view, partial) = if crops.is_empty() {
        let space = view.source_space();
        let (width, height) = (f64::from(space.width()), f64::from(space.height()));
        if bbox.iter().any(|v| !v.is_finite()) || bbox[0] >= bbox[2] || bbox[1] >= bbox[3] {
            return invalid(format!(
                "box {bbox:?} must have left < right and top < bottom"
            ));
        }
        let clipped = [
            bbox[0].max(0.0),
            bbox[1].max(0.0),
            bbox[2].min(width),
            bbox[3].min(height),
        ];
        if clipped[0] >= clipped[2] || clipped[1] >= clipped[3] {
            return invalid(format!(
                "box {bbox:?} lies wholly outside the {}x{} view",
                space.width(),
                space.height()
            ));
        }
        (clipped, clipped != bbox)
    } else {
        let located = box_to_root(bbox, crops)?;
        (located.bbox, located.partial)
    };
    let shape = Shape::Box {
        left: on_view[0],
        top: on_view[1],
        right: on_view[2],
        bottom: on_view[3],
    };
    let origin = Origin::new(Step::new("locate", "1"))?;
    let at = Location::new(
        Binding::root(view.source(), view.source_space()),
        shape,
        origin,
    )?;
    let carried = carry(&at, view, Toward::Destination)?;
    Ok(Footprint {
        parts: carried
            .parts
            .iter()
            .map(|part| part.shape().clone())
            .collect(),
        partial: partial || carried.partial,
    })
}
