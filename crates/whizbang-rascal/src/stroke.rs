//! `stroke`: a rough mark along a path on an image, with a radius in pixels (`SPEC.md`
//! section 7): what a brush over a scratch or a tear gives. `rasterize` draws it as
//! drawn; `tighten` fits it to the defect's edges.
//!
//! Ported from rascal `crates/rascal-ops/src/stroke.rs` at the commit `docs/sources.toml`
//! pins, with one difference, on purpose: the path's points are in pixels of the image,
//! as every location here is (ADR 0002), and not in fractions of a frame.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Shape, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort, PortRef,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};

use crate::grow::{PX_MAX, text_of};
use crate::operation;

/// The action's name.
pub const NAME: &str = "stroke";

/// The action's version, which a location it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// What the command says the action does.
pub const ABOUT: &str = "Make a stroke location on an image: a rough mark along a path of two or more points, with \
a radius in pixels. The location is committed to the store, and its identity is printed.";

/// The shape of a stroke along the points of `path`, with this radius.
pub fn shape(path: &str, radius_px: f64) -> Result<Shape, String> {
    Ok(Shape::Stroke {
        path: operation::points(path, "path")?,
        radius: radius_px,
    })
}

/// The action `stroke`.
pub struct Stroke {
    descriptor: ActionDescriptor,
}

impl Stroke {
    pub fn new() -> Stroke {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "A stroke location on an image: a rough mark along a path, with a radius in pixels.",
        )
        .argument(Argument::required(
            "path",
            "Two or more points in pixels of the image, X Y, points apart by ;.",
            ValueType::Text,
        ))
        .argument(Argument::required(
            "radius_px",
            "The stroke's radius in pixels, more than 0 and at most the image's diagonal.",
            ValueType::number(Some(0.0), Some(PX_MAX)),
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image the stroke is on.",
        ))
        .output(
            OutputPort::one("location", ArtifactType::Location, "The stroke location.")
                .on(PortRef::input("image")),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Stroke { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Stroke {
    fn default() -> Stroke {
        Stroke::new()
    }
}

/// Registers the action.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Stroke::new()))
}

/// What `whizbang rascal stroke` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The image, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// Two or more points in pixels of the image, such as "10 5; 40 30".
    #[arg(long)]
    pub path: String,
    /// The stroke's radius in pixels.
    #[arg(long)]
    pub radius_px: f64,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("path", Value::text(&self.path)),
            ("radius_px", Value::Number(self.radius_px)),
        ]
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![("image", "--image", self.image.as_str())]
    }
}

impl Action for Stroke {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let path = operation::text(call, "path").unwrap_or("");
        let radius_px = operation::number(call, "radius_px").unwrap_or(0.0);
        let step = Step::new(NAME, VERSION)
            .argument("path", path)
            .argument("radius_px", &text_of(radius_px));
        let location = operation::located(call, context, step, |_| shape(path, radius_px))?;
        let points = match location.shape() {
            Shape::Stroke { path, .. } => path.len(),
            _ => 0,
        };
        context.events().progress(&format!(
            "stroke: along {points} points, radius {radius_px} px"
        ));
        Ok(Produced::nothing().with("location", location))
    }
}
