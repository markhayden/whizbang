//! The records of `cut`, `put`, `window` and `unwindow`, `docs/reference/tools/<tool>.md`
//! (plan m3, task D2). Each example calls the operation's function directly, the function
//! its action runs once it has read and checked its inputs, so the test needs no store and
//! no executor. The panorama of `cut` and `put` is the scene, whose left and right edges
//! differ, so the seam shows.

use whizbang_core::artifact::Step;
use whizbang_core::contract::Failure;
use whizbang_core::identity::{CanonicalPixels, Space};
use whizbang_rascal::cut::{STRIP_DEFAULT, canvas_of, strip_width};
use whizbang_rascal::examples::{self, about_the_seam, cut_canvas, painted, seam_mask_on};
use whizbang_rascal::put::put_through;
use whizbang_rascal::unwindow::unwound;
use whizbang_rascal::window::{Windowed, windowed};

use super::super::record::time;
use super::{Page, SIZES, count, hold, image_picture, scene, timed_head, timed_row};

const W: u32 = 96;
const H: u32 = 64;

fn never() -> bool {
    false
}

fn flat(width: u32, height: u32) -> Space {
    Space::flat(width, height).unwrap()
}

/// Where two pictures of one size differ, as a plane.
fn differs(a: &CanonicalPixels, b: &CanonicalPixels) -> Vec<u8> {
    a.rgba()
        .chunks(4)
        .zip(b.rgba().chunks(4))
        .map(|(p, q)| if p == q { 0 } else { 255 })
        .collect()
}

/// The columns a plane is set in, first and last, or "none".
fn columns(plane: &[u8], width: usize) -> String {
    let set: Vec<usize> = (0..width)
        .filter(|x| plane.iter().skip(*x).step_by(width).any(|v| *v != 0))
        .collect();
    match set.as_slice() {
        [] => "none".to_owned(),
        [one] => format!("{one}"),
        many => format!(
            "{} of them, {} to {}",
            many.len(),
            many[0],
            many[many.len() - 1]
        ),
    }
}

/// `cut`'s examples: the strip asked, the strip made, and the canvas.
fn cuts() -> Vec<(&'static str, f64, u32, CanonicalPixels)> {
    [("default", STRIP_DEFAULT), ("half", 0.5)]
        .into_iter()
        .map(|(name, strip)| {
            let s = strip_width(W, strip);
            (name, strip, s, canvas_of(&scene(), s).unwrap())
        })
        .collect()
}

/// `put`'s examples, on the default cut: the canvas put back as it was, and with the four
/// columns about its seam painted through a mask of them.
fn puts() -> Vec<(&'static str, CanonicalPixels)> {
    let (s, cut) = cut_canvas();
    let mask = seam_mask_on(&cut, about_the_seam);
    let given = [
        ("unchanged", cut.clone()),
        ("painted", painted(&cut, about_the_seam)),
    ];
    given
        .into_iter()
        .map(|(name, canvas)| {
            let back = put_through(&scene(), flat(W, H), &canvas, s, &mask, &never);
            (name, back.unwrap().unwrap())
        })
        .collect()
}

/// `window`'s examples: a block in the middle, and one near the top right corner.
fn windows() -> Vec<(&'static str, Windowed)> {
    let made = examples::windows().into_iter();
    made.map(|(name, _, made)| (name, made)).collect()
}

/// `unwindow`'s examples, on the middle window: the crop written back as it was, and
/// painted inside its mask's core.
fn unwound_examples() -> Vec<(&'static str, CanonicalPixels)> {
    let (_, made) = windows().remove(0);
    let core = made.mask.core().to_vec();
    let width = made.pixels.width() as usize;
    let given = [
        ("unchanged", made.pixels.clone()),
        (
            "painted",
            painted(&made.pixels, |x, y| core[y * width + x] != 0),
        ),
    ];
    given
        .into_iter()
        .map(|(name, crop)| {
            let back = unwound(&scene(), flat(W, H), &crop, &made.mask, &never);
            (name, back.unwrap().unwrap().0)
        })
        .collect()
}

pub fn page(tool: &'static str) -> Page {
    let mut files = Vec::new();
    let mut measured = String::new();
    match tool {
        "cut" => {
            measured.push_str(
                "\n| Example | `strip` | Strip in columns | Canvas |\n|---|---|---|---|\n",
            );
            for (name, strip, s, canvas) in cuts() {
                measured.push_str(&format!(
                    "| {name} | {strip} | {s} | {}x{} |\n",
                    canvas.width(),
                    canvas.height()
                ));
                files.push((format!("{name}.png"), image_picture(&canvas, None)));
            }
        }
        "put" | "unwindow" => {
            measured.push_str(
                "\n| Example | Pixels of the image changed | Columns changed |\n|---|---|---|\n",
            );
            let made = if tool == "put" {
                puts()
            } else {
                unwound_examples()
            };
            for (name, out) in made {
                let changed = differs(&scene(), &out);
                files.push((format!("{name}.png"), image_picture(&out, Some(&changed))));
                measured.push_str(&format!(
                    "| {name} | {} | {} |\n",
                    count(&changed),
                    columns(&changed, W as usize)
                ));
            }
        }
        _ => {
            measured.push_str(
                "\n| Example | Support | Window | Crop's projection |\n|---|---|---|---|\n",
            );
            for (name, made) in windows() {
                let crop = image_picture(&made.pixels, Some(made.mask.core()));
                files.push((format!("{name}.png"), crop));
                measured.push_str(&format!(
                    "| {name} | {:?} | {:?} | {:?} |\n",
                    made.support,
                    made.bounds,
                    made.space.projection()
                ));
            }
        }
    }
    measured.push('\n');
    Page {
        tool,
        files,
        measured,
    }
}

/// A picture of any size for the times: the scene's gradient, tiled.
fn wide(width: usize, height: usize) -> CanonicalPixels {
    let rgba = (0..width * height)
        .flat_map(|at| {
            let (x, y) = (at % width, at / width);
            [(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8, 255]
        })
        .collect();
    CanonicalPixels::from_rgba(width as u32, height as u32, rgba).unwrap()
}

pub fn timed(tool: &str) -> String {
    let column = match tool {
        "cut" => "`cut`, the default strip",
        "put" => "`put`, the checks and the write",
        "window" => "`window` of a support a tenth of the image",
        _ => "`unwindow`, the checks and the write",
    };
    let mut out = timed_head(&[column]);
    for (width, height) in SIZES {
        let pixels = wide(width, height);
        let (w, h) = (width as u32, height as u32);
        let s = strip_width(w, STRIP_DEFAULT);
        let block = move |x: usize, y: usize| {
            (width * 9 / 20..width * 11 / 20).contains(&x)
                && (height * 9 / 20..height * 11 / 20).contains(&y)
        };
        let took = match tool {
            "cut" => time(5, || (), |()| canvas_of(&pixels, s).unwrap()),
            "put" => {
                let canvas = canvas_of(&pixels, s).unwrap();
                let mask = seam_mask_on(&canvas, |x, _| x + 2 >= s as usize && x < s as usize + 2);
                time(
                    5,
                    || (),
                    |()| put_through(&pixels, flat(w, h), &canvas, s, &mask, &never),
                )
            }
            _ => {
                let mask = seam_mask_on(&pixels, block);
                let step = || Step::new("window", "1.0.0");
                if tool == "window" {
                    time(5, step, |step| {
                        windowed(&pixels, flat(w, h), &mask, None, 0.5, step)
                    })
                } else {
                    let made = windowed(&pixels, flat(w, h), &mask, None, 0.5, step()).unwrap();
                    time(
                        5,
                        || (),
                        |()| unwound(&pixels, flat(w, h), &made.pixels, &made.mask, &never),
                    )
                }
            }
        };
        out.push_str(&timed_row(width, height, &[took]));
    }
    out.push('\n');
    out
}

#[test]
fn the_records_of_cut_put_window_and_unwindow_are_what_they_give() {
    for tool in ["cut", "put", "window", "unwindow"] {
        hold(&page(tool));
    }
    let px = |p: &CanonicalPixels, x: u32, y: u32| {
        let at = ((y * p.width() + x) * 4) as usize;
        p.rgba()[at..at + 4].to_vec()
    };
    // The canvas is the panorama's last strip and then its first; at 0.5 the strip is
    // half the width, and the canvas is the panorama turned by half.
    let cuts = cuts();
    let (s, canvas) = (cuts[0].2, &cuts[0].3);
    assert_eq!(s, 6);
    assert_eq!(px(canvas, 0, 9), px(&scene(), W - s, 9));
    assert_eq!(px(canvas, s, 9), px(&scene(), 0, 9));
    assert_eq!(px(&cuts[1].3, 0, 9), px(&scene(), 48, 9));
    // Put back unchanged, the canvas gives the panorama; painted, its four middle columns
    // land on the panorama's last two and first two. Paint where the mask weighs nothing
    // is refused.
    let puts = puts();
    assert_eq!(puts[0].1, scene());
    assert_eq!(
        columns(&differs(&scene(), &puts[1].1), 96),
        "4 of them, 0 to 95"
    );
    let cut = canvas_of(&scene(), s).unwrap();
    let mask = seam_mask_on(&cut, |x, _| (4..8).contains(&x));
    let stray = painted(&cut, |x, _| x == 0);
    let refused = put_through(&scene(), flat(W, H), &stray, s, &mask, &never);
    assert!(matches!(refused, Err(Failure::Invalid { why }) if why.contains("weighs nothing")));
    // The window of a 12 by 10 support with half of it as context is a square of 24, about
    // it; near the corner, a square of 20 slid inside the image, the support inside it.
    let windows = windows();
    assert_eq!(windows[0].1.bounds, [34, 19, 58, 43]);
    assert_eq!(windows[1].1.bounds, [76, 0, 96, 20]);
    // Written back unchanged, the crop gives the image; painted inside the mask, only the
    // block changes.
    let unwound = unwound_examples();
    assert_eq!(unwound[0].1, scene());
    assert_eq!(count(&differs(&scene(), &unwound[1].1)), 12 * 10);
}
