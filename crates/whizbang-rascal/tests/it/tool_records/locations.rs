//! The records of `line`, `rect` and `stroke`, `docs/reference/tools/<tool>.md`: each
//! example is a call through the executor, and its picture is the scene with the
//! location's inside, as `rasterize` draws it, in light amber.

use whizbang_core::artifact::{Artifact, Location, Shape};
use whizbang_core::contract::{Call, Value};
use whizbang_core::identity::{CanonicalPixels, Space};
use whizbang_rascal::rasterize::location_core;

use super::super::grow::{made_by, on};
use super::super::paste::image_of;
use super::super::record::time;
use super::super::shaping::Shop;
use super::{Page, SIZES, count, hold, image_picture, scene, timed_head, timed_row};

/// The scene with a scratch: a dark line about two pixels wide from (20, 20) to
/// (76, 44).
pub fn scratched() -> CanonicalPixels {
    let scene = scene();
    let mut rgba = scene.rgba().to_vec();
    for at in 0..96 * 64 {
        let (x, y) = ((at % 96) as f64 + 0.5, (at / 96) as f64 + 0.5);
        // The distance from the segment's line, which runs 56 across and 24 down.
        let along = ((x - 20.0) * 56.0 + (y - 20.0) * 24.0) / (56.0f64 * 56.0 + 24.0 * 24.0);
        let off = ((x - 20.0) * 24.0 - (y - 20.0) * 56.0).abs() / 60.926_184_846_911_27;
        if (0.0..=1.0).contains(&along) && off <= 1.0 {
            rgba[at * 4..at * 4 + 3].copy_from_slice(&[40, 30, 30]);
        }
    }
    CanonicalPixels::from_rgba(96, 64, rgba).unwrap()
}

/// An example of a tool: its name, the scene, and the call's arguments.
type Asked = (&'static str, CanonicalPixels, Vec<(&'static str, Value)>);

/// Each example of a tool.
fn examples(tool: &str) -> Vec<Asked> {
    let number = Value::Number;
    match tool {
        "line" => vec![
            (
                "column",
                scene(),
                vec![("column", number(48.0)), ("width_px", number(6.0))],
            ),
            (
                "path",
                scene(),
                vec![
                    ("path", Value::text("8 56; 88 8")),
                    ("width_px", number(3.0)),
                ],
            ),
        ],
        "rect" => vec![
            ("box", scene(), vec![("box", Value::text("20 10 70 40"))]),
            (
                "edge",
                scene(),
                vec![("box", Value::text("60.4 30.5 120 80"))],
            ),
        ],
        "stroke" => vec![
            (
                "path",
                scene(),
                vec![
                    ("path", Value::text("10 50; 40 14; 86 40")),
                    ("radius_px", number(3.0)),
                ],
            ),
            (
                "scratch",
                scratched(),
                vec![
                    ("path", Value::text("20 20; 76 44")),
                    ("radius_px", number(6.0)),
                ],
            ),
        ],
        other => panic!("{other} has no record here"),
    }
}

/// Each example's location, made through the executor.
fn located(tool: &str) -> Vec<(&'static str, CanonicalPixels, String, Location)> {
    located_in(&Shop::new(tool, None), tool)
}

fn located_in(shop: &Shop, tool: &str) -> Vec<(&'static str, CanonicalPixels, String, Location)> {
    let made = examples(tool).into_iter().map(|(name, pixels, arguments)| {
        let id = shop.put(Artifact::Image(image_of(&pixels, "import")));
        let mut call = Call::new(tool).input("image", id);
        let mut said = Vec::new();
        for (argument, value) in arguments {
            said.push(match &value {
                Value::Number(n) => format!("`{argument}` {n}"),
                Value::Text(text) => format!("`{argument}` \"{text}\""),
                other => panic!("{other:?}"),
            });
            call = call.argument(argument, value);
        }
        let location = shop.location(shop.one(&call));
        (name, pixels, said.join(", "), location)
    });
    made.collect()
}

/// The shape, in words and numbers.
fn shape_text(shape: &Shape) -> String {
    let points = |path: &[whizbang_core::identity::Point]| {
        let each: Vec<String> = path.iter().map(|p| format!("{} {}", p.x, p.y)).collect();
        each.join("; ")
    };
    match shape {
        Shape::Box {
            left,
            top,
            right,
            bottom,
        } => format!("a box, {left} {top} {right} {bottom}"),
        Shape::Line {
            line: whizbang_core::artifact::Line::Column(column),
            width,
        } => format!("a column at {column}, {width} wide"),
        Shape::Line {
            line: whizbang_core::artifact::Line::Path(path),
            width,
        } => format!("a path {}, {width} wide", points(path)),
        Shape::Stroke { path, radius } => {
            format!("a stroke {}, radius {radius}", points(path))
        }
        other => format!("{other:?}"),
    }
}

/// The columns and rows an inside spans: first and last of each.
fn span(plane: &[u8], width: usize) -> String {
    let set: Vec<(usize, usize)> = (0..plane.len())
        .filter(|at| plane[*at] != 0)
        .map(|at| (at % width, at / width))
        .collect();
    let (xs, ys) = (set.iter().map(|p| p.0), set.iter().map(|p| p.1));
    format!(
        "columns {} to {}, rows {} to {}",
        xs.clone().min().unwrap(),
        xs.max().unwrap(),
        ys.clone().min().unwrap(),
        ys.max().unwrap()
    )
}

pub fn page(tool: &'static str) -> Page {
    page_of(tool, located(tool))
}

fn page_of(
    tool: &'static str,
    made: Vec<(&'static str, CanonicalPixels, String, Location)>,
) -> Page {
    let mut files = Vec::new();
    let mut measured = String::from(
        "\n| Example | Arguments | The location | Pixels inside | Where |\n|---|---|---|---|---|\n",
    );
    for (name, pixels, said, location) in made {
        let inside = location_core(location.shape(), 96, 64).unwrap();
        files.push((format!("{name}.png"), image_picture(&pixels, Some(&inside))));
        measured.push_str(&format!(
            "| {name} | {said} | {} | {} | {} |\n",
            shape_text(location.shape()),
            count(&inside),
            span(&inside, 96),
        ));
    }
    measured.push('\n');
    Page {
        tool,
        files,
        measured,
    }
}

/// The shape a tool makes on an image of `space`, as its call at that size would.
fn shape_at(tool: &str, space: Space) -> Shape {
    let (w, h) = (f64::from(space.width()), f64::from(space.height()));
    let path = format!(
        "{} {}; {} {}; {} {}",
        w * 0.1,
        h * 0.8,
        w * 0.4,
        h * 0.2,
        w * 0.9,
        h * 0.6
    );
    match tool {
        "line" => whizbang_rascal::line::shape(None, Some(&path), 6.0).unwrap(),
        "rect" => {
            let text = format!("{} {} {} {}", w * 0.2, h * 0.2, w * 0.7, h * 0.6);
            whizbang_rascal::rect::shape(&text, space).unwrap()
        }
        _ => whizbang_rascal::stroke::shape(&path, 6.0).unwrap(),
    }
}

pub fn timed(tool: &str) -> String {
    let mut out = timed_head(&[
        &format!("`{tool}`: the location made and checked"),
        "Its inside drawn, as `rasterize` draws it",
    ]);
    for (width, height) in SIZES {
        let binding = on(width as u32, height as u32);
        let space = Space::flat(width as u32, height as u32).unwrap();
        let made = time(
            5,
            || (),
            |()| Location::new(binding.clone(), shape_at(tool, space), made_by(tool)).unwrap(),
        );
        let shape = shape_at(tool, space);
        let drawn = time(
            5,
            || (),
            |()| location_core(&shape, width as u32, height as u32),
        );
        out.push_str(&timed_row(width, height, &[made, drawn]));
    }
    out.push('\n');
    out
}

#[test]
fn the_records_of_line_rect_and_stroke_are_what_they_give() {
    let shop = Shop::new("locations", None);
    let [line, rect, stroke] = ["line", "rect", "stroke"].map(|tool| located_in(&shop, tool));
    for (tool, made) in [("line", &line), ("rect", &rect), ("stroke", &stroke)] {
        hold(&page_of(tool, made.clone()));
    }
    // What the records say in words. A column 6 wide at 48 covers the six columns whose
    // centres are within 3 of it.
    let inside = |location: &Location| location_core(location.shape(), 96, 64).unwrap();
    let band = inside(&line[0].3);
    assert_eq!(
        (0..96).filter(|x| band[*x] != 0).collect::<Vec<_>>(),
        (45..51).collect::<Vec<_>>()
    );
    // A box past the image is cut at its edges; a pixel is inside when its centre is.
    assert_eq!(
        rect[1].3.shape(),
        &Shape::Box {
            left: 60.4,
            top: 30.5,
            right: 96.0,
            bottom: 64.0
        }
    );
    let edge = inside(&rect[1].3);
    assert_eq!(count(&edge), 36 * 34);
    assert!(edge[30 * 96 + 60] != 0 && edge[29 * 96 + 60] == 0 && edge[30 * 96 + 59] == 0);
    // A stroke of radius 6 along the scratch covers it and much more.
    let (pixels, wide) = (&stroke[1].1, inside(&stroke[1].3));
    let dark = (0..96 * 64).filter(|at| pixels.rgba()[at * 4] == 40);
    let dark: Vec<usize> = dark.collect();
    assert!(dark.iter().all(|at| wide[*at] != 0));
    assert_eq!((dark.len(), count(&wide)), (120, 840));
}
