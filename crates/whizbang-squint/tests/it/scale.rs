//! Scaling a cut down to the bound, against what the definition gives: each output sample
//! is the mean of the source over its rectangle, rounded half up. These are exact: not
//! one level of difference is allowed.

use whizbang_core::identity::Space;

use super::support::*;

#[test]
fn a_whole_number_ratio_is_the_mean_of_each_block_a_half_going_up() {
    // 2048 by 16 becomes 1024 by 8: each output pixel is a block of two by two.
    let colour = |x: u32, y: u32| {
        [
            // Two of 0 and two of 255: 127.5, which goes up to 128.
            if x.is_multiple_of(2) { 0 } else { 255 },
            // 0, 1, 0, 0: a quarter, which goes down to 0.
            u8::from(x % 2 == 1 && y.is_multiple_of(2)),
            // 1, 1, 2, 2: one and a half, which goes up to 2.
            1 + (y % 2) as u8,
            // One alpha for each block of four, so that weighting by it changes nothing.
            noise(x / 2, y / 2, 3, 21),
        ]
    };
    let image = picture(2048, 16, colour);
    let crop = look(&image, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(crop.image.space(), Space::flat(1024, 8).unwrap());
    let expected = paint(1024, 8, |x, y| [128, 0, 2, noise(x, y, 3, 21)]);
    assert_eq!(rgba_of(&crop.image), expected);
}

#[test]
fn a_flat_colour_stays_flat_at_any_ratio() {
    for (width, height) in [(1537, 37), (1025, 9), (9, 2000), (4099, 8)] {
        let image = picture(width, height, |_, _| [201, 3, 255, 77]);
        let crop = look(&image, [0.0, 0.0, 1.0, 1.0]);
        assert!(crop.image.space().width().max(crop.image.space().height()) == 1024);
        assert!(
            rgba_of(&crop.image)
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [201, 3, 255, 77]),
            "{width}x{height}"
        );
    }
}

#[test]
fn an_edge_is_shown_as_the_share_of_each_pixel_it_covers() {
    // Black left of column `edge` and white from it on. An output pixel that the edge
    // crosses is the white share of its width, exactly; the others are black or white.
    for (width, edge) in [(1500u64, 677u64), (1025, 1), (2047, 1024), (3001, 2999)] {
        let image = picture(width as u32, 9, |x, _| {
            let v = if u64::from(x) < edge { 0 } else { 255 };
            [v, v, v, 255]
        });
        let crop = look(&image, [0.0, 0.0, 1.0, 1.0]);
        let space = crop.image.space();
        assert_eq!(space.width(), 1024);
        let rgba = rgba_of(&crop.image);
        for x in 0..1024u64 {
            // The output pixel covers [x * width, (x + 1) * width) in 1024ths of a
            // source pixel, and the white starts at edge * 1024.
            let white = ((x + 1) * width).saturating_sub((x * width).max(edge * 1024));
            let expected = ((2 * 255 * white + width) / (2 * width)) as u8;
            for y in 0..space.height() as u64 {
                let at = ((y * 1024 + x) * 4) as usize;
                assert_eq!(
                    rgba[at..at + 4],
                    [expected, expected, expected, 255],
                    "width {width}, edge {edge}, output pixel ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn the_shorter_side_is_scaled_by_the_same_ratio_to_the_nearest_pixel() {
    // (width, height) of the cut, and the size it is shown at.
    let cases = [
        ((2048, 64), (1024, 32)),
        // 62 x 1024 / 1639 is 38.7.
        ((1639, 62), (1024, 39)),
        // 8 x 1024 / 2050 is 3.996.
        ((2050, 8), (1024, 4)),
        // 9 x 1024 / 2048 is 4.5: a half goes up.
        ((2048, 9), (1024, 5)),
        // 8 x 1024 / 16400 is 0.4995, and a side is never under one pixel.
        ((16400, 8), (1024, 1)),
        ((8, 16400), (1, 1024)),
        ((9, 3000), (3, 1024)),
        ((1024, 9), (1024, 9)),
        ((9, 1024), (9, 1024)),
        ((1025, 9), (1024, 9)),
    ];
    for ((width, height), shown) in cases {
        let image = picture(width, height, |x, y| [x as u8, y as u8, 0, 255]);
        let crop = look(&image, [0.0, 0.0, 1.0, 1.0]);
        let space = crop.image.space();
        assert_eq!(
            (space.width(), space.height()),
            shown,
            "a cut of {width}x{height}"
        );
    }
}

/// One crop pixel of a reduced crop by each of the three ways the record compares (plan
/// mE, task P1, item 6), from the source pixels its rectangle covers, each counted once:
/// the plain mean of each channel; each colour weighted by alpha as well, alpha the plain
/// mean (the method); and each pixel flattened onto a white ground first, then the mean,
/// opaque.
fn three_ways(pixels: &[[u32; 4]]) -> [[u32; 4]; 3] {
    let n = pixels.len() as u32;
    let mean = |values: &dyn Fn(&[u32; 4]) -> u32| {
        (2 * pixels.iter().map(values).sum::<u32>() + n) / (2 * n)
    };
    let seen: u32 = pixels.iter().map(|p| p[3]).sum();
    let plain = [
        mean(&|p| p[0]),
        mean(&|p| p[1]),
        mean(&|p| p[2]),
        mean(&|p| p[3]),
    ];
    let weighted = |k: usize| {
        let sum: u32 = pixels.iter().map(|p| p[3] * p[k]).sum();
        (2 * sum + seen) / (2 * seen)
    };
    let over_white = |p: &[u32; 4], k: usize| (2 * (p[k] * p[3] + 255 * (255 - p[3])) + 255) / 510;
    [
        plain,
        [weighted(0), weighted(1), weighted(2), plain[3]],
        [
            mean(&|p| over_white(p, 0)),
            mean(&|p| over_white(p, 1)),
            mean(&|p| over_white(p, 2)),
            255,
        ],
    ]
}

#[test]
fn a_reduced_crop_with_transparency_is_the_mean_weighted_by_alpha() {
    // Each block of four is one opaque red pixel and three transparent green ones. The
    // plain mean shows a quarter red and three quarters green, a green nobody sees; the
    // mean weighted by alpha shows red at a quarter of full alpha; flattened onto white
    // it is an opaque pink, the ground's colour where there was nothing.
    let image = picture(2048, 16, |x, y| {
        if x % 2 == 0 && y % 2 == 0 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 0]
        }
    });
    let crop = look(&image, [0.0, 0.0, 1.0, 1.0]);
    let block = [
        [255, 0, 0, 255],
        [0, 255, 0, 0],
        [0, 255, 0, 0],
        [0, 255, 0, 0],
    ];
    let ways = three_ways(&block);
    assert_eq!(
        ways,
        [[64, 191, 0, 64], [255, 0, 0, 64], [255, 191, 191, 255]]
    );
    let method = ways[1].map(|v| v as u8);
    assert!(
        rgba_of(&crop.image)
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == method)
    );
}

#[test]
fn hidden_colour_does_not_show_at_the_edge_of_a_transparent_area_of_a_reduced_crop() {
    // The record's worked example for the rule. Opaque white in the even columns, and
    // in the odd ones nothing to see: black at alpha 0. Over a white page the source
    // shows white everywhere. The plain mean is grey at half alpha, which over a white
    // page shows as 191, a grey the source does not have: the hidden black shows. The
    // mean weighted by alpha, the method, is white at half alpha, which shows as 255.
    // Flattened onto white it is opaque white, and its transparency is gone.
    let image = picture(2048, 16, |x, _| {
        if x % 2 == 0 {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 0]
        }
    });
    let crop = look(&image, [0.0, 0.0, 1.0, 1.0]);
    let ways = three_ways(&[[255, 255, 255, 255], [0, 0, 0, 0]]);
    assert_eq!(
        ways,
        [
            [128, 128, 128, 128],
            [255, 255, 255, 128],
            [255, 255, 255, 255]
        ]
    );
    assert!(
        rgba_of(&crop.image)
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [255, 255, 255, 128])
    );
    // Shown over white, to the nearest level: the crop's colour by its alpha, and white
    // by the rest.
    let over_white =
        |colour: u32, alpha: u32| (2 * (colour * alpha + 255 * (255 - alpha)) + 255) / 510;
    assert_eq!(over_white(128, 128), 191);
    assert_eq!(over_white(255, 128), 255);
    // Where nothing at all is visible, the colours are the plain mean, as before.
    let clear = picture(2048, 16, |x, _| {
        if x % 2 == 0 {
            [200, 100, 0, 0]
        } else {
            [0, 100, 200, 0]
        }
    });
    let crop = look(&clear, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(rgba_of(&crop.image)[..4], [100, 100, 100, 0]);
}
