//! `unwindow` (plan m3, task O2, item 1): rascal's window written back
//! (`crates/rascal-ops/tests/it/window.rs` at the commit `docs/sources.toml` pins), through
//! the executor: unchanged it is the image itself; changed inside the support it lands in
//! the box alone; changed where the mask weighs nothing it is refused.

use whizbang_core::contract::Call;
use whizbang_core::identity::CanonicalPixels;

use super::canvases::{
    World, at, engine, image, invalid, mask, nothing, pano, pixels_of, put, ran,
};

#[test]
fn a_window_unwound_is_the_original_outside_the_support() {
    let world = World::new("unwindow");
    let photo = image(&pano(120, 60), false);
    let on_photo = mask(
        &photo,
        |x, y| (40..60).contains(&x) && (20..30).contains(&y),
        |x, y| (x, y) == (45, 25),
    );
    let ids = (put(&world, photo.clone()), put(&world, on_photo));
    let engine = engine(&world, None);
    let made = ran(
        &engine,
        &Call::new("window")
            .input("image", ids.0)
            .input("mask", ids.1),
    );
    let (crop_id, cmask_id) = (made[0].1, made[1].1);
    let (crop, cmask) = (world.image(crop_id), world.mask(cmask_id));
    let back = |crop: whizbang_core::artifact::ArtifactId| {
        Call::new("unwindow")
            .input("image", ids.0)
            .input("crop", crop)
            .input("mask", cmask_id)
    };
    // Unchanged: the image itself.
    assert_eq!(ran(&engine, &back(crop_id))[0].1, photo.id());

    // Changed inside the support: written into the box; every pixel the mask never
    // reaches, and the protected one, is the image's.
    let cut = pixels_of(&crop);
    let mut rgba = cut.rgba().to_vec();
    for (i, pixel) in rgba.chunks_mut(4).enumerate() {
        if !nothing(&cmask, i) {
            pixel[..3].copy_from_slice(&[1, 2, 3]);
        }
    }
    let edited = CanonicalPixels::from_rgba(cut.width(), cut.height(), rgba.clone()).unwrap();
    let edited_id = put(&world, image(&edited, false));
    let out = world.image(ran(&engine, &back(edited_id))[0].1);
    let (before, after) = (pixels_of(&photo), pixels_of(&out));
    let mut painted = 0;
    for y in 0..60 {
        for x in 0..120 {
            let inside = (40..60).contains(&x) && (20..30).contains(&y) && (x, y) != (45, 25);
            if inside {
                painted += 1;
                assert_eq!(&at(&after, x, y)[..3], &[1, 2, 3], "({x},{y})");
            } else {
                assert_eq!(at(&after, x, y), at(&before, x, y), "({x},{y})");
            }
        }
    }
    assert_eq!(painted, 20 * 10 - 1);

    // Changed where the mask weighs nothing: refused. A crop of another size, and a mask
    // that places no crop onto the image, are refused too.
    rgba[0] ^= 0xff;
    let stray = CanonicalPixels::from_rgba(cut.width(), cut.height(), rgba).unwrap();
    let stray = put(&world, image(&stray, false));
    assert!(invalid(&engine, &back(stray)).contains("where the mask weighs nothing"));
    let small = put(&world, image(&pano(10, 10), false));
    assert!(invalid(&engine, &back(small)).contains("the window's box is"));
    let unplaced = Call::new("unwindow")
        .input("image", ids.0)
        .input("crop", crop_id)
        .input("mask", ids.1);
    assert!(invalid(&engine, &unplaced).contains("by a crop"));
}
