//! The records of the tasks `mask_rect` and `protect`, `docs/reference/tools/<task>.md`:
//! each example runs the shipped recipe through the executor; `protect`'s first pass under
//! a scripted filler, since a test makes no live call.

use std::sync::Arc;

use whizbang_core::artifact::{Artifact, ArtifactId, Location, Mask, Shape, Source, UseCase};
use whizbang_core::contract::{Call, Value};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits};
use whizbang_testkit::capabilities::{FakeFiller, Paint};

use super::super::grow::made_by;
use super::super::paste::image_of;
use super::super::record::{picture, tiled, time};
use super::super::shaping::Shop;
use super::{Page, count, hold, image_picture, scene, timed_head, timed_row};

/// The sizes the tasks are timed at, through the executor: each run writes its outputs'
/// files, and the image is read from its file.
const SIZES: [(usize, usize); 3] = [(1024, 768), (2048, 1536), (4096, 3072)];

/// A shop whose filler paints a brick red wherever the support is.
fn shop(label: &str) -> Shop {
    Shop::new(
        label,
        Some(Arc::new(FakeFiller::new(Paint::Support([200, 60, 50])))),
    )
}

fn use_case(mask: &Mask) -> String {
    format!("{:?}", mask.use_case()).to_lowercase()
}

/// `mask_rect`'s examples: the box given, and the mask made.
fn rects(shop: &Shop) -> Vec<Rect> {
    let image = shop.put(Artifact::Image(image_of(&scene(), "import")));
    [("box", "20 10 70 40"), ("edge", "60.4 30.5 120 80")]
        .into_iter()
        .map(|(name, text)| {
            let call = Call::new("mask_rect")
                .input("image", image)
                .argument("box", Value::text(text));
            (name, text, shop.mask(shop.one(&call)))
        })
        .collect()
}

/// `protect`'s examples: the first pass's mask, its composite, and the mask for the
/// second pass. The first pass is `mask_rect` and `fill`, or `remove`, on one box.
fn passes(shop: &Shop) -> Vec<Pass> {
    let base = image_of(&scene(), "import");
    let image = shop.put(Artifact::Image(base.clone()));
    let first = |name: &str| -> (ArtifactId, ArtifactId) {
        let outputs = if name == "block" {
            let mask = shop.one(
                &Call::new("mask_rect")
                    .input("image", image)
                    .argument("box", Value::text("24 16 56 44")),
            );
            shop.completed(&Call::new("fill").input("image", image).input("mask", mask))
        } else {
            let shape = Shape::Box {
                left: 24.0,
                top: 16.0,
                right: 56.0,
                bottom: 44.0,
            };
            let location = Location::new(base.binding(), shape, made_by("rect")).unwrap();
            let location = shop.put(Artifact::Location(location));
            shop.completed(
                &Call::new("remove")
                    .input("image", image)
                    .input("location", location),
            )
        };
        (outputs[0].1, outputs[1].1)
    };
    [
        ("block", "`mask_rect`, then `fill`"),
        ("feathered", "`remove`"),
    ]
    .into_iter()
    .map(|(name, by)| {
        let (composite, mask) = first(name);
        let made = shop.one(
            &Call::new("protect")
                .input("image", composite)
                .input("mask", mask),
        );
        let pixels = shop.image(composite).pixels(&DecodeLimits::default());
        (name, by, shop.mask(mask), pixels.unwrap(), shop.mask(made))
    })
    .collect()
}

/// A `mask_rect` example: its name, the box, and the mask made.
type Rect = (&'static str, &'static str, Mask);
/// A `protect` example: its name, the first pass, its mask and composite, and the mask made.
type Pass = (&'static str, &'static str, Mask, CanonicalPixels, Mask);

pub fn page(tool: &'static str) -> Page {
    if tool == "mask_rect" {
        page_of(tool, &rects(&shop(tool)), &[])
    } else {
        page_of(tool, &[], &passes(&shop(tool)))
    }
}

fn page_of(tool: &'static str, rects: &[Rect], passes: &[Pass]) -> Page {
    let mut files = Vec::new();
    let mut measured = String::new();
    if tool == "mask_rect" {
        measured.push_str(
            "\n| Example | `box` | Core | Blend | Protected | Use case |\n|---|---|---|---|---|---|\n",
        );
        for (name, text, mask) in rects {
            files.push((format!("{name}.png"), picture(mask)));
            measured.push_str(&format!(
                "| {name} | \"{text}\" | {} | {} | {} | {} |\n",
                count(mask.core()),
                count(mask.blend()),
                count(mask.protect()),
                use_case(mask),
            ));
        }
    } else {
        measured.push_str(
            "\n| Example | First pass | Its core | Its blend | Core after | Blend after \
             | Protected after | Use case |\n|---|---|---|---|---|---|---|---|\n",
        );
        for (name, by, first, composite, made) in passes {
            files.push((format!("{name}-first.png"), picture(first)));
            files.push((
                format!("{name}-composite.png"),
                image_picture(composite, None),
            ));
            files.push((format!("{name}-protect.png"), picture(made)));
            measured.push_str(&format!(
                "| {name} | {by} | {} | {} | {} | {} | {} | {} |\n",
                count(first.core()),
                count(first.blend()),
                count(made.core()),
                count(made.blend()),
                count(made.protect()),
                use_case(made),
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

pub fn timed(tool: &str) -> String {
    let mut out = timed_head(&[&format!("A whole run of `{tool}` through the executor")]);
    for (width, height) in SIZES {
        let ran = time(
            5,
            || {
                let shop = shop(tool);
                let rgba = (0..width * height)
                    .flat_map(|at| [(at % 251) as u8, (at % 241) as u8, 90, 255])
                    .collect();
                let pixels = CanonicalPixels::from_rgba(width as u32, height as u32, rgba);
                let image = image_of(&pixels.unwrap(), "import");
                let id = shop.put(Artifact::Image(image.clone()));
                let call = if tool == "mask_rect" {
                    let text = format!("{} {} {} {}", width / 5, height / 5, width / 2, height / 2);
                    Call::new(tool)
                        .input("image", id)
                        .argument("box", Value::text(&text))
                } else {
                    let first = Mask::from_core(
                        image.binding(),
                        UseCase::Remove,
                        tiled(width, height).core().to_vec(),
                        Source::default(),
                        made_by("fill"),
                    );
                    let mask = shop.put(Artifact::Mask(first.unwrap()));
                    Call::new(tool).input("image", id).input("mask", mask)
                };
                (shop, call)
            },
            |(shop, call)| {
                shop.one(&call);
                shop
            },
        );
        out.push_str(&timed_row(width, height, &[ran]));
    }
    out.push('\n');
    out
}

#[test]
fn the_records_of_mask_rect_and_protect_are_what_they_give() {
    let shop = shop("tasks");
    let (rects, passes) = (rects(&shop), passes(&shop));
    hold(&page_of("mask_rect", &rects, &[]));
    hold(&page_of("protect", &[], &passes));
    // What the records say in words: `mask_rect`'s core is the box's pixels, cut at the
    // image's edges, with no blend and nothing protected.
    assert_eq!(count(rects[0].2.core()), 50 * 30);
    assert_eq!(count(rects[1].2.core()), 36 * 34);
    // `protect`: everything the first mask reached is protected, the rest is the core;
    // after `remove`, that is the feather too, to its faintest pixel.
    for (_, _, first, _, made) in &passes {
        for at in 0..96 * 64 {
            let reached = first.core()[at] != 0 || first.blend()[at] != 0;
            assert_eq!(made.protect()[at] != 0, reached, "at {at}");
            assert_eq!(made.core()[at] != 0, !reached, "at {at}");
        }
        assert_eq!(made.use_case(), UseCase::Protect);
    }
}
