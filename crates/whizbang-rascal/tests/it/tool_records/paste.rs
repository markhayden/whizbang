//! The records of `paste` and `fill`, `docs/reference/tools/<tool>.md`: each example is a
//! call through the executor; `fill`'s under a scripted filler, since a test makes no
//! live call.

use std::sync::Arc;

use whizbang_core::artifact::{Artifact, Image, Mask, Source, UseCase};
use whizbang_core::capability::{Filler, Painted, Picture};
use whizbang_core::contract::Call;
use whizbang_core::identity::{Binding, CanonicalPixels, DecodeLimits, Space, encode};
use whizbang_rascal::composite::composite;
use whizbang_rascal::feather::feather;
use whizbang_testkit::capabilities::{FakeFiller, Paint};

use super::super::grow::{made_by, plane};
use super::super::paste::image_of;
use super::super::record::{picture, tiled, time};
use super::super::shaping::Shop;
use super::{Page, SIZES, hold, image_picture, scene, timed_head, timed_row};

const W: usize = 96;
const H: usize = 64;
/// The colour of the fills: a brick red.
const BRICK: [u8; 3] = [200, 60, 50];

fn never() -> bool {
    false
}

fn pixels_of(image: &Image) -> CanonicalPixels {
    image.pixels(&DecodeLimits::default()).unwrap()
}

/// A disc of radius 12 at (48, 30), feathered by a sigma of 3, on `image`.
fn disc(image: &Image) -> Mask {
    let core = plane(W, H, |x, y| {
        (x as i64 - 48).pow(2) + (y as i64 - 30).pow(2) <= 144
    });
    let mask = Mask::from_core(
        image.binding(),
        UseCase::Remove,
        core,
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    feather(mask, 3.0, &never).unwrap().unwrap()
}

/// A block from (20, 12) to (75, 51), with no blend, and rows 28 to 33 of it protected.
fn striped(image: &Image) -> Mask {
    let core = plane(W, H, |x, y| (20..76).contains(&x) && (12..52).contains(&y));
    let mut mask = Mask::from_core(
        image.binding(),
        UseCase::Remove,
        core,
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    mask.protect_union(&plane(W, H, |x, y| {
        (16..80).contains(&x) && (28..34).contains(&y)
    }))
    .unwrap();
    mask
}

/// A picture of the scene's size, every pixel `rgb`, or every pixel the scene's inverse.
fn painted(rgb: Option<[u8; 3]>) -> CanonicalPixels {
    let scene = scene();
    let rgba = scene.rgba().chunks(4).flat_map(|p| match rgb {
        Some([r, g, b]) => [r, g, b, 255],
        None => [255 - p[0], 255 - p[1], 255 - p[2], 255],
    });
    CanonicalPixels::from_rgba(96, 64, rgba.collect()).unwrap()
}

struct Example {
    name: &'static str,
    /// What paints: the fill given, or the scripted filler.
    by: &'static str,
    base: CanonicalPixels,
    fill: Option<CanonicalPixels>,
    mask: Mask,
    out: CanonicalPixels,
    /// The mask `fill` handed back, and whether it is on the composite.
    rebound: Option<bool>,
}

fn examples(tool: &str) -> Vec<Example> {
    let base = image_of(&scene(), "import");
    let cases: [(
        &'static str,
        &'static str,
        fn(&Image) -> Mask,
        Option<[u8; 3]>,
    ); 2] = [
        ("feathered", "a brick red", disc, Some(BRICK)),
        ("protected", "the scene's inverse", striped, None),
    ];
    cases
        .into_iter()
        .map(|(name, by, mask_of, rgb)| {
            let mask = mask_of(&base);
            let paint = rgb.map_or(Paint::Inverse, Paint::Support);
            let filler: Option<Arc<dyn Filler>> =
                (tool == "fill").then(|| Arc::new(FakeFiller::new(paint)) as Arc<dyn Filler>);
            let shop = Shop::new(tool, filler);
            let (image_id, mask_id) = (
                shop.put(Artifact::Image(base.clone())),
                shop.put(Artifact::Mask(mask.clone())),
            );
            let mut call = Call::new(tool)
                .input("image", image_id)
                .input("mask", mask_id);
            let fill = (tool == "paste").then(|| painted(rgb));
            if let Some(fill) = &fill {
                call = call.input("fill", shop.put(Artifact::Image(image_of(fill, "import"))));
            }
            let outputs = shop.completed(&call);
            let out = shop.image(outputs[0].1);
            let rebound = outputs
                .get(1)
                .map(|(_, id)| shop.mask(*id).image() == out.identity().pixels);
            Example {
                name,
                by: if tool == "fill" {
                    "the scripted filler"
                } else {
                    by
                },
                base: pixels_of(&base),
                fill,
                mask,
                out: pixels_of(&out),
                rebound,
            }
        })
        .collect()
}

/// Whether the mask weighs the pixel at `at` nothing.
fn nothing(mask: &Mask, at: usize) -> bool {
    mask.protect()[at] != 0 || (mask.core()[at] == 0 && mask.blend()[at] == 0)
}

pub fn page(tool: &'static str) -> Page {
    page_of(tool, &examples(tool))
}

fn page_of(tool: &'static str, examples: &[Example]) -> Page {
    let mut files = Vec::new();
    let mut measured = String::from(
        "\n| Example | Painted by | Pixels the mask weighs over 0 | Pixels changed | Pixels \
         it weighs nothing, and the base's | Mask handed back |\n|---|---|---|---|---|---|\n",
    );
    for example in examples {
        let (base, out, mask) = (&example.base, &example.out, &example.mask);
        files.push((format!("{}.png", example.name), image_picture(base, None)));
        files.push((format!("{}-mask.png", example.name), picture(mask)));
        if let Some(fill) = &example.fill {
            files.push((
                format!("{}-fill.png", example.name),
                image_picture(fill, None),
            ));
        }
        files.push((
            format!("{}-out.png", example.name),
            image_picture(out, None),
        ));
        let pixel = |p: &CanonicalPixels, at: usize| p.rgba()[at * 4..at * 4 + 4].to_vec();
        let weighed = (0..W * H).filter(|at| !nothing(mask, *at)).count();
        let changed = (0..W * H)
            .filter(|at| pixel(base, *at) != pixel(out, *at))
            .count();
        let nothing_at: Vec<usize> = (0..W * H).filter(|at| nothing(mask, *at)).collect();
        let kept = nothing_at
            .iter()
            .filter(|at| pixel(base, **at) == pixel(out, **at));
        measured.push_str(&format!(
            "| {} | {} | {weighed} | {changed} | {} of {} | {} |\n",
            example.name,
            example.by,
            kept.count(),
            nothing_at.len(),
            match example.rebound {
                None => "none",
                Some(true) => "on the composite",
                Some(false) => "NOT ON THE COMPOSITE",
            },
        ));
    }
    measured.push('\n');
    Page {
        tool,
        files,
        measured,
    }
}

pub fn timed(tool: &str) -> String {
    let mut out = timed_head(&[
        &format!("`{tool}`: the answer pasted under the mask"),
        "The composite's PNG written",
    ]);
    for (width, height) in SIZES {
        let rgba = (0..width * height)
            .flat_map(|at| [(at % 251) as u8, (at % 241) as u8, (at % 239) as u8, 255])
            .collect();
        let pixels = CanonicalPixels::from_rgba(width as u32, height as u32, rgba).unwrap();
        let binding = Binding::root(
            pixels.digest(),
            Space::flat(width as u32, height as u32).unwrap(),
        );
        let tiles = tiled(width, height);
        let mask = Mask::from_core(
            binding,
            UseCase::Remove,
            tiles.core().to_vec(),
            Source::default(),
            made_by("fixture"),
        )
        .unwrap();
        let mask = feather(mask, 3.0, &never).unwrap().unwrap();
        let rgb = vec![128u8; width * height * 3];
        let answer = Painted::from_rgb(width as u32, height as u32, rgb).unwrap();
        let image = Picture::new(pixels).unwrap();
        let pasted = time(5, || (), |()| composite(&image, &answer, &mask).unwrap());
        let made = composite(&image, &answer, &mask).unwrap();
        let written = time(5, || (), |()| encode::png(made.pixels()).unwrap());
        out.push_str(&timed_row(width, height, &[pasted, written]));
    }
    out.push('\n');
    out
}

#[test]
fn the_records_of_paste_and_fill_are_what_they_give() {
    for tool in ["paste", "fill"] {
        let examples = examples(tool);
        let page = page_of(tool, &examples);
        assert!(!page.measured.contains("NOT ON"), "{}", page.measured);
        hold(&page);
        // What the records say in words: inside the core the fill's colour; on the
        // feather a mix of the two; under protect and outside the mask, the base.
        let [feathered, protected] = <[Example; 2]>::try_from(examples).ok().unwrap();
        let rgb = |p: &CanonicalPixels, x: usize, y: usize| {
            let at = (y * W + x) * 4;
            [p.rgba()[at], p.rgba()[at + 1], p.rgba()[at + 2]]
        };
        assert_eq!(rgb(&feathered.out, 48, 30), BRICK);
        let (edge, base) = (rgb(&feathered.out, 48, 44), rgb(&feathered.base, 48, 44));
        assert!(edge != BRICK && edge != base, "{edge:?}");
        assert_eq!(rgb(&feathered.out, 48, 60), rgb(&feathered.base, 48, 60));
        assert_eq!(rgb(&protected.out, 40, 30), rgb(&protected.base, 40, 30));
        let flipped = rgb(&protected.base, 40, 20).map(|c| 255 - c);
        assert_eq!(rgb(&protected.out, 40, 20), flipped);
    }
}
