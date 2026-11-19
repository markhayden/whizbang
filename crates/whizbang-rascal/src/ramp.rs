//! `ramp`: the colour step across a band's line, row by row, measured on the step windows
//! the band profile kept, and added to the mask as a tone ramp (`SPEC.md` section 7).
//! Nothing is painted here: the prescription says what `apply` will do, so the mask
//! stays a description.
//!
//! **The step** of row `y` and channel `ch` is `mean(right) - mean(left)` over the
//! profile's two step windows (`fan::windows`), smoothed along the rows with a sigma of
//! `ramp_sigma` times the height. **The constant set** is rascal's `ramp-v1`
//! (`constants::RAMP_V1`), named by the argument `constants` (ADR 0033).
//!
//! Ported from rascal `crates/rascal-ops/src/ramp.rs` at the commit `docs/sources.toml`
//! pins, with `fan::smooth_rows`, whose kernel is the same on every machine. A mask
//! without a band profile is refused as invalid, as rascal refused it.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, BandProfile, Origin, Series, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort, PortRef,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};
use whizbang_core::identity::CanonicalPixels;

use crate::constants::{self, RAMP_V1};
use crate::fan::{smooth_rows, windows};
use crate::grow::text_of;
use crate::{canvas, operation, paste};

/// The action's name.
pub const NAME: &str = "ramp";

/// The action's version: rascal's.
pub const VERSION: &str = "1.0.0";

/// What the command says the action does.
pub const ABOUT: &str = "Measure the colour step across a band mask's line, row by row, and add it to the mask as \
a tone ramp for apply; nothing is painted. The mask is committed to the store, and its \
identity is printed.";

/// The step of every row and channel, row by row: `mean(right) - mean(left)` on the
/// band's step windows, smoothed along the rows with `sigma_rows`.
pub fn tone_step(pixels: &CanonicalPixels, band: &BandProfile, sigma_rows: f64) -> Vec<f64> {
    let (w, h) = (pixels.width(), pixels.height());
    let rgba = pixels.rgba();
    let [(_, l0, l1), (_, r0, r1), _, _] = windows(band.column(), band.thin(), band.windows(), w);
    let mut channels: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for y in 0..h {
        for (ch, out) in channels.iter_mut().enumerate() {
            let mean = |lo: i64, hi: i64| {
                let n = (hi - lo).max(1) as f64;
                (lo..hi)
                    .map(|x| f64::from(rgba[((y * w + x as u32) * 4) as usize + ch]))
                    .sum::<f64>()
                    / n
            };
            out.push(mean(r0, r1) - mean(l0, l1));
        }
    }
    let smoothed: Vec<Vec<f64>> = channels
        .iter()
        .map(|values| smooth_rows(values, sigma_rows))
        .collect();
    (0..h as usize)
        .flat_map(|y| (0..3).map(move |ch| (y, ch)))
        .map(|(y, ch)| smoothed[ch][y])
        .collect()
}

/// The number `key` of the call, or else `ramp-v1`'s.
fn number(call: &PreparedCall, key: &str) -> Result<f64, Failure> {
    match operation::number(call, key) {
        Some(value) => Ok(value),
        None => constants::number(RAMP_V1.1, key)
            .ok_or_else(|| Failure::error("constants", &format!("ramp-v1 has no {key}"))),
    }
}

/// The action `ramp`.
pub struct Ramp {
    descriptor: ActionDescriptor,
}

impl Ramp {
    pub fn new() -> Ramp {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Measures the colour step across a band mask's line, row by row, and adds it to the mask as a tone ramp for apply.",
        )
        .argument(Argument::optional(
            "share",
            "The share of the step each side moves toward the other at the line, from 0 to 1; unset, the constant set's.",
            ValueType::number(Some(0.0), Some(1.0)),
        ))
        .argument(Argument::optional(
            "ramp_sigma",
            "The smoothing of the step along the rows, as a share of the height, from 0 to 1; unset, the constant set's.",
            ValueType::number(Some(0.0), Some(1.0)),
        ))
        .argument(Argument::with_default(
            constants::ARGUMENT,
            "The constant set the values a call does not give are taken from.",
            ValueType::choice(&[RAMP_V1.0]),
            Value::text(RAMP_V1.0),
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image the band was measured on.",
        ))
        .input(
            InputPort::required(
                "mask",
                ArtifactType::Mask,
                "The band mask on the image, with its band profile.",
            )
            .on("image"),
        )
        .output(
            OutputPort::one(
                "mask",
                ArtifactType::Mask,
                "The mask with the tone ramp added.",
            )
            .on(PortRef::input("image")),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Ramp { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Ramp {
    fn default() -> Ramp {
        Ramp::new()
    }
}

/// Registers `ramp`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Ramp::new()))
}

impl Action for Ramp {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let set = operation::text(call, constants::ARGUMENT).unwrap_or(RAMP_V1.0);
        if set != RAMP_V1.0 {
            return Err(Failure::invalid(&format!(
                "{set} is not a constant set of ramp"
            )));
        }
        let (share, sigma) = (number(call, "share")?, number(call, "ramp_sigma")?);
        let (image_id, mask_id) = (canvas::one(call, "image")?, canvas::one(call, "mask")?);
        let (image, picture) = paste::picture(context, image_id)?;
        let mut mask = operation::mask(context, mask_id)?;
        mask.require_image(picture.digest(), image.space())
            .map_err(|error| Failure::invalid(&error.without_paths()))?;
        let Some(profile) = mask.profile() else {
            return Err(Failure::invalid(
                "the mask has no band profile; ramp measures on its step windows (run profile first)",
            ));
        };
        let height = f64::from(image.space().height());
        let values = tone_step(picture.pixels(), &profile.band, sigma * height);
        let largest = values.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        let step = Step::new(NAME, VERSION)
            .argument("share", &text_of(share))
            .argument("ramp_sigma", &text_of(sigma))
            .argument(constants::ARGUMENT, set)
            .input("image", image_id.0)
            .input("mask", mask_id.0);
        let fail = |error: whizbang_core::artifact::ArtifactError| {
            Failure::error("mask", &error.without_paths())
        };
        let series = Origin::new(step.clone())
            .and_then(|origin| {
                Series::new(
                    mask.binding().clone(),
                    "tone_ramp.step",
                    "levels",
                    3,
                    values,
                    origin,
                )
            })
            .map_err(fail)?;
        mask.push_tone_ramp(series, share).map_err(fail)?;
        mask.record(step).map_err(fail)?;
        context.events().progress(&format!(
            "ramp: a step of up to {largest:.1} levels across column {}, share {share}",
            mask.profile().map_or(0, |p| p.band.column())
        ));
        Ok(Produced::nothing().with("mask", mask))
    }
}

/// What `whizbang rascal ramp` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The image the band was measured on, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The band mask on it, by its identity in the store.
    #[arg(long)]
    pub mask: String,
    /// The share of the step each side moves, from 0 to 1. Unset: ramp-v1's, 0.5.
    #[arg(long)]
    pub share: Option<f64>,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        self.share
            .map(|share| vec![("share", Value::Number(share))])
            .unwrap_or_default()
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![
            ("image", "--image", self.image.as_str()),
            ("mask", "--mask", self.mask.as_str()),
        ]
    }
}
