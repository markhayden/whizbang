//! `rect`: a box as a location on an image, from its four edges in pixels (`SPEC.md`
//! section 7). What a caller's coordinates or a detector's box become; `rasterize` makes
//! it a mask.
//!
//! Ported from rascal `crates/rascal-ops/src/rect.rs` at the commit `docs/sources.toml`
//! pins, with two differences, on purpose: the edges are in pixels of the image (left,
//! top, right, bottom, as `segment` prints a box), and not in fractions of a frame; and a
//! box is held inside its image, as every location here is (ADR 0002). A box that reaches
//! past an edge is cut at that edge, which covers the pixels rascal's would have, since a
//! pixel is covered when its centre is inside; a box wholly outside the image, which
//! rascal said did not apply, is refused, as is one with no area.
//!
//! **Or a polygon** (plan mE, task L2, item 2): given `polygon` in place of `box`, its
//! points in pixels of the image, the location is that polygon, held to the rules of
//! every polygon location (inside the image, simple, at most its vertices). The loop
//! `remove_checked` makes a place a check cited as a polygon this way, and `rasterize`
//! makes it a mask of its own pixels. Exactly one of the two is given.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Shape, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort, PortRef,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};
use whizbang_core::identity::{Point, Space};

use crate::operation;
use crate::select::boxes;

/// The action's name.
pub const NAME: &str = "rect";

/// The action's version, which a location it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// What the command says the action does.
pub const ABOUT: &str = "Make a box on an image a location, from its left, top, right and bottom edges in pixels; \
or a polygon, from its points in pixels. The location is committed to the store, and its identity is printed.";

/// The box of the text `L T R B`, in pixels of an image of `space`: its edges in order,
/// cut at the image's edges. Refused: a text that is not four finite numbers; edges out
/// of order; and a box with no part inside the image.
pub fn shape(text: &str, space: Space) -> Result<Shape, String> {
    let read = boxes(text)
        .map_err(|_| "a box is four finite numbers: left, top, right and bottom".to_owned())?;
    let [[left, top, right, bottom]] = read.as_slice() else {
        return Err("a box is four finite numbers: left, top, right and bottom".to_owned());
    };
    let (left, top, right, bottom) = (*left, *top, *right, *bottom);
    if left >= right || top >= bottom {
        return Err(format!(
            "a box needs left < right and top < bottom, and has {left} {top} {right} {bottom}"
        ));
    }
    let (width, height) = (f64::from(space.width()), f64::from(space.height()));
    if right <= 0.0 || bottom <= 0.0 || left >= width || top >= height {
        return Err(format!(
            "the box {left} {top} {right} {bottom} lies wholly outside the {width}x{height} image"
        ));
    }
    Ok(Shape::Box {
        left: left.max(0.0),
        top: top.max(0.0),
        right: right.min(width),
        bottom: bottom.min(height),
    })
}

/// The polygon of the text `X Y; X Y; ...`, in pixels of an image: three points at
/// least, each two finite numbers. The location's own rules are held when it is made.
pub fn polygon(text: &str) -> Result<Shape, String> {
    let mut points = Vec::new();
    for part in text.split(';').filter(|part| !part.trim().is_empty()) {
        let numbers: Option<Vec<f64>> = part
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|word| !word.is_empty())
            .map(|word| word.parse::<f64>().ok().filter(|n| n.is_finite()))
            .collect();
        match numbers.as_deref() {
            Some(&[x, y]) => points.push(Point { x, y }),
            _ => return Err("a polygon's point is two finite numbers: X and Y".to_owned()),
        }
    }
    if points.len() < 3 {
        return Err(format!(
            "a polygon has three points at least, and this has {}",
            points.len()
        ));
    }
    Ok(Shape::Polygon { points })
}

/// The action `rect`.
pub struct Rect {
    descriptor: ActionDescriptor,
}

impl Rect {
    pub fn new() -> Rect {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "A box as a location on an image, from its left, top, right and bottom edges in pixels.",
        )
        .argument(Argument::optional(
            "box",
            "The box in pixels of the image: left top right bottom, as segment prints a box.",
            ValueType::Text,
        ))
        .argument(Argument::optional(
            "polygon",
            "In place of a box, a polygon: its points in pixels of the image, X Y each, apart by ;.",
            ValueType::Text,
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image the box is on.",
        ))
        .output(
            OutputPort::one("location", ArtifactType::Location, "The box as a location.")
                .on(PortRef::input("image")),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Rect { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Rect {
    fn default() -> Rect {
        Rect::new()
    }
}

/// Registers the action.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Rect::new()))
}

/// What `whizbang rascal rect` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The image, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The box: left, top, right and bottom in pixels of the image, such as "10 20 30 40".
    #[arg(
        long = "box",
        value_name = "L T R B",
        allow_hyphen_values = true,
        required_unless_present = "polygon",
        conflicts_with = "polygon"
    )]
    pub box_: Option<String>,
    /// In place of a box, a polygon: its points in pixels of the image, such as
    /// "10 20; 30 20; 20 40".
    #[arg(long, value_name = "X Y; X Y; ...", allow_hyphen_values = true)]
    pub polygon: Option<String>,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        let given = [("box", &self.box_), ("polygon", &self.polygon)];
        (given.into_iter())
            .filter_map(|(name, text)| text.as_deref().map(|text| (name, Value::text(text))))
            .collect()
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![("image", "--image", self.image.as_str())]
    }
}

impl Action for Rect {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let location = match (
            operation::text(call, "box"),
            operation::text(call, "polygon"),
        ) {
            (Some(text), None) => {
                let step = Step::new(NAME, VERSION).argument("box", text);
                operation::located(call, context, step, |space| shape(text, space))?
            }
            (None, Some(text)) => {
                let step = Step::new(NAME, VERSION).argument("polygon", text);
                operation::located(call, context, step, |_| polygon(text))?
            }
            _ => {
                return Err(Failure::invalid(
                    "rect takes a box or a polygon, one of the two",
                ));
            }
        };
        if let Shape::Box {
            left,
            top,
            right,
            bottom,
        } = location.shape()
        {
            context
                .events()
                .progress(&format!("rect: the box {left} {top} {right} {bottom}"));
        }
        Ok(Produced::nothing().with("location", location))
    }
}
