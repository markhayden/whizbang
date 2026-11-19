//! `put` (plan m3, task O2, item 1): rascal's put of an unchanged and an edited canvas
//! (`crates/rascal-ops/tests/it/cut_put.rs` at the commit `docs/sources.toml` pins), and
//! the action through the executor: a canvas changed inside its mask's support lands on
//! the panorama's strips, and one changed where the mask weighs nothing is refused.

use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::Call;
use whizbang_core::identity::CanonicalPixels;
use whizbang_rascal::cut::canvas_of;
use whizbang_rascal::put::put_back;

use super::canvases::{
    World, at, engine, image, invalid, mask, nothing, pano, pixels_of, put, ran,
};

/// The canvas with every pixel inverted where `change` holds.
fn changed(canvas: &CanonicalPixels, change: impl Fn(usize) -> bool) -> CanonicalPixels {
    let mut rgba = canvas.rgba().to_vec();
    for (i, pixel) in rgba.chunks_mut(4).enumerate() {
        if change(i) {
            for c in &mut pixel[..3] {
                *c = 255 - *c;
            }
        }
    }
    CanonicalPixels::from_rgba(canvas.width(), canvas.height(), rgba).unwrap()
}

#[test]
fn a_canvas_put_back_through_its_plan_is_the_original_outside_the_support() {
    // Rascal's vectors: a cut then put is the panorama, byte for byte; a canvas changed
    // everywhere lands on the two strips and nowhere else.
    for (w, s) in [(4_u32, 2_u32), (5, 2), (10, 2), (10, 4), (5760, 404)] {
        let panorama = pano(w, 6);
        let canvas = canvas_of(&panorama, s).unwrap();
        assert_eq!(
            put_back(&panorama, &canvas, s).unwrap(),
            panorama,
            "W = {w}"
        );
        let flat = vec![9; canvas.rgba().len()];
        let flat = CanonicalPixels::from_rgba(2 * s, 6, flat).unwrap();
        let back = put_back(&panorama, &flat, s).unwrap();
        for y in 0..6 {
            for x in 0..w {
                let on_strip = x < s || x >= w - s;
                assert_eq!(at(&back, x, y) == [9; 4], on_strip, "W {w} ({x},{y})");
            }
        }
    }

    // Through the executor: a panorama of 40 by 10 cut with strip 8, a mask on its canvas
    // of 16 by 10 across the seam with one pixel protected.
    let world = World::new("put");
    let panorama = image(&pano(40, 10), true);
    let pano_id = put(&world, panorama.clone());
    let engine = engine(&world, None);
    let cut = ran(
        &engine,
        &Call::new("cut")
            .argument("strip", whizbang_core::contract::Value::Number(0.2))
            .input("image", pano_id),
    );
    let (canvas_id, plan_id) = (cut[0].1, cut[1].1);
    let canvas = world.image(canvas_id);
    let on_canvas = mask(
        &canvas,
        |x, y| (5..11).contains(&x) && (2..8).contains(&y),
        |x, y| (x, y) == (8, 5),
    );
    let mask_id = put(&world, on_canvas.clone());
    let call = |canvas: ArtifactId, mask: ArtifactId| {
        Call::new("put")
            .input("image", pano_id)
            .input("canvas", canvas)
            .input("plan", plan_id)
            .input("mask", mask)
    };

    // Unchanged: the panorama itself.
    let back = ran(&engine, &call(canvas_id, mask_id));
    assert_eq!(back[0].1, panorama.id());

    // Changed inside the support: those pixels land on the strips, every other is the
    // panorama's.
    let cut_pixels = pixels_of(&canvas);
    let inside = changed(&cut_pixels, |i| !nothing(&on_canvas, i));
    let edited = put(&world, image(&inside, false));
    let back = world.image(ran(&engine, &call(edited, mask_id))[0].1);
    let (before, after) = (pixels_of(&panorama), pixels_of(&back));
    let mut moved = 0;
    for y in 0..10 {
        for x in 0..40_u32 {
            // The canvas pixel this panorama pixel is, if any.
            let from = if x >= 32 {
                Some(x - 32)
            } else if x < 8 {
                Some(x + 8)
            } else {
                None
            };
            let reached = from.is_some_and(|cx| !nothing(&on_canvas, (y * 16 + cx) as usize));
            if reached {
                moved += 1;
                assert_eq!(at(&after, x, y), at(&inside, from.unwrap(), y), "({x},{y})");
            } else {
                assert_eq!(at(&after, x, y), at(&before, x, y), "({x},{y})");
            }
        }
    }
    assert_eq!(moved, 6 * 6 - 1);
    assert_eq!(back.origin().steps()[0].action, "put");

    // Changed where the mask weighs nothing: refused, naming the first such pixel.
    let outside = changed(&cut_pixels, |i| i == 0 || !nothing(&on_canvas, i));
    let stray = put(&world, image(&outside, false));
    let why = invalid(&engine, &call(stray, mask_id));
    assert!(why.contains("1 pixels, the first at 0, 0"), "{why}");

    // A canvas of another size, a mask on another canvas, a plan of another panorama.
    let small = put(&world, image(&pano(8, 10), false));
    assert!(invalid(&engine, &call(small, mask_id)).contains("the plan's canvas is 16x10"));
    let elsewhere = mask(&image(&pano(16, 10), false), |x, _| x == 3, |_, _| false);
    let elsewhere = put(&world, elsewhere);
    assert!(invalid(&engine, &call(canvas_id, elsewhere)).contains("neither on the canvas"));
    let other = put(&world, image(&pano(48, 10), true));
    let other_cut = ran(&engine, &Call::new("cut").input("image", other));
    let foreign = Call::new("put")
        .input("image", pano_id)
        .input("canvas", canvas_id)
        .input("plan", other_cut[1].1)
        .input("mask", mask_id);
    assert!(invalid(&engine, &foreign).contains("made for the panorama"));
}

#[test]
fn put_refuses_a_mask_rebound_onto_another_canvas_than_the_one_given() {
    // A mask fill rebound onto its composite is taken only with that composite: not with
    // the composite of another fill of the same cut, and not when it was rebound from the
    // cut of another panorama of the same size (task O3).
    use std::sync::Arc;
    use whizbang_core::contract::Value;
    use whizbang_testkit::capabilities::{FakeFiller, Paint};
    let world = World::new("put-rebound");
    let engine = engine(&world, Some(Arc::new(FakeFiller::new(Paint::Inverse))));
    let cut_of = |panorama: &CanonicalPixels| {
        let id = put(&world, image(panorama, true));
        let call = Call::new("cut")
            .argument("strip", Value::Number(0.2))
            .input("image", id);
        let cut = ran(&engine, &call);
        (id, cut[0].1, cut[1].1)
    };
    let (pano_id, canvas_id, plan_id) = cut_of(&pano(40, 10));
    let canvas = world.image(canvas_id);
    let filled = |canvas_id: ArtifactId, cols: std::ops::Range<usize>| {
        let on = mask(
            &world.image(canvas_id),
            |x, y| cols.contains(&x) && (2..8).contains(&y),
            |_, _| false,
        );
        let id = put(&world, on);
        let fill = Call::new("fill")
            .input("image", canvas_id)
            .input("mask", id);
        let made = ran(&engine, &fill);
        (made[0].1, made[1].1)
    };
    let (first, first_mask) = filled(canvas_id, 5..8);
    let (_, second_mask) = filled(canvas_id, 9..12);
    let call = |canvas: ArtifactId, mask: ArtifactId| {
        Call::new("put")
            .input("image", pano_id)
            .input("canvas", canvas)
            .input("plan", plan_id)
            .input("mask", mask)
    };
    // Its own composite: taken.
    assert_eq!(ran(&engine, &call(first, first_mask)).len(), 1);
    // Another fill's composite of the same cut.
    let why = invalid(&engine, &call(first, second_mask));
    assert!(why.contains("neither on the canvas"), "{why}");
    // Rebound from the cut of another panorama of the same size.
    let other = changed(&pano(40, 10), |i| i % 3 == 0);
    let (_, other_canvas, _) = cut_of(&other);
    assert_eq!(world.image(other_canvas).space(), canvas.space());
    let (_, foreign_mask) = filled(other_canvas, 9..12);
    let why = invalid(&engine, &call(first, foreign_mask));
    assert!(why.contains("neither on the canvas"), "{why}");
}
