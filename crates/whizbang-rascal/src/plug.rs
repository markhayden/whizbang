//! `plug`: the core's small holes filled (plan 2A, task R2).
//!
//! A *hole* is a region of pixels outside the core, joined through their four sides,
//! that touches no edge of the image. A *blob* is a region of the core, joined through
//! sides and corners. A hole's *owner* is the largest blob beside it (sides or corners),
//! by its own pixels; of two as large, the one whose first pixel, row by row, comes
//! first. A hole is filled when its area is at most `max_share` of its owner's. Every
//! region and area is taken from the core given before anything is filled, so the order
//! of filling changes nothing.
//!
//! It is a refinement, as `grow` is: the core is replaced, so the blend is cleared and the
//! profile and the prescriptions are removed, and protect is kept and still wins: a
//! protected pixel of a hole stays out ([`Mask::replace_core`]).
//!
//! Ported from rascal `crates/rascal-ops/src/plug.rs` at the commit `docs/sources.toml`
//! pins, with one difference on purpose: the share is compared exactly. Rascal compared
//! `area <= max_share * owner` as doubles multiply; here the double `max_share` is a whole
//! number times a power of two and the comparison is made in whole numbers
//! ([`at_most_share`]), so a hole on the bound is judged the same on every machine and by
//! the arithmetic, not by the rounding of a product.

use std::collections::VecDeque;

use whizbang_core::artifact::{ArtifactError, ArtifactType, Mask, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort,
    PreparedCall, Produced, Value, ValueType,
};

use crate::grow::text_of;
use crate::operation;

/// The action's name.
pub const NAME: &str = "plug";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// The share a call that does not say one fills up to: rascal's.
pub const MAX_SHARE_DEFAULT: f64 = 0.1;

/// How many pixels are labelled between two looks at whether to stop.
const PIXELS_BETWEEN_LOOKS: usize = 1 << 16;

/// Whether `part` is at most `share` of `whole`, exactly: `share` is a whole number
/// `m` times `2^p`, and `part <= m * 2^p * whole` is compared in whole numbers. A share
/// that is not over 0 holds no part over 0.
pub fn at_most_share(part: u64, whole: u64, share: f64) -> bool {
    if share.is_nan() || share <= 0.0 {
        return part == 0;
    }
    if share.is_infinite() {
        return true;
    }
    let bits = share.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1 << 52) - 1);
    let (m, p) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), exponent - 1075)
    };
    // `m * whole` is under 2^117.
    let right = u128::from(m) * u128::from(whole);
    if p >= 0 {
        // A share of 1 or more: of a whole over 0, from 2^63 up it holds any part.
        if whole == 0 {
            return part == 0;
        }
        return p >= 11 || u128::from(part) <= right << p;
    }
    // part * 2^-p <= right, where a left side that would not fit 128 bits is larger.
    let (left, shift) = (u128::from(part), (-p) as u32);
    if part == 0 {
        return true;
    }
    left.leading_zeros() >= shift && left << shift <= right
}

/// Labels the regions of the pixels where `want` holds, joined through sides and, when
/// `corners`, corners: label 0 is none, and labels count up in the order of each region's
/// first pixel, row by row. The areas are by label. Nothing comes back once `stopped`
/// says so.
fn regions(
    width: usize,
    height: usize,
    want: impl Fn(usize) -> bool,
    corners: bool,
    stopped: &dyn Fn() -> bool,
) -> Option<(Vec<u32>, Vec<u64>)> {
    let mut label = vec![0u32; width * height];
    let mut areas = vec![0u64];
    let mut queue = VecDeque::new();
    for start in 0..width * height {
        if start % PIXELS_BETWEEN_LOOKS == 0 && stopped() {
            return None;
        }
        if label[start] != 0 || !want(start) {
            continue;
        }
        let id = areas.len() as u32;
        areas.push(0);
        label[start] = id;
        queue.push_back(start);
        while let Some(at) = queue.pop_front() {
            areas[id as usize] += 1;
            for next in beside(at, width, height, corners) {
                if label[next] == 0 && want(next) {
                    label[next] = id;
                    queue.push_back(next);
                }
            }
        }
    }
    Some((label, areas))
}

/// The pixels beside `at`: through its sides, and through its corners when `corners`.
fn beside(at: usize, width: usize, height: usize, corners: bool) -> impl Iterator<Item = usize> {
    let (x, y) = ((at % width) as i64, (at / width) as i64);
    let steps = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ];
    steps.into_iter().filter_map(move |(dx, dy)| {
        let (nx, ny) = (x + dx, y + dy);
        let inside = nx >= 0 && ny >= 0 && nx < width as i64 && ny < height as i64;
        (inside && (corners || dx == 0 || dy == 0)).then(|| ny as usize * width + nx as usize)
    })
}

/// What [`plugged`] gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugged {
    /// The core with every hole that qualified set, 0 or 255 for each pixel.
    pub core: Vec<u8>,
    /// The holes found: regions outside the core that touch no edge.
    pub found: u64,
    /// The holes filled.
    pub filled: u64,
}

/// The core with every hole of at most `max_share` of its owner's area filled. `core`
/// has one byte for every pixel, row by row; any byte that is not 0 is in it.
///
/// # Panics
/// When `core` does not have `width * height` bytes.
pub fn plugged(
    core: &[u8],
    width: usize,
    height: usize,
    max_share: f64,
    stopped: &dyn Fn() -> bool,
) -> Option<Plugged> {
    assert_eq!(core.len(), width * height, "one byte for every pixel");
    let on = |at: usize| core[at] != 0;
    let (blob, blob_area) = regions(width, height, on, true, stopped)?;
    let (gap, gap_area) = regions(width, height, |at| !on(at), false, stopped)?;
    let gaps = gap_area.len();
    let mut edge = vec![false; gaps];
    let mut owner = vec![0u32; gaps];
    for (at, &g) in gap.iter().enumerate() {
        let g = g as usize;
        if g == 0 {
            continue;
        }
        let (x, y) = (at % width, at / width);
        if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
            edge[g] = true;
        }
        for next in beside(at, width, height, true) {
            let b = blob[next];
            if b == 0 {
                continue;
            }
            let current = owner[g];
            let (area, held) = (blob_area[b as usize], blob_area[current as usize]);
            if current == 0 || area > held || (area == held && b < current) {
                owner[g] = b;
            }
        }
    }
    let fill: Vec<bool> = (0..gaps)
        .map(|g| {
            g != 0
                && !edge[g]
                && owner[g] != 0
                && at_most_share(gap_area[g], blob_area[owner[g] as usize], max_share)
        })
        .collect();
    let found = (1..gaps).filter(|g| !edge[*g]).count() as u64;
    let filled = fill.iter().filter(|fill| **fill).count() as u64;
    let mut out: Vec<u8> = core.iter().map(|s| if *s != 0 { 255 } else { 0 }).collect();
    for (sample, g) in out.iter_mut().zip(&gap) {
        if fill[*g as usize] {
            *sample = 255;
        }
    }
    Some(Plugged {
        core: out,
        found,
        filled,
    })
}

/// Plugs `mask`'s holes, and then does what every replacement of a core does
/// ([`Mask::replace_core`]). The mask that comes back has one more step in its origin,
/// which names this action, its version, `max_share` and the mask it was made from.
/// Nothing comes back once `stopped` says so.
pub fn plug(
    mut mask: Mask,
    max_share: f64,
    stopped: &dyn Fn() -> bool,
) -> Result<Option<(Mask, Plugged)>, ArtifactError> {
    let space = mask.space();
    let (width, height) = (space.width() as usize, space.height() as usize);
    let from = mask.id();
    let Some(plugged) = plugged(mask.core(), width, height, max_share, stopped) else {
        return Ok(None);
    };
    mask.replace_core(plugged.core.clone())?;
    let step = Step::new(NAME, VERSION).argument("max_share", &text_of(max_share));
    mask.record(step.input("mask", from.0))?;
    Ok(Some((mask, plugged)))
}

/// The action `plug`.
pub struct Plug {
    descriptor: ActionDescriptor,
}

impl Plug {
    pub fn new() -> Plug {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Fills the holes of a mask's core up to a share of the area of the shape around each.",
        )
        .argument(Argument::with_default(
            "max_share",
            "The largest hole filled, as a share of the area of the shape around it, from 0 to 1.",
            ValueType::number(Some(0.0), Some(1.0)),
            Value::Number(MAX_SHARE_DEFAULT),
        ))
        .input(InputPort::required(
            "mask",
            ArtifactType::Mask,
            "The mask whose holes are filled.",
        ))
        .output(OutputPort::one(
            "mask",
            ArtifactType::Mask,
            "The mask with its small holes filled, its blend cleared and its protect kept.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Plug { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Plug {
    fn default() -> Plug {
        Plug::new()
    }
}

impl Action for Plug {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let share = operation::number(call, "max_share").unwrap_or(MAX_SHARE_DEFAULT);
        let Some(id) = operation::given(call, "mask").first().copied() else {
            return Err(Failure::error("input", "the call gives no mask"));
        };
        let read = operation::mask(context, id)?;
        let stopped = || operation::stopped(context);
        let plugged = plug(read, share, &stopped)
            .map_err(|error| Failure::error("mask", &error.without_paths()))?;
        let Some((mask, plugged)) = plugged else {
            return Err(operation::stopped_failure(context, NAME));
        };
        context.events().progress(&format!(
            "plug: {} of {} holes filled; the blend is cleared",
            plugged.filled, plugged.found
        ));
        Ok(Produced::nothing().with("mask", mask))
    }
}

/// Registers the action.
pub fn register(
    registry: &mut whizbang_core::contract::Registry,
) -> Result<(), whizbang_core::contract::RegistrationError> {
    registry.register(std::sync::Arc::new(Plug::new()))
}

/// What the command says the action does.
pub const ABOUT: &str = "Fill the holes of a mask's core: each region outside the core that touches no edge of \
the image and is at most a share of the area of the shape around it. The blend is cleared, \
the profile and the prescriptions are removed, and protect is kept. The new mask is \
committed to the store, and its identity is printed.";

/// What `whizbang rascal plug` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The mask, by its identity in the store.
    #[arg(long)]
    pub mask: String,
    /// The largest hole filled, as a share of the area of the shape around it, from 0 to
    /// 1.
    #[arg(long, default_value_t = MAX_SHARE_DEFAULT)]
    pub max_share: f64,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        vec![("max_share", Value::Number(self.max_share))]
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![("mask", "--mask", self.mask.as_str())]
    }
}
