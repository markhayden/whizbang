//! `window`: a crop of whole pixels around a mask's support, with context (`SPEC.md`
//! section 7). Nothing is resampled: the image and the mask's three planes are cut by the
//! same box, and `unwindow` writes the crop back into exactly it.
//!
//! The crop is an image of its own. The mask handed back is on it, and its binding places
//! the crop onto the image it was cut from by ADR 0001's crop, with the box's edges: so
//! the mask says where its image sits, and `unwindow` reads the box from it. The mask
//! keeps its use, its source and its origin, with one more step; its profile and its
//! prescriptions do not travel, since they were measured on the whole image, as rascal's
//! did not.
//!
//! **The context** is a share of the support's width and height on each side; a call that
//! does not give it takes the context of its constant set (the argument `constants`, ADR
//! 0033), `window-v1`, whose one value is 0.5.
//!
//! Ported from rascal `crates/rascal-ops/src/window.rs` at the commit `docs/sources.toml`
//! pins: the box is rascal's to the pixel. An empty support and a support larger than
//! `max_side` are refused as invalid, naming the sizes, where rascal answered that
//! `window` did not apply. A window of the whole image keeps the image's projection and
//! topology; any other is flat, as ADR 0001's crop requires.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Mask, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort, PortRef,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};
use whizbang_core::identity::{Binding, CanonicalPixels, Direction, Mapping, Space};

use crate::constants::{self, WINDOW_V1};
use crate::{canvas, operation, paste};

/// The action's name.
pub const NAME: &str = "window";

/// The action's version, which a crop and its mask record in their origins.
pub const VERSION: &str = "1.0.0";

/// The constant sets a call may name.
pub const SETS: [&str; 1] = [WINDOW_V1.0];

/// The largest `max_side` a call may give.
pub const MAX_SIDE_LIMIT: i64 = 1_000_000;

/// What the command says the action does.
pub const ABOUT: &str = "Crop an image and its mask around the mask's support, with context, in whole pixels and \
with nothing resampled. The mask handed back is on the crop and places it onto the image, so \
unwindow can write the crop back. Both are committed to the store, and their identities \
printed.";

/// A support larger than `max_side` on a side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooLarge {
    pub support: [u32; 4],
    pub max_side: u32,
}

/// The window around `support` (left, top, right, bottom, the right and bottom past the
/// last pixel) in an image `width` by `height`: the support grown by `context` times its
/// width and height on each side, in pixels rounded half up, made square at the larger
/// side, capped to `max_side` and then to the image, and slid inside the image, an odd
/// remainder giving the smaller part to the near side. The support is always inside the
/// window. Rascal's `window_bbox`, unchanged.
pub fn window_box(
    support: [u32; 4],
    width: u32,
    height: u32,
    max_side: Option<u32>,
    context: f64,
) -> Result<[u32; 4], TooLarge> {
    let [sx0, sy0, sx1, sy1] = support;
    let (sw, sh) = (sx1 - sx0, sy1 - sy0);
    if let Some(m) = max_side
        && (sw > m || sh > m)
    {
        return Err(TooLarge {
            support,
            max_side: m,
        });
    }
    let half_up = |v: f64| (v + 0.5).floor() as u32;
    let (cx, cy) = (
        half_up(context * f64::from(sw)),
        half_up(context * f64::from(sh)),
    );
    let side = (sw + 2 * cx).max(sh + 2 * cy);
    let side = max_side.map_or(side, |m| side.min(m));
    let (ww, wh) = (side.min(width), side.min(height));
    let place = |s0: u32, s1: u32, w: u32, limit: u32| -> u32 {
        let extra = w - (s1 - s0);
        let x0 = i64::from(s0) - i64::from(extra / 2);
        x0.clamp(0, i64::from(limit - w)) as u32
    };
    let x0 = place(sx0, sx1, ww, width);
    let y0 = place(sy0, sy1, wh, height);
    Ok([x0, y0, x0 + ww, y0 + wh])
}

/// The box of the pixels the mask weighs over 0, or none.
pub fn support_of(mask: &Mask) -> Option<[u32; 4]> {
    let width = mask.space().width() as usize;
    let mut found: Option<[u32; 4]> = None;
    let planes = mask.core().iter().zip(mask.blend()).zip(mask.protect());
    for (at, ((core, blend), protect)) in planes.enumerate() {
        if *protect != 0 || (*core == 0 && *blend == 0) {
            continue;
        }
        let (x, y) = ((at % width) as u32, (at / width) as u32);
        found = Some(match found {
            None => [x, y, x + 1, y + 1],
            Some([l, t, r, b]) => [l.min(x), t.min(y), r.max(x + 1), b.max(y + 1)],
        });
    }
    found
}

/// One plane cut to the box.
fn cut_plane(plane: &[u8], width: u32, [l, t, r, b]: [u32; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(((r - l) * (b - t)) as usize);
    for y in t..b {
        let start = (y * width + l) as usize;
        out.extend_from_slice(&plane[start..start + (r - l) as usize]);
    }
    out
}

/// The mask's planes cut to the box, on `binding`, whose source is the crop. The use, the
/// source and the origin go with them, and one more step.
pub fn crop_mask(
    mask: &Mask,
    bounds: [u32; 4],
    binding: Binding,
    step: Step,
) -> Result<Mask, Failure> {
    let width = mask.space().width();
    let fail = |error: whizbang_core::artifact::ArtifactError| {
        Failure::error("mask", &error.without_paths())
    };
    let mut out = Mask::from_core(
        binding,
        mask.use_case(),
        cut_plane(mask.core(), width, bounds),
        mask.source().clone(),
        mask.origin().clone(),
    )
    .map_err(fail)?;
    out.set_blend(cut_plane(mask.blend(), width, bounds))
        .map_err(fail)?;
    out.protect_union(&cut_plane(mask.protect(), width, bounds))
        .map_err(fail)?;
    out.record(step).map_err(fail)?;
    Ok(out)
}

/// What [`windowed`] cut.
#[derive(Debug, Clone)]
pub struct Windowed {
    /// The mask's support: left, top, right and bottom.
    pub support: [u32; 4],
    /// The window's box, the same way.
    pub bounds: [u32; 4],
    /// The crop's pixels.
    pub pixels: CanonicalPixels,
    /// The crop's space: the image's for a window of the whole image, flat otherwise.
    pub space: Space,
    /// The mask cut to the box, on the crop, placing it onto the image, with `step`.
    pub mask: Mask,
}

/// The window of `pixels` (an image in `space`) around `mask`'s support, as the action
/// cuts it: refused, in a sentence, when the support is empty or larger than `max_side`.
/// `step` is recorded on the mask handed back.
pub fn windowed(
    pixels: &CanonicalPixels,
    space: Space,
    mask: &Mask,
    max_side: Option<u32>,
    share: f64,
    step: Step,
) -> Result<Windowed, Failure> {
    let Some(support) = support_of(mask) else {
        return Err(Failure::invalid(
            "the mask's support is empty: no pixel has an effective weight over 0",
        ));
    };
    let bounds =
        window_box(support, space.width(), space.height(), max_side, share).map_err(|too| {
            let [l, t, r, b] = too.support;
            Failure::invalid(&format!(
                "the support is {}x{} and max_side is {}; a window is never scaled",
                r - l,
                b - t,
                too.max_side
            ))
        })?;
    let [l, t, r, b] = bounds;
    let whole = (r - l, b - t) == (space.width(), space.height());
    let crop_space = if whole {
        space
    } else {
        Space::flat(r - l, b - t).map_err(|error| Failure::invalid(&error.to_string()))?
    };
    let crop = canvas::crop(pixels, [l, t, r - l, b - t])?;
    let placed = Binding::new(
        crop.digest(),
        crop_space,
        pixels.digest(),
        space,
        Direction::SourceToDestination,
        Mapping::Crop {
            left: f64::from(l),
            top: f64::from(t),
            right: f64::from(r),
            bottom: f64::from(b),
        },
    )
    .map_err(|error| Failure::error("binding", &error.to_string()))?;
    let cut = crop_mask(mask, bounds, placed, step)?;
    Ok(Windowed {
        support,
        bounds,
        pixels: crop,
        space: crop_space,
        mask: cut,
    })
}

/// The action `window`.
pub struct Window {
    descriptor: ActionDescriptor,
}

impl Window {
    pub fn new() -> Window {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Crops an image and its mask around the mask's support, with context; the mask handed back places the crop onto the image.",
        )
        .argument(Argument::optional(
            "max_side",
            "The largest side of the window in pixels; a support larger than it is refused. Unset: the image's.",
            ValueType::integer(Some(1), Some(MAX_SIDE_LIMIT)),
        ))
        .argument(Argument::optional(
            "context",
            "The context on each side of the support, as a share of its width and height, from 0 to 4; unset, the constant set's.",
            ValueType::number(Some(0.0), Some(4.0)),
        ))
        .argument(Argument::with_default(
            constants::ARGUMENT,
            "The constant set the context is taken from when the call does not give it.",
            ValueType::choice(&SETS),
            Value::text(WINDOW_V1.0),
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image to window.",
        ))
        .input(
            InputPort::required(
                "mask",
                ArtifactType::Mask,
                "The mask on the image whose support the window is built around.",
            )
            .on("image"),
        )
        .output(OutputPort::one(
            "image",
            ArtifactType::Image,
            "The crop of the image.",
        ))
        .output(
            OutputPort::one(
                "mask",
                ArtifactType::Mask,
                "The mask's planes cut to the crop, on the crop, placing it onto the image.",
            )
            .on(PortRef::output("image")),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Window { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Window {
    fn default() -> Window {
        Window::new()
    }
}

/// Registers `window`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Window::new()))
}

/// What `whizbang rascal window` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The image, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The mask on the image, by its identity in the store.
    #[arg(long)]
    pub mask: String,
    /// The largest side of the window in pixels.
    #[arg(long, value_parser = clap::value_parser!(i64).range(1..=MAX_SIDE_LIMIT))]
    pub max_side: Option<i64>,
    /// The context on each side of the support, as a share of its width and height,
    /// from 0 to 4. Unset: window-v1's, 0.5.
    #[arg(long)]
    pub context: Option<f64>,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        let mut arguments = Vec::new();
        if let Some(max_side) = self.max_side {
            arguments.push(("max_side", Value::Integer(max_side)));
        }
        if let Some(context) = self.context {
            arguments.push(("context", Value::Number(context)));
        }
        arguments
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![
            ("image", "--image", self.image.as_str()),
            ("mask", "--mask", self.mask.as_str()),
        ]
    }
}

impl Action for Window {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let max_side = call
            .arguments()
            .iter()
            .find_map(|(name, value)| match value {
                Value::Integer(side) if name == "max_side" => u32::try_from(*side).ok(),
                _ => None,
            });
        let set = operation::text(call, constants::ARGUMENT).unwrap_or(WINDOW_V1.0);
        if set != WINDOW_V1.0 {
            return Err(Failure::invalid(&format!(
                "{set} is not a constant set of window"
            )));
        }
        let share = match operation::number(call, "context") {
            Some(share) => share,
            None => constants::number(WINDOW_V1.1, "context")
                .ok_or_else(|| Failure::error("constants", "window-v1 has no context"))?,
        };
        let (image_id, mask_id) = (canvas::one(call, "image")?, canvas::one(call, "mask")?);
        let (image, picture) = paste::picture(context, image_id)?;
        let mask = operation::mask(context, mask_id)?;
        let space = image.space();
        mask.require_image(picture.digest(), space)
            .map_err(|error| Failure::invalid(&error.without_paths()))?;
        let mut step = Step::new(NAME, VERSION)
            .argument("context", &share.to_string())
            .argument(constants::ARGUMENT, set);
        if let Some(side) = max_side {
            step = step.argument("max_side", &side.to_string());
        }
        let step = step.input("image", image_id.0).input("mask", mask_id.0);
        let made = windowed(
            picture.pixels(),
            space,
            &mask,
            max_side,
            share,
            step.clone(),
        )?;
        let crop = canvas::image(&made.pixels, made.space, step)?;
        let ([l, t, r, b], support) = (made.bounds, made.support);
        let cropped = made.mask;
        context.events().progress(&format!(
            "window: [{l}, {t}, {r}, {b}) around a {}x{} support",
            support[2] - support[0],
            support[3] - support[1]
        ));
        Ok(Produced::nothing()
            .with("image", crop)
            .with("mask", cropped))
    }
}
