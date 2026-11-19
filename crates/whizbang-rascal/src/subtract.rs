//! `subtract` of a mask: protect is made larger by a second mask's support, its core or
//! its blend, grown by a margin (`SPEC.md` section 7). The core and the blend are cleared
//! under the new protect; the profile and the prescriptions stay, since what applies
//! them honours protect ([`Mask::protect_union`]).
//!
//! With no second mask, the mask is handed back unchanged, the same artifact: so a recipe
//! whose optional `protect` was left out runs every step, and reads the same either way.
//!
//! # The rule, in whole numbers
//!
//! The second mask's *support* is every pixel where its core or its blend is not 0. The
//! support is grown by `grow_px`, as `grow` grows a core with no `down_px`
//! ([`crate::grow::grown`]): every pixel whose centre is within the whole part of
//! `grow_px * grow_px`, squared exactly, of a support pixel's centre. Every grown pixel
//! joins protect.
//!
//! Ported from rascal `crates/rascal-ops/src/subtract.rs`, the form that takes a mask, at
//! the commit `docs/sources.toml` pins; the form that takes a location needs `rasterize`,
//! and is ported with it. The disc is `grow`'s, with `grow`'s deliberate differences
//! from rascal's distance transform (`crate::grow`). A second mask on another image is
//! refused as rascal refused one in another frame. Rascal handed back a mask under a new
//! id when it had nothing to protect; here the mask is the same artifact.

use whizbang_core::artifact::{ArtifactError, ArtifactType, Mask, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};

use crate::grow::{PX_MAX, grown, text_of};
use crate::operation;

/// The action's name.
pub const NAME: &str = "subtract";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// `grow_px` when a call does not give it: rascal's.
pub const GROW_PX_DEFAULT: f64 = 0.0;

/// What the command says the action does.
pub const ABOUT: &str = "Protect the pixels of a second mask: its core and its blend, grown by a margin, join \
protect, and the core and the blend are cleared under it. The profile and the prescriptions \
are kept. With no second mask the mask is handed back unchanged. The mask is committed to \
the store, and its identity is printed.";

/// Why a mask could not be subtracted.
#[derive(Debug)]
pub enum SubtractError {
    /// What the call gave cannot be used: the second mask is on another image.
    Invalid(String),
    /// The mask refused the change.
    Mask(ArtifactError),
}

/// What subtracting gave.
#[derive(Debug, Clone, PartialEq)]
pub struct Subtracted {
    /// The mask with the larger protect, and one more step in its origin; or the mask as
    /// it was given, when there was no second mask.
    pub mask: Mask,
    /// How many pixels were protected before.
    pub protect_before: u64,
    /// How many are protected now.
    pub protect_after: u64,
}

/// Subtracts `other` from `mask`: protect grows by `other`'s support, grown by
/// `grow_px`. The mask that comes back has one more step in its origin, which names this
/// action, its version, `grow_px` and both masks. With no `other` the mask comes back as
/// it was given. Nothing comes back once `stopped` says so.
pub fn subtract(
    mut mask: Mask,
    other: Option<&Mask>,
    grow_px: f64,
    stopped: &dyn Fn() -> bool,
) -> Result<Option<Subtracted>, SubtractError> {
    let protect_before = operation::set(mask.protect());
    let Some(other) = other else {
        return Ok(Some(Subtracted {
            mask,
            protect_before,
            protect_after: protect_before,
        }));
    };
    other
        .require_image(mask.image(), mask.space())
        .map_err(|error| SubtractError::Invalid(error.without_paths()))?;
    let space = mask.space();
    let (width, height) = (space.width() as usize, space.height() as usize);
    let support: Vec<u8> = other
        .core()
        .iter()
        .zip(other.blend())
        .map(|(core, blend)| if *core != 0 || *blend != 0 { 255 } else { 0 })
        .collect();
    let Some(plane) = grown(&support, width, height, grow_px, 0.0, stopped) else {
        return Ok(None);
    };
    let from = mask.id();
    mask.protect_union(&plane).map_err(SubtractError::Mask)?;
    let step = Step::new(NAME, VERSION)
        .argument("grow_px", &text_of(grow_px))
        .input("mask", from.0)
        .input("subtract", other.id().0);
    mask.record(step).map_err(SubtractError::Mask)?;
    let protect_after = operation::set(mask.protect());
    Ok(Some(Subtracted {
        mask,
        protect_before,
        protect_after,
    }))
}

/// The action `subtract`.
pub struct Subtract {
    descriptor: ActionDescriptor,
}

impl Subtract {
    pub fn new() -> Subtract {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Protects the pixels of a second mask, grown by a margin; with none the mask is handed back unchanged.",
        )
        .argument(Argument::with_default(
            "grow_px",
            "How far the second mask's support is grown, in pixels of the mask's image, from 0 to 10000.",
            ValueType::number(Some(0.0), Some(PX_MAX)),
            Value::Number(GROW_PX_DEFAULT),
        ))
        .input(InputPort::required(
            "mask",
            ArtifactType::Mask,
            "The mask to protect pixels in.",
        ))
        .input(InputPort::optional(
            "subtract",
            ArtifactType::Mask,
            "A mask on the same image whose core and blend become protect.",
        ))
        .output(OutputPort::one(
            "mask",
            ArtifactType::Mask,
            "The mask with the larger protect, its core and blend cleared there.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Subtract { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Subtract {
    fn default() -> Subtract {
        Subtract::new()
    }
}

/// Registers the action.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(std::sync::Arc::new(Subtract::new()))
}

/// What `whizbang rascal subtract` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The mask, by its identity in the store.
    #[arg(long)]
    pub mask: String,
    /// The mask whose core and blend are protected, by its identity in the store: on the
    /// same image. Without it the mask is handed back unchanged.
    #[arg(long)]
    pub subtract: Option<String>,
    /// How far the second mask's support is grown, in pixels of the mask's image, from 0
    /// to 10000.
    #[arg(long, default_value_t = GROW_PX_DEFAULT)]
    pub grow_px: f64,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        vec![("grow_px", Value::Number(self.grow_px))]
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        let mut inputs = vec![("mask", "--mask", self.mask.as_str())];
        if let Some(subtract) = &self.subtract {
            inputs.push(("subtract", "--subtract", subtract.as_str()));
        }
        inputs
    }
}

impl Action for Subtract {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let grow_px = operation::number(call, "grow_px").unwrap_or(GROW_PX_DEFAULT);
        let Some(id) = operation::given(call, "mask").first().copied() else {
            return Err(Failure::error("input", "the call gives no mask"));
        };
        let read = operation::mask(context, id)?;
        let other = match operation::given(call, "subtract").first() {
            Some(other) => Some(operation::mask(context, *other)?),
            None => None,
        };
        let stopped = || operation::stopped(context);
        let subtracted =
            subtract(read, other.as_ref(), grow_px, &stopped).map_err(|e| match e {
                SubtractError::Invalid(why) => Failure::invalid(&why),
                SubtractError::Mask(error) => Failure::error("mask", &error.without_paths()),
            })?;
        let Some(subtracted) = subtracted else {
            return Err(operation::stopped_failure(context, NAME));
        };
        context.events().progress(&match other {
            Some(_) => format!(
                "subtract: protect went from {} to {} pixels",
                subtracted.protect_before, subtracted.protect_after
            ),
            None => "subtract: no mask to subtract; the mask is handed back unchanged".to_owned(),
        });
        Ok(Produced::nothing().with("mask", subtracted.mask))
    }
}
