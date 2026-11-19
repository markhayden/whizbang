//! `profile`: the band mask of a band profile (`SPEC.md` section 7). The core is the band
//! at each row's half-width about the line, `|x + 0.5 - c| <= half[y]`; the blend is the
//! feather of `feather`'s rule with a sigma for each row, `feather_ratio` times the row's
//! half-width; the profile and that sigma, a series, are kept on the mask for `ramp`.
//!
//! **The constant set** is rascal's `profile-v1` (`constants::PROFILE_V1`), named by the
//! argument `constants` (ADR 0033); the feather reaches three sigmas, rascal's `feather-v1`
//! (`feather::SUPPORT`).
//!
//! Ported from rascal `crates/rascal-ops/src/profile.rs` and `feather_blend` in
//! `crates/rascal-ops/src/feather.rs`, at the commit `docs/sources.toml` pins, with
//! `feather`'s two differences on purpose: the distance is in whole numbers and the
//! exponential is `feather::exp_neg`, so a band mask is the same on every machine.

use std::sync::Arc;

use whizbang_core::artifact::{
    self, Artifact, ArtifactType, BandProfile, Mask, Origin, Series, Source, Step, UseCase,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};
use whizbang_core::identity::DecodeLimits;

use crate::constants::{self, PROFILE_V1};
use crate::feather::{blend_at, squared_distances};
use crate::grow::text_of;
use crate::{canvas, operation};

/// The action's name.
pub const NAME: &str = "profile";

/// The action's version: rascal's.
pub const VERSION: &str = "1.0.0";

/// What the command says the action does.
pub const ABOUT: &str = "Make the band mask of a band profile: the core at each row's half-width about the line, \
the blend a feather whose sigma follows the row. The mask, with the profile on it, is \
committed to the store, and its identity is printed.";

/// The core of a band: pixel `x` of row `y` is in it when `|x + 0.5 - c| <= half[y]`.
pub fn band_core(width: u32, column: u32, half: &[f64]) -> Vec<u8> {
    let c = f64::from(column);
    let mut core = Vec::with_capacity(width as usize * half.len());
    for row in half {
        for x in 0..width {
            let d = (f64::from(x) + 0.5 - c).abs();
            core.push(if d <= *row { 255 } else { 0 });
        }
    }
    core
}

/// The feather of `core` with the sigma of each row: rascal's `feather_blend`, by
/// `feather`'s rule. Nothing comes back once `stopped` says so.
pub fn band_blend(
    core: &[u8],
    width: usize,
    sigmas: &[f64],
    stopped: &dyn Fn() -> bool,
) -> Option<Vec<u8>> {
    let distances = squared_distances(core, width, sigmas.len(), stopped)?;
    Some(
        distances
            .iter()
            .enumerate()
            .map(|(at, k)| k.map_or(0, |k| blend_at(k, sigmas[at / width])))
            .collect(),
    )
}

/// The band mask of `band` with this ratio, whose origin is `step`.
pub fn band_mask(
    band: &BandProfile,
    ratio: f64,
    step: Step,
    stopped: &dyn Fn() -> bool,
) -> Result<Option<Mask>, artifact::ArtifactError> {
    let width = band.binding().source_space().width();
    let core = band_core(width, band.column(), band.half());
    let sigmas: Vec<f64> = band.half().iter().map(|half| ratio * half).collect();
    let Some(blend) = band_blend(&core, width as usize, &sigmas, stopped) else {
        return Ok(None);
    };
    let origin = Origin::new(step)?;
    let sigma = Series::new(
        band.binding().clone(),
        "sigma_px",
        "px",
        1,
        sigmas,
        origin.clone(),
    )?;
    let source = Source {
        locations: vec![band.line()],
        instances: Vec::new(),
    };
    let mut mask = Mask::from_core(band.binding().clone(), UseCase::Seam, core, source, origin)?;
    mask.set_profile(band.clone(), sigma)?;
    mask.set_blend(blend)?;
    Ok(Some(mask))
}

/// The action `profile`.
pub struct Profile {
    descriptor: ActionDescriptor,
}

impl Profile {
    pub fn new() -> Profile {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Makes the band mask of a band profile: the core at each row's half-width, the blend a feather whose sigma follows the row.",
        )
        .argument(Argument::optional(
            "feather_ratio",
            "The feather's sigma of a row as a share of its half-width, from 0.05 to 2; unset, the constant set's.",
            ValueType::number(Some(0.05), Some(2.0)),
        ))
        .argument(Argument::with_default(
            constants::ARGUMENT,
            "The constant set the ratio is taken from when the call does not give it.",
            ValueType::choice(&[PROFILE_V1.0]),
            Value::text(PROFILE_V1.0),
        ))
        .input(InputPort::required(
            "band",
            ArtifactType::BandProfile,
            "The band profile fan made.",
        ))
        .output(OutputPort::one(
            "mask",
            ArtifactType::Mask,
            "The band mask, on the band's image, with the profile and its sigma kept.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Profile { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Profile {
    fn default() -> Profile {
        Profile::new()
    }
}

/// Registers `profile`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Profile::new()))
}

/// The band profile of this identity, from the run's store.
fn band(context: &Context, id: artifact::ArtifactId) -> Result<BandProfile, Failure> {
    let read = artifact::get(
        context.store(),
        ArtifactType::BandProfile,
        id,
        &DecodeLimits::default(),
    );
    match read.map_err(|error| Failure::error("input", &error.without_paths()))? {
        Artifact::BandProfile(band) => Ok(band),
        _ => Err(Failure::error(
            "input",
            "what was read is not a band profile",
        )),
    }
}

impl Action for Profile {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let set = operation::text(call, constants::ARGUMENT).unwrap_or(PROFILE_V1.0);
        if set != PROFILE_V1.0 {
            return Err(Failure::invalid(&format!(
                "{set} is not a constant set of profile"
            )));
        }
        let ratio = match operation::number(call, "feather_ratio") {
            Some(ratio) => ratio,
            None => constants::number(PROFILE_V1.1, "feather_ratio")
                .ok_or_else(|| Failure::error("constants", "profile-v1 has no feather_ratio"))?,
        };
        let id = canvas::one(call, "band")?;
        let band = band(context, id)?;
        let step = Step::new(NAME, VERSION)
            .argument("feather_ratio", &text_of(ratio))
            .argument(constants::ARGUMENT, set)
            .input("band", id.0);
        let stopped = || operation::stopped(context);
        let made = band_mask(&band, ratio, step, &stopped)
            .map_err(|error| Failure::error("mask", &error.without_paths()))?;
        let Some(mask) = made else {
            return Err(operation::stopped_failure(context, NAME));
        };
        context.events().progress(&format!(
            "profile: {} pixels of core and {} of blend about column {}",
            operation::set(mask.core()),
            operation::set(mask.blend()),
            band.column()
        ));
        Ok(Produced::nothing().with("mask", mask))
    }
}

/// What `whizbang rascal profile` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The band profile, by its identity in the store.
    #[arg(long)]
    pub band: String,
    /// The feather's sigma of a row as a share of its half-width, from 0.05 to 2.
    /// Unset: profile-v1's, 0.55.
    #[arg(long)]
    pub feather_ratio: Option<f64>,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        self.feather_ratio
            .map(|ratio| vec![("feather_ratio", Value::Number(ratio))])
            .unwrap_or_default()
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![("band", "--band", self.band.as_str())]
    }
}
