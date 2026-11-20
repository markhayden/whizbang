//! `carry`: a mask on a crop becomes a mask on the picture the crop was cut from (plan
//! m4, task L2, item 1). Every plane keeps its pixels, moved by the crop's corner, and
//! nothing is set outside the crop. It is the explicit, checked rebind a mask needs to
//! be used on another image than its own (`SPEC.md` section 3, "Identities"; ADR 0002):
//! `segment` on a crop gives masks that belong to the crop, and the loop
//! `remove_checked` joins them to a mask on the picture.
//!
//! **Checked** means both pictures are looked at ([`Placed::checked`]): the crop, laid
//! on the picture with its corner at `left`, `top`, lies inside it, and each of its
//! pixels is the picture's pixel there. A crop of another picture, or of this picture
//! from another corner, is refused. So the statement cannot be made of two unrelated
//! pictures, unless the picture happens to hold the crop's pixels at that corner, where
//! a mask on the one is a mask on the other.
//!
//! Ported from rascal `crates/rascal-ops/src/invert.rs` (`map_plane_to_parent`, the arm
//! of a crop) at the commit `docs/sources.toml` pins, with these differences, each on
//! purpose:
//!
//! 1. **The crop is given, and compared.** Rascal's crop was a frame that named its
//!    parent and its box, and nothing was compared. Here an image names no parent (ADR
//!    0001), so the call says where the crop sits and the pixels are held to it.
//! 2. **A crop that reaches outside the picture is refused,** where rascal dropped the
//!    pixels that fell outside.
//! 3. **A crop that `view` scaled down is not carried.** Its pixels are averages, not
//!    the picture's, so the comparison refuses it: `view` scales a cut whose longer side
//!    is over 1,024 pixels, and rascal's crops were never scaled.
//! 4. **The arm of a cut canvas is not here.** No shipped recipe inverts or carries a
//!    mask on a cut (`docs/carry-over.md`).
//!
//! A mask with a feather's profile or a prescription is refused: both are series down
//! the rows of the mask's own image, and are made after a mask is where it will be used.

use whizbang_core::artifact::{ArtifactError, ArtifactType, Image, Mask, Origin, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Count, Failure, InputPort, Kind, OutputPort,
    PortRef, PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};
use whizbang_core::identity::{Binding, DecodeLimits, Digest, PixelDigest, Space};

use crate::operation;

/// The action's name.
pub const NAME: &str = "carry";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// The most masks one call carries: the most `select` joins.
pub const MASKS_MAX: u32 = crate::select::MASKS_MAX;

/// What the command says the action does.
pub const ABOUT: &str = "Carry masks from a crop onto the picture the crop was cut from: every plane keeps its \
pixels, moved by the crop's corner, and nothing is set outside the crop. The crop must be the \
picture's own pixels at that corner, as view cuts them when it scales nothing: view prints the \
corner as the left and top of the crop's binding. Each new mask is committed to the store, and \
its identity is printed.";

/// Why a mask was not carried.
#[derive(Debug)]
pub enum CarryError {
    /// The call cannot be worked on: the crop is not of the picture there, or the mask
    /// is not on the crop.
    Invalid(String),
    /// A picture could not be read, or the mask could not be made.
    Mask(ArtifactError),
}

fn invalid<T>(why: String) -> Result<T, CarryError> {
    Err(CarryError::Invalid(why))
}

/// The statement that a crop is a picture's own pixels at a corner, made by looking at
/// both ([`Placed::checked`]) and in no other way.
#[derive(Debug, Clone)]
pub struct Placed {
    crop: (Digest, PixelDigest, Space),
    image: (Digest, Binding),
    at: [u32; 2],
}

impl Placed {
    /// The crop laid on `image` with its corner at `at`, left and top, in pixels of the
    /// image. Refused: the picture itself as its own crop; a crop that reaches outside
    /// the picture; and a crop any pixel of which is not the picture's there, with how
    /// many differ and where the first is.
    pub fn checked(
        crop: &Image,
        image: &Image,
        at: [u32; 2],
        limits: &DecodeLimits,
    ) -> Result<Placed, CarryError> {
        if crop.identity().pixels == image.identity().pixels {
            return invalid(
                "the crop is the picture itself, and a mask on it is carried nowhere".to_owned(),
            );
        }
        let (small, large) = (crop.space(), image.space());
        let [left, top] = at.map(u64::from);
        let (width, height) = (u64::from(small.width()), u64::from(small.height()));
        if left + width > u64::from(large.width()) || top + height > u64::from(large.height()) {
            return invalid(format!(
                "a crop of {width} by {height} at {left}, {top} reaches outside the picture of {} by {}",
                large.width(),
                large.height()
            ));
        }
        let cut = crop.pixels(limits).map_err(CarryError::Mask)?;
        let whole = image.pixels(limits).map_err(CarryError::Mask)?;
        let (width, across) = (width as usize, large.width() as usize);
        let (left, top) = (left as usize, top as usize);
        let (mut differ, mut first) = (0_u64, None);
        for (y, row) in cut.rgba().chunks(width * 4).enumerate() {
            let from = ((top + y) * across + left) * 4;
            let there = &whole.rgba()[from..from + width * 4];
            for (x, (one, other)) in row.chunks(4).zip(there.chunks(4)).enumerate() {
                if one != other {
                    differ += 1;
                    first.get_or_insert((x, y));
                }
            }
        }
        if let Some((x, y)) = first {
            return invalid(format!(
                "the crop is not the picture's own pixels at {left}, {top}: {differ} of its pixels \
                 differ, the first at {x}, {y} of the crop"
            ));
        }
        Ok(Placed {
            crop: (crop.id().0, crop.identity().pixels, small),
            image: (image.id().0, image.binding()),
            at,
        })
    }

    /// `mask`, which is on the crop, as a mask on the picture: its core, its blend and
    /// its protect where the crop sits, and nothing set outside it; made for the same
    /// use, from the same locations and instances; its origin the mask's with one more
    /// step, this action, the corner, and the mask, the crop and the picture as inputs.
    pub fn carry(&self, mask: &Mask) -> Result<Mask, CarryError> {
        let (crop_id, crop, small) = &self.crop;
        if let Err(error) = mask.require_image(*crop, *small) {
            return invalid(format!(
                "the mask is not on the crop: {}",
                error.without_paths()
            ));
        }
        if mask.profile().is_some() || !mask.prescriptions().is_empty() {
            return invalid(
                "the mask holds a feather's profile or a prescription, which are made on the \
                 picture a mask is used on: carry it before it is feathered"
                    .to_owned(),
            );
        }
        let (image_id, binding) = &self.image;
        let large = binding.source_space();
        let (across, width) = (large.width() as usize, small.width() as usize);
        let [left, top] = self.at.map(|side| side as usize);
        let moved = |plane: &[u8]| {
            let mut out = vec![0_u8; across * large.height() as usize];
            for (y, row) in plane.chunks(width).enumerate() {
                let from = (top + y) * across + left;
                out[from..from + width].copy_from_slice(row);
            }
            out
        };
        let step = Step::new(NAME, VERSION)
            .argument("left", &self.at[0].to_string())
            .argument("top", &self.at[1].to_string())
            .input("mask", mask.id().0)
            .input("crop", *crop_id)
            .input("image", *image_id);
        let origin: Origin = (mask.origin().clone().then(step)).map_err(CarryError::Mask)?;
        let mut carried = Mask::from_core(
            binding.clone(),
            mask.use_case(),
            moved(mask.core()),
            mask.source().clone(),
            origin,
        )
        .map_err(CarryError::Mask)?;
        (carried.protect_union(&moved(mask.protect()))).map_err(CarryError::Mask)?;
        (carried.set_blend(moved(mask.blend()))).map_err(CarryError::Mask)?;
        Ok(carried)
    }
}

/// The corner a call gives as `left` and `top`: both, or neither.
pub(crate) fn corner(call: &PreparedCall) -> Result<Option<[u32; 2]>, Failure> {
    let side = |name: &str| -> Result<Option<u32>, Failure> {
        let given = call.arguments().iter().find(|(given, _)| given == name);
        match given {
            None => Ok(None),
            Some((_, Value::Integer(side))) => u32::try_from(*side).map(Some).map_err(|_| {
                Failure::invalid(&format!("{name} is a whole number of pixels, from 0"))
            }),
            Some(_) => Err(Failure::invalid(&format!(
                "{name} is a whole number of pixels"
            ))),
        }
    };
    match (side("left")?, side("top")?) {
        (Some(left), Some(top)) => Ok(Some([left, top])),
        (None, None) => Ok(None),
        _ => Err(Failure::invalid(
            "left and top are the corner of a crop, and are given together",
        )),
    }
}

/// The failure of a run that could not carry.
pub(crate) fn failed(error: CarryError) -> Failure {
    match error {
        CarryError::Invalid(why) => Failure::invalid(&why),
        CarryError::Mask(error) => Failure::error("mask", &error.without_paths()),
    }
}

/// The two arguments that are a crop's corner.
pub(crate) fn corner_arguments(required: bool) -> [Argument; 2] {
    let pixels = || ValueType::integer(Some(0), None);
    let of = |name: &str, purpose: &str| match required {
        true => Argument::required(name, purpose, pixels()),
        false => Argument::optional(name, purpose, pixels()),
    };
    [
        of(
            "left",
            "The crop's left edge on the picture, in whole pixels.",
        ),
        of(
            "top",
            "The crop's top edge on the picture, in whole pixels.",
        ),
    ]
}

/// The action `carry`.
pub struct Carry {
    descriptor: ActionDescriptor,
}

impl Carry {
    pub fn new() -> Carry {
        let [left, top] = corner_arguments(true);
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Carries masks from a crop onto the picture it was cut from: the same pixels, where the crop sits.",
        )
        .argument(left)
        .argument(top)
        .input(
            InputPort::variadic(
                "mask",
                ArtifactType::Mask,
                "The masks to carry, each on the crop.",
                Count::between(1, MASKS_MAX),
            )
            .on("crop"),
        )
        .input(InputPort::required(
            "crop",
            ArtifactType::Image,
            "The crop the masks are on: the picture's own pixels, not scaled.",
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The picture the crop was cut from.",
        ))
        .output(
            OutputPort::counted(
                "mask",
                ArtifactType::Mask,
                "Each mask on the picture, in the order given, empty outside the crop.",
                Count::between(1, MASKS_MAX),
            )
            .on(PortRef::input("image")),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Carry { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Carry {
    fn default() -> Carry {
        Carry::new()
    }
}

/// Registers the action.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(std::sync::Arc::new(Carry::new()))
}

/// The crop of a call laid on its picture, checked: the images at `crop` and `image`,
/// and the corner the call gives.
pub(crate) fn placed(
    call: &PreparedCall,
    context: &Context,
    at: [u32; 2],
) -> Result<Placed, Failure> {
    let crop = operation::image(context, operation::one(call, "crop")?)?;
    let image = operation::image(context, operation::one(call, "image")?)?;
    Placed::checked(&crop, &image, at, &DecodeLimits::default()).map_err(failed)
}

/// What `whizbang rascal carry` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// A mask on the crop, by its identity in the store; once for each mask.
    #[arg(long = "mask", required = true)]
    pub masks: Vec<String>,
    /// The crop the masks are on, by its identity in the store.
    #[arg(long)]
    pub crop: String,
    /// The picture the crop was cut from, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The crop's left edge on the picture, in whole pixels.
    #[arg(long)]
    pub left: u32,
    /// The crop's top edge on the picture, in whole pixels.
    #[arg(long)]
    pub top: u32,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("left", Value::Integer(i64::from(self.left))),
            ("top", Value::Integer(i64::from(self.top))),
        ]
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        let masks = self
            .masks
            .iter()
            .map(|mask| ("mask", "--mask", mask.as_str()));
        masks
            .chain([
                ("crop", "--crop", self.crop.as_str()),
                ("image", "--image", self.image.as_str()),
            ])
            .collect()
    }
}

impl Action for Carry {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let Some(at) = corner(call)? else {
            return Err(Failure::invalid("the call gives no left and top"));
        };
        let placed = placed(call, context, at)?;
        let mut produced = Produced::nothing();
        let mut set = 0;
        let given = operation::given(call, "mask");
        for id in &given {
            let carried = placed
                .carry(&operation::mask(context, *id)?)
                .map_err(failed)?;
            set += operation::set(carried.core());
            produced = produced.with("mask", carried);
            if operation::stopped(context) {
                return Err(operation::stopped_failure(context, NAME));
            }
        }
        context.events().progress(&format!(
            "carry: {} masks onto the picture, {set} pixels of core",
            given.len()
        ));
        Ok(produced)
    }
}
