//! `unwindow`: a crop written back into exactly its box of the image it was cut from
//! (`SPEC.md` section 7), the inverse of `window` for the image: a crop written back
//! unchanged gives the image itself.
//!
//! Ported from rascal `crates/rascal-ops/src/unwindow.rs` at the commit `docs/sources.toml`
//! pins. Rascal read the box from the crop's frame. An image here has no frame, so
//! `unwindow` takes the mask `window` handed back, whose binding places the window's crop
//! onto the image by ADR 0001's crop: its box, at the image's own scale. The window's crop
//! is cut again from the image by that box, and must be the image the mask is on; the
//! crop given (a fill's composite of it, say) must be that crop wherever the mask weighs
//! nothing, outside its core and blend or under protect. So every pixel of the image the
//! mask never reaches, and every protected one, is the image's (plan m3, task O2, item
//! 3). A mask that does not place a crop onto this image, a scaled crop and a crop of
//! another size are refused, as rascal refused a crop of another frame.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Mask, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Context, Failure, InputPort, Kind, OutputPort, PreparedCall,
    Produced, RegistrationError, Registry, Value,
};
use whizbang_core::identity::{Destination, Direction, Mapping, PixelDigest, Space};

use crate::{canvas, operation, paste};

/// The action's name.
pub const NAME: &str = "unwindow";

/// The action's version, which an image it writes records in its origin.
pub const VERSION: &str = "1.0.0";

/// What the command says the action does.
pub const ABOUT: &str = "Write a crop back into exactly the box of the image that window cut it from, read from \
the mask window handed back. The crop must be the window's wherever that mask weighs nothing; \
nothing outside the box changes. The image is committed to the store, and its identity \
printed.";

/// The box, left, top, right and bottom, that the mask's binding places its image at on
/// the image `onto` in `space`: a crop in whole pixels, at that image's scale.
pub fn placed_box(mask: &Mask, onto: PixelDigest, space: Space) -> Result<[u32; 4], String> {
    let binding = mask.binding();
    let Mapping::Crop {
        left,
        top,
        right,
        bottom,
    } = *binding.mapping()
    else {
        return Err("the mask does not place its image onto another by a crop".to_owned());
    };
    if binding.destination() != Destination::Image(onto)
        || binding.destination_space() != space
        || binding.direction() != Direction::SourceToDestination
    {
        return Err("the mask's crop was not cut from this image".to_owned());
    }
    let edges = [left, top, right, bottom];
    if edges.iter().any(|edge| edge.fract() != 0.0) {
        return Err(format!(
            "the crop ({left}, {top}) to ({right}, {bottom}) is not in whole pixels"
        ));
    }
    // Inside the image, by the binding's own check: each edge a whole number of pixels.
    let [l, t, r, b] = edges.map(|edge| edge as u32);
    let size = (mask.space().width(), mask.space().height());
    if (r - l, b - t) != size {
        return Err(format!(
            "the crop is {}x{} and its box {}x{}: a scaled crop is not written back pixel for pixel",
            size.0,
            size.1,
            r - l,
            b - t
        ));
    }
    Ok([l, t, r, b])
}

/// The action `unwindow`.
pub struct Unwindow {
    descriptor: ActionDescriptor,
}

impl Unwindow {
    pub fn new() -> Unwindow {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Writes a crop back into exactly the box of the image window cut it from; nothing outside the box changes.",
        )
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image the window was cut from.",
        ))
        .input(InputPort::required(
            "crop",
            ArtifactType::Image,
            "The crop to write back: the window's, wherever its mask weighs nothing.",
        ))
        .input(InputPort::required(
            "mask",
            ArtifactType::Mask,
            "The mask window handed back, which places its crop onto the image.",
        ))
        .output(OutputPort::one(
            "image",
            ArtifactType::Image,
            "The image with the crop written back, or the image when nothing changed.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Unwindow { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Unwindow {
    fn default() -> Unwindow {
        Unwindow::new()
    }
}

/// Registers `unwindow`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Unwindow::new()))
}

/// What `whizbang rascal unwindow` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The image the window was cut from, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The crop to write back, by its identity in the store.
    #[arg(long)]
    pub crop: String,
    /// The mask window handed back, by its identity in the store.
    #[arg(long)]
    pub mask: String,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![
            ("image", "--image", self.image.as_str()),
            ("crop", "--crop", self.crop.as_str()),
            ("mask", "--mask", self.mask.as_str()),
        ]
    }
}

impl Action for Unwindow {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let ids = (
            canvas::one(call, "image")?,
            canvas::one(call, "crop")?,
            canvas::one(call, "mask")?,
        );
        let (image, picture) = paste::picture(context, ids.0)?;
        let (_, given) = paste::picture(context, ids.1)?;
        let mask = operation::mask(context, ids.2)?;
        let space = image.space();
        let [l, t, r, b] =
            placed_box(&mask, picture.digest(), space).map_err(|why| Failure::invalid(&why))?;
        if (given.width(), given.height()) != (r - l, b - t) {
            return Err(Failure::invalid(&format!(
                "the crop is {}x{} but the window's box is {}x{}",
                given.width(),
                given.height(),
                r - l,
                b - t
            )));
        }
        let window = canvas::crop(picture.pixels(), [l, t, r - l, b - t])?;
        if window.digest() != mask.image() {
            return Err(Failure::invalid(
                "the mask is not on the crop of this image that its box cuts",
            ));
        }
        canvas::kept(&window, given.pixels(), &mask, "the crop")?;
        if operation::stopped(context) {
            return Err(operation::stopped_failure(context, NAME));
        }
        let pixels = canvas::written(picture.pixels(), given.pixels(), l, t)?;
        let out = if pixels == *picture.pixels() {
            image
        } else {
            let step = Step::new(NAME, VERSION)
                .input("image", ids.0.0)
                .input("crop", ids.1.0)
                .input("mask", ids.2.0);
            canvas::image(&pixels, space, step)?
        };
        context.events().progress(&format!(
            "unwindow: the crop written back into [{l}, {t}, {r}, {b})"
        ));
        Ok(Produced::nothing().with("image", out))
    }
}
