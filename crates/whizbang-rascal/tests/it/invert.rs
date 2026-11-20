//! `invert` (plan m3, task O1, item 1). Rascal's vectors
//! (`crates/rascal-ops/tests/it/subtract_select_invert.rs` at the commit
//! `docs/sources.toml` pins) that stay on the mask's own frame: with `to` naming the
//! mask's frame, protect counts the core, the blend and protect, and every other pixel is
//! the core; a mask that reaches every pixel leaves nothing to edit. Rascal's picture of
//! 100 by 60 and its canvas mask's planes, on a picture of their own; the carrying of a
//! mask up a cut or a crop is task O2's.

use whizbang_core::artifact::{Artifact, Mask, Source, UseCase};
use whizbang_core::contract::Call;
use whizbang_rascal::invert::{InvertError, NAME, invert};

use super::grow::{made_by, plane};
use super::shaping::{Shop, count, picture};

const W: usize = 100;
const H: usize = 60;

#[test]
fn the_ported_vectors_give_rascals_masks() {
    let shop = Shop::new("invert", None);
    let image = picture(W as u32, H as u32);
    let image_id = shop.put(Artifact::Image(image.clone()));
    // Rascal's canvas mask, s = 10: a band about the middle on the upper half, its blend
    // two columns to its left, and a protected patch at the far left of the lower part.
    let s = 10;
    let mut mask = Mask::from_core(
        image.binding(),
        UseCase::Seam,
        plane(W, H, |x, y| (s - 3..s + 3).contains(&x) && y < 30),
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    mask.set_blend(plane(W, H, |x, y| (s - 5..s - 3).contains(&x) && y < 30))
        .unwrap();
    mask.protect_union(&plane(W, H, |x, y| x < 2 && y >= 40))
        .unwrap();
    let mask_id = shop.put(mask.clone().into());
    let made = shop.mask(
        shop.one(
            &Call::new("invert")
                .input("mask", mask_id)
                .input("image", image_id),
        ),
    );
    assert_eq!(made.use_case(), UseCase::Protect);
    assert_eq!(made.binding(), mask.binding());
    // Rascal's count: protect is the core, the blend and protect, which do not overlap.
    assert_eq!(
        count(made.protect()),
        count(mask.core()) + count(mask.blend()) + count(mask.protect())
    );
    for at in 0..W * H {
        let (x, y) = (at % W, at / W);
        let reached = ((s - 5..s + 3).contains(&x) && y < 30) || (x < 2 && y >= 40);
        assert_eq!(made.protect()[at] != 0, reached, "protect at {x},{y}");
        assert_eq!(made.core()[at] != 0, !reached, "core at {x},{y}");
    }
    assert!(made.blend().iter().all(|b| *b == 0));
    assert_eq!(made.origin().steps()[0].action, NAME);
    // A mask that reaches everything leaves nothing to edit: refused, where rascal said it
    // did not apply.
    let full = Mask::from_core(
        image.binding(),
        UseCase::Remove,
        plane(W, H, |_, _| true),
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    assert!(matches!(
        invert(&full, image_id.0),
        Err(InvertError::Everything)
    ));
    let full = shop.put(full.into());
    let why = shop.refused(
        &Call::new("invert")
            .input("mask", full)
            .input("image", image_id),
    );
    assert!(why.contains("nothing is left to edit"), "{why}");
}

/// Rascal's vector of a crop (`invert_from_a_crop_mask_lands_protect_on_its_bbox`, the
/// same file at the pinned commit): a crop of 30 by 30 at 20, 10 of the picture, and a
/// mask on it from 5 to 25 each way. Inverted onto the picture, protect is the mask
/// where the crop sits, from 25, 15 to 45, 35, and every other pixel is the core. Where
/// rascal's `to` named a frame of the mask's chain, the picture is the image given and
/// the crop the mask is on is given beside it, with its corner.
#[test]
fn invert_from_a_crop_mask_lands_protect_on_its_bbox() {
    use whizbang_core::contract::Value;
    use whizbang_core::identity::{CanonicalPixels, DecodeLimits};

    let shop = Shop::new("invert-crop", None);
    let image = picture(W as u32, H as u32);
    let pixels = image.pixels(&DecodeLimits::default()).unwrap();
    let mut rgba = Vec::new();
    for y in 10..40 {
        rgba.extend_from_slice(&pixels.rgba()[(y * W + 20) * 4..(y * W + 50) * 4]);
    }
    let crop = super::paste::image_of(&CanonicalPixels::from_rgba(30, 30, rgba).unwrap(), "view");
    let mask = Mask::from_core(
        crop.binding(),
        UseCase::Remove,
        plane(30, 30, |x, y| (5..25).contains(&x) && (5..25).contains(&y)),
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    let image_id = shop.put(Artifact::Image(image.clone()));
    let crop_id = shop.put(Artifact::Image(crop));
    let mask_id = shop.put(mask.into());
    let call = |left: i64| {
        Call::new("invert")
            .argument("left", Value::Integer(left))
            .argument("top", Value::Integer(10))
            .input("mask", mask_id)
            .input("crop", crop_id)
            .input("image", image_id)
    };
    let made = shop.mask(shop.one(&call(20)));
    assert_eq!(*made.binding(), image.binding());
    assert_eq!(made.use_case(), UseCase::Protect);
    for at in 0..W * H {
        let (x, y) = (at % W, at / W);
        let expect = (25..45).contains(&x) && (15..35).contains(&y);
        assert_eq!(made.protect()[at] != 0, expect, "protect at {x},{y}");
        assert_eq!(made.core()[at] != 0, !expect, "core at {x},{y}");
    }
    assert_eq!(made.origin().steps().last().unwrap().action, NAME);
    // A crop that is not of the picture where the call says is refused, and a mask on
    // a crop is refused without the crop.
    let why = shop.refused(&call(21));
    assert!(why.contains("is not the picture's"), "{why}");
    let why = shop.refused(
        &Call::new("invert")
            .input("mask", mask_id)
            .input("image", image_id),
    );
    assert!(why.contains("belongs to the image"), "{why}");
    // A corner without a crop says nothing.
    let why = shop.refused(
        &Call::new("invert")
            .argument("left", Value::Integer(20))
            .input("mask", mask_id)
            .input("image", image_id),
    );
    assert!(
        why.contains("left and top are the corner of a crop"),
        "{why}"
    );
}
