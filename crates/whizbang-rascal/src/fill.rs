//! `fill`: an image and a mask in; the fill capability paints the mask's support, and the
//! answer is pasted by `paste`'s code (plan m3, task F1, item 5; `SPEC.md` section 7).
//! The composite and the mask rebound onto it come out.
//!
//! **What the filler is sent** is the image and the mask's support (`Support::of`: every
//! pixel the mask weighs over 0), through the handle the executor hands the run, so its
//! cancellation, deadline, admission, gate and record are the executor's; the action never
//! names a backend itself. The backend is the default for fill, or the one the call names
//! in `backend`.
//!
//! **What is handed back:** the composite, an image derived from the input, whose every
//! pixel outside the support and every protected one is the original's, and the mask
//! rebound onto it through the derivation `Derivation::checked` made of the two pictures
//! (ADR 0006, "Who vouches for a derivation"). Before the rebind the mask records a step
//! of this action, so its origin names it. **An unchanged result** (a fill answered with
//! the picture it was sent) has no derivation: the input image and the input mask are
//! handed back as they were, the mask keeping its binding.
//!
//! **What the run keeps of the answer** (ADR 0048, "What the store keeps for replay"):
//! the composite cannot be undone into the picture the filler painted, so the run keeps
//! that picture as the diagnostic [`ANSWER`], its red, green and blue with an opaque
//! alpha, written by the core's one encoder as a canonical image's PNG and kept exactly
//! or not at all; only an answer of the image's size is kept, so it is at most the
//! image's size. A replay answers the fill from it.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Step};
use whizbang_core::capability::{FillRequest, Painted, Support};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Capability, Context, Failure, InputPort, Kind, OutputPort,
    PortRef, PreparedCall, Produced, RegistrationError, Registry, Requirement, ValueType,
};

use whizbang_core::identity::{CanonicalPixels, encode};

use crate::operation;
use crate::paste::{pasted, picture};

/// The action's name.
pub const NAME: &str = "fill";

/// The action's version, which a composite and a rebound mask record in their origins.
pub const VERSION: &str = "1.0.0";

/// The diagnostic that keeps the picture the filler painted (ADR 0048).
pub const ANSWER: &str = "fill-answer.png";

/// The picture a filler painted as a canonical image's PNG: its red, green and blue with
/// an opaque alpha. Why not, as a sentence.
pub fn answer_png(painted: &Painted) -> Result<Vec<u8>, String> {
    let rgba: Vec<u8> = (painted.rgb().chunks(3))
        .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
        .collect();
    let pixels = CanonicalPixels::from_rgba(painted.width(), painted.height(), rgba)
        .map_err(|error| error.to_string())?;
    encode::png(&pixels)
}

/// The action `fill`.
pub struct Fill {
    descriptor: ActionDescriptor,
}

impl Fill {
    pub fn new() -> Fill {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Fills what a mask covers in an image with the configured filler, and rebinds the mask onto the result.",
        )
        .argument(Argument::optional(
            "backend",
            "The filler, by its name in the config; the default when not given.",
            ValueType::Text,
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The picture to fill.",
        ))
        .input(
            InputPort::required(
                "mask",
                ArtifactType::Mask,
                "The mask on the picture: the filler paints where it weighs over 0.",
            )
            .on("image"),
        )
        .output(
            OutputPort::one(
                "image",
                ArtifactType::Image,
                "The composite, or the picture when the fill changed nothing.",
            )
            .derived_from(PortRef::input("image")),
        )
        .output(
            OutputPort::one(
                "mask",
                ArtifactType::Mask,
                "The mask, rebound onto the composite.",
            )
            .on(PortRef::input("image"))
            .rebound_onto("image"),
        )
        .needs(Requirement::new(Capability::Fill).chosen_by("backend"))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Fill { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Fill {
    fn default() -> Fill {
        Fill::new()
    }
}

/// Registers `fill`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Fill::new()))
}

impl Action for Fill {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let one = |port: &str| {
            operation::given(call, port)
                .first()
                .copied()
                .ok_or_else(|| Failure::error("input", &format!("the call gives no {port}")))
        };
        let (image_id, mask_id) = (one("image")?, one("mask")?);
        let filler = context
            .capabilities()
            .fill()
            .ok_or_else(|| Failure::error("fill", "the run was handed no filler"))?;
        let (base, picture) = picture(context, image_id)?;
        let mask = operation::mask(context, mask_id)?;
        let support = Support::of(&mask);
        let request = FillRequest::new(picture.clone(), support, None)
            .map_err(|unfit| Failure::invalid(&unfit.to_string()))?;
        context
            .events()
            .progress("fill: read the image and the mask");
        let painted = filler
            .fill(&request)
            .map_err(|fault| Failure::error("fill", fault.sentence()))?;
        // The answer, kept for replay, when it is the image's size: the paste below
        // refuses any other. It is best kept: the run is what it is either way.
        if (painted.width(), painted.height()) == (picture.width(), picture.height()) {
            let kept = answer_png(&painted);
            let _ = kept.and_then(|file| context.diagnostics().write_exact(ANSWER, &file));
        }
        if operation::stopped(context) {
            return Err(operation::stopped_failure(context, NAME));
        }
        let step = Step::new(NAME, VERSION)
            .input("image", image_id.0)
            .input("mask", mask_id.0);
        let pasted = pasted(&base, &picture, &painted, &mask, step.clone())?;
        let mask = match &pasted.derivation {
            None => mask,
            Some(derivation) => {
                let mut recorded = mask;
                recorded
                    .record(step)
                    .and_then(|()| recorded.rebind(derivation))
                    .map_err(|error| Failure::error("mask", &error.without_paths()))?
            }
        };
        context
            .events()
            .progress(&format!("fill: {} pixels painted", pasted.painted));
        Ok(Produced::nothing()
            .with("image", pasted.image)
            .with("mask", mask))
    }
}
