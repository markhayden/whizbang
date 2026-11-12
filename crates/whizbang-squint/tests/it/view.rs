//! `view` on flat images: the crop it cuts, the snapping, the bound, the second image,
//! what it refuses, and what a crop carries. The first four are squint's tests of its
//! `view` (`crates/squint-tools/tests/it/builtin.rs`), on artifacts and bindings.

use whizbang_core::artifact::ImageRole;
use whizbang_core::identity::{Destination, Direction, Projection, Space, Topology};
use whizbang_squint::view::{MAX_SIDE, MIN_CROP, Request, ViewError, view};

use super::support::*;

/// The message of a request refused as the caller's mistake.
fn refused(request: &Request<'_>) -> String {
    match view(request, &limits()) {
        Err(ViewError::Invalid(why)) => why,
        other => panic!("expected a refusal of the request, got {other:?}"),
    }
}

#[test]
fn a_crop_is_the_region_it_cuts() {
    let colour = noisy(1, false);
    let image = picture(512, 384, &colour);
    let crop = look(&image, [0.25, 0.25, 0.75, 0.75]);

    let space = crop.image.space();
    assert_eq!((space.width(), space.height()), (256, 192));
    let expected = paint(256, 192, |x, y| colour(x + 128, y + 96));
    assert_eq!(rgba_of(&crop.image), expected, "pixel for pixel");

    // The crop is bound onto the image it was cut from, by the pixels that were cut.
    let binding = &crop.binding;
    assert_eq!(binding.source(), crop.image.identity().pixels);
    assert_eq!(binding.source_space(), space);
    assert_eq!(
        binding.destination(),
        Destination::Image(image.identity().pixels)
    );
    assert_eq!(binding.destination_space(), image.space());
    assert_eq!(binding.direction(), Direction::SourceToDestination);
    assert_eq!(cut_of(binding), [128.0, 96.0, 384.0, 288.0]);
}

#[test]
fn a_box_snaps_outward_to_whole_pixels_and_the_long_edge_is_capped() {
    let image = picture(2048, 64, noisy(2, false));
    let crop = look(&image, [0.1001, 0.2, 0.9, 0.8]);
    // 0.1001 x 2048 = 205.0048 snaps down to pixel 205 and 0.9 x 2048 = 1843.2 up to
    // 1844; 0.2 x 64 = 12.8 down to 12 and 0.8 x 64 = 51.2 up to 52. The binding records
    // the pixels actually cut, and the 1639 pixel cut is scaled to the cap: 40 rows
    // become 40 x 1024 / 1639 = 24.99, which is 25.
    assert_eq!(cut_of(&crop.binding), [205.0, 12.0, 1844.0, 52.0]);
    let space = crop.image.space();
    assert_eq!((space.width(), space.height()), (MAX_SIDE, 25));
}

#[test]
fn a_cut_at_the_cap_is_not_scaled_and_one_pixel_over_is() {
    let image = picture(1025, 16, noisy(3, false));
    let at = look(&image, [0.0, 0.0, 1024.0 / 1025.0, 1.0]);
    assert_eq!(cut_of(&at.binding), [0.0, 0.0, 1024.0, 16.0]);
    assert_eq!(at.image.space(), Space::flat(1024, 16).unwrap());
    let over = look(&image, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(cut_of(&over.binding), [0.0, 0.0, 1025.0, 16.0]);
    assert_eq!(over.image.space(), Space::flat(1024, 16).unwrap());
    assert_ne!(at.image.identity().pixels, over.image.identity().pixels);
}

#[test]
fn a_box_that_ends_on_a_pixel_edge_is_not_widened() {
    let image = picture(40, 30, noisy(4, false));
    let crop = look(&image, [0.25, 0.2, 0.75, 0.8]);
    assert_eq!(cut_of(&crop.binding), [10.0, 6.0, 30.0, 24.0]);
    // The whole image is itself.
    let whole = look(&image, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(cut_of(&whole.binding), [0.0, 0.0, 40.0, 30.0]);
    assert_eq!(rgba_of(&whole.image), rgba_of(&image));
}

#[test]
fn a_fraction_a_hair_past_a_pixel_edge_widens_the_cut_by_a_pixel() {
    // A known limit, in the record. 0.07 of 100 pixels was meant as the edge of pixel 7,
    // but the nearest double to 0.07 times 100 is 7.000000000000001, and the box snaps
    // outward, to 8. 0.7 of 10 is 7 exactly, and stays.
    let image = picture(100, 10, noisy(16, false));
    let crop = look(&image, [0.0, 0.0, 0.07, 1.0]);
    assert_eq!(cut_of(&crop.binding), [0.0, 0.0, 8.0, 10.0]);
    let image = picture(10, 100, noisy(17, false));
    let crop = look(&image, [0.0, 0.0, 1.0, 0.7]);
    assert_eq!(cut_of(&crop.binding), [0.0, 0.0, 10.0, 70.0]);
}

#[test]
fn the_same_region_is_shown_in_a_second_image() {
    let first = picture(256, 192, noisy(5, false));
    let second = picture(512, 384, noisy(6, false));
    let crops = view(
        &Request {
            image: &first,
            also: Some(&second),
            bbox: [0.5, 0.5, 1.0, 1.0],
        },
        &limits(),
    )
    .unwrap();
    assert_eq!(crops.len(), 2);
    // In the order asked, each bound onto its own source, the box taken as fractions of
    // each.
    assert_eq!(
        crops[0].binding.destination(),
        Destination::Image(first.identity().pixels)
    );
    assert_eq!(
        crops[1].binding.destination(),
        Destination::Image(second.identity().pixels)
    );
    assert_eq!(cut_of(&crops[0].binding), [128.0, 96.0, 256.0, 192.0]);
    assert_eq!(cut_of(&crops[1].binding), [256.0, 192.0, 512.0, 384.0]);
    assert_eq!(crops[0].image.space().width(), 128);
    assert_eq!(crops[1].image.space().width(), 256);
    // Each is the crop the image alone would give.
    assert_eq!(crops[0], look(&first, [0.5, 0.5, 1.0, 1.0]));
    assert_eq!(crops[1], look(&second, [0.5, 0.5, 1.0, 1.0]));
}

#[test]
fn a_bad_request_is_the_callers_mistake() {
    let image = picture(256, 192, noisy(7, false));
    let other = picture(256, 192, noisy(8, false));
    let tiny = picture(64, 4, noisy(9, false));
    let alone = |bbox| Request {
        image: &image,
        also: None,
        bbox,
    };
    let cases = [
        (alone([0.5, 0.5, 0.4, 0.9]), "left < right"),
        (alone([0.1, 0.5, 0.4, 0.5]), "top < bottom"),
        // A box past an edge is not clipped: it is refused.
        (alone([0.9, 0.25, 1.1, 0.75]), "fractions"),
        (alone([-0.1, 0.25, 0.5, 0.75]), "fractions"),
        (alone([f64::NAN, 0.25, 0.5, 0.75]), "fractions"),
        (alone([0.0, 0.0, f64::INFINITY, 1.0]), "fractions"),
        // 0.5 to 0.51 of 256 pixels is pixels 128 to 131: three across.
        (alone([0.5, 0.5, 0.51, 0.51]), "at least 8x8"),
        (
            Request {
                image: &image,
                also: Some(&image),
                bbox: [0.1, 0.1, 0.5, 0.5],
            },
            "a different image",
        ),
        // The second image is too small, though the first is not.
        (
            Request {
                image: &other,
                also: Some(&tiny),
                bbox: [0.0, 0.0, 1.0, 1.0],
            },
            "covers only 64x4 pixels",
        ),
    ];
    for (request, needle) in cases {
        let why = refused(&request);
        assert!(
            why.contains(needle),
            "{:?}: {why:?} should mention {needle:?}",
            request.bbox
        );
    }
    assert_eq!(MIN_CROP, 8);
    // Exactly eight across and eight down is enough.
    let least = look(&image, [0.0, 0.0, 8.0 / 256.0, 8.0 / 192.0]);
    assert_eq!(least.image.space(), Space::flat(8, 8).unwrap());
}

#[test]
fn the_same_picture_in_another_file_is_not_a_second_image() {
    let rgba = paint(64, 48, noisy(10, false));
    let one = image_in(png(64, 48, &rgba), Projection::Flat, Topology::Bounded);
    // The same pixels without the alpha channel: another file, the same picture.
    let rgb: Vec<u8> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut file = Vec::new();
    {
        use image::ImageEncoder;
        image::codecs::png::PngEncoder::new(&mut file)
            .write_image(&rgb, 64, 48, image::ExtendedColorType::Rgb8)
            .unwrap();
    }
    let two = image_in(file, Projection::Flat, Topology::Bounded);
    assert_ne!(one.identity().file, two.identity().file);
    assert_eq!(one.identity().pixels, two.identity().pixels);
    let why = refused(&Request {
        image: &one,
        also: Some(&two),
        bbox: [0.0, 0.0, 1.0, 1.0],
    });
    assert!(why.contains("a different image"), "{why}");
}

#[test]
fn a_panorama_is_not_viewed_yet() {
    let rgba = paint(128, 64, noisy(11, false));
    let flat = image_in(png(128, 64, &rgba), Projection::Flat, Topology::Bounded);
    let other = paint(128, 64, noisy(12, false));
    for topology in [Topology::Wrap, Topology::Bounded] {
        let panorama = image_in(png(128, 64, &other), Projection::Equirectangular, topology);
        let why = refused(&Request {
            image: &panorama,
            also: None,
            bbox: [0.0, 0.25, 0.25, 0.75],
        });
        assert!(why.contains("image is a panorama"), "{why}");
        let why = refused(&Request {
            image: &flat,
            also: Some(&panorama),
            bbox: [0.0, 0.25, 0.25, 0.75],
        });
        assert!(why.contains("also is a panorama"), "{why}");
    }
}

#[test]
fn a_crop_says_what_made_it() {
    let image = picture(64, 48, noisy(13, false));
    let crop = look(&image, [0.125, 0.25, 0.75, 1.0]);
    assert_eq!(crop.image.role(), ImageRole::Picture);
    assert_eq!(crop.image.space(), Space::flat(40, 36).unwrap());
    let steps = crop.image.origin().steps();
    assert_eq!(steps.len(), 1);
    let step = &steps[0];
    assert_eq!((step.action.as_str(), step.version.as_str()), ("view", "1"));
    let arguments: Vec<(&str, &str)> = step
        .arguments
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    assert_eq!(
        arguments,
        [
            ("left", "0.125"),
            ("top", "0.25"),
            ("right", "0.75"),
            ("bottom", "1")
        ]
    );
    let inputs: Vec<(&str, _)> = step
        .inputs
        .iter()
        .map(|(port, identity)| (port.as_str(), *identity))
        .collect();
    assert_eq!(inputs, [("image", image.id().0)]);
}

#[test]
fn a_crop_keeps_alpha_and_the_colour_under_it() {
    let colour = |x: u32, y: u32| [200, (x * 5) as u8, (y * 7) as u8, ((x + y) * 3) as u8];
    let image = picture(40, 30, colour);
    let crop = look(&image, [0.25, 0.2, 0.75, 0.8]);
    assert_eq!(
        rgba_of(&crop.image),
        paint(20, 18, |x, y| colour(x + 10, y + 6))
    );
}

#[test]
fn the_same_request_gives_the_same_crop() {
    let image = picture(300, 200, noisy(14, true));
    let once = look(&image, [0.1, 0.2, 0.9, 0.6]);
    let again = look(&image, [0.1, 0.2, 0.9, 0.6]);
    assert_eq!(once.image.id(), again.image.id());
    assert_eq!(once.image.file(), again.image.file());
    assert_eq!(once.binding.id(), again.binding.id());
    // The same picture in another file gives the same pixels and the same binding: a
    // binding names pixels, not files.
    let copy = image_in(
        {
            let rgba = rgba_of(&image);
            let mut file = Vec::new();
            use image::ImageEncoder;
            image::codecs::png::PngEncoder::new_with_quality(
                &mut file,
                image::codecs::png::CompressionType::Best,
                image::codecs::png::FilterType::Paeth,
            )
            .write_image(&rgba, 300, 200, image::ExtendedColorType::Rgba8)
            .unwrap();
            file
        },
        Projection::Flat,
        Topology::Bounded,
    );
    assert_ne!(copy.identity().file, image.identity().file);
    let other = look(&copy, [0.1, 0.2, 0.9, 0.6]);
    assert_eq!(other.image.identity(), once.image.identity());
    assert_eq!(other.binding.id(), once.binding.id());
}

#[test]
fn a_crop_can_be_viewed_again() {
    let colour = noisy(15, false);
    let image = picture(200, 100, &colour);
    let outer = look(&image, [0.5, 0.0, 1.0, 1.0]);
    let inner = look(&outer.image, [0.0, 0.5, 0.5, 1.0]);
    // The inner crop is bound onto the outer one, in the outer one's pixels.
    assert_eq!(
        inner.binding.destination(),
        Destination::Image(outer.image.identity().pixels)
    );
    assert_eq!(cut_of(&inner.binding), [0.0, 50.0, 50.0, 100.0]);
    assert_eq!(
        rgba_of(&inner.image),
        paint(50, 50, |x, y| colour(x + 100, y + 50))
    );
    // Its origin names the outer crop, whose origin names the image.
    assert_eq!(
        inner.image.origin().steps()[0].inputs[0].1,
        outer.image.id().0
    );
}
