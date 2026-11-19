//! What `cut`, `put`, `window`, `unwindow` and `apply` share: pixels cut from a picture and
//! written back into one, the check that a picture changed only where a mask says it may,
//! and a new picture made into an image.

use whizbang_core::artifact::{self, Artifact};
use whizbang_core::artifact::{
    ArtifactId, ArtifactType, Image, ImageRole, Mask, Origin, Plan, Step,
};
use whizbang_core::contract::{Context, Failure};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Space, encode};

/// The rectangle of `pixels` from `left`, `top`, `width` across and `height` down. The
/// rectangle is inside the picture.
pub(crate) fn crop(
    pixels: &CanonicalPixels,
    [left, top, width, height]: [u32; 4],
) -> Result<CanonicalPixels, Failure> {
    let stride = pixels.width() as usize * 4;
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in top..top + height {
        let start = y as usize * stride + left as usize * 4;
        rgba.extend_from_slice(&pixels.rgba()[start..start + width as usize * 4]);
    }
    CanonicalPixels::from_rgba(width, height, rgba)
        .map_err(|error| Failure::error("pixels", &error.to_string()))
}

/// `pixels` with `patch` written over it from `left`, `top`; nothing else changes. The
/// patch lies inside the picture.
pub(crate) fn written(
    pixels: &CanonicalPixels,
    patch: &CanonicalPixels,
    left: u32,
    top: u32,
) -> Result<CanonicalPixels, Failure> {
    let stride = pixels.width() as usize * 4;
    let row = patch.width() as usize * 4;
    let mut rgba = pixels.rgba().to_vec();
    for (y, line) in patch.rgba().chunks(row).enumerate() {
        let start = (top as usize + y) * stride + left as usize * 4;
        rgba[start..start + row].copy_from_slice(line);
    }
    CanonicalPixels::from_rgba(pixels.width(), pixels.height(), rgba)
        .map_err(|error| Failure::error("pixels", &error.to_string()))
}

/// Whether `after` is `before` wherever the mask weighs nothing: outside its core and its
/// blend, or under protect. The mask's planes are read in the pictures' coordinates; the
/// two pictures and the mask are one size. A refusal names how many pixels differ and
/// the first.
pub(crate) fn kept(
    before: &CanonicalPixels,
    after: &CanonicalPixels,
    mask: &Mask,
    what: &str,
) -> Result<(), Failure> {
    let width = before.width() as usize;
    let (mut changed, mut first) = (0_u64, None);
    let pixels = before.rgba().chunks(4).zip(after.rgba().chunks(4));
    for (at, (was, is)) in pixels.enumerate() {
        let nothing = mask.protect()[at] != 0 || (mask.core()[at] == 0 && mask.blend()[at] == 0);
        if nothing && was != is {
            changed += 1;
            first.get_or_insert(at);
        }
    }
    match first {
        None => Ok(()),
        Some(at) => Err(Failure::invalid(&format!(
            "{what} differs from what was cut where the mask weighs nothing: {changed} \
             pixels, the first at {}, {}",
            at % width,
            at / width
        ))),
    }
}

/// New pixels made into a picture image in `space`'s projection and topology, written by
/// the core's one encoder, whose origin is `step`.
pub(crate) fn image(pixels: &CanonicalPixels, space: Space, step: Step) -> Result<Image, Failure> {
    let file = encode::png(pixels).map_err(|why| Failure::error("encode", &why))?;
    Origin::new(step)
        .and_then(|origin| {
            Image::new(
                file,
                space.projection(),
                space.topology(),
                ImageRole::Picture,
                origin,
                &DecodeLimits::default(),
            )
        })
        .map_err(|error| Failure::error("image", &error.without_paths()))
}

/// The plan of this identity, from the run's store.
pub(crate) fn plan(context: &Context, id: ArtifactId) -> Result<Plan, Failure> {
    let read = artifact::get(
        context.store(),
        ArtifactType::Plan,
        id,
        &DecodeLimits::default(),
    )
    .map_err(|error| Failure::error("input", &error.without_paths()))?;
    match read {
        Artifact::Plan(plan) => Ok(plan),
        _ => Err(Failure::error("input", "what was read is not a plan")),
    }
}

/// The one identity given at `port`.
pub(crate) fn one(
    call: &whizbang_core::contract::PreparedCall,
    port: &str,
) -> Result<ArtifactId, Failure> {
    crate::operation::given(call, port)
        .first()
        .copied()
        .ok_or_else(|| Failure::error("input", &format!("the call gives no {port}")))
}
