//! The records of `rasterize`, `shrink`, `invert` and `inspect`,
//! `docs/reference/tools/<tool>.md` (plan m3, task D2). Each example calls the operation's
//! function directly, the function its action runs, so the test needs no store and no
//! executor.

use whizbang_core::artifact::{Location, Mask, Shape, Source, UseCase};
use whizbang_core::identity::{Binding, CanonicalPixels, Digest, Point, Space};
use whizbang_rascal::feather::feather;
use whizbang_rascal::inspect::{TINT, core_widths, overlay, report};
use whizbang_rascal::invert::{InvertError, invert};
use whizbang_rascal::rasterize::{location_core, rasterize};
use whizbang_rascal::shrink::shrink;

use super::super::grow::{made_by, plane};
use super::super::record::{picture, tiled, time};
use super::{Page, SIZES, count, hold, image_picture, scene, timed_head, timed_row};

const W: usize = 96;
const H: usize = 64;

fn never() -> bool {
    false
}

/// The root binding of these pixels.
fn onto(pixels: &CanonicalPixels) -> Binding {
    Binding::root(
        pixels.digest(),
        Space::flat(pixels.width(), pixels.height()).unwrap(),
    )
}

/// A mask on these pixels with this core, and these pixels protected.
fn mask_on(pixels: &CanonicalPixels, core: Vec<u8>, protect: &[u8]) -> Mask {
    let mut mask = Mask::from_core(
        onto(pixels),
        UseCase::Remove,
        core,
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    mask.protect_union(protect).unwrap();
    mask
}

fn disc(cx: i64, cy: i64, r: i64) -> impl Fn(usize, usize) -> bool {
    move |x, y| (x as i64 - cx).pow(2) + (y as i64 - cy).pow(2) <= r * r
}

/// A disc of radius 14 at (40, 30) on the scene, a protected patch across its right edge,
/// feathered by a sigma of 2.
fn feathered() -> Mask {
    let core = plane(W, H, disc(40, 30, 14));
    let patch = plane(W, H, |x, y| (50..58).contains(&x) && (26..34).contains(&y));
    feather(mask_on(&scene(), core, &patch), 2.0, &never)
        .unwrap()
        .unwrap()
}

/// A block against the image's top and left edges.
fn cornered() -> Mask {
    let core = plane(W, H, |x, y| x < 30 && y < 24);
    mask_on(&scene(), core, &[0; W * H])
}

/// A disc of radius 20 with a protected disc of radius 6 at its centre.
fn holed() -> Mask {
    let core = plane(W, H, disc(48, 32, 20));
    mask_on(&scene(), core, &plane(W, H, disc(48, 32, 6)))
}

fn points(at: &[(i32, i32)]) -> Vec<Point> {
    at.iter()
        .map(|(x, y)| Point {
            x: f64::from(*x),
            y: f64::from(*y),
        })
        .collect()
}

/// `rasterize`'s examples: a triangle, and a chevron, whose notch is outside.
fn shapes() -> [(&'static str, Shape); 2] {
    [
        (
            "triangle",
            Shape::Polygon {
                points: points(&[(12, 56), (48, 6), (84, 56)]),
            },
        ),
        (
            "chevron",
            Shape::Polygon {
                points: points(&[(10, 10), (50, 32), (10, 54), (30, 32)]),
            },
        ),
    ]
}

fn rasterized() -> Vec<(&'static str, Mask)> {
    shapes()
        .into_iter()
        .map(|(name, shape)| {
            let location = Location::new(onto(&scene()), shape, made_by("fixture")).unwrap();
            (
                name,
                rasterize(&location, ("custom", UseCase::Custom)).unwrap(),
            )
        })
        .collect()
}

/// `shrink`'s and `invert`'s examples: the mask given and the mask made.
fn changed(tool: &str) -> Vec<(&'static str, Mask, Mask)> {
    let digest = scene().digest().0;
    let made = |mask: &Mask| match tool {
        "shrink" => shrink(mask.clone(), 3.0, &never).unwrap().unwrap().mask,
        _ => invert(mask, digest).unwrap(),
    };
    [("feathered", feathered()), ("cornered", cornered())]
        .into_iter()
        .map(|(name, mask)| (name, mask.clone(), made(&mask)))
        .collect()
}

/// `inspect`'s examples: the mask and its overlay on the scene.
fn inspected() -> Vec<(&'static str, Mask, CanonicalPixels)> {
    [("feathered", feathered()), ("holed", holed())]
        .into_iter()
        .map(|(name, mask)| {
            let rgba = overlay(&mask, &scene(), TINT).unwrap();
            (
                name,
                mask,
                CanonicalPixels::from_rgba(96, 64, rgba).unwrap(),
            )
        })
        .collect()
}

fn planes(mask: &Mask) -> String {
    format!(
        "{}, {}, {}",
        count(mask.core()),
        count(mask.blend()),
        count(mask.protect())
    )
}

pub fn page(tool: &'static str) -> Page {
    let mut files = Vec::new();
    let mut measured = String::new();
    match tool {
        "rasterize" => {
            measured.push_str("\n| Example | Pixels inside | Their box |\n|---|---|---|\n");
            for (name, mask) in rasterized() {
                files.push((
                    format!("{name}.png"),
                    image_picture(&scene(), Some(mask.core())),
                ));
                let support = mask.statistics().support;
                measured.push_str(&format!(
                    "| {name} | {} | {support:?} |\n",
                    count(mask.core())
                ));
            }
        }
        "shrink" | "invert" => {
            measured.push_str(
                "\n| Example | Core, blend and protect before | Core, blend and protect after |\n\
                 |---|---|---|\n",
            );
            for (name, before, after) in changed(tool) {
                files.push((format!("{name}.png"), picture(&before)));
                files.push((format!("{name}-{tool}.png"), picture(&after)));
                measured.push_str(&format!(
                    "| {name} | {} | {} |\n",
                    planes(&before),
                    planes(&after)
                ));
            }
        }
        _ => {
            measured
                .push_str("\n| Example | The report | Widest row of the core |\n|---|---|---|\n");
            for (name, mask, shown) in inspected() {
                files.push((format!("{name}.png"), image_picture(&shown, None)));
                let widest = core_widths(&mask).into_iter().fold(0.0, f64::max);
                measured.push_str(&format!(
                    "| {name} | {} | {widest} |\n",
                    report(&mask).line()
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

/// The chevron of the examples, scaled to an image `width` by `height`.
fn chevron_at(width: usize, height: usize) -> Shape {
    let (sx, sy) = (width as f64 / 96.0, height as f64 / 64.0);
    let at = [(10, 10), (50, 32), (10, 54), (30, 32)];
    Shape::Polygon {
        points: at
            .iter()
            .map(|(x, y)| Point {
                x: f64::from(*x) * sx,
                y: f64::from(*y) * sy,
            })
            .collect(),
    }
}

pub fn timed(tool: &str) -> String {
    let columns: &[&str] = match tool {
        "rasterize" => &[
            "The chevron, scaled to the image",
            "A stroke of radius 20 across it",
        ],
        "shrink" => &["`shrink`, `px` 3", "`shrink`, `px` 40"],
        "invert" => &["`invert`"],
        _ => &["The overlay", "The report and the widths"],
    };
    let mut out = timed_head(columns);
    for (width, height) in SIZES {
        let mask = tiled(width, height);
        let times = match tool {
            "rasterize" => {
                let chevron = chevron_at(width, height);
                let stroke = Shape::Stroke {
                    path: vec![
                        Point { x: 0.0, y: 0.0 },
                        Point {
                            x: width as f64,
                            y: height as f64,
                        },
                    ],
                    radius: 20.0,
                };
                let (w, h) = (width as u32, height as u32);
                vec![
                    time(5, || (), |()| location_core(&chevron, w, h)),
                    time(5, || (), |()| location_core(&stroke, w, h)),
                ]
            }
            "shrink" => vec![
                time(5, || mask.clone(), |m| shrink(m, 3.0, &never).unwrap()),
                time(5, || mask.clone(), |m| shrink(m, 40.0, &never).unwrap()),
            ],
            "invert" => vec![time(
                5,
                || (),
                |()| invert(&mask, Digest::of_bytes(b"photo")),
            )],
            _ => {
                let grey = CanonicalPixels::from_rgba(
                    width as u32,
                    height as u32,
                    vec![128; width * height * 4],
                )
                .unwrap();
                let on = mask_on(&grey, mask.core().to_vec(), mask.protect());
                vec![
                    time(5, || (), |()| overlay(&on, &grey, TINT).unwrap()),
                    time(5, || (), |()| (report(&on), core_widths(&on))),
                ]
            }
        };
        out.push_str(&timed_row(width, height, &times));
    }
    out.push('\n');
    out
}

#[test]
fn the_records_of_rasterize_shrink_invert_and_inspect_are_what_they_give() {
    for tool in ["rasterize", "shrink", "invert", "inspect"] {
        hold(&page(tool));
    }
    let at = |plane: &[u8], x: usize, y: usize| plane[y * W + x] != 0;
    // The chevron's notch is outside it. A polygon whose edges cross is no location.
    let [(_, triangle), (_, chevron)] = &rasterized()[..] else {
        panic!()
    };
    assert!(at(triangle.core(), 48, 32));
    assert!(!at(chevron.core(), 20, 32) && at(chevron.core(), 40, 32));
    let star = Shape::Polygon {
        points: points(&[(48, 4), (64, 56), (20, 24), (76, 24), (32, 56)]),
    };
    assert!(Location::new(onto(&scene()), star, made_by("fixture")).is_err());
    // `shrink` clears the blend and keeps protect; a block against the border shrinks
    // from the border too, three pixels each way.
    let shrunk = changed("shrink");
    assert_eq!(count(shrunk[0].2.blend()), 0);
    assert_eq!(shrunk[0].1.protect(), shrunk[0].2.protect());
    let block = shrunk[1].2.core();
    assert!(at(block, 3, 3) && !at(block, 2, 3) && !at(block, 3, 2));
    assert!(at(block, 26, 20) && !at(block, 27, 20) && !at(block, 26, 21));
    // `invert` protects all the first mask reached, its blend and protect among it, and
    // makes the rest the core; a mask that reached every pixel is refused.
    for (_, before, after) in changed("invert") {
        for p in 0..W * H {
            let reached = before.core()[p] | before.blend()[p] | before.protect()[p] != 0;
            assert_eq!(
                (after.protect()[p] != 0, after.core()[p] != 0),
                (reached, !reached)
            );
        }
        assert_eq!(count(after.blend()), 0);
        assert_eq!(after.use_case(), UseCase::Protect);
    }
    let whole = mask_on(&scene(), vec![255; W * H], &[0; W * H]);
    assert!(matches!(
        invert(&whole, Digest::of_bytes(b"photo")),
        Err(InvertError::Everything)
    ));
    // `inspect` draws the core's edge in white, around a protected hole too, and tints
    // by half the weight: the feather's outer pixels barely.
    let shown = inspected();
    let rgb = |pixels: &CanonicalPixels, x: usize, y: usize| {
        let p = (y * W + x) * 4;
        pixels.rgba()[p..p + 3].to_vec()
    };
    assert_eq!(rgb(&shown[1].2, 48, 25), [255, 255, 255]);
    assert_eq!(rgb(&shown[1].2, 48, 32), rgb(&scene(), 48, 32));
    assert_eq!(
        rgb(&shown[0].2, 40, 30)[1],
        u16::from(rgb(&scene(), 40, 30)[1]).div_ceil(2) as u8
    );
}
