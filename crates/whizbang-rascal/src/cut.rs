//! `cut`: the two edge strips of a complete panorama side by side, so its seam is the
//! middle column of a flat canvas (`SPEC.md` section 7). Nothing is resampled: the
//! canvas's left half is the panorama's last `s` columns and its right half the first `s`,
//! and the plan it hands back (ADR 0002, `plan/1`) is how `put` writes the canvas back.
//! The canvas and the panorama are ADR 0001's wrap: a binding of the canvas onto the
//! panorama is `Mapping::Wrap` with the plan's strip.
//!
//! Ported from rascal `crates/rascal-ops/src/cut.rs` and `Plan::strip_width` in
//! `crates/rascal-core/src/frame.rs`, at the commit `docs/sources.toml` pins. Rascal
//! named the canvas's frame after the image and the plan; here the plan names the
//! panorama by its pixel digest, and the canvas is an image of its own. A picture that is
//! not a complete panorama, or narrower than 4 columns, is refused as invalid where
//! rascal answered that `cut` did not apply.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Origin, Plan, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};
use whizbang_core::identity::{CanonicalPixels, Projection, Space, Topology};

use crate::{canvas, operation, paste};

/// The action's name.
pub const NAME: &str = "cut";

/// The action's version, which a canvas and a plan record in their origins.
pub const VERSION: &str = "1.0.0";

/// The strip fraction when a call does not give it: rascal's.
pub const STRIP_DEFAULT: f64 = 0.07;

/// What the command says the action does.
pub const ABOUT: &str = "Cut a complete panorama at its seam: its last and first strips side by side on a flat \
canvas, so the seam is the canvas's middle column. Nothing is resampled. The canvas and the \
plan that `put` writes it back by are committed to the store, and their identities printed.";

/// The strip width for a panorama `width` wide and a strip fraction `fraction` in
/// `(0, 0.5]`: `min(max(2, 2 round_half_up(fraction width / 2)), 2 floor(width / 4))`,
/// rascal's rule. It is even, at least 2 and at most half the width for a width of at
/// least 4.
pub fn strip_width(width: u32, fraction: f64) -> u32 {
    let half = f64::from(width) * fraction / 2.0;
    let rounded = (half + 0.5).floor() as u32;
    (2 * rounded).max(2).min(2 * (width / 4))
}

/// The canvas of a cut with strip `s`: the last `s` columns of `pixels`, then the first
/// `s`. The picture is at least `2 s` wide.
pub fn canvas_of(pixels: &CanonicalPixels, s: u32) -> Result<CanonicalPixels, Failure> {
    let (w, h) = (pixels.width(), pixels.height());
    let left = canvas::crop(pixels, [w - s, 0, s, h])?;
    let right = canvas::crop(pixels, [0, 0, s, h])?;
    let rows = left
        .rgba()
        .chunks(s as usize * 4)
        .zip(right.rgba().chunks(s as usize * 4));
    let rgba = rows.flat_map(|(l, r)| l.iter().chain(r).copied()).collect();
    CanonicalPixels::from_rgba(2 * s, h, rgba)
        .map_err(|error| Failure::error("pixels", &error.to_string()))
}

/// The action `cut`.
pub struct Cut {
    descriptor: ActionDescriptor,
}

impl Cut {
    pub fn new() -> Cut {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Cuts a panorama at its seam: its last and first strips side by side on a flat canvas whose middle column is the seam.",
        )
        .argument(Argument::with_default(
            "strip",
            "The strip as a share of the panorama's width, over 0 and at most 0.5; the strip is made even, at least 2 and at most a quarter of the width, twice.",
            ValueType::number(Some(1e-9), Some(0.5)),
            Value::Number(STRIP_DEFAULT),
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "A complete panorama, at least 4 columns wide.",
        ))
        .output(OutputPort::one(
            "image",
            ArtifactType::Image,
            "The canvas: the panorama's last strip, then its first.",
        ))
        .output(OutputPort::one(
            "plan",
            ArtifactType::Plan,
            "The panorama and the strip, for put.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Cut { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Cut {
    fn default() -> Cut {
        Cut::new()
    }
}

/// Registers `cut`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Cut::new()))
}

/// What `whizbang rascal cut` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The panorama, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The strip as a share of the panorama's width, over 0 and at most 0.5.
    #[arg(long, default_value_t = STRIP_DEFAULT)]
    pub strip: f64,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        vec![("strip", Value::Number(self.strip))]
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![("image", "--image", self.image.as_str())]
    }
}

impl Action for Cut {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let fraction = operation::number(call, "strip").unwrap_or(STRIP_DEFAULT);
        let image_id = canvas::one(call, "image")?;
        let (image, picture) = paste::picture(context, image_id)?;
        let space = image.space();
        if (space.projection(), space.topology()) != (Projection::Equirectangular, Topology::Wrap) {
            return Err(Failure::invalid(
                "the image is not a complete panorama; cut needs one, as rascal's needed an equirectangular frame that wraps",
            ));
        }
        if space.width() < 4 {
            return Err(Failure::invalid(&format!(
                "the image is {} wide; cut needs at least 4 columns",
                space.width()
            )));
        }
        let s = strip_width(space.width(), fraction);
        let step = Step::new(NAME, VERSION)
            .argument("strip", &fraction.to_string())
            .input("image", image_id.0);
        let pixels = canvas_of(picture.pixels(), s)?;
        let flat =
            Space::flat(2 * s, space.height()).map_err(|e| Failure::invalid(&e.to_string()))?;
        let made = canvas::image(&pixels, flat, step.clone())?;
        let plan = Origin::new(step)
            .and_then(|origin| Plan::new(image.binding(), s, origin))
            .map_err(|error| Failure::error("plan", &error.without_paths()))?;
        context.events().progress(&format!(
            "cut: {}x{} with a strip of {s} into a {}x{} canvas",
            space.width(),
            space.height(),
            2 * s,
            space.height()
        ));
        Ok(Produced::nothing().with("image", made).with("plan", plan))
    }
}
