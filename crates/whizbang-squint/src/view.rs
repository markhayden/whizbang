//! `view`: shows a region of an image closer. A crop of a flat image by box, and the same
//! region of a second image when asked. Each crop is a new image, bound onto the image it
//! was cut from (ADR 0001), so a location on the crop maps back.
//!
//! Ported from squint, `crates/squint-tools/src/builtin/view.rs` and `raster.rs`. The
//! perspective view into a panorama is not here yet (`docs/carry-over.md`). The tool's
//! record is `docs/reference/tools/view.md`.

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use whizbang_core::artifact::{ArtifactError, Image, ImageRole, Origin, Step};
use whizbang_core::identity::{
    Binding, CanonicalPixels, DecodeLimits, Direction, IdentityError, Mapping, Projection, Topology,
};

mod locate;
mod scale;

pub use locate::{Located, box_to_root};
use scale::Region;

/// The action's name, as an artifact's origin records it.
pub const NAME: &str = "view";
/// The action's version. It changes when the same request would give other pixels.
pub const VERSION: &str = "1";
/// The longer side of a crop is at most this many pixels; a larger cut is scaled down.
pub const MAX_SIDE: u32 = 1024;
/// A cut smaller than this on either side is refused as too small to show anything.
pub const MIN_CROP: u32 = 8;

/// What to show.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The image to look into. It may itself be a crop that `view` returned.
    pub image: &'a Image,
    /// A second image to show the same region of, for comparison.
    pub also: Option<&'a Image>,
    /// The region: left, top, right and bottom, as fractions of each image, so the same
    /// box names the same region of two images of different sizes.
    pub bbox: [f64; 4],
}

/// One crop: the image, and how it sits on the image it was cut from.
#[derive(Debug, Clone, PartialEq)]
pub struct Crop {
    /// The crop, a picture. Its origin names this action, the box and the source image.
    pub image: Image,
    /// The crop placed onto its source: a crop mapping whose box is the whole pixels that
    /// were cut, in the source's pixels.
    pub binding: Binding,
}

/// Why `view` returned nothing.
#[derive(Debug)]
pub enum ViewError {
    /// The request cannot be met as it was made. The text says what to send instead, and
    /// is meant for whoever made the request: a judge model receives it as the tool's
    /// result.
    Invalid(String),
    /// An image could not be read, or a crop could not be made into an artifact.
    Artifact(ArtifactError),
    /// `view` did not make what it should have: the encoder failed, or a crop did not
    /// read back as the pixels it holds.
    Failed(String),
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewError::Invalid(why) => write!(f, "invalid request: {why}"),
            ViewError::Artifact(error) => error.fmt(f),
            ViewError::Failed(why) => write!(f, "view failed: {why}"),
        }
    }
}

impl std::error::Error for ViewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ViewError::Artifact(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ArtifactError> for ViewError {
    fn from(error: ArtifactError) -> ViewError {
        ViewError::Artifact(error)
    }
}

impl From<IdentityError> for ViewError {
    fn from(error: IdentityError) -> ViewError {
        ViewError::Artifact(ArtifactError::Identity(error))
    }
}

fn invalid<T>(why: String) -> Result<T, ViewError> {
    Err(ViewError::Invalid(why))
}

/// Cuts the request's box out of its image, and out of its second image when it names
/// one, in that order. Either both crops are returned or none is.
///
/// The box snaps outward to whole pixels, so a crop never shows part of a pixel. A cut
/// whose longer side is over [`MAX_SIDE`] is scaled down to it; a smaller one is returned
/// pixel for pixel and never enlarged.
pub fn view(request: &Request<'_>, limits: &DecodeLimits) -> Result<Vec<Crop>, ViewError> {
    let bbox = request.bbox;
    let fractions = bbox.iter().all(|v| (0.0..=1.0).contains(v));
    if !fractions || bbox[0] >= bbox[2] || bbox[1] >= bbox[3] {
        return invalid(format!(
            "bbox {bbox:?} must be fractions with left < right and top < bottom"
        ));
    }
    let images = [Some(request.image), request.also];
    for (image, name) in images.iter().zip(["image", "also"]) {
        let Some(image) = image else { continue };
        if image.space().projection() != Projection::Flat {
            return invalid(format!(
                "{name} is a panorama: view shows flat images only, so far"
            ));
        }
    }
    if let Some(also) = request.also
        && also.identity().pixels == request.image.identity().pixels
    {
        return invalid("also must be a different image from image".to_owned());
    }
    images
        .into_iter()
        .flatten()
        .map(|image| crop(image, &bbox, limits))
        .collect()
}

/// One image's crop.
fn crop(image: &Image, bbox: &[f64; 4], limits: &DecodeLimits) -> Result<Crop, ViewError> {
    let space = image.space();
    let (left, right) = snap(bbox[0], bbox[2], space.width());
    let (top, bottom) = snap(bbox[1], bbox[3], space.height());
    let region = Region {
        left,
        top,
        right,
        bottom,
    };
    if region.width() < MIN_CROP || region.height() < MIN_CROP {
        return invalid(format!(
            "bbox {bbox:?} covers only {}x{} pixels of the {}x{} image; at least \
             {MIN_CROP}x{MIN_CROP} are needed",
            region.width(),
            region.height(),
            space.width(),
            space.height()
        ));
    }
    let pixels = image.pixels(limits)?;
    let (width, height) = scale::fit(region.width(), region.height(), MAX_SIDE);
    let rgba = if (width, height) == (region.width(), region.height()) {
        scale::cut(&pixels, &region)
    } else {
        scale::downscale(&pixels, &region, width, height).ok_or_else(|| {
            ViewError::Failed(format!(
                "a cut of {}x{} pixels is too large to average",
                region.width(),
                region.height()
            ))
        })?
    };
    let shown = CanonicalPixels::from_rgba(width, height, rgba)?;
    let origin = Origin::new(
        Step::new(NAME, VERSION)
            .argument("left", &bbox[0].to_string())
            .argument("top", &bbox[1].to_string())
            .argument("right", &bbox[2].to_string())
            .argument("bottom", &bbox[3].to_string())
            .input("image", image.id().0),
    )?;
    let crop = Image::new(
        png(&shown)?,
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        origin,
        limits,
    )?;
    // The file is read back through the one decoder every image is read through, and
    // must hold the pixels that were put in it.
    if crop.identity().pixels != shown.digest() {
        return Err(ViewError::Failed(
            "the crop's file does not read back as its pixels".to_owned(),
        ));
    }
    let binding = Binding::new(
        crop.identity().pixels,
        crop.space(),
        image.identity().pixels,
        space,
        Direction::SourceToDestination,
        Mapping::Crop {
            left: f64::from(left),
            top: f64::from(top),
            right: f64::from(right),
            bottom: f64::from(bottom),
        },
    )?;
    Ok(Crop {
        image: crop,
        binding,
    })
}

/// The whole pixels that the fractions `low` to `high` of a side of `size` pixels cover,
/// snapped outward: the first pixel, and one past the last. The span holds at least one
/// pixel. The fractions are from 0 to 1 and `low` is under `high`.
///
/// A product of a fraction and a size is not negative and is at most the size, so the
/// cast to a whole number rounds it down exactly; nothing here depends on a machine's
/// mathematics library.
fn snap(low: f64, high: f64, size: u32) -> (u32, u32) {
    let first = ((low * f64::from(size)) as u32).min(size - 1);
    let end = high * f64::from(size);
    let down = end as u32;
    let up = if f64::from(down) < end {
        down + 1
    } else {
        down
    };
    (first, up.clamp(first + 1, size))
}

/// The pixels as a PNG: 8 bits a channel, red, green, blue and alpha, with no other chunk.
fn png(pixels: &CanonicalPixels) -> Result<Vec<u8>, ViewError> {
    let mut file = Vec::new();
    PngEncoder::new_with_quality(&mut file, CompressionType::Default, FilterType::Adaptive)
        .write_image(
            pixels.rgba(),
            pixels.width(),
            pixels.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| ViewError::Failed(format!("cannot encode the crop as a PNG: {error}")))?;
    Ok(file)
}
