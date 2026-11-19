//! `put`: a cut's canvas written back onto its panorama (`SPEC.md` section 7): the
//! canvas's left half onto the panorama's last `s` columns and its right half onto the
//! first `s`; no other pixel changes, and a canvas put back unchanged gives the panorama
//! itself.
//!
//! Ported from rascal `crates/rascal-ops/src/put.rs` at the commit `docs/sources.toml`
//! pins. Rascal knew a canvas was this panorama's cut by its frame, named after the image
//! and the plan. An image here has no frame, so `put` takes the mask the canvas was worked
//! through as well: on the canvas the plan cuts from this panorama, or rebound onto the
//! canvas given (as `fill` hands one back, after `apply` or not). The canvas given must be that cut
//! wherever the mask weighs nothing, outside its core and blend or under protect; so
//! every pixel of the panorama the mask never reaches, and every protected one, is the
//! panorama's (plan m3, task O2, item 3). A plan made for another panorama, a canvas of
//! another size and a mask on another canvas are refused, as rascal refused them.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Mask, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Context, Failure, InputPort, Kind, OutputPort, PreparedCall,
    Produced, RegistrationError, Registry, Value,
};
use whizbang_core::identity::{CanonicalPixels, Destination, Mapping, PixelDigest, Space};

use crate::{canvas, cut, operation, paste};

/// The action's name.
pub const NAME: &str = "put";

/// The action's version, which a panorama it writes records in its origin.
pub const VERSION: &str = "1.1.0";

/// What the command says the action does.
pub const ABOUT: &str = "Put a cut's canvas back onto its panorama: its two halves onto the panorama's last and \
first strips, and nothing else changed. The canvas must be the cut wherever the mask it was \
worked through weighs nothing. The panorama is committed to the store, and its identity \
printed.";

/// `canvas` written back onto `panorama` through a cut of strip `s`: the canvas's left
/// half onto the last `s` columns, its right half onto the first `s`.
pub fn put_back(
    panorama: &CanonicalPixels,
    canvas: &CanonicalPixels,
    s: u32,
) -> Result<CanonicalPixels, Failure> {
    let (w, h) = (panorama.width(), panorama.height());
    let left = canvas::crop(canvas, [0, 0, s, h])?;
    let right = canvas::crop(canvas, [s, 0, s, h])?;
    let once = canvas::written(panorama, &left, w - s, 0)?;
    canvas::written(&once, &right, 0, 0)
}

/// Whether the mask's planes are those of the cut canvas `cut`: a mask on it, or one
/// rebound by the identity onto the canvas `given`, from the cut or from an image derived
/// from it, as `apply` and then `fill` hand one back (task O3). A rebound mask names only
/// the image it was rebound from, so what holds the canvas to the cut is the check that
/// follows: the canvas is the cut wherever the mask weighs nothing.
fn on_the_cut(mask: &Mask, cut: PixelDigest, given: PixelDigest, space: Space) -> bool {
    let binding = mask.binding();
    let rebound = matches!(binding.destination(), Destination::Image(_))
        && *binding.mapping() == Mapping::Identity
        && mask.image() == given;
    mask.space() == space && (mask.image() == cut || rebound)
}

/// The action `put`.
pub struct Put {
    descriptor: ActionDescriptor,
}

impl Put {
    pub fn new() -> Put {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Puts a cut's canvas back onto its panorama; nothing outside the two strips changes.",
        )
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The panorama the plan was made for.",
        ))
        .input(InputPort::required(
            "canvas",
            ArtifactType::Image,
            "The canvas, the cut wherever the mask weighs nothing.",
        ))
        .input(InputPort::required(
            "plan",
            ArtifactType::Plan,
            "The plan cut handed back.",
        ))
        .input(InputPort::required(
            "mask",
            ArtifactType::Mask,
            "The mask the canvas was worked through: on the cut, or rebound from it onto the canvas.",
        ))
        .output(OutputPort::one(
            "image",
            ArtifactType::Image,
            "The panorama with the canvas put back, or the panorama when nothing changed.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Put { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Put {
    fn default() -> Put {
        Put::new()
    }
}

/// Registers `put`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Put::new()))
}

/// What `whizbang rascal put` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The panorama, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The canvas, by its identity in the store.
    #[arg(long)]
    pub canvas: String,
    /// The plan cut handed back, by its identity in the store.
    #[arg(long)]
    pub plan: String,
    /// The mask the canvas was worked through, by its identity in the store.
    #[arg(long)]
    pub mask: String,
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
            ("image", "--image", self.image.as_str()),
            ("canvas", "--canvas", self.canvas.as_str()),
            ("plan", "--plan", self.plan.as_str()),
            ("mask", "--mask", self.mask.as_str()),
        ]
    }
}

impl Action for Put {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let ids = (
            canvas::one(call, "image")?,
            canvas::one(call, "canvas")?,
            canvas::one(call, "plan")?,
            canvas::one(call, "mask")?,
        );
        let (image, panorama) = paste::picture(context, ids.0)?;
        let (_, given) = paste::picture(context, ids.1)?;
        let plan = canvas::plan(context, ids.2)?;
        let mask = operation::mask(context, ids.3)?;
        let space = image.space();
        let made_for = plan.binding();
        if (made_for.source(), made_for.source_space()) != (panorama.digest(), space) {
            return Err(Failure::invalid(&format!(
                "the plan was made for the panorama {}, not for this one",
                made_for.source()
            )));
        }
        let s = plan.strip();
        if (given.width(), given.height()) != (2 * s, space.height()) {
            return Err(Failure::invalid(&format!(
                "the canvas is {}x{} but the plan's canvas is {}x{}",
                given.width(),
                given.height(),
                2 * s,
                space.height()
            )));
        }
        let cut = cut::canvas_of(panorama.pixels(), s)?;
        let flat = Space::flat(2 * s, space.height())
            .map_err(|error| Failure::invalid(&error.to_string()))?;
        if !on_the_cut(&mask, cut.digest(), given.digest(), flat) {
            return Err(Failure::invalid(
                "the mask is neither on the canvas this plan cuts from the panorama nor rebound onto the canvas given",
            ));
        }
        canvas::kept(&cut, given.pixels(), &mask, "the canvas")?;
        if operation::stopped(context) {
            return Err(operation::stopped_failure(context, NAME));
        }
        let pixels = put_back(panorama.pixels(), given.pixels(), s)?;
        let out = if pixels == *panorama.pixels() {
            image
        } else {
            let step = Step::new(NAME, VERSION)
                .input("image", ids.0.0)
                .input("canvas", ids.1.0)
                .input("plan", ids.2.0)
                .input("mask", ids.3.0);
            canvas::image(&pixels, space, step)?
        };
        context.events().progress(&format!(
            "put: a {}x{} canvas back onto the {}x{} panorama",
            2 * s,
            space.height(),
            space.width(),
            space.height()
        ));
        Ok(Produced::nothing().with("image", out))
    }
}
