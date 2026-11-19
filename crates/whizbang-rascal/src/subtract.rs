//! `subtract`: protect is made larger by a second mask's support, its core or its
//! blend, or by a location's inside, grown by a margin (`SPEC.md` section 7). The core and the blend are cleared
//! under the new protect; the profile and the prescriptions stay, since what applies
//! them honours protect ([`Mask::protect_union`]).
//!
//! At most one of the second mask and the location is given. With neither, the mask is
//! handed back unchanged, the same artifact: so a recipe
//! whose optional `protect` was left out runs every step, and reads the same either way.
//!
//! # The rule, in whole numbers
//!
//! The second mask's *support* is every pixel where its core or its blend is not 0. The
//! support is grown by `grow_px`, as `grow` grows a core with no `down_px`
//! ([`crate::grow::grown`]): every pixel whose centre is within the whole part of
//! `grow_px * grow_px`, squared exactly, of a support pixel's centre. Every grown pixel
//! joins protect. A location's inside is the plane `rasterize` draws of it
//! ([`crate::rasterize::location_core`]), grown the same way.
//!
//! Ported from rascal `crates/rascal-ops/src/subtract.rs` at the commit
//! `docs/sources.toml` pins; the form that takes a location came with `rasterize` (plan
//! m3, task O1, item 1). Both given are refused, as rascal refused them. A location on
//! another image, or one with no inside, is refused, as rascal refused one in another
//! frame or said it did not apply. The disc is `grow`'s, with `grow`'s deliberate differences
//! from rascal's distance transform (`crate::grow`). A second mask on another image is
//! refused as rascal refused one in another frame. Rascal handed back a mask under a new
//! id when it had nothing to protect; here the mask is the same artifact.

use whizbang_core::artifact::{ArtifactError, ArtifactType, Location, Mask, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};

use crate::grow::{PX_MAX, grown, text_of};
use crate::operation;
use crate::rasterize::{self, location_core};

/// The action's name.
pub const NAME: &str = "subtract";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.1.0";

/// `grow_px` when a call does not give it: rascal's.
pub const GROW_PX_DEFAULT: f64 = 0.0;

/// What the command says the action does.
pub const ABOUT: &str = "Protect the pixels of a second mask, its core and its blend, or of a location's inside: \
grown by a margin, they join protect, and the core and the blend are cleared under them. The \
profile and the prescriptions are kept. With neither the mask is handed back unchanged. The mask is committed to \
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

/// What is subtracted from a mask: a second mask, or a location.
#[derive(Debug, Clone, Copy)]
pub enum Other<'a> {
    /// A mask on the same image, whose core and blend are protected.
    Mask(&'a Mask),
    /// A location on the same image, whose inside is protected.
    Location(&'a Location),
}

/// Subtracts `other` from `mask`: protect grows by `other`'s support, or its inside,
/// grown by `grow_px`. The mask that comes back has one more step in its origin, which
/// names this action, its version, `grow_px`, the mask and what was subtracted. With no
/// `other` the mask comes back as it was given. Nothing comes back once `stopped` says
/// so.
pub fn subtract(
    mut mask: Mask,
    other: Option<Other<'_>>,
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
    let space = mask.space();
    let (width, height) = (space.width() as usize, space.height() as usize);
    let (support, port, id) = match other {
        Other::Mask(other) => {
            other
                .require_image(mask.image(), mask.space())
                .map_err(|error| SubtractError::Invalid(error.without_paths()))?;
            let support: Vec<u8> = other
                .core()
                .iter()
                .zip(other.blend())
                .map(|(core, blend)| if *core != 0 || *blend != 0 { 255 } else { 0 })
                .collect();
            (support, "subtract", other.id())
        }
        Other::Location(location) => {
            let binding = location.binding();
            if binding.source() != mask.image() || binding.source_space() != space {
                return Err(SubtractError::Invalid(format!(
                    "the location is on the image {}, and the mask on {}",
                    binding.source(),
                    mask.image()
                )));
            }
            let shape = location.shape();
            let inside = location_core(shape, space.width(), space.height()).ok_or_else(|| {
                SubtractError::Invalid(format!(
                    "a {} location has no inside to protect",
                    rasterize::kind(shape)
                ))
            })?;
            (inside, "location", location.id())
        }
    };
    let Some(plane) = grown(&support, width, height, grow_px, 0.0, stopped) else {
        return Ok(None);
    };
    let from = mask.id();
    mask.protect_union(&plane).map_err(SubtractError::Mask)?;
    let step = Step::new(NAME, VERSION)
        .argument("grow_px", &text_of(grow_px))
        .input("mask", from.0)
        .input(port, id.0);
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
            "Protects the pixels of a second mask or a location, grown by a margin; with neither the mask is unchanged.",
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
        .input(InputPort::optional(
            "location",
            ArtifactType::Location,
            "A location on the same image whose inside becomes protect; not with subtract.",
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
    #[arg(long, conflicts_with = "location")]
    pub subtract: Option<String>,
    /// A location whose inside is protected, by its identity in the store: on the same
    /// image.
    #[arg(long)]
    pub location: Option<String>,
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
        if let Some(location) = &self.location {
            inputs.push(("location", "--location", location.as_str()));
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
        let location = match operation::given(call, "location").first() {
            Some(location) => Some(operation::location(context, *location)?),
            None => None,
        };
        let given = match (&other, &location) {
            (Some(_), Some(_)) => {
                return Err(Failure::invalid("give subtract or location, not both"));
            }
            (Some(other), None) => Some(Other::Mask(other)),
            (None, Some(location)) => Some(Other::Location(location)),
            (None, None) => None,
        };
        let stopped = || operation::stopped(context);
        let subtracted = subtract(read, given, grow_px, &stopped).map_err(|e| match e {
            SubtractError::Invalid(why) => Failure::invalid(&why),
            SubtractError::Mask(error) => Failure::error("mask", &error.without_paths()),
        })?;
        let Some(subtracted) = subtracted else {
            return Err(operation::stopped_failure(context, NAME));
        };
        context.events().progress(&match given {
            Some(_) => format!(
                "subtract: protect went from {} to {} pixels",
                subtracted.protect_before, subtracted.protect_after
            ),
            None => "subtract: nothing to subtract; the mask is handed back unchanged".to_owned(),
        });
        Ok(Produced::nothing().with("mask", subtracted.mask))
    }
}
