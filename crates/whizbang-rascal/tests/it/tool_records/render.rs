//! The record of `render`, `docs/reference/tools/render.md`. The examples call the
//! function the action runs; a rendering's file is read back by the core's decoder.

use whizbang_core::artifact::{Image, Mask, Source, UseCase};
use whizbang_core::identity::{Binding, CanonicalPixels, DecodeLimits, Space};
use whizbang_rascal::feather::feather;
use whizbang_rascal::render::{Profile, render};

use super::super::grow::{made_by, plane};
use super::super::paste::image_of;
use super::super::record::{picture, tiled, time};
use super::{Page, SIZES, hold, image_picture, scene, timed_head, timed_row};

const W: usize = 96;
const H: usize = 64;

fn never() -> bool {
    false
}

/// The mask of both examples, on the scene: an oval feathered by a sigma of 3, with a
/// protected bar across its right edge.
fn mask_on(image: &Image) -> Mask {
    let core = plane(W, H, |x, y| {
        let (dx, dy) = (x as i64 - 44, y as i64 - 30);
        dx * dx * 196 + dy * dy * 400 <= 78_400
    });
    let mut mask = Mask::from_core(
        image.binding(),
        UseCase::Remove,
        core,
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    mask.protect_union(&plane(W, H, |x, y| (60..64).contains(&x) && y < 44))
        .unwrap();
    feather(mask, 3.0, &never).unwrap().unwrap()
}

/// The pixels of a rendering, read back.
fn pixels(rendering: &Image) -> CanonicalPixels {
    rendering.pixels(&DecodeLimits::default()).unwrap()
}

/// The pixels over a checkerboard of 4 pixels, by their alpha, so the page shows what is
/// clear.
fn over_checker(pixels: &CanonicalPixels) -> CanonicalPixels {
    let (width, height) = (pixels.width(), pixels.height());
    let rgba = pixels.rgba().chunks(4).enumerate().flat_map(|(at, p)| {
        let (x, y) = (at as u32 % width, at as u32 / width);
        let ground = if (x / 4 + y / 4) % 2 == 0 {
            200u32
        } else {
            150
        };
        let a = u32::from(p[3]);
        let mix = |c: u8| ((u32::from(c) * a + ground * (255 - a) + 127) / 255) as u8;
        [mix(p[0]), mix(p[1]), mix(p[2]), 255]
    });
    CanonicalPixels::from_rgba(width, height, rgba.collect()).unwrap()
}

pub struct Rendered {
    pub mask: Mask,
    pub plain: CanonicalPixels,
    pub alpha: CanonicalPixels,
}

fn rendered() -> Rendered {
    let image = image_of(&scene(), "import");
    let mask = mask_on(&image);
    let plain = pixels(&render(&mask, Profile::Plain, None).unwrap());
    let scene = scene();
    let alpha = render(&mask, Profile::Alpha, Some((&scene, image.id()))).unwrap();
    Rendered {
        mask,
        plain,
        alpha: pixels(&alpha),
    }
}

pub fn page(tool: &'static str) -> Page {
    page_of(tool, &rendered())
}

fn page_of(tool: &'static str, made: &Rendered) -> Page {
    let files = vec![
        ("mask.png".to_owned(), picture(&made.mask)),
        ("plain.png".to_owned(), image_picture(&made.plain, None)),
        (
            "alpha.png".to_owned(),
            image_picture(&over_checker(&made.alpha), None),
        ),
    ];
    let grey =
        |p: &CanonicalPixels, v: fn(&[u8]) -> bool| p.rgba().chunks(4).filter(|c| v(c)).count();
    let measured = format!(
        "\n| Rendering | Pixels at 255 | Pixels between | Pixels at 0 |\n|---|---|---|---|\n\
         | plain, the grey | {} | {} | {} |\n| alpha, its alpha | {} | {} | {} |\n\n",
        grey(&made.plain, |c| c[0] == 255),
        grey(&made.plain, |c| c[0] != 0 && c[0] != 255),
        grey(&made.plain, |c| c[0] == 0),
        grey(&made.alpha, |c| c[3] == 255),
        grey(&made.alpha, |c| c[3] != 0 && c[3] != 255),
        grey(&made.alpha, |c| c[3] == 0),
    );
    Page {
        tool,
        files,
        measured,
    }
}

pub fn timed(_: &str) -> String {
    let mut out = timed_head(&["`render`, plain", "`render`, alpha"]);
    for (width, height) in SIZES {
        let rgba = (0..width * height)
            .flat_map(|at| [(at % 251) as u8, (at % 241) as u8, 90, 255])
            .collect();
        let pixels = CanonicalPixels::from_rgba(width as u32, height as u32, rgba).unwrap();
        let binding = Binding::root(
            pixels.digest(),
            Space::flat(width as u32, height as u32).unwrap(),
        );
        let mask = Mask::from_core(
            binding,
            UseCase::Remove,
            tiled(width, height).core().to_vec(),
            Source::default(),
            made_by("fixture"),
        )
        .unwrap();
        let id = whizbang_core::artifact::ArtifactId(pixels.digest().0);
        let plain = time(5, || (), |()| render(&mask, Profile::Plain, None).unwrap());
        let alpha = time(
            5,
            || (),
            |()| render(&mask, Profile::Alpha, Some((&pixels, id))).unwrap(),
        );
        out.push_str(&timed_row(width, height, &[plain, alpha]));
    }
    out.push('\n');
    out
}

#[test]
fn the_record_of_render_is_what_render_gives() {
    let made = rendered();
    hold(&page_of("render", &made));
    let at = |p: &CanonicalPixels, x: usize, y: usize| {
        let i = (y * W + x) * 4;
        [
            p.rgba()[i],
            p.rgba()[i + 1],
            p.rgba()[i + 2],
            p.rgba()[i + 3],
        ]
    };
    // The core is white and opaque; the bar is black and clear; the feather between.
    assert_eq!(at(&made.plain, 44, 30)[0], 255);
    assert_eq!(at(&made.plain, 61, 30)[0], 0);
    assert_eq!(at(&made.alpha, 61, 30)[3], 0);
    let (edge, weight) = (at(&made.alpha, 44, 46), made.mask.blend()[46 * W + 44]);
    assert!(weight > 0 && weight < 255 && edge[3] == weight);
    assert_eq!(at(&made.alpha, 44, 30)[..3], at(&scene(), 44, 30)[..3]);
}
