//! The records of the five recipes that end in a fill, `remove`, `remove_prompt`, `repair`,
//! `repair_area` and `seam`, `docs/reference/tools/<recipe>.md` (plan m3, task B1, item 2):
//! each example runs the shipped recipe through the executor under a scripted filler that
//! paints brick red wherever the support is, and `remove_prompt`'s under the test kit's
//! fake segmenter too, which answers the box it is told for a phrase: a test makes no live
//! call. Rung 2, the recipes' task success and preservation with Bria through fal and
//! SAM 3 on the Mac, is the edits' record (`docs/results/m3-edits.md`), which
//! `whizbang`'s `evidence::` tests hold.

use std::sync::Arc;

use whizbang_core::artifact::{Artifact, ArtifactId, Location, Mask, Shape, Source, UseCase};
use whizbang_core::capability::{Filler, Segmenter};
use whizbang_core::contract::{Call, Value};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Point};
use whizbang_testkit::capabilities::{FakeFiller, FakeSegmenter, Paint};

use super::super::canvases::image as image_as;
use super::super::grow::made_by;
use super::super::record::{picture, time};
use super::super::shaping::Shop;
use super::{Page, count, hold, image_picture, scene, timed_head, timed_row};

const W: u32 = 96;
const H: u32 = 64;
/// The colour of the fills: a brick red.
const BRICK: [u8; 3] = [200, 60, 50];
/// The phrase `remove_prompt` is asked for, as the edits' round asked SAM 3.
const PHRASE: &str = "a red ball";
/// The panoramas of `seam`'s examples, and the strip its cut takes: 0.07 of 256 columns.
const PW: u32 = 256;
const PH: u32 = 128;
const STRIP: u32 = 18;
/// The sizes the recipes are timed at, through the executor, as `mask_rect`'s are; the
/// seam's panoramas are two to one.
const SIZES: [(u32, u32); 3] = [(1024, 768), (2048, 1536), (4096, 3072)];
const PANORAMAS: [(u32, u32); 3] = [(1024, 512), (2048, 1024), (4096, 2048)];

/// What is drawn over the scene: a colour, or none for the scene's own.
type Drawing = dyn Fn(f64, f64) -> Option<[u8; 3]>;

/// The scene, `w` by `h`, scaled from its 96 by 64, with `drawing` over it.
fn drawn(w: u32, h: u32, drawing: &Drawing) -> CanonicalPixels {
    let base = scene();
    let rgba = (0..w * h)
        .flat_map(|at| {
            let (x, y) = (at % w, at / w);
            let (sx, sy) = (x * W / w, y * H / h);
            let from = ((sy * W + sx) * 4) as usize;
            let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            match drawing(
                cx * f64::from(W) / f64::from(w),
                cy * f64::from(H) / f64::from(h),
            ) {
                Some([r, g, b]) => [r, g, b, 255],
                None => base.rgba()[from..from + 4].try_into().unwrap(),
            }
        })
        .collect();
    CanonicalPixels::from_rgba(w, h, rgba).unwrap()
}

/// A shaded red ball at `(x, y)` of radius `r`, in the scene's units.
fn ball(x: f64, y: f64, r: f64) -> impl Fn(f64, f64) -> Option<[u8; 3]> {
    move |cx, cy| {
        let d = ((cx - x).powi(2) + (cy - y).powi(2)).sqrt() / r;
        (d <= 1.0).then(|| [(215.0 - 70.0 * d) as u8, 40, 40])
    }
}

/// A dark spot at `(x, y)` of radius `r`.
fn spot(x: f64, y: f64, r: f64) -> impl Fn(f64, f64) -> Option<[u8; 3]> {
    move |cx, cy| ((cx - x).powi(2) + (cy - y).powi(2) <= r * r).then_some([60, 48, 40])
}

/// A light scratch a pixel and a half wide along the segment from `a` to `b`.
fn scratch(a: [f64; 2], b: [f64; 2]) -> impl Fn(f64, f64) -> Option<[u8; 3]> {
    move |cx, cy| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let t = (((cx - a[0]) * dx + (cy - a[1]) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        let (px, py) = (a[0] + t * dx, a[1] + t * dy);
        ((cx - px).powi(2) + (cy - py).powi(2) <= 0.75 * 0.75).then_some([238, 232, 220])
    }
}

/// A stain: the scene darkened inside the triangle `points`.
fn stain(points: [[f64; 2]; 3]) -> impl Fn(f64, f64) -> Option<[u8; 3]> {
    move |cx, cy| {
        let side =
            |p: [f64; 2], q: [f64; 2]| (q[0] - p[0]) * (cy - p[1]) - (q[1] - p[1]) * (cx - p[0]);
        let s = [
            side(points[0], points[1]),
            side(points[1], points[2]),
            side(points[2], points[0]),
        ];
        (s.iter().all(|v| *v >= 0.0) || s.iter().all(|v| *v <= 0.0)).then_some([92, 70, 52])
    }
}

/// A panorama `w` by `h` whose left and right edges do not meet: a tone that brightens
/// to the right, as the task tests' `Seam` has it; with `stripes`, light bands across it
/// every sixteen rows.
fn panorama(w: u32, h: u32, stripes: bool) -> CanonicalPixels {
    let rgba = (0..w * h)
        .flat_map(|at| {
            let (x, y) = (at % w, at / w);
            let band = if stripes && (y * 128 / h) % 16 < 3 {
                40
            } else {
                0
            };
            let shade = x * 256 / w;
            [
                (100 + shade / 8 + band) as u8,
                (120 + shade / 16 + band) as u8,
                (90 + band) as u8,
                255,
            ]
        })
        .collect();
    CanonicalPixels::from_rgba(w, h, rgba).unwrap()
}

/// What an example gives the recipe beside its picture.
enum Given {
    Location(Shape),
    Protected(Shape, [u32; 4]),
    Phrase(Vec<[u32; 4]>),
    Stroke(Vec<Point>, f64),
    Panorama,
}

struct Example {
    name: &'static str,
    /// What the recipe was given, in words for the table.
    said: String,
    base: CanonicalPixels,
    mask: Mask,
    out: CanonicalPixels,
    /// Whether the mask handed back is on the composite.
    rebound: bool,
}

fn boxed(left: f64, top: f64, right: f64, bottom: f64) -> Shape {
    Shape::Box {
        left,
        top,
        right,
        bottom,
    }
}

/// The two examples of each recipe: a name, the picture, and what the recipe is given.
fn cases(tool: &str) -> Vec<(&'static str, CanonicalPixels, Given)> {
    match tool {
        "remove" => vec![
            (
                "ball",
                drawn(W, H, &ball(40.0, 28.0, 9.0)),
                Given::Location(boxed(30.0, 18.0, 50.0, 38.0)),
            ),
            (
                "protected",
                drawn(W, H, &ball(40.0, 28.0, 9.0)),
                Given::Protected(boxed(30.0, 18.0, 50.0, 38.0), [44, 0, 96, 64]),
            ),
        ],
        "remove_prompt" => vec![
            (
                "one",
                drawn(W, H, &ball(40.0, 28.0, 9.0)),
                Given::Phrase(vec![[31, 19, 50, 38]]),
            ),
            (
                "two",
                drawn(W, H, &|x, y| {
                    ball(22.0, 20.0, 7.0)(x, y).or_else(|| ball(70.0, 34.0, 8.0)(x, y))
                }),
                Given::Phrase(vec![[15, 13, 30, 28], [62, 26, 79, 43]]),
            ),
        ],
        "repair" => vec![
            (
                "straight",
                drawn(W, H, &scratch([12.0, 30.0], [84.0, 30.0])),
                Given::Stroke(vec![Point::new(10.0, 30.0), Point::new(86.0, 30.0)], 6.0),
            ),
            (
                "diagonal",
                drawn(W, H, &scratch([16.0, 10.0], [80.0, 54.0])),
                Given::Stroke(vec![Point::new(14.0, 8.5), Point::new(82.0, 55.5)], 5.0),
            ),
        ],
        "repair_area" => vec![
            (
                "spot",
                drawn(W, H, &spot(60.0, 24.0, 6.0)),
                Given::Location(boxed(53.0, 17.0, 67.0, 31.0)),
            ),
            (
                "stain",
                drawn(W, H, &stain([[14.0, 38.0], [42.0, 38.0], [28.0, 58.0]])),
                Given::Location(Shape::Polygon {
                    points: vec![
                        Point::new(12.0, 36.0),
                        Point::new(44.0, 36.0),
                        Point::new(28.0, 60.0),
                    ],
                }),
            ),
        ],
        "seam" => vec![
            ("step", panorama(PW, PH, false), Given::Panorama),
            ("striped", panorama(PW, PH, true), Given::Panorama),
        ],
        other => panic!("{other} is no recipe of these records"),
    }
}

/// A shop whose filler paints brick red where the support is; for `remove_prompt`, with
/// a segmenter that answers each box of `found` for the phrase.
fn shop(label: &str, found: &[[u32; 4]]) -> Shop {
    let filler: Arc<dyn Filler> = Arc::new(FakeFiller::new(Paint::Support(BRICK)));
    let segmenter = (!found.is_empty()).then(|| {
        let made = (found.iter()).fold(FakeSegmenter::new(), |s, b| s.finding(PHRASE, *b));
        Arc::new(made) as Arc<dyn Segmenter>
    });
    Shop::finding(label, Some(filler), segmenter)
}

/// The recipe's call on `image` in the shop, and the words for the table.
fn call(tool: &str, shop: &Shop, base: &CanonicalPixels, given: &Given) -> (Call, String) {
    let panorama = matches!(given, Given::Panorama);
    let image = image_as(base, panorama);
    let id = shop.put(Artifact::Image(image.clone()));
    let at = |shape: Shape| -> ArtifactId {
        let location = Location::new(image.binding(), shape, made_by("rect")).unwrap();
        shop.put(Artifact::Location(location))
    };
    let call = Call::new(tool).input("image", id);
    match given {
        Given::Location(shape) => (call.input("location", at(shape.clone())), described(shape)),
        Given::Protected(shape, [l, t, r, b]) => {
            let plane = (0..base.width() * base.height())
                .map(|at| {
                    let (x, y) = (at % base.width(), at / base.width());
                    if (*l..*r).contains(&x) && (*t..*b).contains(&y) {
                        255
                    } else {
                        0
                    }
                })
                .collect();
            let guard = Mask::from_core(
                image.binding(),
                UseCase::Custom,
                plane,
                Source::default(),
                made_by("fixture"),
            )
            .unwrap();
            let guard = shop.put(Artifact::Mask(guard));
            let said = format!("{}, protect columns {l} to {}", described(shape), r - 1);
            (
                call.input("location", at(shape.clone()))
                    .input("protect", guard),
                said,
            )
        }
        Given::Phrase(found) => (
            call.argument("prompt", Value::text(PHRASE)),
            format!("\"{PHRASE}\", found at {}", boxes(found)),
        ),
        Given::Stroke(path, radius) => {
            let shape = Shape::Stroke {
                path: path.clone(),
                radius: *radius,
            };
            (call.input("stroke", at(shape.clone())), described(&shape))
        }
        Given::Panorama => (call, format!("a panorama of {PW} by {PH}")),
    }
}

fn boxes(found: &[[u32; 4]]) -> String {
    let each: Vec<String> = (found.iter())
        .map(|[l, t, r, b]| format!("{l} {t} {r} {b}"))
        .collect();
    each.join(" and ")
}

fn described(shape: &Shape) -> String {
    let point = |p: &Point| format!("({}, {})", p.x, p.y);
    match shape {
        Shape::Box {
            left,
            top,
            right,
            bottom,
        } => format!("a box {left} {top} {right} {bottom}"),
        Shape::Polygon { points } => {
            let each: Vec<String> = points.iter().map(point).collect();
            format!("a polygon {}", each.join(" "))
        }
        Shape::Stroke { path, radius } => {
            let each: Vec<String> = path.iter().map(point).collect();
            format!("a stroke {}, radius {radius}", each.join(" to "))
        }
        other => format!("{other:?}"),
    }
}

fn examples(tool: &str) -> Vec<Example> {
    cases(tool)
        .into_iter()
        .map(|(name, base, given)| {
            let found = match &given {
                Given::Phrase(found) => found.clone(),
                _ => Vec::new(),
            };
            let shop = shop(&format!("{tool}-{name}"), &found);
            let (call, said) = call(tool, &shop, &base, &given);
            let outputs = shop.completed(&call);
            let ports: Vec<&str> = outputs.iter().map(|(port, _)| port.as_str()).collect();
            assert_eq!(ports, ["image", "mask"], "{tool}");
            let out = shop.image(outputs[0].1);
            let mask = shop.mask(outputs[1].1);
            Example {
                name,
                said,
                base,
                rebound: mask.image() == out.identity().pixels,
                mask,
                out: out.pixels(&DecodeLimits::default()).unwrap(),
            }
        })
        .collect()
}

/// Whether the mask weighs the pixel at `at` nothing.
fn nothing(mask: &Mask, at: usize) -> bool {
    mask.protect()[at] != 0 || (mask.core()[at] == 0 && mask.blend()[at] == 0)
}

fn pixel(p: &CanonicalPixels, at: usize) -> &[u8] {
    &p.rgba()[at * 4..at * 4 + 4]
}

/// The columns of a panorama `width` wide that its seam's canvas holds: the strip at each
/// edge.
fn in_strips(x: u32, width: u32) -> bool {
    x < STRIP || x >= width - STRIP
}

pub fn page(tool: &'static str) -> Page {
    page_of(tool, &examples(tool))
}

fn page_of(tool: &'static str, examples: &[Example]) -> Page {
    let mut files = Vec::new();
    let mut measured = if tool == "seam" {
        String::from(
            "\n| Example | Given | Core, on the canvas | Blend | Protected | Pixels of the \
             panorama changed | Changed outside the two strips | Mask handed back |\n\
             |---|---|---|---|---|---|---|---|\n",
        )
    } else {
        String::from(
            "\n| Example | Given | Core | Blend | Protected | Pixels changed | Pixels it weighs \
             nothing, and the base's | Mask handed back |\n|---|---|---|---|---|---|---|---|\n",
        )
    };
    for example in examples {
        let (base, out, mask) = (&example.base, &example.out, &example.mask);
        files.push((format!("{}.png", example.name), image_picture(base, None)));
        files.push((format!("{}-mask.png", example.name), picture(mask)));
        files.push((
            format!("{}-out.png", example.name),
            image_picture(out, None),
        ));
        let all = (base.width() * base.height()) as usize;
        let changed: Vec<usize> = (0..all)
            .filter(|at| pixel(base, *at) != pixel(out, *at))
            .collect();
        let planes = format!(
            "{} | {} | {}",
            count(mask.core()),
            count(mask.blend()),
            count(mask.protect())
        );
        if tool == "seam" {
            let width = base.width();
            let outside = (changed.iter())
                .filter(|at| !in_strips(**at as u32 % width, width))
                .count();
            measured.push_str(&format!(
                "| {} | {} | {planes} | {} | {outside} | {} |\n",
                example.name,
                example.said,
                changed.len(),
                if mask.image() == canvas_digest(base) {
                    "on the canvas, with its tone ramp"
                } else {
                    "NOT ON THE CANVAS"
                },
            ));
        } else {
            let nothing_at: Vec<usize> = (0..all).filter(|at| nothing(mask, *at)).collect();
            let kept = (nothing_at.iter())
                .filter(|at| pixel(base, **at) == pixel(out, **at))
                .count();
            measured.push_str(&format!(
                "| {} | {} | {planes} | {} | {kept} of {} | {} |\n",
                example.name,
                example.said,
                changed.len(),
                nothing_at.len(),
                if example.rebound {
                    "on the composite"
                } else {
                    "NOT ON THE COMPOSITE"
                },
            ));
        }
    }
    measured.push('\n');
    Page {
        tool,
        files,
        measured,
    }
}

/// The digest of the canvas a cut of this panorama makes.
fn canvas_digest(panorama: &CanonicalPixels) -> whizbang_core::identity::PixelDigest {
    whizbang_rascal::cut::canvas_of(panorama, STRIP)
        .unwrap()
        .digest()
}

/// A whole run of the recipe through the executor, from a fresh store each time, at three
/// sizes: the picture is read from its file and every step's output written.
pub fn timed(tool: &str) -> String {
    let mut out = timed_head(&[&format!("A whole run of `{tool}` through the executor")]);
    let sizes = if tool == "seam" { PANORAMAS } else { SIZES };
    for (width, height) in sizes {
        let (w, h) = (f64::from(width), f64::from(height));
        let (sx, sy) = (w / f64::from(W), h / f64::from(H));
        let ran = time(
            5,
            || {
                let (base, given) = match tool {
                    "seam" => (panorama(width, height, false), Given::Panorama),
                    "repair" => (
                        drawn(width, height, &scratch([12.0, 30.0], [84.0, 30.0])),
                        Given::Stroke(
                            vec![
                                Point::new(10.0 * sx, 30.0 * sy),
                                Point::new(86.0 * sx, 30.0 * sy),
                            ],
                            6.0,
                        ),
                    ),
                    _ => {
                        let at = [30.0 * sx, 18.0 * sy, 50.0 * sx, 38.0 * sy];
                        let given = if tool == "remove_prompt" {
                            Given::Phrase(vec![at.map(|v| v as u32)])
                        } else {
                            Given::Location(boxed(at[0], at[1], at[2], at[3]))
                        };
                        (drawn(width, height, &ball(40.0, 28.0, 9.0)), given)
                    }
                };
                let found = match &given {
                    Given::Phrase(found) => found.clone(),
                    _ => Vec::new(),
                };
                let shop = shop(tool, &found);
                let (call, _) = call(tool, &shop, &base, &given);
                (shop, call)
            },
            |(shop, call)| {
                shop.completed(&call);
                shop
            },
        );
        out.push_str(&timed_row(width as usize, height as usize, &[ran]));
    }
    out.push('\n');
    out
}

// Each record is what its recipe gives: one test a recipe, so that they run side by side.

#[test]
fn the_record_of_remove_is_what_it_gives() {
    held("remove");
}

#[test]
fn the_record_of_remove_prompt_is_what_it_gives() {
    held("remove_prompt");
}

#[test]
fn the_record_of_repair_is_what_it_gives() {
    held("repair");
}

#[test]
fn the_record_of_repair_area_is_what_it_gives() {
    held("repair_area");
}

#[test]
fn the_record_of_seam_is_what_it_gives() {
    held("seam");
}

fn held(tool: &'static str) {
    {
        let examples = examples(tool);
        let page = page_of(tool, &examples);
        assert!(!page.measured.contains("NOT ON"), "{}", page.measured);
        hold(&page);
        for example in &examples {
            let (base, out, mask) = (&example.base, &example.out, &example.mask);
            let all = (base.width() * base.height()) as usize;
            if tool == "seam" {
                // Every pixel the seam changed is in a strip of the cut, and some are.
                let width = base.width();
                let changed = (0..all).filter(|at| pixel(base, *at) != pixel(out, *at));
                let mut changed = changed.peekable();
                assert!(changed.peek().is_some(), "{tool} {}", example.name);
                assert!(changed.all(|at| in_strips(at as u32 % width, width)));
                continue;
            }
            // What the records say in words: every pixel the mask weighs nothing is the
            // original's, and every core pixel not protected is the fill's whole colour.
            for at in 0..all {
                if nothing(mask, at) {
                    assert_eq!(pixel(base, at), pixel(out, at), "{tool} at {at}");
                } else if mask.core()[at] != 0 {
                    assert_eq!(pixel(out, at)[..3], BRICK, "{tool} at {at}");
                }
            }
        }
    }
}
