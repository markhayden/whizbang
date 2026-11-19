//! A fill put back onto the picture it came from (plan m3, task O2, item 3; rascal's
//! `heal` graph, `crates/rascal-mask/src/heal.rs` at the commit `docs/sources.toml`
//! pins): `window`, `fill`, `unwindow`, and `cut`, `fill`, `put`, through the executor
//! under a filler that repaints every pixel it is sent. Every pixel of the original that
//! the mask never reaches, and every protected one, is the original's, as rascal's
//! preservation check held in the coordinates of the image given.

use std::sync::Arc;

use whizbang_core::contract::{Call, Value};
use whizbang_testkit::capabilities::{FakeFiller, Paint};

use super::canvases::{World, at, engine, image, mask, nothing, pano, pixels_of, put, ran};

#[test]
fn a_fill_through_a_window_keeps_every_pixel_the_mask_never_reaches() {
    let world = World::new("heal-window");
    let photo = image(&pano(160, 96), false);
    let on_photo = mask(
        &photo,
        |x, y| (70..90).contains(&x) && (40..52).contains(&y),
        |x, y| (x, y) == (75, 45) || (x, y) == (89, 51),
    );
    let ids = (put(&world, photo.clone()), put(&world, on_photo.clone()));
    let engine = engine(&world, Some(Arc::new(FakeFiller::new(Paint::Inverse))));
    let window = ran(
        &engine,
        &Call::new("window")
            .argument("max_side", Value::Integer(64))
            .input("image", ids.0)
            .input("mask", ids.1),
    );
    let filled = ran(
        &engine,
        &Call::new("fill")
            .input("image", window[0].1)
            .input("mask", window[1].1),
    );
    let healed = ran(
        &engine,
        &Call::new("unwindow")
            .input("image", ids.0)
            .input("crop", filled[0].1)
            .input("mask", window[1].1),
    );
    let (before, after) = (pixels_of(&photo), pixels_of(&world.image(healed[0].1)));
    let mut painted = 0;
    for y in 0..96 {
        for x in 0..160 {
            let (was, is) = (at(&before, x, y), at(&after, x, y));
            if nothing(&on_photo, (y * 160 + x) as usize) {
                assert_eq!(is, was, "({x},{y})");
            } else {
                painted += 1;
                assert_eq!(is, [255 - was[0], 255 - was[1], 255 - was[2], was[3]]);
            }
        }
    }
    assert_eq!(painted, 20 * 12 - 2);
}

#[test]
fn a_fill_through_a_wrapped_cut_keeps_every_pixel_the_mask_never_reaches() {
    // A panorama of 256 by 64 cut with a strip of 26: a canvas of 52 by 64, whose mask
    // crosses the seam at its middle column, one pixel protected on each side.
    let world = World::new("heal-cut");
    let panorama = image(&pano(256, 64), true);
    let pano_id = put(&world, panorama.clone());
    let engine = engine(&world, Some(Arc::new(FakeFiller::new(Paint::Inverse))));
    let cut = ran(
        &engine,
        &Call::new("cut")
            .argument("strip", Value::Number(0.1))
            .input("image", pano_id),
    );
    let canvas = world.image(cut[0].1);
    assert_eq!(canvas.space().width(), 52);
    let on_canvas = mask(
        &canvas,
        |x, y| (20..32).contains(&x) && (10..40).contains(&y),
        |x, y| (x, y) == (21, 20) || (x, y) == (30, 20),
    );
    let mask_id = put(&world, on_canvas.clone());
    let filled = ran(
        &engine,
        &Call::new("fill")
            .input("image", cut[0].1)
            .input("mask", mask_id),
    );
    // Put back through the mask fill handed back, rebound onto its composite.
    let healed = ran(
        &engine,
        &Call::new("put")
            .input("image", pano_id)
            .input("canvas", filled[0].1)
            .input("plan", cut[1].1)
            .input("mask", filled[1].1),
    );
    let (before, after) = (pixels_of(&panorama), pixels_of(&world.image(healed[0].1)));
    let mut painted = 0;
    for y in 0..64 {
        for x in 0..256_u32 {
            // The canvas pixel this panorama pixel is: its last 26 columns are the
            // canvas's first, its first 26 the canvas's last.
            let from = if x >= 230 {
                Some(x - 230)
            } else if x < 26 {
                Some(x + 26)
            } else {
                None
            };
            let reached = from.is_some_and(|cx| !nothing(&on_canvas, (y * 52 + cx) as usize));
            let (was, is) = (at(&before, x, y), at(&after, x, y));
            if reached {
                painted += 1;
                assert_eq!(is, [255 - was[0], 255 - was[1], 255 - was[2], was[3]]);
            } else {
                assert_eq!(is, was, "({x},{y})");
            }
        }
    }
    assert_eq!(painted, 12 * 30 - 2);
}
