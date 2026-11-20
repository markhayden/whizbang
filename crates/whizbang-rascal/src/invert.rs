//! `invert`: the mask for a second pass (`SPEC.md` section 7; the task `protect`):
//! everything the first mask reached, its core, its blend and its protect, becomes the
//! new protect, and every other pixel of the image the new core. The blend is empty, and
//! the mask is recorded as made to protect.
//!
//! The image is given beside the mask, and the mask must be on it: after a fill, the
//! image is the composite and the mask the one `fill` rebound onto it, so the second pass
//! works on the composite and keeps the first edit.
//!
//! **A mask on a crop** (plan m4, task L2, item 1). When the first mask is on a crop of
//! the image, the call gives the crop beside it, with the crop's corner on the image as
//! `left` and `top`: the mask is carried onto the image first ([`crate::carry`], which
//! compares the crop with the image there), and inverted on the image. So a mask that
//! `segment` made inside a crop that `view` cut protects its place on the whole picture.
//!
//! Ported from rascal `crates/rascal-ops/src/invert.rs` at the commit
//! `docs/sources.toml` pins, with these differences, each on purpose. Rascal's `to` named
//! a frame on the mask's chain, a cut canvas's panorama or a crop's image, and the mask
//! was carried up to it before it was inverted. Here an image names no parent: the
//! image to invert on is the one given, and a mask on a crop of it comes with the crop
//! and its corner. A mask on a cut canvas is not carried (`docs/carry-over.md`). A mask
//! that reached every pixel, which rascal said did not apply, is refused.

use whizbang_core::artifact::{ArtifactError, ArtifactType, Mask, Origin, Step, UseCase};
use whizbang_core::contract::{
    Action, ActionDescriptor, Context, Failure, InputPort, Kind, OutputPort, PortRef, PreparedCall,
    Produced, RegistrationError, Registry, Value,
};

use crate::operation;

/// The action's name.
pub const NAME: &str = "invert";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// What the command says the action does.
pub const ABOUT: &str = "Make the mask for a second pass on an image: everything the first mask reached, its core, \
its blend and its protect, is protected, and every other pixel is the core. The mask must \
be on the image: after a fill, the composite and the mask fill rebound onto it. A mask on a \
crop of the image is given with the crop and the crop's corner on the image, left and top, \
and is carried onto the image first. The new mask is committed to the store, and its \
identity is printed.";

/// Why a mask could not be inverted.
#[derive(Debug)]
pub enum InvertError {
    /// The first mask reached every pixel: nothing is left to edit.
    Everything,
    /// The mask could not be made.
    Mask(ArtifactError),
}

/// The mask for a second pass after `mask`, on its binding: protect where `mask` has a
/// core, a blend or protect; the core everywhere else; no blend; made to protect, from
/// the same locations and instances. Its origin is one step, this action, with the mask
/// and the image `image` as its inputs.
pub fn invert(mask: &Mask, image: whizbang_core::identity::Digest) -> Result<Mask, InvertError> {
    let reached: Vec<u8> = (0..mask.core().len())
        .map(|at| {
            let on = mask.core()[at] != 0 || mask.blend()[at] != 0 || mask.protect()[at] != 0;
            if on { 255 } else { 0 }
        })
        .collect();
    if reached.iter().all(|sample| *sample != 0) {
        return Err(InvertError::Everything);
    }
    let core = reached.iter().map(|sample| 255 - *sample).collect();
    let step = Step::new(NAME, VERSION)
        .input("mask", mask.id().0)
        .input("image", image);
    let origin = Origin::new(step).map_err(InvertError::Mask)?;
    let mut out = Mask::from_core(
        mask.binding().clone(),
        UseCase::Protect,
        core,
        mask.source().clone(),
        origin,
    )
    .map_err(InvertError::Mask)?;
    out.protect_union(&reached).map_err(InvertError::Mask)?;
    Ok(out)
}

/// The action `invert`.
pub struct Invert {
    descriptor: ActionDescriptor,
}

impl Invert {
    pub fn new() -> Invert {
        let [left, top] = crate::carry::corner_arguments(false);
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "The mask for a second pass: what the first mask reached is protected, and every other pixel is the core.",
        )
        .argument(left)
        .argument(top)
        // On the image, or on the crop when one is given: held when the call runs,
        // since a port belongs to one image.
        .input(InputPort::required(
            "mask",
            ArtifactType::Mask,
            "The first pass's mask, on the image, or on the crop when one is given.",
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image of the second pass: after a fill, its composite.",
        ))
        .input(InputPort::optional(
            "crop",
            ArtifactType::Image,
            "The crop of the image the mask is on, when it is on one; with left and top.",
        ))
        .output(
            OutputPort::one(
                "mask",
                ArtifactType::Mask,
                "The second pass's mask: the first's reach protected, the rest the core.",
            )
            .on(PortRef::input("image")),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Invert { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Invert {
    fn default() -> Invert {
        Invert::new()
    }
}

/// Registers the action.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(std::sync::Arc::new(Invert::new()))
}

/// What `whizbang rascal invert` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The first pass's mask, by its identity in the store: on the image.
    #[arg(long)]
    pub mask: String,
    /// The image of the second pass, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The crop of the image the mask is on, when it is on one, by its identity.
    #[arg(long, requires_all = ["left", "top"])]
    pub crop: Option<String>,
    /// The crop's left edge on the image, in whole pixels.
    #[arg(long, requires = "crop")]
    pub left: Option<u32>,
    /// The crop's top edge on the image, in whole pixels.
    #[arg(long, requires = "crop")]
    pub top: Option<u32>,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        let side =
            |name, side: Option<u32>| side.map(|side| (name, Value::Integer(i64::from(side))));
        [side("left", self.left), side("top", self.top)]
            .into_iter()
            .flatten()
            .collect()
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        let mut inputs = vec![
            ("mask", "--mask", self.mask.as_str()),
            ("image", "--image", self.image.as_str()),
        ];
        if let Some(crop) = &self.crop {
            inputs.push(("crop", "--crop", crop.as_str()));
        }
        inputs
    }
}

impl Action for Invert {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let image = operation::one(call, "image")?;
        let read = operation::mask(context, operation::one(call, "mask")?)?;
        let corner = crate::carry::corner(call)?;
        let read = match (operation::given(call, "crop").first(), corner) {
            (Some(_), Some(at)) => (crate::carry::placed(call, context, at)?.carry(&read))
                .map_err(crate::carry::failed)?,
            (None, None) => {
                // The image's digest and space, without its pixels decoded.
                let on = operation::image(context, image)?;
                (read.require_image(on.identity().pixels, on.space()))
                    .map_err(|error| Failure::invalid(&error.without_paths()))?;
                read
            }
            _ => {
                return Err(Failure::invalid(
                    "left and top are the corner of a crop, and are given with the crop",
                ));
            }
        };
        let made = invert(&read, image.0).map_err(|error| match error {
            InvertError::Everything => Failure::invalid(
                "the first mask reached every pixel of the image; nothing is left to edit",
            ),
            InvertError::Mask(error) => Failure::error("mask", &error.without_paths()),
        })?;
        context.events().progress(&format!(
            "invert: {} pixels to edit, {} protected",
            operation::set(made.core()),
            operation::set(made.protect())
        ));
        Ok(Produced::nothing().with("mask", made))
    }
}
