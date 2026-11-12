//! A box on a crop, mapped back to the image the crop was cut from, through the crop's
//! binding and the bindings of the crops it was cut from in turn (ADR 0001).
//!
//! Ported from squint, `crates/squint-core/src/frames/mod.rs` (`to_root` and
//! `map_through_bbox`), for boxes through crops. Other shapes and other mappings are on
//! `docs/carry-over.md`.

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
