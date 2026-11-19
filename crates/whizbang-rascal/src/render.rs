//! `render`: a mask made into a picture to look at, an image whose role is a rendering
//! (plan 2A, task R2). A rendering is never accepted where the mask is required.
//!
//! Every profile reads the mask by one number for each pixel, its *weight* as a byte:
//! 0 where protected, else 255 in the core, else the blend. That is the core's
//! [`effective_weight`](whizbang_core::artifact::effective_weight) times 255, in whole
//! numbers. Two profiles:
//!
//! - **`plain`:** the weight in grey, 255 white where the mask paints: a PNG of one
//!   grey channel.
//! - **`alpha`:** the cut-out. The image's red, green and blue, and as alpha the
//!   image's alpha times the weight over 255, the nearest whole number: an opaque image
//!   gets the weight itself. It takes the image the mask is on, and a PNG of four
//!   channels.
//!
//! Ported from rascal `crates/rascal-ops/src/render.rs` and `crates/rascal-core/src/
//! rendering.rs` at the commit `docs/sources.toml` pins: `plain` is rascal's `grey`, in
//! whole numbers where rascal rounded a float. `alpha` is the other way round from
//! rascal's, on purpose: rascal's made the core transparent, for a filler to paint into;
//! this one keeps what the mask covers, as the plan's cut-out asks, and keeps the blend
//! where rascal kept only the core. `binary`, `tinted` and `overlay` wait for milestone 3.
//!
//! The PNG is written by the core's one encoder of an image artifact's file
//! (`whizbang_core::identity::encode`), as `view` writes a crop. Its file is part of the rendering's identity, so a change of that encoder
//! changes the identity; its pixel digest stays.

use whizbang_core::artifact::{self, Artifact, ArtifactType, Image, ImageRole, Mask, Origin, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort,
    PreparedCall, Produced, ValueType,
};
use whizbang_core::identity::encode::{self, Channels};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits};

use crate::operation;

/// The action's name.
pub const NAME: &str = "render";

/// The action's version, which a rendering records in its origin.
pub const VERSION: &str = "1.0.0";

/// A mask as a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// The image with the mask's weight as its alpha: the cut-out.
    Alpha,
    /// The mask's weight in grey.
    Plain,
}

impl Profile {
    /// The profile of this name.
    pub fn named(name: &str) -> Option<Profile> {
        match name {
            "alpha" => Some(Profile::Alpha),
            "plain" => Some(Profile::Plain),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Profile::Alpha => "alpha",
            Profile::Plain => "plain",
        }
    }
}

/// The weight of one pixel as a byte: 0 under protect, else 255 in the core, else the
/// blend.
pub fn weight(core: u8, blend: u8, protect: u8) -> u8 {
    match (protect, core) {
        (0, 0) => blend,
        (0, _) => 255,
        _ => 0,
    }
}

/// The mask's weights, one byte for each pixel, row by row: the `plain` rendering.
pub fn plain(mask: &Mask) -> Vec<u8> {
    let planes = mask.core().iter().zip(mask.blend()).zip(mask.protect());
    planes.map(|((c, b), p)| weight(*c, *b, *p)).collect()
}

/// The image with the mask's weight as its alpha: the `alpha` rendering, four bytes for
/// each pixel. The mask must be on the image, in its space.
pub fn alpha(mask: &Mask, pixels: &CanonicalPixels) -> Result<Vec<u8>, String> {
    if pixels.digest() != mask.image() {
        return Err(format!(
            "the mask belongs to the image {}, not to {}",
            mask.image(),
            pixels.digest()
        ));
    }
    let mut rgba = pixels.rgba().to_vec();
    for (pixel, weight) in rgba.chunks_mut(4).zip(plain(mask)) {
        let (a, w) = (u32::from(pixel[3]), u32::from(weight));
        // At most 255 * 255 + 127, and the quotient at most 255.
        pixel[3] = ((a * w + 127) / 255) as u8;
    }
    Ok(rgba)
}

/// The samples as a PNG of grey or red, green, blue and alpha, by the core's one
/// encoder (`whizbang_core::identity::encode`).
fn png(samples: &[u8], width: u32, height: u32, channels: Channels) -> Result<Vec<u8>, String> {
    encode::png_of(samples, width, height, channels)
        .map_err(|why| format!("cannot encode the rendering: {why}"))
}

/// The rendering of `mask` in `profile`: an image whose role is a rendering, in the
/// mask's space, whose origin is one step that names this action, its version, the
/// profile, the mask and, for `alpha`, the image. `alpha` takes the image's pixels and
/// its identity; `plain` takes neither.
pub fn render(
    mask: &Mask,
    profile: Profile,
    image: Option<(&CanonicalPixels, artifact::ArtifactId)>,
) -> Result<Image, String> {
    let space = mask.space();
    let mut step = Step::new(NAME, VERSION)
        .argument("profile", profile.name())
        .input("mask", mask.id().0);
    let file = match (profile, image) {
        (Profile::Plain, _) => png(&plain(mask), space.width(), space.height(), Channels::Grey)?,
        (Profile::Alpha, Some((pixels, id))) => {
            step = step.input("image", id.0);
            let rgba = alpha(mask, pixels)?;
            png(&rgba, space.width(), space.height(), Channels::Rgba)?
        }
        (Profile::Alpha, None) => {
            return Err("alpha draws on the image the mask is on: give it".to_owned());
        }
    };
    let origin = Origin::new(step).map_err(|error| error.to_string())?;
    Image::new(
        file,
        space.projection(),
        space.topology(),
        ImageRole::Rendering,
        origin,
        &DecodeLimits::default(),
    )
    .map_err(|error| error.without_paths())
}

/// The action `render`.
pub struct Render {
    descriptor: ActionDescriptor,
}

impl Render {
    pub fn new() -> Render {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Makes a mask into a picture: the image cut out by it (alpha), or its weight in grey (plain).",
        )
        .argument(Argument::required(
            "profile",
            "alpha: the image with the mask's weight as alpha; plain: the weight in grey.",
            ValueType::choice(&["alpha", "plain"]),
        ))
        .input(InputPort::required(
            "mask",
            ArtifactType::Mask,
            "The mask to render.",
        ))
        .input(InputPort::optional(
            "image",
            ArtifactType::Image,
            "The image the mask is on, which alpha requires.",
        ))
        .output(OutputPort::one(
            "image",
            ArtifactType::Image,
            "The rendering, an image whose role is a rendering.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Render { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Render {
    fn default() -> Render {
        Render::new()
    }
}

impl Action for Render {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let named = operation::text(call, "profile").unwrap_or("");
        let Some(profile) = Profile::named(named) else {
            return Err(Failure::invalid("the profile is alpha or plain"));
        };
        let Some(id) = operation::given(call, "mask").first().copied() else {
            return Err(Failure::error("input", "the call gives no mask"));
        };
        let mask = operation::mask(context, id)?;
        let limits = DecodeLimits::default();
        let image = match operation::given(call, "image").first().copied() {
            Some(id) => {
                let read = artifact::get(context.store(), ArtifactType::Image, id, &limits);
                let pixels = match read {
                    Ok(Artifact::Image(image)) => image.pixels(&limits),
                    Ok(_) => return Err(Failure::error("input", "the input is not an image")),
                    Err(error) => Err(error),
                };
                let pixels =
                    pixels.map_err(|error| Failure::error("input", &error.without_paths()))?;
                Some((pixels, id))
            }
            None => None,
        };
        if operation::stopped(context) {
            return Err(operation::stopped_failure(context, NAME));
        }
        let given = image.as_ref().map(|(pixels, id)| (pixels, *id));
        let rendering = render(&mask, profile, given).map_err(|why| Failure::invalid(&why))?;
        context
            .events()
            .progress(&format!("render: the {} rendering", profile.name()));
        Ok(Produced::nothing().with("image", rendering))
    }
}
