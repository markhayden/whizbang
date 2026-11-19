//! `shrink`: the core of a mask eroded by a disc (`SPEC.md` section 7). It is a
//! refinement: the core is replaced, so the blend is cleared and the profile and the
//! prescriptions are removed, and protect is kept and still wins
//! ([`Mask::replace_core`]). A core eroded to nothing makes no mask: the call is refused.
//!
//! # The rule, in whole numbers
//!
//! `K` is the whole part of `px * px`, squared exactly, as `grow` takes it
//! ([`crate::grow::radius_squared`]), and `R` the largest whole number whose square is at
//! most `K`: how far the disc reaches along a row or a column. A core pixel stays when no
//! pixel of the image within the disc of it is outside the core, and the disc lies
//! wholly inside the image: at least `R` pixels from each edge. Outside the image counts
//! as outside the core, so a core against the border shrinks from the border too: the
//! convention rascal called `erode`, and the other of `close`'s
//! ([`crate::close::eroded_within`]).
//!
//! Ported from rascal `crates/rascal-ops/src/shrink.rs` and the erosion of
//! `primitives.rs`, at the commit `docs/sources.toml` pins. The disc is `grow`'s, with
//! `grow`'s deliberate differences from rascal's distance transform (`crate::grow`). A
//! core eroded to nothing, which rascal said did not apply, is refused.

use whizbang_core::artifact::{ArtifactError, ArtifactType, Mask, Refinement, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};

use crate::close::eroded_within;
use crate::grow::{PX_MAX, radius_squared, text_of};
use crate::operation;

/// The action's name.
pub const NAME: &str = "shrink";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// What the command says the action does.
pub const ABOUT: &str = "Shrink the core of a mask by a disc; outside the image counts as outside the core, so a \
core against the border shrinks from it too. The blend is cleared, the profile and the \
prescriptions are removed, and protect is kept. The new mask is committed to the store, and \
its identity is printed.";

/// How far the disc of `px` reaches along a row: the largest whole number whose square
/// is at most the whole part of `px * px`.
pub fn reach(px: f64) -> u64 {
    let k = radius_squared(px);
    let mut r = (k as f64).sqrt() as u64;
    while r.saturating_mul(r) > k {
        r -= 1;
    }
    while (r + 1).saturating_mul(r + 1) <= k {
        r += 1;
    }
    r
}

/// The erosion of `plane` by the disc of `px`, where outside the image counts as not set.
/// Nothing comes back once `stopped` says so.
///
/// # Panics
/// When `plane` does not have `width * height` bytes.
pub fn eroded(
    plane: &[u8],
    width: usize,
    height: usize,
    px: f64,
    stopped: &dyn Fn() -> bool,
) -> Option<Vec<u8>> {
    let within = eroded_within(plane, width, height, px, stopped)?;
    let r = usize::try_from(reach(px)).unwrap_or(usize::MAX);
    let fits = |at: usize| {
        let (x, y) = (at % width, at / width);
        x >= r && y >= r && x.saturating_add(r) < width && y.saturating_add(r) < height
    };
    Some(
        within
            .iter()
            .enumerate()
            .map(|(at, sample)| if *sample != 0 && fits(at) { 255 } else { 0 })
            .collect(),
    )
}

/// Why a mask could not be shrunk.
#[derive(Debug)]
pub enum ShrinkError {
    /// Nothing of the core is left.
    Nothing { px: f64, before: u64 },
    /// The mask refused the change.
    Mask(ArtifactError),
}

/// What shrinking a mask gave.
#[derive(Debug, Clone, PartialEq)]
pub struct Shrunk {
    /// The mask with its new core, and one more step in its origin.
    pub mask: Mask,
    /// What replacing the core cleared.
    pub cleared: Refinement,
    /// How many pixels the core had.
    pub core_before: u64,
    /// How many it has now.
    pub core_after: u64,
}

/// Shrinks `mask` by the disc of `px`: the rule, and then what every replacement of a
/// core does ([`Mask::replace_core`]). The mask that comes back has one more step in its
/// origin, which names this action, its version, `px` and the mask it was made from.
/// Nothing comes back once `stopped` says so.
pub fn shrink(
    mut mask: Mask,
    px: f64,
    stopped: &dyn Fn() -> bool,
) -> Result<Option<Shrunk>, ShrinkError> {
    let space = mask.space();
    let (width, height) = (space.width() as usize, space.height() as usize);
    let core_before = operation::set(mask.core());
    let from = mask.id();
    let Some(plane) = eroded(mask.core(), width, height, px, stopped) else {
        return Ok(None);
    };
    if operation::set(&plane) == 0 {
        return Err(ShrinkError::Nothing {
            px,
            before: core_before,
        });
    }
    let cleared = mask.replace_core(plane).map_err(ShrinkError::Mask)?;
    let step = Step::new(NAME, VERSION).argument("px", &text_of(px));
    mask.record(step.input("mask", from.0))
        .map_err(ShrinkError::Mask)?;
    let core_after = operation::set(mask.core());
    Ok(Some(Shrunk {
        mask,
        cleared,
        core_before,
        core_after,
    }))
}

/// The action `shrink`.
pub struct Shrink {
    descriptor: ActionDescriptor,
}

impl Shrink {
    pub fn new() -> Shrink {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Shrinks the core of a mask by a disc; a core against the border shrinks from it too.",
        )
        .argument(Argument::required(
            "px",
            "The radius of the disc, in pixels of the mask's image, from 0 to 10000.",
            ValueType::number(Some(0.0), Some(PX_MAX)),
        ))
        .input(InputPort::required(
            "mask",
            ArtifactType::Mask,
            "The mask whose core is shrunk.",
        ))
        .output(OutputPort::one(
            "mask",
            ArtifactType::Mask,
            "The mask with the shrunk core, its blend cleared and its protect kept.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Shrink { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Shrink {
    fn default() -> Shrink {
        Shrink::new()
    }
}

/// Registers the action.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(std::sync::Arc::new(Shrink::new()))
}

/// What `whizbang rascal shrink` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The mask, by its identity in the store.
    #[arg(long)]
    pub mask: String,
    /// The radius of the disc, in pixels of the mask's image, from 0 to 10000.
    #[arg(long)]
    pub px: f64,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        vec![("px", Value::Number(self.px))]
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![("mask", "--mask", self.mask.as_str())]
    }
}

impl Action for Shrink {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let px = operation::number(call, "px")
            .ok_or_else(|| Failure::error("argument", "the call gives no number px"))?;
        let read = operation::mask(context, operation::one(call, "mask")?)?;
        let stopped = || operation::stopped(context);
        let shrunk = shrink(read, px, &stopped).map_err(|error| match error {
            ShrinkError::Nothing { px, before } => Failure::invalid(&format!(
                "shrinking by {px} px leaves no core ({before} core pixels before)"
            )),
            ShrinkError::Mask(error) => Failure::error("mask", &error.without_paths()),
        })?;
        let Some(shrunk) = shrunk else {
            return Err(operation::stopped_failure(context, NAME));
        };
        context.events().progress(&format!(
            "shrink: the core went from {} to {} pixels; the blend is cleared",
            shrunk.core_before, shrunk.core_after
        ));
        Ok(Produced::nothing().with("mask", shrunk.mask))
    }
}
