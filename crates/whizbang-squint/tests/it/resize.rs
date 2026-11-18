//! `resize` (plan `docs/plans/m2c-milestone-2.md`, task A2, item 1): an image fitted to the
//! long edge asked, its aspect kept, bound onto the whole of the image it was made from.

use image::RgbImage;
use image::imageops::{self, FilterType};
use whizbang_core::identity::Mapping;
use whizbang_squint::resize::{ResizeError, resize};

use super::support::*;

#[test]
fn a_picture_is_fitted_to_the_size_asked_keeping_its_aspect() {
    let image = picture(200, 100, noisy(7, false));
    let resized = resize(&image, 64, &limits()).unwrap();
    // The longer side is the one asked; the shorter keeps the aspect, to the nearest pixel.
    let space = resized.image.space();
    assert_eq!((space.width(), space.height()), (64, 32));
    let tall = resize(&picture(90, 250, noisy(8, false)), 100, &limits()).unwrap();
    assert_eq!(
        (tall.image.space().width(), tall.image.space().height()),
        (36, 100)
    );
    // The pixels are squint's: the image crate's triangle filter over red, green and blue.
    let rgb: Vec<u8> = rgba_of(&image)
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let want = imageops::resize(
        &RgbImage::from_raw(200, 100, rgb).unwrap(),
        64,
        32,
        FilterType::Triangle,
    );
    let got: Vec<u8> = rgba_of(&resized.image)
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    assert_eq!(got, want.into_raw());
    // A flat colour stays that colour.
    let flat = resize(&picture(150, 120, |_, _| [40, 90, 200, 255]), 75, &limits()).unwrap();
    assert!(
        rgba_of(&flat.image)
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [40, 90, 200, 255])
    );
    // Bound onto the whole of the image: a crop of all of it, stated from the result.
    assert_eq!(resized.binding.source(), resized.image.identity().pixels);
    assert_eq!(
        resized.binding.mapping(),
        &Mapping::Crop {
            left: 0.0,
            top: 0.0,
            right: 200.0,
            bottom: 100.0
        }
    );
    // Never enlarged, and only within 64 to 1024: each refused with a sentence.
    for (long_edge, said) in [
        (200, "would not shrink"),
        (300, "would not shrink"),
        (63, "outside [64, 1024]"),
        (1025, "outside [64, 1024]"),
    ] {
        match resize(&image, long_edge, &limits()) {
            Err(ResizeError::Invalid(why)) => assert!(why.contains(said), "{why}"),
            other => panic!("{long_edge}: {other:?}"),
        }
    }
}
