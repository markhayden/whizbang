//! `paste`: a fill pasted onto its base image under a mask, as an action (plan m3, task
//! F1, item 5; `SPEC.md` section 7). A base image, a fill of its size and a mask on the
//! base go in; the composite comes out, an image derived from the base.
//!
//! The arithmetic is the composite's ([`crate::composite`], rascal's `feather` paste in
//! `crates/rascal-ops/src/paste.rs` at the commit `docs/sources.toml` pins): every pixel
//! the mask weighs nothing, outside its core and blend or under protect, is the base's
//! four bytes. `mode` is an argument: `feather` (rascal's), by the mask's weight; `hard`
//! (rascal's, task O4), the fill on the core alone; or `poisson` (rascal's, task O4), the
//! fill's gradients solved against the base's values round the domain
//! ([`crate::poisson`]) and the solution blended by the weight, with the numbers of the
//! constant set `poisson-v1`. A picture that wraps is refused under `poisson`, which has
//! no neighbours across the seam, as is a domain with nothing to anchor to; a solve that
//! does not converge is an error.
//!
//! **An unchanged result:** a fill that changes no pixel of the base makes no
//! derivation (`Derivation::checked` refuses identical pictures), and the base itself is
//! handed back. Otherwise the composite is written by the core's one encoder as an image
//! whose origin is one step: this action, its version, the mode and the three inputs.
//! [`pasted`] is the same code for `fill`.

use std::sync::Arc;

use whizbang_core::artifact::{
    self, Artifact, ArtifactType, Derivation, Image, ImageRole, Mask, Origin, Step,
};
use whizbang_core::capability::{Painted, Picture};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort, PortRef,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};
use whizbang_core::identity::{DecodeLimits, Topology, encode};

use crate::composite::{Composite, Mode, composite_in, composite_solved};
use crate::constants::{self, POISSON_V1};
use crate::operation;
use crate::poisson::{self, Solve};

/// The action's name.
pub const NAME: &str = "paste";

/// The action's version, which a composite records in its origin.
pub const VERSION: &str = "1.0.0";

/// The modes of a paste this build has.
pub const MODES: [&str; 3] = ["feather", "hard", "poisson"];

/// The constant sets a call may name, which `poisson` reads.
pub const SETS: [&str; 1] = [POISSON_V1.0];

/// An image read from the run's store, its pixels as a picture. A rendering is refused:
/// it is a picture of another artifact, and nothing is pasted onto it.
pub(crate) fn picture(
    context: &Context,
    id: artifact::ArtifactId,
) -> Result<(Image, Picture), Failure> {
    let limits = DecodeLimits::default();
    let read = artifact::get(context.store(), ArtifactType::Image, id, &limits)
        .map_err(|error| Failure::error("input", &error.without_paths()))?;
    let Artifact::Image(image) = read else {
        return Err(Failure::error("input", "what was read is not an image"));
    };
    if image.role() != ImageRole::Picture {
        return Err(Failure::invalid(
            "the image is a rendering of another artifact, and nothing is pasted onto one",
        ));
    }
    let pixels = image
        .pixels(&limits)
        .map_err(|error| Failure::error("input", &error.without_paths()))?;
    let picture = Picture::new(pixels).map_err(|unfit| Failure::invalid(&unfit.to_string()))?;
    Ok((image, picture))
}

/// What a paste made: the composite as an image, and the derivation it was checked
/// through; or, for an unchanged result, the base image and no derivation.
pub(crate) struct Pasted {
    pub(crate) image: Image,
    pub(crate) derivation: Option<Derivation>,
    /// How many pixels were read from the fill.
    pub(crate) painted: u64,
}

/// `fill` pasted onto `base` under `mask`, made into an image whose origin is `step`.
pub(crate) fn pasted(
    base: &Image,
    picture: &Picture,
    fill: &Painted,
    mask: &Mask,
    step: Step,
) -> Result<Pasted, Failure> {
    pasted_in(base, picture, fill, mask, Mode::Feather, step)
}

/// The same, in `mode`.
pub(crate) fn pasted_in(
    base: &Image,
    picture: &Picture,
    fill: &Painted,
    mask: &Mask,
    mode: Mode,
    step: Step,
) -> Result<Pasted, Failure> {
    let made = composite_in(picture, fill, mask, mode)
        .map_err(|error| Failure::invalid(&error.to_string()))?;
    made_into(base, made, step)
}

/// A composite made into an image whose origin is `step`; for an unchanged result, the
/// base.
fn made_into(base: &Image, made: Composite, step: Step) -> Result<Pasted, Failure> {
    let Some(derivation) = made.derivation().cloned() else {
        return Ok(Pasted {
            image: base.clone(),
            derivation: None,
            painted: made.painted(),
        });
    };
    let file = encode::png(made.pixels()).map_err(|why| Failure::error("encode", &why))?;
    let space = base.space();
    let image = Origin::new(step)
        .and_then(|origin| {
            Image::new(
                file,
                space.projection(),
                space.topology(),
                ImageRole::Picture,
                origin,
                &DecodeLimits::default(),
            )
        })
        .map_err(|error| Failure::error("composite", &error.without_paths()))?;
    Ok(Pasted {
        image,
        derivation: Some(derivation),
        painted: made.painted(),
    })
}

/// `fill`, four bytes a pixel, pasted onto `base` under `mask` in the mode `poisson`, with
/// `poisson-v1`'s numbers: refused for a picture that wraps and a domain with nothing to
/// anchor to; an error when the solve does not converge.
fn solved(
    context: &Context,
    base: &Image,
    picture: &Picture,
    fill: &[u8],
    mask: &Mask,
    step: Step,
) -> Result<Pasted, Failure> {
    if base.space().topology() == Topology::Wrap {
        return Err(Failure::invalid(
            "poisson refuses a picture that wraps: it has no neighbours across the seam; cut the band first",
        ));
    }
    if (picture.width(), picture.height()) != (mask.space().width(), mask.space().height())
        || fill.len() != picture.rgba().len()
    {
        return Err(Failure::invalid(
            "the base, the fill and the mask are not one size",
        ));
    }
    let settings =
        poisson::Settings::of(POISSON_V1.1).map_err(|why| Failure::error("constants", &why))?;
    let stopped = || operation::stopped(context);
    let solve = poisson::solve(picture.rgba(), fill, mask, &settings, &stopped)
        .map_err(|why| Failure::error("poisson", &why))?;
    let (solution, domain_px, iterations) = match solve {
        None => return Err(operation::stopped_failure(context, NAME)),
        Some(Solve::Unanchored { component_px }) => {
            return Err(Failure::invalid(&format!(
                "poisson has nothing to anchor to: the {component_px} pixels the mask weighs cover the whole image"
            )));
        }
        Some(Solve::Solved {
            solution,
            domain_px,
            iterations,
        }) => (solution, domain_px, iterations),
    };
    context.events().progress(&format!(
        "paste: poisson solved {domain_px} pixels in {iterations} iterations"
    ));
    let made = composite_solved(picture, &solution, mask)
        .map_err(|error| Failure::invalid(&error.to_string()))?;
    made_into(base, made, step)
}

/// The action `paste`.
pub struct Paste {
    descriptor: ActionDescriptor,
}

impl Paste {
    pub fn new() -> Paste {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Pastes a fill onto its base image under a mask; every pixel the mask weighs nothing is the base's.",
        )
        .argument(Argument::with_default(
            "mode",
            "How the fill is pasted: feather, by the mask's core and blend; hard, on the core alone; poisson, by the fill's gradients.",
            ValueType::choice(&MODES),
            Value::text("feather"),
        ))
        .argument(Argument::with_default(
            constants::ARGUMENT,
            "The constant set poisson takes its tolerance and its ceiling of iterations from.",
            ValueType::choice(&SETS),
            Value::text(POISSON_V1.0),
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The base image.",
        ))
        .input(InputPort::required(
            "fill",
            ArtifactType::Image,
            "The fill: a picture of the base's size, read only where the mask weighs over 0.",
        ))
        .input(InputPort::required("mask", ArtifactType::Mask, "The mask, on the base.").on("image"))
        .output(
            OutputPort::one(
                "image",
                ArtifactType::Image,
                "The composite, or the base when the fill changed nothing.",
            )
            .derived_from(PortRef::input("image")),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Paste { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Paste {
    fn default() -> Paste {
        Paste::new()
    }
}

/// Registers `paste`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Paste::new()))
}

impl Action for Paste {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let mode = operation::text(call, "mode").unwrap_or("feather");
        let how = Mode::named(mode);
        if how.is_none() && mode != "poisson" {
            return Err(Failure::invalid("the mode is feather, hard or poisson"));
        }
        let set = operation::text(call, constants::ARGUMENT).unwrap_or(POISSON_V1.0);
        if set != POISSON_V1.0 {
            return Err(Failure::invalid(&format!(
                "{set} is not a constant set of paste"
            )));
        }
        let one = |port: &str| {
            operation::given(call, port)
                .first()
                .copied()
                .ok_or_else(|| Failure::error("input", &format!("the call gives no {port}")))
        };
        let (image_id, fill_id, mask_id) = (one("image")?, one("fill")?, one("mask")?);
        let (base, picture) = self::picture(context, image_id)?;
        let (_, fill) = self::picture(context, fill_id)?;
        let mask = operation::mask(context, mask_id)?;
        if operation::stopped(context) {
            return Err(operation::stopped_failure(context, NAME));
        }
        let rgb = fill
            .rgba()
            .chunks(4)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();
        let painted = Painted::from_rgb(fill.width(), fill.height(), rgb)
            .map_err(|unfit| Failure::invalid(&unfit.to_string()))?;
        let step = Step::new(NAME, VERSION)
            .argument("mode", mode)
            .input("image", image_id.0)
            .input("fill", fill_id.0)
            .input("mask", mask_id.0);
        let pasted = match how {
            Some(how) => pasted_in(&base, &picture, &painted, &mask, how, step)?,
            None => {
                let step = step.argument(constants::ARGUMENT, set);
                solved(context, &base, &picture, fill.rgba(), &mask, step)?
            }
        };
        context.events().progress(&format!(
            "paste: {} pixels read from the fill",
            pasted.painted
        ));
        Ok(Produced::nothing().with("image", pasted.image))
    }
}
