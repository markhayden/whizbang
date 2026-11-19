//! `rasterize`: a location's inside as the core of a new mask on its image (`SPEC.md`
//! section 7). A box and a polygon as drawn; the whole image, every pixel; a line on a
//! column, a band of its width about the column, and a line on a path, the path widened
//! by half its width; a stroke, its path widened by its radius. A view and an absent
//! thing have no inside, and a location that covers no pixel makes no mask: the call is
//! refused for either.
//!
//! # The rules, as rascal wrote them
//!
//! A pixel is covered when its centre, `(x + 0.5, y + 0.5)`, is:
//!
//! - **a box:** at or right of the left edge and left of the right one, at or under the
//!   top and above the bottom ([`box_pixels`]);
//! - **a polygon:** inside by the even-odd rule, along each row's centre, a centre on a
//!   crossing taken on the side of the left one ([`fill_polygon`]);
//! - **a column:** no further than half the width from the column;
//! - **a path:** within the radius of a segment of it, the ends rounded
//!   ([`dilate_path`]), with rascal's slack of 0.000000001 on the radius squared.
//!
//! Ported from rascal `crates/rascal-ops/src/rasterize.rs` at the commit
//! `docs/sources.toml` pins, with one difference, on purpose: a location is in pixels of
//! its image (ADR 0002), so each rule is rascal's after its own step from fractions to
//! pixels, the same arithmetic in doubles from there on. A call whose location covers
//! nothing, which rascal said did not apply, is refused.

use std::sync::Arc;

use whizbang_core::artifact::{
    ArtifactType, Line, Location, Mask, Origin, Shape, Source, Step, UseCase,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort, PortRef,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};
use whizbang_core::identity::Point;

use crate::operation;
use crate::select::USE_CASES;

/// The action's name.
pub const NAME: &str = "rasterize";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// What the command says the action does.
pub const ABOUT: &str = "Make a location's inside the core of a new mask on its image: a box or a polygon as \
drawn, a line as a band of its width, a stroke as its path widened by its radius. The mask \
is committed to the store, and its identity is printed.";

/// The pixels a box covers, `(left, top, right, bottom)` with the right and the bottom
/// one past the last, cut at the image's edges: those whose centres are at or past the
/// box's left and top edges and before its right and bottom ones. None when it covers no
/// pixel.
pub fn box_pixels(
    [left, top, right, bottom]: [f64; 4],
    width: u32,
    height: u32,
) -> Option<(u32, u32, u32, u32)> {
    // The first pixel whose centre is at or past `v`.
    let first_at = |v: f64, size: u32| {
        ((v.clamp(0.0, f64::from(size)) - 0.5).ceil().max(0.0) as u32).min(size)
    };
    let (x0, x1) = (first_at(left, width), first_at(right, width));
    let (y0, y1) = (first_at(top, height), first_at(bottom, height));
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
}

/// A polygon filled along each row's centre by the even-odd rule: one byte a pixel, 255
/// inside and 0 outside.
pub fn fill_polygon(points: &[Point], width: u32, height: u32) -> Vec<u8> {
    let mut core = vec![0u8; width as usize * height as usize];
    let n = points.len();
    for y in 0..height {
        let cy = f64::from(y) + 0.5;
        let mut crossings: Vec<f64> = Vec::new();
        for i in 0..n {
            let (a, b) = (points[i], points[(i + 1) % n]);
            if (a.y <= cy && b.y > cy) || (b.y <= cy && a.y > cy) {
                crossings.push(a.x + (cy - a.y) / (b.y - a.y) * (b.x - a.x));
            }
        }
        crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in crossings.chunks(2) {
            if let [a, b] = pair {
                let start = (a - 0.5).ceil().max(0.0) as u32;
                // Centres at or past a and before b: a shared edge is painted once.
                let end = ((b - 0.5).ceil() - 1.0).min(f64::from(width) - 1.0);
                if end >= 0.0 {
                    for x in start..=end as u32 {
                        core[(y * width + x) as usize] = 255;
                    }
                }
            }
        }
    }
    core
}

/// The pixels whose centres are within `radius` of a segment of the path, the ends
/// rounded: one byte a pixel, 255 inside and 0 outside.
pub fn dilate_path(path: &[Point], radius: f64, width: u32, height: u32) -> Vec<u8> {
    let r2 = radius * radius;
    let (min_x, max_x, min_y, max_y) = path.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |a, p| (a.0.min(p.x), a.1.max(p.x), a.2.min(p.y), a.3.max(p.y)),
    );
    let x_lo = (min_x - radius - 1.0).floor().max(0.0) as u32;
    let x_hi = ((max_x + radius + 1.0).ceil().max(0.0) as u32).min(width);
    let y_lo = (min_y - radius - 1.0).floor().max(0.0) as u32;
    let y_hi = ((max_y + radius + 1.0).ceil().max(0.0) as u32).min(height);
    let mut core = vec![0u8; width as usize * height as usize];
    for y in y_lo..y_hi {
        let cy = f64::from(y) + 0.5;
        for x in x_lo..x_hi {
            let cx = f64::from(x) + 0.5;
            let hit = path.windows(2).any(|segment| {
                let (a, b) = (segment[0], segment[1]);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let len2 = dx * dx + dy * dy;
                let t = if len2 == 0.0 {
                    0.0
                } else {
                    (((cx - a.x) * dx + (cy - a.y) * dy) / len2).clamp(0.0, 1.0)
                };
                let (px, py) = (a.x + t * dx, a.y + t * dy);
                (cx - px).powi(2) + (cy - py).powi(2) <= r2 + 1e-9
            });
            if hit {
                core[(y * width + x) as usize] = 255;
            }
        }
    }
    core
}

/// The plane a shape covers on an image `width` by `height`, by the rules above; none for
/// a view and an absent thing, which have no inside.
pub fn location_core(shape: &Shape, width: u32, height: u32) -> Option<Vec<u8>> {
    let samples = width as usize * height as usize;
    Some(match shape {
        Shape::Box {
            left,
            top,
            right,
            bottom,
        } => {
            let mut core = vec![0u8; samples];
            if let Some((x0, y0, x1, y1)) =
                box_pixels([*left, *top, *right, *bottom], width, height)
            {
                for y in y0..y1 {
                    for x in x0..x1 {
                        core[(y * width + x) as usize] = 255;
                    }
                }
            }
            core
        }
        Shape::WholeImage => vec![255; samples],
        Shape::Polygon { points } => fill_polygon(points, width, height),
        Shape::Line {
            line: Line::Column(centre),
            width: across,
        } => {
            let half = across / 2.0;
            (0..samples)
                .map(|at| {
                    let x = (at % width as usize) as f64;
                    if (x + 0.5 - centre).abs() <= half {
                        255
                    } else {
                        0
                    }
                })
                .collect()
        }
        Shape::Line {
            line: Line::Path(path),
            width: across,
        } => dilate_path(path, across / 2.0, width, height),
        Shape::Stroke { path, radius } => dilate_path(path, *radius, width, height),
        Shape::View { .. } | Shape::Absent => return None,
    })
}

/// The name of a shape's kind, as a refusal says it.
pub fn kind(shape: &Shape) -> &'static str {
    match shape {
        Shape::Box { .. } => "box",
        Shape::Polygon { .. } => "polygon",
        Shape::View { .. } => "view",
        Shape::WholeImage => "whole image",
        Shape::Absent => "absent",
        Shape::Line { .. } => "line",
        Shape::Stroke { .. } => "stroke",
    }
}

/// The mask whose core is the inside of `location`, on its binding, recorded for
/// `use_case`; its source the location; its origin one step, this action, the use case
/// and the location. Refused, in a sentence: a location with no inside, and one that
/// covers no pixel.
pub fn rasterize(location: &Location, use_case: (&str, UseCase)) -> Result<Mask, String> {
    let space = location.binding().source_space();
    let shape = location.shape();
    let Some(core) = location_core(shape, space.width(), space.height()) else {
        return Err(format!(
            "a {} location has no inside to rasterize",
            kind(shape)
        ));
    };
    if core.iter().all(|sample| *sample == 0) {
        return Err(format!(
            "the {} location covers no pixel of its image",
            kind(shape)
        ));
    }
    let source = Source {
        locations: vec![location.id()],
        instances: Vec::new(),
    };
    let step = Step::new(NAME, VERSION)
        .argument("use_case", use_case.0)
        .input("location", location.id().0);
    let origin = Origin::new(step).map_err(|error| error.without_paths())?;
    Mask::from_core(location.binding().clone(), use_case.1, core, source, origin)
        .map_err(|error| error.without_paths())
}

/// The action `rasterize`.
pub struct Rasterize {
    descriptor: ActionDescriptor,
}

impl Rasterize {
    pub fn new() -> Rasterize {
        let names: Vec<&str> = USE_CASES.iter().map(|(name, _)| *name).collect();
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "A location's inside as the core of a new mask: a box or polygon as drawn, a line as a band, a stroke widened.",
        )
        .argument(Argument::with_default(
            "use_case",
            "What the mask is for, recorded on it.",
            ValueType::choice(&names),
            Value::text("custom"),
        ))
        .input(
            InputPort::required("location", ArtifactType::Location, "The location to rasterize.")
                .on("image"),
        )
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image the location is on.",
        ))
        .output(
            OutputPort::one(
                "mask",
                ArtifactType::Mask,
                "The mask whose core is the location's inside; no blend and nothing protected.",
            )
            .on(PortRef::input("image")),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Rasterize { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Rasterize {
    fn default() -> Rasterize {
        Rasterize::new()
    }
}

/// Registers the action.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Rasterize::new()))
}

/// What `whizbang rascal rasterize` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The location, by its identity in the store.
    #[arg(long)]
    pub location: String,
    /// The image the location is on, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// What the mask is for: seam, remove, repair, protect or custom.
    #[arg(long, default_value = "custom")]
    pub use_case: String,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        vec![("use_case", Value::text(&self.use_case))]
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![
            ("location", "--location", self.location.as_str()),
            ("image", "--image", self.image.as_str()),
        ]
    }
}

impl Action for Rasterize {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let named = operation::text(call, "use_case").unwrap_or("custom");
        let use_case = USE_CASES
            .iter()
            .find(|(name, _)| *name == named)
            .copied()
            .unwrap_or(("custom", UseCase::Custom));
        let location = operation::location(context, operation::one(call, "location")?)?;
        let mask = rasterize(&location, use_case).map_err(|why| Failure::invalid(&why))?;
        context.events().progress(&format!(
            "rasterize: a core of {} pixels from a {} location",
            operation::set(mask.core()),
            kind(location.shape())
        ));
        Ok(Produced::nothing().with("mask", mask))
    }
}
