//! `feather`: the blend set to a feather of the core's boundary, outward, with one sigma
//! for the whole mask (plan 2A, task R2). The blend there was is replaced; the core, the
//! profile and the prescriptions are kept, and protect is kept and still wins: the blend
//! is 0 under it ([`Mask::set_blend`]).
//!
//! # The rule
//!
//! For each pixel outside the core, `k` is the squared distance from its centre to the
//! nearest core pixel's centre, a whole number, and `d` is `sqrt(k) - 0.5`, the distance
//! to that pixel's edge. The blend is the nearest whole number to
//! `255 * exp(-d * d / (2 * sigma * sigma))` where `0 < d <= 3 * sigma`, and 0 elsewhere,
//! and in the core. Three is rascal's support, its constant set `feather-v1`.
//!
//! Ported from rascal `crates/rascal-ops/src/feather.rs` and the distance transform of
//! `primitives.rs`, at the commit `docs/sources.toml` pins, with two differences on
//! purpose, so that the same mask and sigma give the same blend, and the same identity,
//! on every machine:
//!
//! 1. **The distance transform is in whole numbers.** Rascal's ran in doubles and gave
//!    each distance as a single-precision float. Here the squared distance is exact
//!    (Meijster, Roerdink and Hesselink's two passes, [`squared_distances`]), and the one
//!    square root is IEEE's, which is correctly rounded everywhere.
//! 2. **The exponential is this module's** ([`exp_neg`]): powers of two taken out and a
//!    series, in additions, multiplications and divisions alone, each correctly rounded. Rascal
//!    called the platform's `exp`, which is not the same function on every machine. The
//!    two agree to within a few units in the last place, which moves a blend only where
//!    `255 * exp` is within that of a half.

use whizbang_core::artifact::{ArtifactError, ArtifactType, Mask, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort,
    PreparedCall, Produced, ValueType,
};

use crate::grow::text_of;
use crate::operation;

/// The action's name.
pub const NAME: &str = "feather";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// How many sigmas out the blend reaches: rascal's `feather-v1`.
pub const SUPPORT: f64 = 3.0;

/// The smallest and the largest sigma, in pixels: rascal's.
pub const SIGMA_MIN: f64 = 0.1;
pub const SIGMA_MAX: f64 = 10_000.0;

/// How many rows are worked between two looks at whether to stop.
const ROWS_BETWEEN_LOOKS: usize = 64;

/// `exp(-x)` for `x` from 0 to about 5, in additions, multiplications and divisions
/// alone: `x` is `n` times the double nearest `ln 2`, plus `r` under it, and `exp(-r)`
/// is its series to the twenty-fifth term, past the last place, halved `n` times, which
/// is exact.
pub fn exp_neg(x: f64) -> f64 {
    let mut n = 0;
    let mut r = x;
    while r >= std::f64::consts::LN_2 {
        r -= std::f64::consts::LN_2;
        n += 1;
    }
    let (mut term, mut sum) = (1.0, 1.0);
    for k in 1..=25 {
        term = term * -r / f64::from(k);
        sum += term;
    }
    for _ in 0..n {
        sum /= 2.0;
    }
    sum
}

/// For every pixel, the squared distance from its centre to the nearest core pixel's
/// centre: 0 in the core, and `None` for every pixel when the core is empty. Nothing
/// comes back once `stopped` says so.
///
/// # Panics
/// When `core` does not have `width * height` bytes.
pub fn squared_distances(
    core: &[u8],
    width: usize,
    height: usize,
    stopped: &dyn Fn() -> bool,
) -> Option<Vec<Option<u64>>> {
    assert_eq!(core.len(), width * height, "one byte for every pixel");
    // Further than any pixel of the image is from any other.
    let far = (width + height) as i64;
    // Down each column: how many rows away the nearest core pixel of the column is.
    let mut g = vec![far; core.len()];
    for x in 0..width {
        if x % ROWS_BETWEEN_LOOKS == 0 && stopped() {
            return None;
        }
        let mut last = far;
        for y in 0..height {
            last = if core[y * width + x] != 0 {
                0
            } else {
                (last + 1).min(far)
            };
            g[y * width + x] = last;
        }
        for y in (0..height.saturating_sub(1)).rev() {
            let below = g[(y + 1) * width + x];
            if below + 1 < g[y * width + x] {
                g[y * width + x] = below + 1;
            }
        }
    }
    // Along each row: the lower envelope of the parabolas of the columns.
    let mut out = vec![None; core.len()];
    let mut s = vec![0i64; width];
    let mut t = vec![0i64; width];
    for y in 0..height {
        if y % ROWS_BETWEEN_LOOKS == 0 && stopped() {
            return None;
        }
        let g = &g[y * width..(y + 1) * width];
        let f = |x: i64, i: i64| (x - i) * (x - i) + g[i as usize] * g[i as usize];
        let sep = |i: i64, u: i64| {
            let (gi, gu) = (g[i as usize], g[u as usize]);
            (u * u - i * i + gu * gu - gi * gi).div_euclid(2 * (u - i))
        };
        let mut q: i64 = 0;
        s[0] = 0;
        t[0] = 0;
        for u in 1..width as i64 {
            while q >= 0 && f(t[q as usize], s[q as usize]) > f(t[q as usize], u) {
                q -= 1;
            }
            if q < 0 {
                q = 0;
                s[0] = u;
            } else {
                let w = 1 + sep(s[q as usize], u);
                if w < width as i64 {
                    q += 1;
                    s[q as usize] = u;
                    t[q as usize] = w;
                }
            }
        }
        for u in (0..width as i64).rev() {
            let k = f(u, s[q as usize]);
            if k < far * far {
                out[y * width + u as usize] = Some(k as u64);
            }
            if u == t[q as usize] {
                q -= 1;
            }
        }
    }
    Some(out)
}

/// The blend a pixel at squared distance `k` from the core has, by the rule.
pub fn blend_at(k: u64, sigma: f64) -> u8 {
    let d = (k as f64).sqrt() - 0.5;
    if !(d > 0.0 && d <= SUPPORT * sigma) {
        return 0;
    }
    let value = (255.0 * exp_neg(d * d / (2.0 * sigma * sigma))).round();
    value.clamp(0.0, 255.0) as u8
}

/// The feather of a core: the blend by the rule, before protect is taken out of it.
/// Nothing comes back once `stopped` says so.
pub fn feathered(
    core: &[u8],
    width: usize,
    height: usize,
    sigma: f64,
    stopped: &dyn Fn() -> bool,
) -> Option<Vec<u8>> {
    let distances = squared_distances(core, width, height, stopped)?;
    Some(
        distances
            .iter()
            .map(|k| k.map_or(0, |k| blend_at(k, sigma)))
            .collect(),
    )
}

/// Feathers `mask`: its blend set by the rule, and one more step in its origin, which
/// names this action, its version, `sigma_px` and the mask it was made from. Nothing
/// comes back once `stopped` says so.
pub fn feather(
    mut mask: Mask,
    sigma: f64,
    stopped: &dyn Fn() -> bool,
) -> Result<Option<Mask>, ArtifactError> {
    let space = mask.space();
    let (width, height) = (space.width() as usize, space.height() as usize);
    let from = mask.id();
    let Some(blend) = feathered(mask.core(), width, height, sigma, stopped) else {
        return Ok(None);
    };
    mask.set_blend(blend)?;
    let step = Step::new(NAME, VERSION).argument("sigma_px", &text_of(sigma));
    mask.record(step.input("mask", from.0))?;
    Ok(Some(mask))
}

/// The action `feather`.
pub struct Feather {
    descriptor: ActionDescriptor,
}

impl Feather {
    pub fn new() -> Feather {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Sets the blend of a mask to a feather of its core's edge, outward, with one sigma.",
        )
        .argument(Argument::required(
            "sigma_px",
            "The feather's sigma, in pixels of the mask's image, from 0.1 to 10000.",
            ValueType::number(Some(SIGMA_MIN), Some(SIGMA_MAX)),
        ))
        .input(InputPort::required(
            "mask",
            ArtifactType::Mask,
            "The mask whose core's edge is feathered.",
        ))
        .output(OutputPort::one(
            "mask",
            ArtifactType::Mask,
            "The mask with the new blend, its core and its protect kept.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Feather { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Feather {
    fn default() -> Feather {
        Feather::new()
    }
}

impl Action for Feather {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let sigma = operation::number(call, "sigma_px")
            .ok_or_else(|| Failure::error("argument", "the call gives no number sigma_px"))?;
        let Some(id) = operation::given(call, "mask").first().copied() else {
            return Err(Failure::error("input", "the call gives no mask"));
        };
        let read = operation::mask(context, id)?;
        let stopped = || operation::stopped(context);
        let feathered = feather(read, sigma, &stopped)
            .map_err(|error| Failure::error("mask", &error.without_paths()))?;
        let Some(mask) = feathered else {
            return Err(operation::stopped_failure(context, NAME));
        };
        context.events().progress(&format!(
            "feather: {} pixels of blend",
            operation::set(mask.blend())
        ));
        Ok(Produced::nothing().with("mask", mask))
    }
}

/// Registers the action.
pub fn register(
    registry: &mut whizbang_core::contract::Registry,
) -> Result<(), whizbang_core::contract::RegistrationError> {
    registry.register(std::sync::Arc::new(Feather::new()))
}

/// What the command says the action does.
pub const ABOUT: &str = "Set the blend of a mask to a feather of its core's edge, outward, out to three sigmas. \
The core is kept, and protect is kept and wins. The new mask is committed to the store, and \
its identity is printed.";

/// What `whizbang rascal feather` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The mask, by its identity in the store.
    #[arg(long)]
    pub mask: String,
    /// The feather's sigma, in pixels of the mask's image, from 0.1 to 10000.
    #[arg(long)]
    pub sigma_px: f64,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, whizbang_core::contract::Value)> {
        vec![(
            "sigma_px",
            whizbang_core::contract::Value::Number(self.sigma_px),
        )]
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![("mask", "--mask", self.mask.as_str())]
    }
}
