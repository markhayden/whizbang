//! The composite of a fill onto an image under a mask (ADR 0006, "The composite"): what
//! a filler returned is read inside the mask's support and nowhere else, so every pixel
//! outside the support, and every protected pixel, comes out exactly as it went in.
//!
//! The fixed values of a mix below were worked out apart from the code, from the words
//! of the record alone, in exact fractions: the weight of a blend `b` is `b / 255`, a
//! channel becomes `(1 - w) original + w fill`, and then the nearest whole number.

use std::sync::Arc;

use whizbang_core::artifact::{Mask, Origin, Source, Step, UseCase};
use whizbang_core::capability::{FillHandle, FillRequest, Painted, Picture, Support, Unfit};
use whizbang_core::contract::Cancellation;
use whizbang_core::identity::{Binding, CanonicalPixels, Space};
use whizbang_rascal::composite::{CompositeError, composite, mix};
use whizbang_testkit::capabilities::{FakeFiller, Paint};

const WIDTH: u32 = 8;
const HEIGHT: u32 = 6;
const PIXELS: usize = (WIDTH * HEIGHT) as usize;

/// A picture in which every pixel is its own colour, and whose alpha runs from opaque
/// to fully transparent: its last pixel has an alpha of 0.
fn picture() -> Picture {
    let mut rgba = Vec::with_capacity(PIXELS * 4);
    for at in 0..PIXELS as u32 {
        let alpha = if at == PIXELS as u32 - 1 {
            0
        } else {
            255 - at as u8
        };
        rgba.extend([
            (at * 5) as u8,
            (at * 3 + 1) as u8,
            (250 - at * 2) as u8,
            alpha,
        ]);
    }
    Picture::new(CanonicalPixels::from_rgba(WIDTH, HEIGHT, rgba).unwrap()).unwrap()
}

fn plane(set: impl Fn(u32, u32) -> u8) -> Vec<u8> {
    (0..PIXELS as u32)
        .map(|at| set(at % WIDTH, at / WIDTH))
        .collect()
}

/// A mask on `image` with a core, a blend of each given weight in a column of its own,
/// and a protected row that crosses the core and the blend.
///
/// - core: columns 2 and 3;
/// - blend: column 1 at 128, column 4 at 127, column 5 at 1, column 6 at 255;
/// - protect: row 2, in every column;
/// - columns 0 and 7 hold nothing.
fn mask_on(image: &Picture) -> Mask {
    let binding = Binding::root(image.digest(), Space::flat(WIDTH, HEIGHT).unwrap());
    let origin = Origin::new(Step::new("test", "1")).unwrap();
    let core = plane(|x, _| if x == 2 || x == 3 { 255 } else { 0 });
    let mut mask =
        Mask::from_core(binding, UseCase::Remove, core, Source::default(), origin).unwrap();
    mask.set_blend(plane(|x, _| match x {
        1 => 128,
        4 => 127,
        5 => 1,
        6 => 255,
        _ => 0,
    }))
    .unwrap();
    mask.protect_union(&plane(|_, y| if y == 2 { 255 } else { 0 }))
        .unwrap();
    mask
}

fn pixel(rgba: &[u8], at: usize) -> [u8; 4] {
    [
        rgba[at * 4],
        rgba[at * 4 + 1],
        rgba[at * 4 + 2],
        rgba[at * 4 + 3],
    ]
}

fn flat(colour: [u8; 3]) -> Painted {
    Painted::from_rgb(WIDTH, HEIGHT, colour.repeat(PIXELS)).unwrap()
}

// --- preservation -----------------------------------------------------------------------------

#[test]
fn after_a_filler_repaints_every_pixel_only_the_support_is_changed() {
    let image = picture();
    let mask = mask_on(&image);
    let support = Support::of(&mask);

    // The fake filler repaints the whole image: no pixel it returns is the original.
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let request = FillRequest::new(image.clone(), support.clone(), None).unwrap();
    let fill = FillHandle::new(filler.clone())
        .fill(&request, &Cancellation::new())
        .unwrap();
    assert_eq!(filler.requests(), 1);
    for (at, original) in image.rgba().chunks(4).enumerate() {
        assert_ne!(&fill.rgb()[at * 3..at * 3 + 3], &original[..3]);
    }

    let made = composite(&image, &fill, &mask).unwrap();
    let (before, after) = (image.rgba(), made.pixels().rgba());
    let (mut outside, mut protected, mut in_core) = (0, 0, 0);
    for at in 0..PIXELS {
        // Every pixel outside the support is exactly the original, all four bytes.
        if support.plane()[at] == Support::KEEP {
            assert_eq!(pixel(after, at), pixel(before, at), "pixel {at}, outside");
            outside += 1;
        }
        // Every protected pixel is exactly the original, whatever else covers it.
        if mask.protect()[at] != 0 {
            assert_eq!(pixel(after, at), pixel(before, at), "pixel {at}, protected");
            protected += 1;
        }
        // In the core the colour is the fill's, and the alpha is still the image's.
        if mask.core()[at] != 0 {
            let [r, g, b, a] = pixel(before, at);
            assert_eq!(
                pixel(after, at),
                [255 - r, 255 - g, 255 - b, a],
                "pixel {at}"
            );
            in_core += 1;
        }
    }
    // The mask had each kind of pixel: 2 empty columns and a protected row outside the
    // support, a row of 8 protected, and 2 core columns less the protected row.
    assert_eq!((outside, protected, in_core), (18, 8, 10));
    assert_eq!(made.painted(), support.painted());
    assert_eq!(made.painted(), 30);
    // No alpha changed anywhere.
    for at in 0..PIXELS {
        assert_eq!(
            after[at * 4 + 3],
            before[at * 4 + 3],
            "the alpha of pixel {at}"
        );
    }
}

#[test]
fn a_protected_pixel_is_kept_where_the_core_and_the_blend_would_have_reached_it() {
    // Row 2 is protected across the core's columns and the blend's. Without the protect
    // those pixels are painted; with it they are the original.
    let image = picture();
    let mask = mask_on(&image);
    let made = composite(&image, &flat([9, 9, 9]), &mask).unwrap();
    for x in 0..WIDTH {
        let at = (2 * WIDTH + x) as usize;
        assert_eq!(pixel(made.pixels().rgba(), at), pixel(image.rgba(), at));
    }
    // The same columns in the row above are painted.
    for x in [1, 2, 3, 4, 6] {
        let at = (WIDTH + x) as usize;
        assert_ne!(pixel(made.pixels().rgba(), at), pixel(image.rgba(), at));
    }
}

// --- the mix ------------------------------------------------------------------------------------

/// Original, fill, blend, and the channel the record gives: worked out apart from the
/// code, in exact fractions.
const STATED: [(u8, u8, u8, u8); 20] = [
    (100, 200, 128, 150),
    (100, 200, 127, 150),
    (0, 255, 128, 128),
    (0, 255, 127, 127),
    (255, 0, 128, 127),
    (255, 0, 127, 128),
    (0, 1, 128, 1),
    (0, 1, 127, 0),
    (10, 21, 128, 16),
    (10, 21, 127, 15),
    (0, 255, 1, 1),
    (255, 0, 1, 254),
    (0, 255, 254, 254),
    (255, 0, 254, 1),
    (37, 201, 64, 78),
    (201, 37, 191, 78),
    (128, 127, 1, 128),
    (7, 7, 99, 7),
    (0, 255, 255, 255),
    (13, 250, 255, 250),
];

#[test]
fn a_blend_weight_of_one_half_gives_the_stated_mix() {
    // A blend is a count of 255ths, so no blend is exactly one half: 128 is the nearest
    // above (128 / 255) and 127 the nearest below. Both are in the mask, in columns 1
    // and 4, and each gives the mix the record states, channel by channel.
    let image = picture();
    let mask = mask_on(&image);
    let fill = flat([200, 21, 0]);
    let made = composite(&image, &fill, &mask).unwrap();
    for (x, blend) in [(1_u32, 128_u32), (4, 127)] {
        let at = x as usize; // the first row
        assert_eq!(u32::from(mask.blend()[at]), blend);
        let [r, g, b, a] = pixel(image.rgba(), at);
        let got = pixel(made.pixels().rgba(), at);
        // The nearest whole number to ((255 - blend) original + blend fill) / 255,
        // found here by trying every value: the one within half of the exact mix.
        let nearest = |original: u8, fill: u8| -> u8 {
            let exact = (255 - blend) * u32::from(original) + blend * u32::from(fill);
            let near: Vec<u32> = (0..=255_u32)
                .filter(|v| (2 * i64::from(255 * v) - 2 * i64::from(exact)).abs() < 255)
                .collect();
            assert_eq!(near.len(), 1);
            near[0] as u8
        };
        assert_eq!(got, [nearest(r, 200), nearest(g, 21), nearest(b, 0), a]);
    }
    // And the fixed values, by hand: an original of 100 under a fill of 200 is 150 at
    // either blend; black under white is 128 at 128 and 127 at 127.
    assert_eq!(mix(100, 200, 128), 150);
    assert_eq!(mix(100, 200, 127), 150);
    assert_eq!((mix(0, 255, 128), mix(0, 255, 127)), (128, 127));
}

#[test]
fn the_mix_is_the_stated_value_for_each_fixed_vector() {
    for (original, fill, blend, stated) in STATED {
        assert_eq!(
            mix(original, fill, blend),
            stated,
            "original {original}, fill {fill}, blend {blend}"
        );
    }
    // The same values through the composite: one pixel for each vector, its red channel.
    let count = STATED.len() as u32;
    let rgba: Vec<u8> = STATED
        .iter()
        .flat_map(|(original, _, _, _)| [*original, 0, 0, 255])
        .collect();
    let image = Picture::new(CanonicalPixels::from_rgba(count, 1, rgba).unwrap()).unwrap();
    let rgb: Vec<u8> = STATED
        .iter()
        .flat_map(|(_, fill, _, _)| [*fill, 0, 0])
        .collect();
    let fill = Painted::from_rgb(count, 1, rgb).unwrap();
    let binding = Binding::root(image.digest(), Space::flat(count, 1).unwrap());
    let origin = Origin::new(Step::new("test", "1")).unwrap();
    let mut mask = Mask::from_core(
        binding,
        UseCase::Custom,
        vec![0; STATED.len()],
        Source::default(),
        origin,
    )
    .unwrap();
    mask.set_blend(STATED.iter().map(|(_, _, blend, _)| *blend).collect())
        .unwrap();
    let made = composite(&image, &fill, &mask).unwrap();
    let reds: Vec<u8> = made.pixels().rgba().chunks(4).map(|p| p[0]).collect();
    let stated: Vec<u8> = STATED.iter().map(|(_, _, _, stated)| *stated).collect();
    assert_eq!(reds, stated);
}

#[test]
fn every_mix_is_the_nearest_whole_number_and_none_is_half_way() {
    // Every original, every fill and every blend: twice the distance from the exact mix,
    // in 255ths, is under 255, which is to say the result is within a half of it; and it
    // is never exactly a half, so no rule for a tie is needed.
    for blend in 0..=255_u32 {
        for original in 0..=255_u32 {
            for fill in 0..=255_u32 {
                let exact = (255 - blend) * original + blend * fill;
                let got = u32::from(mix(original as u8, fill as u8, blend as u8));
                let twice = 2 * i64::from(255 * got) - 2 * i64::from(exact);
                assert!(
                    twice.abs() < 255,
                    "original {original}, fill {fill}, blend {blend}: {got}"
                );
            }
        }
    }
    // The ends: a blend of 0 is the original and a blend of 255 is the fill.
    assert_eq!((mix(13, 250, 0), mix(13, 250, 255)), (13, 250));
}

#[test]
fn the_mix_is_rascals_feather_for_every_original_fill_and_blend() {
    // Characterisation of the old code (rascal `crates/rascal-ops/src/paste.rs`,
    // `composite`, mode `feather`, at the pinned commit): per channel, in `f32`,
    // `(1 - w) base + w fill` with `w = blend / 255`, rounded and clamped. The integer
    // rule here gives the same byte for every case.
    for blend in 1..=255_u8 {
        let weight = f32::from(blend) / 255.0;
        for original in 0..=255_u8 {
            for fill in 0..=255_u8 {
                let old = (1.0 - weight) * f32::from(original) + weight * f32::from(fill);
                let old = old.round().clamp(0.0, 255.0) as u8;
                assert_eq!(
                    mix(original, fill, blend),
                    old,
                    "original {original}, fill {fill}, blend {blend}"
                );
            }
        }
    }
}

// --- alpha ---------------------------------------------------------------------------------------

#[test]
fn a_transparent_source_pixel_keeps_its_alpha_and_takes_colour_like_any_other() {
    // Three pixels in a row, each fully transparent, with a colour under it: one in the
    // core, one under a blend of 128, one outside the mask. And one half transparent, in
    // the core.
    let rgba = vec![
        10, 20, 30, 0, // core
        10, 20, 30, 0, // blend 128
        10, 20, 30, 0, // outside
        10, 20, 30, 128, // core
    ];
    let image = Picture::new(CanonicalPixels::from_rgba(4, 1, rgba).unwrap()).unwrap();
    let binding = Binding::root(image.digest(), Space::flat(4, 1).unwrap());
    let origin = Origin::new(Step::new("test", "1")).unwrap();
    let mut mask = Mask::from_core(
        binding,
        UseCase::Remove,
        vec![255, 0, 0, 255],
        Source::default(),
        origin,
    )
    .unwrap();
    mask.set_blend(vec![0, 128, 0, 0]).unwrap();
    let fill = Painted::from_rgb(4, 1, [200, 100, 50].repeat(4)).unwrap();

    let made = composite(&image, &fill, &mask).unwrap();
    assert_eq!(
        made.pixels().rgba(),
        [
            // The colour under the transparent pixel is the fill's; it is still
            // transparent.
            200, 100, 50, 0,
            // The colours are mixed as they are stored, not multiplied by the alpha:
            // (127 * 10 + 128 * 200 + 127) / 255 = 105, and so on. Still transparent.
            105, 60, 40, 0,
            // Outside the mask nothing is touched, the hidden colour included.
            10, 20, 30, 0, 200, 100, 50, 128,
        ]
    );
}

// --- what is refused ----------------------------------------------------------------------------

#[test]
fn a_fill_of_the_wrong_dimensions_is_refused() {
    let image = picture();
    let mask = mask_on(&image);
    for (width, height) in [
        (HEIGHT, WIDTH),
        (WIDTH + 1, HEIGHT),
        (WIDTH, HEIGHT - 1),
        (1, 1),
    ] {
        let fill =
            Painted::from_rgb(width, height, vec![7; (width * height * 3) as usize]).unwrap();
        let error = composite(&image, &fill, &mask).unwrap_err();
        assert_eq!(
            error,
            CompositeError::FillSize {
                expected: (WIDTH, HEIGHT),
                found: (width, height),
            }
        );
        assert_eq!(
            error.to_string(),
            format!("the fill is {width} by {height}, and the image is {WIDTH} by {HEIGHT}")
        );
    }
}

#[test]
fn a_support_plane_with_values_outside_its_range_is_refused() {
    // A support plane is 0 or 255 (ADR 0006, "Fill"), and one that holds anything else
    // cannot be made, so no request and no composite is ever given one. The composite
    // itself reads the mask's planes, which the mask's own type keeps.
    let image = picture();
    let good = Support::of(&mask_on(&image));
    for value in [1_u8, 127, 128, 254] {
        let mut bytes = good.plane().to_vec();
        bytes[11] = value;
        assert_eq!(
            Support::from_plane(WIDTH, HEIGHT, bytes).unwrap_err(),
            Unfit::OutOfRange {
                what: "the support plane",
                at: Some(11),
                found: value.to_string(),
                allowed: "0 or 255",
            }
        );
    }
    Support::from_plane(WIDTH, HEIGHT, good.plane().to_vec()).unwrap();
}

#[test]
fn a_mask_of_another_image_is_refused() {
    let image = picture();
    let mask = mask_on(&image);
    // A picture of the same size that is not the mask's.
    let other =
        Picture::new(CanonicalPixels::from_rgba(WIDTH, HEIGHT, vec![3; PIXELS * 4]).unwrap())
            .unwrap();
    let error = composite(&other, &flat([1, 2, 3]), &mask).unwrap_err();
    assert!(matches!(error, CompositeError::WrongImage(_)), "{error:?}");
    assert!(error.to_string().contains("the mask belongs to the image"));
}

// --- the derivation ------------------------------------------------------------------------------

#[test]
fn the_composite_states_the_derivation_of_the_picture_it_made() {
    let image = picture();
    let mask = mask_on(&image);
    let made = composite(&image, &flat([9, 9, 9]), &mask).unwrap();
    let derivation = made.derivation().unwrap();
    assert_eq!(derivation.parent(), image.digest());
    assert_eq!(derivation.image(), made.pixels().digest());

    // The mask goes onto the composite through it, and protects the same places there:
    // a second composite, of a second fill, keeps them again.
    let rebound = mask.rebind(derivation).unwrap();
    let second = Picture::new(made.pixels().clone()).unwrap();
    let again = composite(&second, &flat([250, 250, 250]), &rebound).unwrap();
    for at in 0..PIXELS {
        if rebound.protect()[at] != 0 {
            assert_eq!(pixel(again.pixels().rgba(), at), pixel(image.rgba(), at));
        }
    }
    // The first mask is not one for the composite.
    assert!(matches!(
        composite(&second, &flat([1, 1, 1]), &mask),
        Err(CompositeError::WrongImage(_))
    ));
}

#[test]
fn a_composite_that_changed_nothing_states_no_derivation() {
    let image = picture();
    let origin = Origin::new(Step::new("test", "1")).unwrap();
    let binding = Binding::root(image.digest(), Space::flat(WIDTH, HEIGHT).unwrap());
    // A mask that holds nothing: the picture comes back as it was.
    let empty = Mask::from_core(
        binding,
        UseCase::Custom,
        vec![0; PIXELS],
        Source::default(),
        origin,
    )
    .unwrap();
    let made = composite(&image, &flat([9, 9, 9]), &empty).unwrap();
    assert_eq!(made.pixels(), image.pixels());
    assert_eq!(made.painted(), 0);
    assert!(made.derivation().is_none());
    // A fill that is the image's own colours, painted through a mask: the same.
    let own: Vec<u8> = image
        .rgba()
        .chunks(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let own = Painted::from_rgb(WIDTH, HEIGHT, own).unwrap();
    let made = composite(&image, &own, &mask_on(&image)).unwrap();
    assert_eq!(made.pixels(), image.pixels());
    assert_eq!(made.painted(), 30);
    assert!(made.derivation().is_none());
}
