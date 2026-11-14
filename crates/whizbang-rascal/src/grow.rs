//! `grow`: the core of a mask made larger by a disc, and then extended downward, as for
//! the shadow under an object (`SPEC.md` section 7). Its tool record, with worked examples,
//! what it was measured against and its limits, is `docs/reference/tools/grow.md`.
//!
//! It is a refinement: the core is replaced, so the blend is cleared and the profile
//! and the prescriptions are removed, and protect is kept and still wins
//! ([`Mask::replace_core`]).
//!
//! # The rule, in whole numbers
//!
//! A pixel is a place `(x, y)`: a column and a row, the first of each numbered 0, rows
//! counted downward. The distance between two pixels is between their centres, so its
//! square, `dx * dx + dy * dy`, is a whole number.
//!
//! 1. `K` is the whole part of `px * px`, where `px` is the double it was given as,
//!    squared exactly and not as doubles multiply ([`radius_squared`]).
//! 2. A pixel is *grown* when some pixel of the core is at a squared distance of at
//!    most `K` from it. The core's own pixels are at 0, so they are all grown.
//! 3. `R` is the whole part of `down_px`, or 0 without it. A pixel is *extended* when it
//!    is grown, or a grown pixel is straight above it, at most `R` rows up.
//! 4. The new core is every extended pixel that is not protected.
//!
//! Nothing here is a floating-point comparison, so the same mask and the same
//! arguments give the same core on every machine.
//!
//! # The method
//!
//! Step 2 as it is written costs the mask's pixels times the disc's, and a disc of the
//! largest radius has three hundred million. So it is done in two sweeps of the rows,
//! one downward and one upward, whose cost is the mask's pixels whatever the radius.
//! The sweep downward keeps, for each column, how many rows up the nearest core pixel
//! at or above the row is. A core pixel `v` rows away in its column, with `v * v` at most
//! `K`, reaches along the row to every pixel at most `h` columns from it, where `h` is
//! the largest whole number with `h * h + v * v` at most `K`: those are the pixels of the
//! row it grows. The sweep upward does the same for the core pixels below. A pixel is
//! grown exactly when one of the two sweeps reaches it, because the nearest core pixel
//! of a column, above or below, reaches at least as far along the row as any other in
//! that column and that direction.
//!
//! # Three deliberate differences from rascal
//!
//! Ported from rascal `crates/rascal-ops/src/grow.rs` and `refine.rs`, and the disc
//! dilation of `primitives.rs`, which went through a distance transform in doubles.
//! The port differs from it in three ways, each on purpose:
//!
//! 1. **No slack.** Rascal grew a pixel when its squared distance was at most the
//!    radius squared as doubles multiply it, plus 0.000000001. Here it is the whole
//!    part of the exact square: a radius a hair under a square root leaves that
//!    distance out.
//! 2. **No 32-bit float.** Rascal took each distance through a single-precision float
//!    and got the square back by rounding. A float of 24 bits cannot hold every square
//!    root apart: by arithmetic on rascal's formula, which was not run, the first
//!    squared distance that does not come back is 4,197,201, and for every whole radius
//!    `r` from 4096 up the pixel at `r` columns and 1 row from the core, at a squared
//!    distance of `r * r + 1`, came back as `r * r` and was grown. Here it is left out.
//! 3. **`down_px` is floored.** Rascal rounded it, a half going up.

use whizbang_core::artifact::{self, ArtifactError, ArtifactType, Mask, Refinement, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort,
    PreparedCall, Produced, Value, ValueType,
};
use whizbang_core::identity::DecodeLimits;

/// The action's name.
pub const NAME: &str = "grow";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// The largest `px` and the largest `down_px`, in pixels. The time a call takes does
/// not depend on either; the bound is there so that a call says something a mask can
/// mean, and it is rascal's.
pub const PX_MAX: f64 = 10_000.0;

/// The diagnostic every run of `grow` writes before it reads its input: the numbers
/// the rule was applied with.
pub const DIAGNOSTIC: &str = "grow.json";

/// How many rows are worked between two looks at whether to stop.
const ROWS_BETWEEN_LOOKS: usize = 64;

/// The whole part of the square of `px`, squared exactly: the double is a whole number
/// times a power of two, the whole number is squared in 128 bits, and the power of two
/// is applied by a shift, which drops what is under one. So it is never off by the
/// rounding of a multiplication of doubles. A number that is not over 0 gives 0, and one
/// whose square is over the largest `u64`, that largest.
pub fn radius_squared(px: f64) -> u64 {
    if px.is_nan() || px <= 0.0 {
        return 0;
    }
    let bits = px.to_bits();
    let exponent = (bits >> 52) & 0x7ff;
    let fraction = bits & ((1 << 52) - 1);
    if exponent == 0x7ff {
        return u64::MAX;
    }
    // The double is `whole * 2^power`.
    let (whole, power) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), exponent as i32 - 1075)
    };
    // Under 2^106.
    let square = u128::from(whole) * u128::from(whole);
    let power = 2 * power;
    let floor = if power >= 0 {
        // A shift of 22 or more could push a bit out of 128.
        if power >= 22 {
            return u64::MAX;
        }
        square << power
    } else if power <= -128 {
        0
    } else {
        square >> -power
    };
    u64::try_from(floor).unwrap_or(u64::MAX)
}

/// The whole part of `down_px`: how many rows down the grown core is extended. A
/// number that is not over 0 gives 0.
pub fn rows_down(down_px: f64) -> u64 {
    if down_px.is_nan() || down_px <= 0.0 {
        return 0;
    }
    // The whole part of a double is a double exactly, and the cast stops at the largest.
    down_px.floor() as u64
}

/// No core pixel in this column on this side, within reach.
const NONE: u32 = u32::MAX;

/// One sweep of the rows, in the order given: marks in `out` every pixel a core pixel
/// of its own row, or of a row already swept, grows. `reach[v]` is how far along a row
/// a core pixel `v` rows away grows. False when `stopped` said to stop.
fn sweep(
    core: &[u8],
    width: usize,
    rows: impl Iterator<Item = usize>,
    reach: &[u32],
    out: &mut [u8],
    stopped: &dyn Fn() -> bool,
) -> bool {
    // For each column: how many rows back the nearest core pixel is.
    let mut back = vec![NONE; width];
    // Where the stretches of a row that are grown begin (+1) and end (-1).
    let mut edges = vec![0i32; width + 1];
    for (count, y) in rows.enumerate() {
        if count % ROWS_BETWEEN_LOOKS == 0 && stopped() {
            return false;
        }
        let row = &core[y * width..(y + 1) * width];
        edges.fill(0);
        for (x, sample) in row.iter().enumerate() {
            if *sample != 0 {
                back[x] = 0;
            } else if back[x] != NONE {
                back[x] += 1;
                if back[x] as usize >= reach.len() {
                    back[x] = NONE;
                }
            }
            if back[x] != NONE {
                let along = reach[back[x] as usize] as usize;
                edges[x.saturating_sub(along)] += 1;
                edges[x.saturating_add(along).saturating_add(1).min(width)] -= 1;
            }
        }
        let mut inside = 0;
        for (x, sample) in out[y * width..(y + 1) * width].iter_mut().enumerate() {
            inside += edges[x];
            if inside > 0 {
                *sample = 255;
            }
        }
    }
    true
}

/// The grown and extended plane of a core, before protect is taken out of it: steps 1
/// to 3 of the rule. `core` has one byte for every pixel, row by row, and any byte that
/// is not 0 is in the core. What comes back has 0 or 255 for each. `stopped` is asked
/// from time to time, and nothing comes back once it says so.
///
/// # Panics
/// When `core` does not have `width * height` bytes.
pub fn grown(
    core: &[u8],
    width: usize,
    height: usize,
    px: f64,
    down_px: f64,
    stopped: &dyn Fn() -> bool,
) -> Option<Vec<u8>> {
    assert_eq!(core.len(), width * height, "one byte for every pixel");
    let k = radius_squared(px);
    // A core pixel further than this many rows away grows nothing in the row, and none
    // is further away than the mask is high.
    let furthest = k.isqrt().min(height as u64);
    // For each count of rows `v` up to that: the largest `h` with `h * h + v * v <= K`,
    // and no more than the mask is wide.
    let reach: Vec<u32> = (0..=furthest)
        .map(|v| (k - v * v).isqrt().min(width as u64) as u32)
        .collect();
    let mut out = vec![0u8; core.len()];
    if !sweep(core, width, 0..height, &reach, &mut out, stopped)
        || !sweep(core, width, (0..height).rev(), &reach, &mut out, stopped)
    {
        return None;
    }

    let rows = rows_down(down_px);
    if rows > 0 {
        // For each column: how many rows up the nearest grown pixel is.
        let mut up = vec![NONE; width];
        for (y, row) in out.chunks_mut(width.max(1)).enumerate() {
            if y % ROWS_BETWEEN_LOOKS == 0 && stopped() {
                return None;
            }
            for (sample, up) in row.iter_mut().zip(&mut up) {
                if *sample != 0 {
                    *up = 0;
                } else if *up != NONE {
                    // At most the mask's height, which is under `NONE`.
                    *up += 1;
                    if u64::from(*up) <= rows {
                        *sample = 255;
                    }
                }
            }
        }
    }
    Some(out)
}

/// What growing a mask gave.
#[derive(Debug, Clone, PartialEq)]
pub struct Grown {
    /// The mask with its new core, and one more step in its origin.
    pub mask: Mask,
    /// What replacing the core cleared.
    pub cleared: Refinement,
    /// How many pixels the core had.
    pub core_before: u64,
    /// How many it has now.
    pub core_after: u64,
}

fn set(plane: &[u8]) -> u64 {
    plane.iter().filter(|sample| **sample != 0).count() as u64
}

/// A number as an origin and a diagnostic write it: the shortest decimal that reads
/// back as the same double, which is the same text on every machine.
/// Zero of either sign is `0`: a zero that carries a sign is the same radius, and an
/// origin has one spelling of it.
pub fn text_of(number: f64) -> String {
    if number == 0.0 {
        return "0".to_owned();
    }
    format!("{number}")
}

/// Grows `mask`: the four steps of the rule, and then what every replacement of a core
/// does ([`Mask::replace_core`]). The mask that comes back has the same binding, use,
/// protect plane and source, a new identity unless nothing changed, and one more step
/// in its origin, which names this action, its version, `px`, `down_px` when one was
/// given, and the mask it was made from. Nothing comes back once `stopped` says so.
pub fn grow(
    mut mask: Mask,
    px: f64,
    down_px: Option<f64>,
    stopped: &dyn Fn() -> bool,
) -> Result<Option<Grown>, ArtifactError> {
    let space = mask.space();
    let (width, height) = (space.width() as usize, space.height() as usize);
    let core_before = set(mask.core());
    let from = mask.id();
    let Some(plane) = grown(
        mask.core(),
        width,
        height,
        px,
        down_px.unwrap_or(0.0),
        stopped,
    ) else {
        return Ok(None);
    };
    let cleared = mask.replace_core(plane)?;
    let mut step = Step::new(NAME, VERSION).argument("px", &text_of(px));
    if let Some(down_px) = down_px {
        step = step.argument("down_px", &text_of(down_px));
    }
    mask.record(step.input("mask", from.0))?;
    let core_after = set(mask.core());
    Ok(Some(Grown {
        mask,
        cleared,
        core_before,
        core_after,
    }))
}

/// The action `grow`.
pub struct Grow {
    descriptor: ActionDescriptor,
}

impl Grow {
    pub fn new() -> Grow {
        let within = || ValueType::number(Some(0.0), Some(PX_MAX));
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Grows the core of a mask by a disc, and further downward for a shadow.",
        )
        .argument(Argument::required(
            "px",
            "The radius of the disc, in pixels of the mask's image, from 0 to 10000.",
            within(),
        ))
        .argument(Argument::optional(
            "down_px",
            "How many pixels further down the grown core reaches, from 0 to 10000.",
            within(),
        ))
        .input(InputPort::required(
            "mask",
            ArtifactType::Mask,
            "The mask whose core is grown.",
        ))
        .output(OutputPort::one(
            "mask",
            ArtifactType::Mask,
            "The mask with the grown core, its blend cleared and its protect kept.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Grow { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Grow {
    fn default() -> Grow {
        Grow::new()
    }
}

/// The number of an argument, when the call has it.
fn number(call: &PreparedCall, name: &str) -> Option<f64> {
    call.arguments().iter().find_map(|(n, value)| match value {
        Value::Number(number) if n == name => Some(*number),
        _ => None,
    })
}

impl Action for Grow {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        // A call that was prepared has both: the descriptor requires them.
        let px = number(call, "px")
            .ok_or_else(|| Failure::error("argument", "the call gives no number px"))?;
        let down_px = number(call, "down_px");
        let input = call
            .inputs()
            .iter()
            .find(|input| input.port() == "mask")
            .ok_or_else(|| Failure::error("input", "the call gives no mask"))?;

        // Written first, so that a run that fails from here on still says what it was
        // asked, in the numbers the rule uses.
        let asked = format!(
            "{{\"px\":\"{}\",\"down_px\":{},\"radius_squared\":{},\"rows_down\":{}}}\n",
            text_of(px),
            down_px.map_or("null".to_owned(), |down| format!("\"{}\"", text_of(down))),
            radius_squared(px),
            rows_down(down_px.unwrap_or(0.0)),
        );
        context
            .diagnostics()
            .write(DIAGNOSTIC, asked.as_bytes())
            .map_err(|why| Failure::error("diagnostic", &why))?;

        // The store's own words for a failure name its files by their paths, and a
        // message is kept in the run's record: so the words without paths.
        let read = artifact::get(
            context.store(),
            ArtifactType::Mask,
            input.id(),
            &DecodeLimits::default(),
        )
        .and_then(artifact::Artifact::into_mask)
        .map_err(|error| Failure::error("input", &error.without_paths()))?;
        context.events().progress("grow: read the mask");

        // The deadline by the executor's clock, as the context tells it.
        let stopped = || context.cancellation().is_cancelled() || context.deadline_passed();
        let grown = grow(read, px, down_px, &stopped)
            .map_err(|error| Failure::error("mask", &error.without_paths()))?;
        let Some(grown) = grown else {
            return Err(if context.cancellation().is_cancelled() {
                Failure::error("cancelled", "grow stopped: its run was cancelled")
            } else {
                Failure::error("deadline", "grow stopped: its deadline passed")
            });
        };
        context.events().progress(&format!(
            "grow: the core went from {} to {} pixels; the blend is cleared{}",
            grown.core_before,
            grown.core_after,
            if grown.cleared.profile_cleared {
                ", and the profile and the prescriptions are removed"
            } else {
                ""
            }
        ));
        Ok(Produced::nothing().with("mask", grown.mask))
    }
}
