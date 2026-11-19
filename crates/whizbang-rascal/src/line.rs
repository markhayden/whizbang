//! `line`: a line location on an image, a whole column or a path, with its full width
//! in pixels (`SPEC.md` section 7). The seam's recipe takes the column at the middle of a
//! cut canvas; the path is for any feature that runs straight or nearly.
//!
//! Ported from rascal `crates/rascal-ops/src/line.rs` at the commit `docs/sources.toml`
//! pins, with one difference, on purpose: the column and the path's points are in
//! pixels of the image, as every location here is (ADR 0002), and not in fractions of a
//! frame. Exactly one of `column` and `path` is given, as rascal held it.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Line as Along, Shape, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort, PortRef,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};

use crate::grow::{PX_MAX, text_of};
use crate::operation;

/// The action's name.
pub const NAME: &str = "line";

/// The action's version, which a location it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// What the command says the action does.
pub const ABOUT: &str = "Make a line location on an image: a whole column (--column) or a path of two or more \
points (--path), with its full width in pixels. The location is committed to the store, and \
its identity is printed.";

/// The shape of a line: exactly one of `column` and `path`, and the full width.
pub fn shape(column: Option<f64>, path: Option<&str>, width_px: f64) -> Result<Shape, String> {
    let line = match (column, path) {
        (Some(column), None) => Along::Column(column),
        (None, Some(path)) => Along::Path(operation::points(path, "path")?),
        _ => return Err("a line takes exactly one of column and path".to_owned()),
    };
    Ok(Shape::Line {
        line,
        width: width_px,
    })
}

/// The action `line`.
pub struct Line {
    descriptor: ActionDescriptor,
}

impl Line {
    pub fn new() -> Line {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "A line location on an image: a whole column or a path, with its full width in pixels.",
        )
        .argument(Argument::optional(
            "column",
            "The column, in pixels from the image's left edge, for a line from top to bottom.",
            ValueType::number(Some(0.0), Some(PX_MAX)),
        ))
        .argument(Argument::optional(
            "path",
            "Two or more points in pixels of the image, X Y, points apart by ;.",
            ValueType::Text,
        ))
        .argument(Argument::required(
            "width_px",
            "The line's full width in pixels, more than 0 and at most the image's diagonal.",
            ValueType::number(Some(0.0), Some(PX_MAX)),
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image the line is on.",
        ))
        .output(
            OutputPort::one("location", ArtifactType::Location, "The line location.")
                .on(PortRef::input("image")),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Line { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Line {
    fn default() -> Line {
        Line::new()
    }
}

/// Registers the action.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Line::new()))
}

/// What `whizbang rascal line` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
#[command(group(clap::ArgGroup::new("along").required(true).args(["column", "path"])))]
pub struct Args {
    /// The image, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The column, in pixels from the image's left edge.
    #[arg(long)]
    pub column: Option<f64>,
    /// Two or more points in pixels of the image, such as "10 5; 40 30".
    #[arg(long)]
    pub path: Option<String>,
    /// The line's full width in pixels.
    #[arg(long)]
    pub width_px: f64,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        let mut arguments = vec![("width_px", Value::Number(self.width_px))];
        if let Some(column) = self.column {
            arguments.push(("column", Value::Number(column)));
        }
        if let Some(path) = &self.path {
            arguments.push(("path", Value::text(path)));
        }
        arguments
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![("image", "--image", self.image.as_str())]
    }
}

impl Action for Line {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let column = operation::number(call, "column");
        let path = operation::text(call, "path");
        let width_px = operation::number(call, "width_px").unwrap_or(0.0);
        let mut step = Step::new(NAME, VERSION);
        if let Some(column) = column {
            step = step.argument("column", &text_of(column));
        }
        if let Some(path) = path {
            step = step.argument("path", path);
        }
        let step = step.argument("width_px", &text_of(width_px));
        let location = operation::located(call, context, step, |_| shape(column, path, width_px))?;
        context.events().progress(&match location.shape() {
            Shape::Line {
                line: Along::Column(column),
                ..
            } => format!("line: at column {column}, {width_px} px wide"),
            _ => format!("line: along a path, {width_px} px wide"),
        });
        Ok(Produced::nothing().with("location", location))
    }
}
