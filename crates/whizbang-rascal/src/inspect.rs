//! `inspect`: a mask as a person wants to see it (`SPEC.md` section 7). The overlay, the
//! image tinted by the mask's weight with the core's edge in white, an image whose role is
//! a rendering; the core's width on each row, a series on the mask's image; and the
//! report: the counts of each zone, the box of what the mask weighs, its band profile and
//! its prescriptions.
//!
//! Ported from rascal `crates/rascal-ops/src/inspect.rs`, `crates/rascal-core/src/
//! rendering.rs` (`overlay`) and `crates/rascal-core/src/mask.rs` (`inspect_report`) at
//! the commit `docs/sources.toml` pins: the overlay's arithmetic is rascal's, in 32-bit
//! floats rounded once; the report is rascal's but for the path of the mask's folder,
//! which a mask in the store has not. The overlay is opaque, as rascal's was. The report
//! is said in the run's journal; handing it back as a result of its own waits for the
//! result kinds of task Q1 (row on what an action hands back that is not an artifact).

use std::sync::Arc;

use whizbang_core::artifact::{
    self, ArtifactId, ArtifactType, Image, ImageRole, Mask, Origin, Series, Step, UseCase,
    ZoneStatistics,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Context, Failure, InputPort, Kind, OutputPort, PortRef, PreparedCall,
    Produced, RegistrationError, Registry, Value,
};
use whizbang_core::identity::encode::{self, Channels};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits};

use crate::{operation, paste};

/// The action's name.
pub const NAME: &str = "inspect";

/// The action's version, which an overlay and a series record in their origins.
pub const VERSION: &str = "1.0.0";

/// The colour the mask's weight tints the image toward: rascal's magenta.
pub const TINT: [u8; 3] = [255, 0, 255];

/// What the command says the action does.
pub const ABOUT: &str = "Inspect a mask: the overlay, the image tinted by the mask's weight with the core's edge \
in white, and the core's width on each row as a series. Both are committed to the store, \
and their identities printed; the counts of each zone and the box of what the mask weighs \
are printed in the run's journal.";

/// The band profile of a mask that is a band along a line, in a report.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileSummary {
    /// The line location the band follows.
    pub line: ArtifactId,
    /// The band profile's identity.
    pub band: ArtifactId,
    /// The identity of the series of the feather's sigma.
    pub sigma: ArtifactId,
    pub rows: usize,
    pub column_px: u32,
    pub thin_px: f64,
}

/// Rascal's inspect report of a mask, version 1.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub identity: ArtifactId,
    pub use_case: UseCase,
    pub zones: ZoneStatistics,
    pub profile: Option<ProfileSummary>,
    pub prescriptions: usize,
}

/// The report of `mask`.
pub fn report(mask: &Mask) -> Report {
    Report {
        identity: mask.id(),
        use_case: mask.use_case(),
        zones: mask.statistics(),
        profile: mask.profile().map(|p| ProfileSummary {
            line: p.band.line(),
            band: p.band.id(),
            sigma: p.sigma.id(),
            rows: p.band.half().len(),
            column_px: p.band.column(),
            thin_px: p.band.thin(),
        }),
        prescriptions: mask.prescriptions().len(),
    }
}

impl Report {
    /// The report in one line, as the run's journal says it.
    pub fn line(&self) -> String {
        let z = &self.zones;
        let support = match z.support {
            Some([l, t, r, b]) => format!("[{l}, {t}, {r}, {b})"),
            None => "none".to_owned(),
        };
        let profile = match &self.profile {
            Some(p) => format!("a band of {} rows about column {} px", p.rows, p.column_px),
            None => "no band".to_owned(),
        };
        format!(
            "inspect: {}x{}; core {}, blend {}, protect {}, painted {} (weight {}); support {support}; {profile}; {} prescriptions",
            z.width,
            z.height,
            z.core,
            z.blend,
            z.protect,
            z.painted,
            z.weight_sum,
            self.prescriptions
        )
    }
}

/// The number of core pixels on each row.
pub fn core_widths(mask: &Mask) -> Vec<f64> {
    let width = mask.space().width() as usize;
    mask.core()
        .chunks(width.max(1))
        .map(|row| row.iter().filter(|v| **v != 0).count() as f64)
        .collect()
}

/// One channel of `base` mixed toward `colour` by `amount`, in 32-bit floats, rounded.
fn mix(base: u8, colour: u8, amount: f32) -> u8 {
    ((1.0 - amount) * f32::from(base) + amount * f32::from(colour))
        .round()
        .clamp(0.0, 255.0) as u8
}

/// The overlay, four bytes a pixel and opaque: white on the core's edge (a core pixel
/// with a four-neighbour outside the core or the image), else the image tinted toward
/// `colour` by half the mask's weight, else the image. The mask must be on the image.
pub fn overlay(mask: &Mask, pixels: &CanonicalPixels, colour: [u8; 3]) -> Result<Vec<u8>, String> {
    if pixels.digest() != mask.image() {
        return Err(format!(
            "the mask belongs to the image {}, not to {}",
            mask.image(),
            pixels.digest()
        ));
    }
    let space = mask.space();
    let (width, height) = (i64::from(space.width()), i64::from(space.height()));
    let core = |x: i64, y: i64| {
        x >= 0 && y >= 0 && x < width && y < height && mask.core()[(y * width + x) as usize] != 0
    };
    let rgba = pixels.rgba();
    let mut out = Vec::with_capacity(rgba.len());
    for y in 0..height {
        for x in 0..width {
            let at = (y * width + x) as usize;
            if core(x, y) && !(core(x - 1, y) && core(x + 1, y) && core(x, y - 1) && core(x, y + 1))
            {
                out.extend_from_slice(&[255, 255, 255, 255]);
                continue;
            }
            let p = &rgba[at * 4..at * 4 + 3];
            let w =
                artifact::effective_weight(mask.core()[at], mask.blend()[at], mask.protect()[at]);
            if w > 0.0 {
                let a = 0.5 * w;
                out.extend_from_slice(&[
                    mix(p[0], colour[0], a),
                    mix(p[1], colour[1], a),
                    mix(p[2], colour[2], a),
                    255,
                ]);
            } else {
                out.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
    }
    Ok(out)
}

/// The action `inspect`.
pub struct Inspect {
    descriptor: ActionDescriptor,
}

impl Inspect {
    pub fn new() -> Inspect {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "A mask for the eye: the overlay on its image, the core's width on each row, and its counts in the journal.",
        )
        .input(InputPort::required("mask", ArtifactType::Mask, "The mask to inspect.").on("image"))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image the mask is on, which the overlay is drawn on.",
        ))
        .output(OutputPort::one(
            "overlay",
            ArtifactType::Image,
            "The overlay, an image whose role is a rendering.",
        ))
        .output(
            OutputPort::one(
                "widths",
                ArtifactType::Series,
                "The core's width on each row of the image, in pixels.",
            )
            .on(PortRef::input("image")),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Inspect { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Inspect {
    fn default() -> Inspect {
        Inspect::new()
    }
}

/// Registers `inspect`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Inspect::new()))
}

/// What `whizbang rascal inspect` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The mask, by its identity in the store.
    #[arg(long)]
    pub mask: String,
    /// The image the mask is on, by its identity in the store.
    #[arg(long)]
    pub image: String,
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
            ("mask", "--mask", self.mask.as_str()),
            ("image", "--image", self.image.as_str()),
        ]
    }
}

impl Action for Inspect {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let (mask_id, image_id) = (
            operation::one(call, "mask")?,
            operation::one(call, "image")?,
        );
        let mask = operation::mask(context, mask_id)?;
        let (_, picture) = paste::picture(context, image_id)?;
        if operation::stopped(context) {
            return Err(operation::stopped_failure(context, NAME));
        }
        let space = mask.space();
        let rgba = overlay(&mask, picture.pixels(), TINT).map_err(|why| Failure::invalid(&why))?;
        let file = encode::png_of(&rgba, space.width(), space.height(), Channels::Rgba)
            .map_err(|why| Failure::error("encode", &why))?;
        let step = Step::new(NAME, VERSION)
            .input("mask", mask_id.0)
            .input("image", image_id.0);
        let failed = |error: whizbang_core::artifact::ArtifactError| {
            Failure::error("inspect", &error.without_paths())
        };
        let origin = Origin::new(step).map_err(failed)?;
        let overlay = Image::new(
            file,
            space.projection(),
            space.topology(),
            ImageRole::Rendering,
            origin.clone(),
            &DecodeLimits::default(),
        )
        .map_err(failed)?;
        let widths = Series::new(
            mask.binding().clone(),
            "core_width",
            "px",
            1,
            core_widths(&mask),
            origin,
        )
        .map_err(failed)?;
        context.events().progress(&report(&mask).line());
        Ok(Produced::nothing()
            .with("overlay", overlay)
            .with("widths", widths))
    }
}
