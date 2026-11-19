//! The record of `tighten`, `docs/reference/tools/tighten.md` (plan m3, task D2). Each
//! example calls the operation's function directly, the function its action runs, on the
//! scene of `stroke`'s record with its scratch.

use whizbang_core::artifact::Shape;
use whizbang_core::identity::{CanonicalPixels, Point};
use whizbang_rascal::constants::TIGHTEN_V1;
use whizbang_rascal::rasterize::location_core;
use whizbang_rascal::tighten::{Tightened, Tunables, tighten};

use super::super::record::time;
use super::locations::scratched;
use super::{Page, SIZES, count, hold, image_picture, timed_head, timed_row};

fn never() -> bool {
    false
}

fn path(at: &[(f64, f64)]) -> Vec<Point> {
    at.iter().map(|(x, y)| Point { x: *x, y: *y }).collect()
}

/// The examples: the stroke of `stroke`'s record along the scratch, and one as wide across
/// the scene's plain upper part, where there is no edge.
fn strokes() -> [(&'static str, Vec<Point>, f64); 2] {
    [
        ("scratch", path(&[(20.0, 20.0), (76.0, 44.0)]), 6.0),
        ("plain", path(&[(10.0, 8.0), (86.0, 12.0)]), 6.0),
    ]
}

fn tightened() -> Vec<(&'static str, Vec<Point>, f64, Tightened)> {
    let pixels = scratched();
    let tunables = Tunables::of(TIGHTEN_V1.1, |_| None).unwrap();
    strokes()
        .into_iter()
        .map(|(name, path, radius)| {
            let made = tighten(&path, radius, pixels.rgba(), 96, 64, &tunables, &never);
            (name, path, radius, made.unwrap().unwrap())
        })
        .collect()
}

pub fn page(tool: &'static str) -> Page {
    let mut files = Vec::new();
    let mut measured = String::from(
        "\n| Example | Samples | Tightened | Core | The stroke's inside, as `rasterize` draws it |\n\
         |---|---|---|---|---|\n",
    );
    for (name, path, radius, made) in tightened() {
        files.push((
            format!("{name}.png"),
            image_picture(&scratched(), Some(&made.core)),
        ));
        let whole = location_core(&Shape::Stroke { path, radius }, 96, 64).unwrap();
        measured.push_str(&format!(
            "| {name} | {} | {} | {} | {} |\n",
            made.extents.len(),
            made.tightened(),
            count(&made.core),
            count(&whole)
        ));
    }
    measured.push('\n');
    Page {
        tool,
        files,
        measured,
    }
}

/// A picture of any size with a dark scratch two pixels wide along its diagonal.
fn scratched_at(width: usize, height: usize) -> CanonicalPixels {
    let rgba = (0..width * height)
        .flat_map(|at| {
            let (x, y) = ((at % width) as f64, (at / width) as f64);
            let off = (x * height as f64 - y * width as f64).abs()
                / ((width * width + height * height) as f64).sqrt();
            if off <= 1.0 {
                [40, 30, 30, 255]
            } else {
                [150, 140, 120, 255]
            }
        })
        .collect();
    CanonicalPixels::from_rgba(width as u32, height as u32, rgba).unwrap()
}

pub fn timed(_: &str) -> String {
    let mut out = timed_head(&[
        "A stroke 100 px long, radius 6, on the scratch",
        "A stroke 400 px long",
    ]);
    let tunables = Tunables::of(TIGHTEN_V1.1, |_| None).unwrap();
    for (width, height) in SIZES {
        let pixels = scratched_at(width, height);
        let (w, h) = (width as f64, height as f64);
        let diagonal = (w * w + h * h).sqrt();
        let along = |long: f64| {
            let (dx, dy) = (long / 2.0 * w / diagonal, long / 2.0 * h / diagonal);
            path(&[(w / 2.0 - dx, h / 2.0 - dy), (w / 2.0 + dx, h / 2.0 + dy)])
        };
        let (pw, ph) = (width as u32, height as u32);
        let times = [100.0, 400.0].map(|long| {
            let stroke = along(long);
            time(
                5,
                || (),
                |()| tighten(&stroke, 6.0, pixels.rgba(), pw, ph, &tunables, &never),
            )
        });
        out.push_str(&timed_row(width, height, &times));
    }
    out.push('\n');
    out
}

#[test]
fn the_record_of_tighten_is_what_tighten_gives() {
    hold(&page("tighten"));
    let made = tightened();
    // Along the scratch the core keeps to the scratch: every dark pixel is in it, and it
    // is far smaller than the stroke. Where there is no edge nothing is tightened, and the
    // core is exactly the stroke's inside as `rasterize` draws it.
    let pixels = scratched();
    let (_, path, radius, scratch) = &made[0];
    let dark = (0..96 * 64).filter(|at| pixels.rgba()[at * 4] == 40);
    assert!(dark.clone().all(|at| scratch.core[at] != 0));
    let stroke = Shape::Stroke {
        path: path.clone(),
        radius: *radius,
    };
    assert!(count(&scratch.core) * 2 < count(&location_core(&stroke, 96, 64).unwrap()));
    let (_, path, radius, plain) = &made[1];
    assert_eq!(plain.tightened(), 0);
    let stroke = Shape::Stroke {
        path: path.clone(),
        radius: *radius,
    };
    assert_eq!(plain.core, location_core(&stroke, 96, 64).unwrap());
}
