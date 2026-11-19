//! `apply` (plan m3, task O2, item 1): rascal's tone ramp performed to the pixel
//! (`crates/rascal-ops/tests/it/ramp_apply.rs` at the commit `docs/sources.toml` pins,
//! with the ramp's step given, since `ramp` is O3's), and the working image and the mask
//! rebound onto it, through the executor.

use whizbang_core::artifact::{
    ArtifactId, BandProfile, Derivation, Mask, Origin, Series, Source, Step, UseCase, Windows,
};
use whizbang_core::contract::Call;
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Destination, Digest, Mapping};
use whizbang_rascal::apply::performed;

use super::canvases::{World, at, engine, image, pixels_of, put, ran};
use super::grow::plane;

const W: u32 = 200;
const H: u32 = 30;
const C: u32 = 100;

fn made() -> Origin {
    Origin::new(Step::new("fixture", "1")).unwrap()
}

/// Rascal's flat canvas with a step of `delta` levels a channel across the centre, the
/// left side darker.
fn stepped(delta: [f64; 3]) -> CanonicalPixels {
    let rgba = (0..W * H)
        .flat_map(|at| {
            let left = at % W < C;
            let v = |ch: usize| {
                let base = [120.0, 130.0, 140.0][ch];
                (if left { base } else { base + delta[ch] }).round() as u8
            };
            [v(0), v(1), v(2), 255]
        })
        .collect();
    CanonicalPixels::from_rgba(W, H, rgba).unwrap()
}

/// A mask on `image` whose core is rascal's `band_core` of half-width `core_half`, with
/// a band of half-width `band_half` as its profile, `protect` protected, and a tone ramp
/// of `step` levels a channel and share `share`.
fn ramped(
    image: &whizbang_core::artifact::Image,
    core_half: f64,
    band_half: f64,
    step: [f64; 3],
    share: f64,
    protect: impl Fn(usize, usize) -> bool,
) -> Mask {
    let binding = image.binding();
    let core = plane(W as usize, H as usize, |x, _| {
        (x as f64 + 0.5 - f64::from(C)).abs() <= core_half
    });
    let mut mask = Mask::from_core(
        binding.clone(),
        UseCase::Seam,
        core,
        Source::default(),
        made(),
    )
    .unwrap();
    let band = BandProfile::new(
        binding.clone(),
        ArtifactId(Digest::of_bytes(b"line")),
        C,
        3.0,
        vec![band_half; H as usize],
        Windows {
            gap: 1,
            step_columns: 3,
            texture_columns: 20,
        },
        "fan-v1",
        made(),
    )
    .unwrap();
    let sigma = Series::new(
        binding.clone(),
        "sigma",
        "px",
        1,
        vec![1.0; H as usize],
        made(),
    )
    .unwrap();
    mask.set_profile(band, sigma).unwrap();
    let values = (0..H).flat_map(|_| step).collect();
    let step = Series::new(binding, "tone_ramp.step", "levels", 3, values, made()).unwrap();
    mask.push_tone_ramp(step, share).unwrap();
    mask.protect_union(&plane(W as usize, H as usize, protect))
        .unwrap();
    mask
}

/// The image and the mask `apply` hands back for this picture and mask.
fn applied(
    label: &str,
    pixels: &CanonicalPixels,
    make: impl Fn(&whizbang_core::artifact::Image) -> Mask,
) -> (
    whizbang_core::artifact::Image,
    Mask,
    whizbang_core::artifact::Image,
    Mask,
) {
    let world = World::new(label);
    let picture = image(pixels, false);
    let mask = make(&picture);
    let ids = (put(&world, picture.clone()), put(&world, mask.clone()));
    let engine = engine(&world, None);
    let made = ran(
        &engine,
        &Call::new("apply")
            .input("image", ids.0)
            .input("mask", ids.1),
    );
    (picture, mask, world.image(made[0].1), world.mask(made[1].1))
}

#[test]
fn the_ported_vectors_give_rascals_working_image() {
    // The step of [10, -6, 4] with ramp-v1's share of 0.5 on a band of 20: each pixel
    // the mask weighs moves by share step (1 - d / half) toward the other side, and the
    // two sides meet within a level at the line.
    let delta = [10.0, -6.0, 4.0];
    let canvas = stepped(delta);
    let (picture, mask, working, _) = applied("apply-ported", &canvas, |image| {
        ramped(image, 20.0, 20.0, delta, 0.5, |_, _| false)
    });
    assert_ne!(working.id(), picture.id());
    let after = pixels_of(&working);
    for y in 0..H {
        for x in 0..W {
            let (before, got) = (at(&canvas, x, y), at(&after, x, y));
            let offset = f64::from(x) + 0.5 - f64::from(C);
            let d = offset.abs();
            if mask.weight(x, y).unwrap() <= 0.0 || d >= 20.0 {
                assert_eq!(got, before, "{x},{y}");
                continue;
            }
            let sign = if offset < 0.0 { 1.0 } else { -1.0 };
            for (ch, step) in delta.iter().enumerate() {
                let expected =
                    (f64::from(before[ch]) + sign * 0.5 * step * (1.0 - d / 20.0)).round() as u8;
                assert_eq!(got[ch], expected, "{x},{y} ch {ch}");
            }
            assert_eq!(got[3], 255);
        }
        let (l, r) = (at(&after, C - 1, y), at(&after, C, y));
        for ch in 0..3 {
            assert!(l[ch].abs_diff(r[ch]) <= 1, "row {y} ch {ch}: {l:?} {r:?}");
        }
    }
    let direct = performed(&canvas, &mask).unwrap();
    assert_eq!(direct.pixels, after);
    assert!(direct.changed > 0 && direct.largest >= 4);

    // A core of 4 inside a band of 20, with a block protected: the 8 core columns move on
    // the 25 rows not protected, and no other pixel.
    let canvas = stepped([12.0; 3]);
    let (_, mask, working, _) = applied("apply-narrow", &canvas, |image| {
        ramped(image, 4.0, 20.0, [12.0; 3], 0.5, |x, y| {
            (10..15).contains(&y) && (C as usize - 4..C as usize + 4).contains(&x)
        })
    });
    let after = pixels_of(&working);
    let mut moved = 0;
    for y in 0..H {
        for x in 0..W {
            let same = at(&after, x, y) == at(&canvas, x, y);
            if mask.weight(x, y).unwrap() <= 0.0 {
                assert!(same, "{x},{y}");
            } else {
                assert!(!same, "{x},{y}");
                moved += 1;
            }
        }
    }
    assert_eq!(moved, (H as usize - 5) * 8);
}

#[test]
fn a_mask_without_a_prescription_leaves_the_image_and_the_masks_binding_unchanged() {
    let canvas = stepped([12.0; 3]);
    let (picture, mask, working, handed) = applied("apply-plain", &canvas, |image| {
        let core = plane(W as usize, H as usize, |x, _| (96..104).contains(&x));
        Mask::from_core(
            image.binding(),
            UseCase::Remove,
            core,
            Source::default(),
            made(),
        )
        .unwrap()
    });
    assert_eq!(working.id(), picture.id());
    assert_eq!(handed.id(), mask.id());
    assert_eq!(handed.binding(), mask.binding());
}

#[test]
fn a_ramp_of_no_effect_leaves_the_image_and_the_masks_binding_unchanged() {
    // A step of nothing moves no pixel: nothing was derived.
    let canvas = stepped([12.0; 3]);
    let (picture, mask, working, handed) = applied("apply-none", &canvas, |image| {
        ramped(image, 4.0, 20.0, [0.0; 3], 0.5, |_, _| false)
    });
    assert_eq!(working.id(), picture.id());
    assert_eq!(handed.id(), mask.id());
    assert_eq!(handed.binding(), mask.binding());
}

#[test]
fn the_mask_handed_back_is_rebound_onto_the_working_image() {
    let canvas = stepped([12.0; 3]);
    let (picture, mask, working, handed) = applied("apply-rebound", &canvas, |image| {
        ramped(image, 4.0, 20.0, [12.0; 3], 0.5, |_, _| false)
    });
    let binding = handed.binding();
    assert_eq!(binding.source(), working.identity().pixels);
    assert_eq!(
        binding.destination(),
        Destination::Image(picture.identity().pixels)
    );
    assert_eq!(*binding.mapping(), Mapping::Identity);
    assert_eq!(
        (handed.core(), handed.blend(), handed.protect()),
        (mask.core(), mask.blend(), mask.protect())
    );
    assert_eq!(handed.prescriptions().len(), 1);
    let steps: Vec<&str> = handed
        .origin()
        .steps()
        .iter()
        .map(|s| s.action.as_str())
        .collect();
    assert_eq!(steps, ["fixture", "apply", "rebind"]);
    let limits = DecodeLimits::default();
    let derivation = Derivation::checked(
        &picture.pixels(&limits).unwrap(),
        &working.pixels(&limits).unwrap(),
        &mask,
    )
    .unwrap();
    let mut recorded = mask.clone();
    recorded
        .record(
            Step::new("apply", "1.0.0")
                .input("image", picture.id().0)
                .input("mask", mask.id().0),
        )
        .unwrap();
    assert_eq!(handed.id(), recorded.rebind(&derivation).unwrap().id());
}
