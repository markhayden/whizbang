//! `render` (plan 2A, task R2): `alpha` is the image with the mask's weight as its alpha,
//! `plain` the weight in grey; each is an image whose role is a rendering, read back
//! through the core's decoder.

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use whizbang_core::artifact::{ArtifactId, Image, ImageRole, Mask, Source, Step, UseCase};
use whizbang_core::identity::{
    CanonicalPixels, DecodeLimits, Digest, Projection, Topology, identify,
};
use whizbang_rascal::render::{Profile, render};

use super::grow::{made_by, plane};

/// A picture `w` by `h` whose pixel at `(x, y)` is `(x, y, x + y)`, opaque but for the
/// first pixel, whose alpha is 128: its pixels and an image holding it as a PNG.
pub fn picture(w: u32, h: u32) -> (CanonicalPixels, Image) {
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let alpha = if (x, y) == (0, 0) { 128 } else { 255 };
            rgba.extend_from_slice(&[x as u8 * 10, y as u8 * 10, (x + y) as u8, alpha]);
        }
    }
    let mut file = Vec::new();
    PngEncoder::new(&mut file)
        .write_image(&rgba, w, h, ExtendedColorType::Rgba8)
        .unwrap();
    let image = Image::new(
        file,
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        made_by("fixture"),
        &DecodeLimits::default(),
    )
    .unwrap();
    (CanonicalPixels::from_rgba(w, h, rgba).unwrap(), image)
}

/// A mask on `image`: a block of core, a feathered ring of blend around it, and one
/// protected pixel inside the block.
pub fn mask_on(image: &Image) -> Mask {
    let (w, h) = (
        image.space().width() as usize,
        image.space().height() as usize,
    );
    let core = plane(w, h, |x, y| (3..6).contains(&x) && (2..5).contains(&y));
    let mut mask = Mask::from_core(
        image.binding(),
        UseCase::Remove,
        core,
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    let blend = (0..w * h).map(|at| (at % 200) as u8 + 1).collect();
    mask.set_blend(blend).unwrap();
    let mut protect = vec![0u8; w * h];
    protect[3 * w + 4] = 255;
    mask.protect_union(&protect).unwrap();
    mask
}

fn weight_of(mask: &Mask, at: usize) -> u8 {
    match (mask.protect()[at], mask.core()[at]) {
        (0, 0) => mask.blend()[at],
        (0, _) => 255,
        _ => 0,
    }
}

#[test]
fn alpha_is_the_image_with_the_masks_weight_as_alpha() {
    let (pixels, image) = picture(9, 7);
    let mask = mask_on(&image);
    let rendering = render(&mask, Profile::Alpha, Some((&pixels, image.id()))).unwrap();
    assert_eq!(rendering.role(), ImageRole::Rendering);
    assert_eq!(rendering.space(), mask.space());
    let shown = identify(rendering.file(), &DecodeLimits::default())
        .unwrap()
        .pixels;
    for (at, (out, given)) in shown
        .rgba()
        .chunks(4)
        .zip(pixels.rgba().chunks(4))
        .enumerate()
    {
        assert_eq!(out[..3], given[..3], "pixel {at}");
        // The image's alpha times the weight over 255: the weight where it is opaque.
        let weight = u32::from(weight_of(&mask, at));
        let expected = (u32::from(given[3]) * weight + 127) / 255;
        assert_eq!(u32::from(out[3]), expected, "pixel {at}");
    }
    // The core is opaque, the protected pixel in it clear, the first pixel half its
    // blend of 1, which rounds to 1.
    assert_eq!(shown.rgba()[(3 * 9 + 3) * 4 + 3], 255);
    assert_eq!(shown.rgba()[(3 * 9 + 4) * 4 + 3], 0);
    assert_eq!(shown.rgba()[3], 1);
    assert_eq!(
        rendering.origin().steps(),
        [Step::new("render", "1.0.0")
            .argument("profile", "alpha")
            .input("mask", mask.id().0)
            .input("image", image.id().0)]
    );
    // Without the image, or on another image's pixels, it is refused.
    assert!(render(&mask, Profile::Alpha, None).is_err());
    let (other, _) = picture(9, 7);
    let mut changed = other.rgba().to_vec();
    changed[0] = 99;
    let other = CanonicalPixels::from_rgba(9, 7, changed).unwrap();
    let id = ArtifactId(Digest::of_bytes(b"other"));
    let refused = render(&mask, Profile::Alpha, Some((&other, id))).unwrap_err();
    assert!(refused.contains("belongs to the image"), "{refused}");
}

#[test]
fn plain_is_the_masks_weight_in_grey() {
    let (_, image) = picture(9, 7);
    let mask = mask_on(&image);
    let rendering = render(&mask, Profile::Plain, None).unwrap();
    assert_eq!(rendering.role(), ImageRole::Rendering);
    let shown = identify(rendering.file(), &DecodeLimits::default())
        .unwrap()
        .pixels;
    for (at, out) in shown.rgba().chunks(4).enumerate() {
        let weight = weight_of(&mask, at);
        assert_eq!(out, [weight, weight, weight, 255], "pixel {at}");
    }
    // The same mask renders to the same file: its identity is the same everywhere.
    assert_eq!(
        render(&mask, Profile::Plain, None).unwrap().id(),
        rendering.id()
    );
}

#[test]
fn the_ported_vectors_give_rascals_grey() {
    // Rascal's world (`crates/rascal-ops/tests/it/render.rs` at the commit
    // `docs/sources.toml` pins): 8 by 4, the core at columns 2 and 3, a blend of 51 at
    // column 4 and 128 at column 5, protect at (3, 1) and (5, 2). Rascal's `grey` is
    // `plain`: 255 times the weight, rounded, which is the byte itself.
    let (_, image) = picture(8, 4);
    let mut mask = Mask::from_core(
        image.binding(),
        UseCase::Custom,
        plane(8, 4, |x, _| (2..4).contains(&x)),
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    let blend = (0..32).map(|at| match at % 8 {
        4 => 51,
        5 => 128,
        _ => 0,
    });
    mask.set_blend(blend.collect()).unwrap();
    mask.protect_union(&plane(8, 4, |x, y| (x, y) == (3, 1) || (x, y) == (5, 2)))
        .unwrap();
    let grey = whizbang_rascal::render::plain(&mask);
    let rows: Vec<&[u8]> = grey.chunks(8).collect();
    assert_eq!(rows[0], [0, 0, 255, 255, 51, 128, 0, 0]);
    assert_eq!(rows[1], [0, 0, 255, 0, 51, 128, 0, 0]);
    assert_eq!(rows[2], [0, 0, 255, 255, 51, 0, 0, 0]);
    for (at, byte) in grey.iter().enumerate() {
        let (x, y) = ((at % 8) as u32, (at / 8) as u32);
        let weight = mask.weight(x, y).unwrap();
        assert_eq!(*byte, (255.0 * weight).round() as u8, "({x}, {y})");
    }
}
