//! `carry` (plan m4, task L2, item 1): a mask on a crop becomes a mask on the picture
//! the crop was cut from, empty outside the crop, and is refused when the crop is not of
//! that picture. Exact arithmetic on a picture of 32 by 32.

use whizbang_core::artifact::{Artifact, Image, Mask, Source, UseCase};
use whizbang_core::contract::{Call, Value};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits};
use whizbang_rascal::carry::{CarryError, NAME, Placed};

use super::grow::{made_by, plane};
use super::paste::image_of;
use super::shaping::{Shop, picture};

const SIDE: usize = 32;

/// Where the crop of these tests sits: left, top, right and bottom, in pixels.
const CUT: [usize; 4] = [8, 4, 24, 20];

/// The pixels of `image` inside a box, as a picture of their own: what `view` cuts.
fn cut(image: &Image, [left, top, right, bottom]: [usize; 4]) -> Image {
    let pixels = image.pixels(&DecodeLimits::default()).unwrap();
    let width = pixels.width() as usize;
    let mut rgba = Vec::new();
    for y in top..bottom {
        rgba.extend_from_slice(&pixels.rgba()[(y * width + left) * 4..(y * width + right) * 4]);
    }
    let cut = CanonicalPixels::from_rgba((right - left) as u32, (bottom - top) as u32, rgba);
    image_of(&cut.unwrap(), "view")
}

/// A mask on the crop: a core, a blend beside it and a protected patch.
fn on_crop(crop: &Image) -> Mask {
    let mut mask = Mask::from_core(
        crop.binding(),
        UseCase::Remove,
        plane(16, 16, |x, y| (2..6).contains(&x) && (3..9).contains(&y)),
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    mask.protect_union(&plane(16, 16, |x, y| x == 15 && y == 15))
        .unwrap();
    mask.set_blend(plane(16, 16, |x, y| x == 6 && (3..9).contains(&y)))
        .unwrap();
    mask
}

#[test]
fn a_mask_on_a_crop_lands_on_its_picture_and_is_empty_outside_the_crop() {
    let image = picture(SIDE as u32, SIDE as u32);
    let crop = cut(&image, CUT);
    let mask = on_crop(&crop);
    let limits = DecodeLimits::default();
    let placed = Placed::checked(&crop, &image, [CUT[0] as u32, CUT[1] as u32], &limits).unwrap();
    let carried = placed.carry(&mask).unwrap();
    // On the picture, in the picture's own binding; what it is for and where it came
    // from are the mask's.
    assert_eq!(*carried.binding(), image.binding());
    assert_eq!(carried.use_case(), UseCase::Remove);
    assert_eq!(carried.source(), mask.source());
    let steps = carried.origin().steps();
    assert_eq!(steps.last().unwrap().action, NAME);
    assert_eq!(steps.len(), mask.origin().steps().len() + 1);
    // Every plane keeps its pixels, moved by the crop's corner, and nothing is set
    // outside the crop.
    let planes = [
        (carried.core(), mask.core()),
        (carried.blend(), mask.blend()),
        (carried.protect(), mask.protect()),
    ];
    for (name, (on_picture, on_crop)) in ["core", "blend", "protect"].iter().zip(planes) {
        for (at, sample) in on_picture.iter().enumerate() {
            let (x, y) = (at % SIDE, at / SIDE);
            let inside = (CUT[0]..CUT[2]).contains(&x) && (CUT[1]..CUT[3]).contains(&y);
            let expected = match inside {
                true => on_crop[(y - CUT[1]) * 16 + (x - CUT[0])],
                false => 0,
            };
            assert_eq!(*sample, expected, "{name} at {x},{y}");
        }
    }
    assert_eq!(carried.core().iter().filter(|s| **s != 0).count(), 24);

    // The action: each mask of the crop, in the order given, on the picture.
    let shop = Shop::new("carry-lands", None);
    let image_id = shop.put(Artifact::Image(image.clone()));
    let crop_id = shop.put(Artifact::Image(crop.clone()));
    let second = Mask::from_core(
        crop.binding(),
        UseCase::Remove,
        plane(16, 16, |x, y| x == 0 && y == 0),
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    let (first_id, second_id) = (shop.put(mask.clone().into()), shop.put(second.into()));
    let call = Call::new(NAME)
        .argument("left", Value::Integer(CUT[0] as i64))
        .argument("top", Value::Integer(CUT[1] as i64))
        .input("mask", first_id)
        .input("mask", second_id)
        .input("crop", crop_id)
        .input("image", image_id);
    let outputs = shop.completed(&call);
    assert_eq!(outputs.len(), 2, "{outputs:?}");
    assert!(outputs.iter().all(|(port, _)| port == "mask"));
    assert_eq!(outputs[0].1, carried.id());
    let corner = shop.mask(outputs[1].1);
    let set: Vec<usize> = (0..SIDE * SIDE)
        .filter(|at| corner.core()[*at] != 0)
        .collect();
    assert_eq!(set, [CUT[1] * SIDE + CUT[0]]);
}

#[test]
fn a_mask_on_a_crop_of_another_picture_is_refused() {
    let image = picture(SIDE as u32, SIDE as u32);
    let crop = cut(&image, CUT);
    let mask = on_crop(&crop);
    let limits = DecodeLimits::default();
    let at = [CUT[0] as u32, CUT[1] as u32];
    let said = |refused: Result<Placed, CarryError>| match refused {
        Err(CarryError::Invalid(why)) => why,
        other => panic!("{:?}", other.map(|_| "placed")),
    };
    // Another picture of the same size: its pixels there are not the crop's.
    let rgba = vec![7; SIDE * SIDE * 4];
    let other = CanonicalPixels::from_rgba(SIDE as u32, SIDE as u32, rgba).unwrap();
    let other = image_of(&other, "import");
    let why = said(Placed::checked(&crop, &other, at, &limits));
    assert!(why.contains("is not the picture's"), "{why}");
    // The same picture, and another corner.
    let why = said(Placed::checked(&crop, &image, [at[0] + 1, at[1]], &limits));
    assert!(why.contains("is not the picture's"), "{why}");
    // A corner from which the crop would reach outside the picture.
    let why = said(Placed::checked(&crop, &image, [20, 4], &limits));
    assert!(why.contains("reaches outside"), "{why}");
    // A picture is no crop of itself.
    let why = said(Placed::checked(&image, &image, [0, 0], &limits));
    assert!(why.contains("the picture itself"), "{why}");
    // A mask that is not on the crop.
    let placed = Placed::checked(&crop, &image, at, &limits).unwrap();
    let elsewhere = Mask::from_core(
        image.binding(),
        UseCase::Remove,
        plane(SIDE, SIDE, |x, y| x == 1 && y == 1),
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    match placed.carry(&elsewhere) {
        Err(CarryError::Invalid(why)) => assert!(why.contains("is not on the crop"), "{why}"),
        other => panic!("{:?}", other.map(|mask| mask.id())),
    }

    // The action refuses the same, as a call that cannot be worked on, and commits
    // nothing.
    let shop = Shop::new("carry-refused", None);
    let other_id = shop.put(Artifact::Image(other));
    let crop_id = shop.put(Artifact::Image(crop));
    let mask_id = shop.put(mask.into());
    let why = shop.refused(
        &Call::new(NAME)
            .argument("left", Value::Integer(i64::from(at[0])))
            .argument("top", Value::Integer(i64::from(at[1])))
            .input("mask", mask_id)
            .input("crop", crop_id)
            .input("image", other_id),
    );
    assert!(why.contains("is not the picture's"), "{why}");
}
