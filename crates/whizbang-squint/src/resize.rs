//! `resize`: an image scaled down to a chosen long edge, so the whole picture can be seen at a
//! size the eye takes in at once. The result covers the whole of the image, bound onto it by a
//! crop of the whole (ADR 0001), so a location on it maps back one to one.
//!
//! Ported from squint, `crates/squint-tools/src/builtin/resize.rs`, its manifest
//! `crates/squint-tools/manifests/resize.toml`, and the scaling of `builtin/raster.rs`
//! (`fit`): the pixels as squint loaded them (red, green and blue; an alpha is dropped),
//! scaled with the `image` crate's triangle filter. The record is
//! `docs/reference/tools/resize.md`.

use image::RgbImage;
use image::imageops::{self, FilterType};
use whizbang_core::artifact::{self, Artifact, ArtifactError, ArtifactType, Image, ImageRole};
use whizbang_core::artifact::{Origin, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, DescriptorError, Failure, InputPort, Kind,
    OutputPort, PreparedCall, Produced, Value, ValueType,
};
use whizbang_core::identity::{Binding, CanonicalPixels, DecodeLimits, Direction, Mapping};

/// The tool's name.
pub const NAME: &str = "resize";
/// The version a result's origin records: squint's manifest's.
pub const VERSION: &str = "1.0.0";
/// The version of the action's descriptor.
pub const ACTION_VERSION: &str = "1.0.0";
/// The least and the most long edge that may be asked for, in pixels.
pub const LONG_EDGE: (u32, u32) = (64, 1024);

/// The image scaled down, and its binding onto the image.
#[derive(Debug, Clone, PartialEq)]
pub struct Resized {
    pub image: Image,
    pub binding: Binding,
}

/// Why `resize` returned nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResizeError {
    /// The request cannot be met as it was made; the sentence is for whoever made it.
    Invalid(String),
    /// The image could not be read, or the result could not be made.
    Failed(String),
}

impl std::fmt::Display for ResizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResizeError::Invalid(why) => write!(f, "invalid request: {why}"),
            ResizeError::Failed(why) => write!(f, "resize failed: {why}"),
        }
    }
}

/// Scales `image` down so that its longer side is `long_edge`. A long edge outside
/// [`LONG_EDGE`], or one that would not shrink the image, is refused with a sentence for
/// whoever asked: `resize` never enlarges.
pub fn resize(
    image: &Image,
    long_edge: u32,
    limits: &DecodeLimits,
) -> Result<Resized, ResizeError> {
    let (low, high) = LONG_EDGE;
    if !(low..=high).contains(&long_edge) {
        return Err(ResizeError::Invalid(format!(
            "long_edge {long_edge} outside [{low}, {high}]"
        )));
    }
    let space = image.space();
    let longer = space.width().max(space.height());
    if long_edge >= longer {
        return Err(ResizeError::Invalid(format!(
            "long_edge {long_edge} would not shrink the image, whose longer side is {longer}; \
             resize never enlarges"
        )));
    }
    let fault = |error: ArtifactError| ResizeError::Failed(error.to_string());
    let pixels = image.pixels(limits).map_err(fault)?;
    let rgb: Vec<u8> = pixels
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    let rgb = RgbImage::from_raw(pixels.width(), pixels.height(), rgb)
        .ok_or_else(|| ResizeError::Failed("the pixels do not make an image".to_owned()))?;
    let out = fit(rgb, long_edge);
    let rgba: Vec<u8> = out
        .pixels()
        .flat_map(|pixel| [pixel.0[0], pixel.0[1], pixel.0[2], 255])
        .collect();
    let shown = CanonicalPixels::from_rgba(out.width(), out.height(), rgba)
        .map_err(|error| fault(error.into()))?;
    let origin = Origin::new(
        Step::new(NAME, VERSION)
            .argument("long_edge", &long_edge.to_string())
            .input("image", image.id().0),
    )
    .map_err(fault)?;
    let file = crate::view::png(&shown).map_err(|error| ResizeError::Failed(error.to_string()))?;
    let made = Image::new(
        file,
        space.projection(),
        space.topology(),
        ImageRole::Picture,
        origin,
        limits,
    )
    .map_err(fault)?;
    if made.identity().pixels != shown.digest() {
        return Err(ResizeError::Failed(
            "the result's file does not read back as its pixels".to_owned(),
        ));
    }
    let binding = Binding::new(
        made.identity().pixels,
        made.space(),
        image.identity().pixels,
        space,
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 0.0,
            top: 0.0,
            right: f64::from(space.width()),
            bottom: f64::from(space.height()),
        },
    )
    .map_err(|error| fault(error.into()))?;
    Ok(Resized {
        image: made,
        binding,
    })
}

/// Scales an image down so its longer side is `long_edge` (a triangle filter); returns it
/// unchanged when it is already no larger. Squint's `raster::fit`.
fn fit(img: RgbImage, long_edge: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    let longer = w.max(h);
    if longer <= long_edge {
        return img;
    }
    let scale = f64::from(long_edge) / f64::from(longer);
    let nw = ((f64::from(w) * scale).round() as u32).max(1);
    let nh = ((f64::from(h) * scale).round() as u32).max(1);
    imageops::resize(&img, nw, nh, FilterType::Triangle)
}

/// `resize` as an action: the image at `image`, the long edge as `long_edge`, the result at
/// `resized`. A long edge it refuses ends the run as invalid, with its sentence.
pub struct Resize(ActionDescriptor);

impl Resize {
    pub fn new() -> Result<Resize, DescriptorError> {
        let (low, high) = LONG_EDGE;
        let draft = ActionDescriptor::draft(
            NAME,
            ACTION_VERSION,
            Kind::Primitive,
            "Scales an image down to a long edge, so the whole picture can be seen at once.",
        )
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image to scale down.",
        ))
        .argument(Argument::required(
            "long_edge",
            "The longer side of the result in pixels, under the image's own.",
            ValueType::integer(Some(i64::from(low)), Some(i64::from(high))),
        ))
        .output(OutputPort::one(
            "resized",
            ArtifactType::Image,
            "The whole image at the size asked.",
        ))
        .concurrent(true);
        Ok(Resize(draft.build()?))
    }
}

impl Action for Resize {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let limits = DecodeLimits::default();
        let failed = |why: &dyn std::fmt::Display| Failure::error(NAME, &why.to_string());
        let Some(input) = call.inputs().iter().find(|input| input.port() == "image") else {
            return Err(failed(&"the call gives no image"));
        };
        let image = match artifact::get(context.store(), ArtifactType::Image, input.id(), &limits) {
            Ok(Artifact::Image(image)) => image,
            Ok(_) => return Err(failed(&"the input is not an image")),
            Err(error) => return Err(failed(&error)),
        };
        let given = call
            .arguments()
            .iter()
            .find(|(name, _)| name == "long_edge");
        let Some((_, Value::Integer(long_edge))) = given else {
            return Err(failed(&"the call gives no long_edge"));
        };
        let long_edge = u32::try_from(*long_edge).unwrap_or(0);
        let resized = resize(&image, long_edge, &limits).map_err(|error| match error {
            ResizeError::Invalid(why) => Failure::invalid(&why),
            other => failed(&other),
        })?;
        Ok(Produced::nothing().with("resized", resized.image))
    }
}
