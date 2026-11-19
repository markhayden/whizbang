//! `apply`: every prescription on a mask performed once, in order, on its image
//! (`SPEC.md` section 7): the working image, as rascal's `heal_once` made it before it
//! filled. A tone ramp moves each pixel the mask weighs over 0 toward the other side of
//! the band's line by `share step[y] max(0, 1 - d / half[y])`, where `d` is the pixel
//! centre's distance from the line's column; the left side moves by `+step`, the right by
//! `-step`, each channel rounded to the nearest level and kept from 0 to 255, the alpha
//! never touched. A pixel the mask weighs nothing, outside its core and blend or under
//! protect, is the image's four bytes.
//!
//! **What is handed back** is the working image, derived from the image, and the mask
//! rebound onto it through the derivation `Derivation::checked` makes of the two pictures
//! (ADR 0006, "Who vouches for a derivation"), so a later step fills the working image
//! through a mask that is its own. Rascal shared one frame between them; whizbang's mask
//! belongs to one image (`Mask::require_image`). **An unchanged result**, a mask with no
//! prescription or one that moves no pixel, hands back the image and the mask as they
//! were, the mask keeping its binding (F1, item 5).
//!
//! Ported from rascal `crates/rascal-ops/src/apply.rs` at the commit `docs/sources.toml`
//! pins, where a mask without a prescription was answered as not applicable.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Derivation, Mask, Prescription, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Context, Failure, InputPort, Kind, OutputPort, PortRef, PreparedCall,
    Produced, RegistrationError, Registry, Value,
};
use whizbang_core::identity::CanonicalPixels;

use crate::{canvas, operation, paste};

/// The action's name.
pub const NAME: &str = "apply";

/// The action's version, which a working image and a rebound mask record.
pub const VERSION: &str = "1.0.0";

/// What the command says the action does.
pub const ABOUT: &str = "Perform a mask's prescriptions on its image, once each and in order: the working image, \
changed only where the mask weighs over 0. The working image and the mask rebound onto it \
are committed to the store, and their identities printed; a mask with nothing to do hands \
both back as they were.";

/// What performing the prescriptions made.
pub struct Applied {
    /// The working image's pixels.
    pub pixels: CanonicalPixels,
    /// How many pixels changed.
    pub changed: u64,
    /// The largest change of one channel, in levels.
    pub largest: u8,
}

/// The mask's prescriptions performed on `pixels`, which are its image's. Without a
/// profile there is no line and no half-width, and nothing to perform by; a mask with
/// prescriptions always has one.
pub fn performed(pixels: &CanonicalPixels, mask: &Mask) -> Result<Applied, String> {
    let mut rgba = pixels.rgba().to_vec();
    let (w, h) = (pixels.width(), pixels.height());
    if mask.prescriptions().is_empty() {
        return Ok(Applied {
            pixels: pixels.clone(),
            changed: 0,
            largest: 0,
        });
    }
    let Some(profile) = mask.profile() else {
        return Err(
            "the mask's prescriptions need its band profile for the line and the half-widths"
                .to_owned(),
        );
    };
    let column = f64::from(profile.band.column());
    let halves = profile.band.half();
    let mut largest = 0_u8;
    for prescription in mask.prescriptions() {
        let Prescription::ToneRamp { step, share, .. } = prescription;
        for y in 0..h {
            let (Some(half), Some(delta)) = (halves.get(y as usize), step.row(y)) else {
                return Err(format!("the prescription has no entry for row {y}"));
            };
            for x in 0..w {
                if mask.weight(x, y).unwrap_or(0.0) <= 0.0 {
                    continue;
                }
                let offset = f64::from(x) + 0.5 - column;
                let reach = (1.0 - offset.abs() / half).max(0.0);
                if reach <= 0.0 {
                    continue;
                }
                // The left side moves toward the right (+step), the right toward the left.
                let sign = if offset < 0.0 { 1.0 } else { -1.0 };
                let at = (y as usize * w as usize + x as usize) * 4;
                for (channel, delta) in rgba[at..at + 3].iter_mut().zip(delta) {
                    let moved = (f64::from(*channel) + sign * share * delta * reach)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    largest = largest.max(moved.abs_diff(*channel));
                    *channel = moved;
                }
            }
        }
    }
    let changed = rgba
        .chunks(4)
        .zip(pixels.rgba().chunks(4))
        .filter(|(a, b)| a != b)
        .count() as u64;
    let pixels = CanonicalPixels::from_rgba(w, h, rgba).map_err(|error| error.to_string())?;
    Ok(Applied {
        pixels,
        changed,
        largest,
    })
}

/// The action `apply`.
pub struct Apply {
    descriptor: ActionDescriptor,
}

impl Apply {
    pub fn new() -> Apply {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Performs a mask's prescriptions on its image, once each and in order, and rebinds the mask onto the working image.",
        )
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image to work on.",
        ))
        .input(
            InputPort::required(
                "mask",
                ArtifactType::Mask,
                "The mask on the image, carrying the prescriptions.",
            )
            .on("image"),
        )
        .output(
            OutputPort::one(
                "image",
                ArtifactType::Image,
                "The working image, or the image when nothing changed.",
            )
            .derived_from(PortRef::input("image")),
        )
        .output(
            OutputPort::one(
                "mask",
                ArtifactType::Mask,
                "The mask, rebound onto the working image.",
            )
            .on(PortRef::input("image"))
            .rebound_onto("image"),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Apply { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Apply {
    fn default() -> Apply {
        Apply::new()
    }
}

/// Registers `apply`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Apply::new()))
}

/// What `whizbang rascal apply` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The image, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The mask on the image, carrying the prescriptions, by its identity in the store.
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
            ("mask", "--mask", self.mask.as_str()),
        ]
    }
}

impl Action for Apply {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let (image_id, mask_id) = (canvas::one(call, "image")?, canvas::one(call, "mask")?);
        let (image, picture) = paste::picture(context, image_id)?;
        let mask = operation::mask(context, mask_id)?;
        mask.require_image(picture.digest(), image.space())
            .map_err(|error| Failure::invalid(&error.without_paths()))?;
        let applied = performed(picture.pixels(), &mask).map_err(|why| Failure::invalid(&why))?;
        if operation::stopped(context) {
            return Err(operation::stopped_failure(context, NAME));
        }
        if applied.pixels == *picture.pixels() {
            context
                .events()
                .progress("apply: nothing to change; the image and the mask are handed back");
            return Ok(Produced::nothing().with("image", image).with("mask", mask));
        }
        let step = Step::new(NAME, VERSION)
            .input("image", image_id.0)
            .input("mask", mask_id.0);
        let working = canvas::image(&applied.pixels, image.space(), step.clone())?;
        let derivation = Derivation::checked_of(&picture, &applied.pixels, &mask)
            .map_err(|error| Failure::error("derivation", &error.without_paths()))?;
        let mut recorded = mask;
        let rebound = recorded
            .record(step)
            .and_then(|()| recorded.rebind(&derivation))
            .map_err(|error| Failure::error("mask", &error.without_paths()))?;
        context.events().progress(&format!(
            "apply: {} prescriptions performed; {} pixels moved, by up to {} levels",
            rebound.prescriptions().len(),
            applied.changed,
            applied.largest
        ));
        Ok(Produced::nothing()
            .with("image", working)
            .with("mask", rebound))
    }
}
