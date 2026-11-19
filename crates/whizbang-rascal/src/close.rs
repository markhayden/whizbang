//! `close`: the core of a mask closed by a disc, grown and then eroded by it, so that gaps
//! and notches narrower than the disc fill in (`SPEC.md` section 7). It is a refinement:
//! the core is replaced, so the blend is cleared and the profile and the prescriptions
//! are removed, and protect is kept and still wins ([`Mask::replace_core`]).
//!
//! # The rule, in whole numbers
//!
//! `K` is the whole part of `radius_px * radius_px`, squared exactly, as `grow` takes it
//! ([`crate::grow::radius_squared`]). A pixel is *near* another when the squared distance
//! between their centres is at most `K`.
//!
//! 1. The *grown* core is every pixel near a core pixel: `grow` with no `down_px`.
//! 2. The *closed* core is every pixel of the image that is near no pixel of the image
//!    outside the grown core. Outside the image counts as grown: a shape against the
//!    border keeps its edge, so closing changes nothing but gaps and notches.
//! 3. The new core is every closed pixel that is not protected.
//!
//! Step 2 is the erosion of the grown core in the convention rascal called `erode_within`
//! ([`eroded_within`]): the complement of the grown core, inside the image, grown by the
//! same disc, and the complement of that. Both steps are the sweeps of `grow`, so a call
//! costs the mask's pixels whatever the radius.
//!
//! # Deliberate differences from rascal
//!
//! Ported from rascal `crates/rascal-ops/src/close.rs`, `refine.rs` and the disc dilation
//! and erosion of `primitives.rs`, at the commit `docs/sources.toml` pins. Rascal's disc
//! went through a distance transform in doubles and single-precision floats; this one is
//! `grow`'s, with `grow`'s three differences (`crate::grow`, "Three deliberate
//! differences from rascal"): no slack of 0.000000001 on the radius squared, and no
//! distance taken through a 32-bit float. Both only move a pixel at a radius a hair
//! under a square root, or at distances over two thousand pixels.

use whizbang_core::artifact::{ArtifactError, ArtifactType, Mask, Refinement, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};

use crate::grow::{PX_MAX, grown, text_of};
use crate::operation;

/// The action's name.
pub const NAME: &str = "close";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// What the command says the action does.
pub const ABOUT: &str = "Close the core of a mask by a disc: grown and then eroded by it, so gaps and notches \
narrower than the disc fill in. The blend is cleared, the profile and the prescriptions are \
removed, and protect is kept. The new mask is committed to the store, and its identity is \
printed.";

/// The erosion of `plane` by the disc of `radius_px`, where outside the image counts as
/// set: a pixel stays when no pixel of the image that is not set is near it. `plane` has
/// one byte for every pixel, row by row; what comes back has 0 or 255 for each. Nothing
/// comes back once `stopped` says so.
///
/// # Panics
/// When `plane` does not have `width * height` bytes.
pub fn eroded_within(
    plane: &[u8],
    width: usize,
    height: usize,
    radius_px: f64,
    stopped: &dyn Fn() -> bool,
) -> Option<Vec<u8>> {
    let outside: Vec<u8> = plane
        .iter()
        .map(|sample| if *sample == 0 { 255 } else { 0 })
        .collect();
    let reached = grown(&outside, width, height, radius_px, 0.0, stopped)?;
    Some(
        reached
            .iter()
            .map(|sample| if *sample == 0 { 255 } else { 0 })
            .collect(),
    )
}

/// The closed plane of a core, before protect is taken out of it: steps 1 and 2 of the
/// rule. Nothing comes back once `stopped` says so.
///
/// # Panics
/// When `core` does not have `width * height` bytes.
pub fn closed(
    core: &[u8],
    width: usize,
    height: usize,
    radius_px: f64,
    stopped: &dyn Fn() -> bool,
) -> Option<Vec<u8>> {
    let grown = grown(core, width, height, radius_px, 0.0, stopped)?;
    eroded_within(&grown, width, height, radius_px, stopped)
}

/// What closing a mask gave.
#[derive(Debug, Clone, PartialEq)]
pub struct Closed {
    /// The mask with its new core, and one more step in its origin.
    pub mask: Mask,
    /// What replacing the core cleared.
    pub cleared: Refinement,
    /// How many pixels the core had.
    pub core_before: u64,
    /// How many it has now.
    pub core_after: u64,
}

/// Closes `mask`: the three steps of the rule, and then what every replacement of a core
/// does ([`Mask::replace_core`]). The mask that comes back has one more step in its
/// origin, which names this action, its version, `radius_px` and the mask it was made
/// from. Nothing comes back once `stopped` says so.
pub fn close(
    mut mask: Mask,
    radius_px: f64,
    stopped: &dyn Fn() -> bool,
) -> Result<Option<Closed>, ArtifactError> {
    let space = mask.space();
    let (width, height) = (space.width() as usize, space.height() as usize);
    let core_before = operation::set(mask.core());
    let from = mask.id();
    let Some(plane) = closed(mask.core(), width, height, radius_px, stopped) else {
        return Ok(None);
    };
    let cleared = mask.replace_core(plane)?;
    let step = Step::new(NAME, VERSION).argument("radius_px", &text_of(radius_px));
    mask.record(step.input("mask", from.0))?;
    let core_after = operation::set(mask.core());
    Ok(Some(Closed {
        mask,
        cleared,
        core_before,
        core_after,
    }))
}

/// The action `close`.
pub struct Close {
    descriptor: ActionDescriptor,
}

impl Close {
    pub fn new() -> Close {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Closes the core of a mask by a disc, so gaps and notches narrower than the disc fill in.",
        )
        .argument(Argument::required(
            "radius_px",
            "The radius of the disc, in pixels of the mask's image, from 0 to 10000.",
            ValueType::number(Some(0.0), Some(PX_MAX)),
        ))
        .input(InputPort::required(
            "mask",
            ArtifactType::Mask,
            "The mask whose core is closed.",
        ))
        .output(OutputPort::one(
            "mask",
            ArtifactType::Mask,
            "The mask with the closed core, its blend cleared and its protect kept.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Close { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Close {
    fn default() -> Close {
        Close::new()
    }
}

/// Registers the action.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(std::sync::Arc::new(Close::new()))
}

/// What `whizbang rascal close` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The mask, by its identity in the store.
    #[arg(long)]
    pub mask: String,
    /// The radius of the disc, in pixels of the mask's image, from 0 to 10000. A pixel is
    /// near another when their centres are at most this far apart.
    #[arg(long)]
    pub radius_px: f64,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        vec![("radius_px", Value::Number(self.radius_px))]
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![("mask", "--mask", self.mask.as_str())]
    }
}

impl Action for Close {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        // A call that was prepared has it: the descriptor requires it.
        let radius = operation::number(call, "radius_px")
            .ok_or_else(|| Failure::error("argument", "the call gives no number radius_px"))?;
        let Some(id) = operation::given(call, "mask").first().copied() else {
            return Err(Failure::error("input", "the call gives no mask"));
        };
        let read = operation::mask(context, id)?;
        context.events().progress("close: read the mask");
        let stopped = || operation::stopped(context);
        let closed = close(read, radius, &stopped)
            .map_err(|error| Failure::error("mask", &error.without_paths()))?;
        let Some(closed) = closed else {
            return Err(operation::stopped_failure(context, NAME));
        };
        context.events().progress(&format!(
            "close: the core went from {} to {} pixels; the blend is cleared{}",
            closed.core_before,
            closed.core_after,
            if closed.cleared.profile_cleared {
                ", and the profile and the prescriptions are removed"
            } else {
                ""
            }
        ));
        Ok(Produced::nothing().with("mask", closed.mask))
    }
}
