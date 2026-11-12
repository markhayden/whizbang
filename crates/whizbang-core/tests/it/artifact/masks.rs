//! The mask record: the one rule for a pixel's weight, the invariants of the three planes,
//! the profile and the prescriptions, and what a mask's identity depends on.

use whizbang_core::artifact::{
    ArtifactError, ArtifactType, Mask, Prescription, Refinement, Source, Step, UseCase,
    effective_weight,
};
use whizbang_core::identity::Digest;

use super::support::*;

fn at(x: u32, y: u32) -> usize {
    (y * WIDTH + x) as usize
}

fn invalid<T: std::fmt::Debug>(result: Result<T, ArtifactError>) -> String {
    match result {
        Err(ArtifactError::Invalid {
            artifact: ArtifactType::Mask,
            why,
        }) => why,
        other => panic!("not refused as a mask: {other:?}"),
    }
}

#[test]
fn the_weight_of_a_pixel_follows_one_rule() {
    for blend in 0..=255u8 {
        for core in [0u8, 255] {
            for protect in [0u8, 255] {
                let expected = if protect != 0 {
                    0.0
                } else if core != 0 {
                    1.0
                } else {
                    f32::from(blend) / 255.0
                };
                assert_eq!(
                    effective_weight(core, blend, protect),
                    expected,
                    "core {core} blend {blend} protect {protect}"
                );
            }
        }
    }
    let mut mask = rect_mask(on("photo"));
    let mut blend = vec![0; (WIDTH * HEIGHT) as usize];
    blend[at(0, 5)] = 51;
    blend[at(3, 2)] = 200; // inside the core: forced to zero
    mask.set_blend(blend).unwrap();
    assert_eq!(mask.weight(3, 2), Some(1.0));
    assert_eq!(mask.weight(0, 5), Some(0.2));
    assert_eq!(mask.weight(0, 0), Some(0.0));
    assert_eq!(mask.blend()[at(3, 2)], 0);
    let mut protect = vec![0; (WIDTH * HEIGHT) as usize];
    protect[at(3, 2)] = 1;
    protect[at(0, 5)] = 255;
    mask.protect_union(&protect).unwrap();
    assert_eq!(mask.weight(3, 2), Some(0.0));
    assert_eq!(mask.weight(0, 5), Some(0.0));
    assert_eq!(
        mask.core()[at(3, 2)],
        0,
        "the core is cleared under protect"
    );
    assert_eq!(
        mask.blend()[at(0, 5)],
        0,
        "the blend is cleared under protect"
    );
    assert_eq!(mask.protect()[at(3, 2)], 255, "protect is binary");
    assert_eq!(mask.weight(2, 1), Some(1.0));
}

#[test]
fn the_three_planes_are_the_size_of_the_image_and_the_core_is_binary() {
    let binding = on("photo");
    let make = |core: Vec<u8>| {
        Mask::from_core(
            binding.clone(),
            UseCase::Custom,
            core,
            Source::default(),
            made_by("rect"),
        )
    };
    assert!(invalid(make(vec![0; 47])).contains("47 samples"));
    assert!(invalid(make(vec![0; 49])).contains("49 samples"));
    // Any sample that is not 0 is in the core.
    let mut soft = vec![0; 48];
    soft[5] = 1;
    soft[6] = 200;
    let mask = make(soft).unwrap();
    assert_eq!(&mask.core()[4..8], [0, 255, 255, 0]);
    assert!(mask.blend().iter().all(|sample| *sample == 0));
    assert!(mask.protect().iter().all(|sample| *sample == 0));
    assert_eq!(mask.image(), picture("photo"));
    assert_eq!(mask.space(), space());
    assert_eq!(mask.use_case(), UseCase::Custom);

    let mut mask = rect_mask(binding);
    assert!(invalid(mask.set_blend(vec![0; 47])).contains("blend"));
    assert!(invalid(mask.protect_union(&[0; 49])).contains("protect"));
    assert!(invalid(mask.replace_core(vec![0; 1])).contains("core"));
}

#[test]
fn a_profile_and_a_prescription_belong_to_the_mask_and_cover_every_row() {
    let binding = on("photo");
    let line = line(binding.clone()).id();
    let mut mask = rect_mask(binding.clone());

    // On another image of the same size.
    let other = on("another photo");
    let why =
        invalid(mask.set_profile(band(other.clone(), line, 1.5), sigma(binding.clone(), 1.0)));
    assert!(why.contains("binding"), "{why}");
    let why =
        invalid(mask.set_profile(band(binding.clone(), line, 1.5), sigma(other.clone(), 1.0)));
    assert!(why.contains("binding"), "{why}");
    // A sigma of three channels, and one that misses a row.
    let why =
        invalid(mask.set_profile(band(binding.clone(), line, 1.5), step(binding.clone(), 1.0)));
    assert!(why.contains("every row"), "{why}");
    let short = whizbang_core::artifact::Series::new(
        binding.clone(),
        "sigma_px",
        "px",
        1,
        vec![1.0; 5],
        made_by("feather"),
    )
    .unwrap();
    assert!(
        invalid(mask.set_profile(band(binding.clone(), line, 1.5), short)).contains("every row")
    );
    assert!(mask.profile().is_none());

    // A ramp on another image, of one channel, and with a share outside 0 to 1.
    assert!(invalid(mask.push_tone_ramp(step(other, 1.0), 0.5)).contains("binding"));
    assert!(invalid(mask.push_tone_ramp(sigma(binding.clone(), 1.0), 0.5)).contains("three"));
    for share in [-0.1, 1.1, f64::NAN] {
        assert!(invalid(mask.push_tone_ramp(step(binding.clone(), 1.0), share)).contains("share"));
    }
    assert!(mask.prescriptions().is_empty());

    let before = mask.id();
    mask.set_profile(
        band(binding.clone(), line, 1.5),
        sigma(binding.clone(), 1.0),
    )
    .unwrap();
    let with_profile = mask.id();
    assert_ne!(before, with_profile);
    mask.push_tone_ramp(step(binding.clone(), 1.0), 0.5)
        .unwrap();
    assert_ne!(with_profile, mask.id());
    let profile = mask.profile().unwrap();
    assert_eq!(profile.band.line(), line);
    assert_eq!(profile.sigma.values(), [1.0; 6]);
    assert!(matches!(
        &mask.prescriptions()[0],
        Prescription::ToneRamp { version: 1, share, step }
            if *share == 0.5 && step.channels() == 3
    ));
}

#[test]
fn a_masks_identity_holds_its_content_and_its_references_and_nothing_else() {
    let binding = on("photo");
    let mask = full_mask(binding.clone());

    // The origin is not part of it: the same mask recorded as made another way.
    let mut retold = mask.clone();
    retold
        .record(Step::new("touch", "9").input("anything", Digest::of_bytes(b"x")))
        .unwrap();
    assert_eq!(retold.id(), mask.id());
    assert_eq!(retold.origin().steps().len(), 3);
    assert_ne!(retold.to_bytes(), mask.to_bytes());

    // Each of these is.
    let mut ids = vec![mask.id()];
    let mut differs = |what: &str, other: &Mask| {
        assert!(!ids.contains(&other.id()), "{what}");
        ids.push(other.id());
    };
    differs("the image", &full_mask(on("another photo")));
    let mut other = mask.clone();
    other.protect_union(&rect(7, 5, 8, 6)).unwrap();
    differs("a protected pixel", &other);
    let mut other = mask.clone();
    let mut blend = mask.blend().to_vec();
    blend[at(6, 3)] = 9;
    other.set_blend(blend).unwrap();
    differs("a blend sample", &other);
    let mut other = mask.clone();
    other
        .push_tone_ramp(step(binding.clone(), 1.0), 0.25)
        .unwrap();
    differs("a prescription", &other);
    let mut other = mask.clone();
    other
        .set_profile(
            band(binding.clone(), mask.profile().unwrap().band.line(), 1.5),
            sigma(binding.clone(), 0.8),
        )
        .unwrap();
    differs("the feather", &other);
    let mut other = mask.clone();
    other.replace_core(rect(3, 0, 6, HEIGHT)).unwrap();
    differs("the core", &other);

    // The use and the sources, on a plain mask.
    let plain = |use_case, source: Source| {
        Mask::from_core(
            binding.clone(),
            use_case,
            rect(2, 1, 5, 4),
            source,
            made_by("rect"),
        )
        .unwrap()
    };
    let custom = plain(UseCase::Custom, Source::default());
    assert_ne!(custom.id(), plain(UseCase::Repair, Source::default()).id());
    let from = |name: &[u8]| Source {
        locations: vec![],
        instances: vec![Digest::of_bytes(name)],
    };
    assert_ne!(custom.id(), plain(UseCase::Custom, from(b"a")).id());
    assert_ne!(
        plain(UseCase::Custom, from(b"a")).id(),
        plain(UseCase::Custom, from(b"b")).id()
    );
    // A location and an instance of the same digest are two different sources.
    let as_location = Source {
        locations: vec![whizbang_core::artifact::ArtifactId(Digest::of_bytes(b"a"))],
        instances: vec![],
    };
    assert_ne!(
        plain(UseCase::Custom, from(b"a")).id(),
        plain(UseCase::Custom, as_location).id()
    );
}

#[test]
fn replacing_the_core_clears_the_blend_the_profile_and_the_prescriptions_and_keeps_protect() {
    let mut mask = full_mask(on("photo"));
    assert!(mask.profile().is_some());
    assert_eq!(mask.prescriptions().len(), 1);
    assert!(mask.blend().iter().any(|sample| *sample != 0));
    let protect = mask.protect().to_vec();
    assert!(protect.iter().any(|sample| *sample != 0));

    let report = mask.replace_core(rect(0, 0, 4, 3)).unwrap();
    assert_eq!(
        report,
        Refinement {
            blend_cleared: true,
            profile_cleared: true
        }
    );
    assert!(mask.profile().is_none());
    assert!(mask.prescriptions().is_empty());
    assert!(mask.blend().iter().all(|sample| *sample == 0));
    assert_eq!(mask.protect(), protect);
    // Protect still wins where the new core lies under it: the core is cleared there.
    assert_eq!(mask.core()[at(1, 0)], 0);
    assert_eq!(mask.core()[at(1, 1)], 255);
    assert_eq!(mask.weight(1, 0), Some(0.0));
    assert_eq!(mask.weight(1, 1), Some(1.0));

    let report = mask.replace_core(rect(0, 0, 4, 3)).unwrap();
    assert!(!report.profile_cleared);
}

#[test]
fn a_blend_set_after_protect_is_zero_under_it() {
    let mut mask = rect_mask(on("photo"));
    mask.protect_union(&rect(0, 0, 2, 2)).unwrap();
    // A blend everywhere: it stays only where there is neither core nor protect.
    mask.set_blend(vec![77; (WIDTH * HEIGHT) as usize]).unwrap();
    assert_eq!(mask.blend()[at(1, 1)], 0, "under protect");
    assert_eq!(mask.blend()[at(3, 2)], 0, "inside the core");
    assert_eq!(mask.blend()[at(7, 5)], 77, "outside both");
    // The mask is one its own file accepts.
    assert_eq!(Mask::from_bytes(&mask.to_bytes(), &limits()).unwrap(), mask);
}

#[test]
fn a_replaced_core_is_binary() {
    let mut mask = rect_mask(on("photo"));
    let mut core = vec![0; (WIDTH * HEIGHT) as usize];
    core[at(0, 0)] = 1;
    core[at(1, 0)] = 128;
    core[at(2, 0)] = 255;
    mask.replace_core(core).unwrap();
    assert_eq!(&mask.core()[..4], [255, 255, 255, 0]);
    assert_eq!(Mask::from_bytes(&mask.to_bytes(), &limits()).unwrap(), mask);
}

#[test]
fn a_tone_ramps_step_that_misses_rows_is_refused() {
    let binding = on("photo");
    let mut mask = rect_mask(binding.clone());
    // Three channels, and five rows of an image of six.
    let short = whizbang_core::artifact::Series::new(
        binding,
        "tone_ramp.step",
        "levels",
        3,
        vec![1.0; 15],
        made_by("ramp"),
    )
    .unwrap();
    assert_eq!((short.channels(), short.rows().len()), (3, 5));
    assert!(invalid(mask.push_tone_ramp(short, 0.5)).contains("every row"));
    assert!(mask.prescriptions().is_empty());
}

#[test]
fn a_weight_is_asked_of_a_pixel_the_image_has() {
    let mask = rect_mask(on("photo"));
    // The last pixel of the first row, and the first of the second: column 8 of row 0 is
    // no pixel, and is not read as column 0 of row 1.
    assert_eq!(mask.weight(7, 0), Some(0.0));
    assert_eq!(mask.weight(2, 1), Some(1.0));
    assert_eq!(mask.weight(WIDTH, 0), None);
    assert_eq!(mask.weight(WIDTH + 2, 0), None, "not the core at (2, 1)");
    assert_eq!(mask.weight(0, HEIGHT), None);
    assert_eq!(mask.weight(u32::MAX, u32::MAX), None);
    assert_eq!(mask.weight(WIDTH - 1, HEIGHT - 1), Some(0.0));
}

#[test]
fn two_masks_that_weigh_alike_have_the_same_planes_and_one_identity() {
    let protect = rect(0, 0, 3, 2);
    let core = rect(1, 1, 5, 4);
    // Protect first and then the core, and the core first and then protect.
    let mut one = rect_mask(on("photo"));
    one.protect_union(&protect).unwrap();
    one.replace_core(core.clone()).unwrap();
    let mut other = rect_mask(on("photo"));
    other.replace_core(core).unwrap();
    other.protect_union(&protect).unwrap();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            assert_eq!(one.weight(x, y), other.weight(x, y), "({x}, {y})");
        }
    }
    assert_eq!(one.core()[at(1, 1)], 0, "the core is cleared under protect");
    assert_eq!(one.core()[at(3, 1)], 255);
    assert_eq!(one.core(), other.core());
    assert_eq!(one.id(), other.id());
    assert_eq!(Mask::from_bytes(&one.to_bytes(), &limits()).unwrap(), one);
}
