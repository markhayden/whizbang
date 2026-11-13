//! Which image a mask may be used on, and the checked rebind onto an image derived from
//! its own (`SPEC.md` section 3). A derivation is made only by checking the two pictures
//! against a mask (ADR 0006, "Who vouches for a derivation").

use whizbang_core::artifact::{ArtifactError, Derivation, Mask};
use whizbang_core::identity::{
    Binding, CanonicalPixels, Destination, Direction, Mapping, Projection, Space, Topology,
};

use super::support::*;

/// The pixels with one byte changed: the byte `channel` of the pixel at `(x, y)`.
fn with_byte(pixels: &CanonicalPixels, x: u32, y: u32, channel: usize) -> CanonicalPixels {
    let mut rgba = pixels.rgba().to_vec();
    let at = ((y * WIDTH + x) * 4) as usize + channel;
    rgba[at] = rgba[at].wrapping_add(1);
    CanonicalPixels::from_rgba(WIDTH, HEIGHT, rgba).unwrap()
}

fn refused(result: Result<Mask, ArtifactError>) -> String {
    match result {
        Err(ArtifactError::Rebind(why)) => why,
        other => panic!("not refused as a rebind: {other:?}"),
    }
}

fn not_derived(result: Result<Derivation, ArtifactError>) -> String {
    match result {
        Err(ArtifactError::NotDerived(why)) => why,
        other => panic!("not refused as no derivation: {other:?}"),
    }
}

#[test]
fn a_mask_is_refused_on_an_unrelated_image_of_the_same_size() {
    let (photo, unrelated) = (pixels("photo"), pixels("another photo"));
    let mask = full_mask(shown("photo"));
    mask.require_image(photo.digest(), space()).unwrap();
    // Another image, of the same size and in the same space.
    assert!(matches!(
        mask.require_image(unrelated.digest(), space()),
        Err(ArtifactError::WrongImage(_))
    ));
    // Its own image, said to be in another space of the same size.
    let partial = Space::new(
        WIDTH,
        HEIGHT,
        Projection::Equirectangular,
        Topology::Bounded,
    )
    .unwrap();
    assert!(matches!(
        mask.require_image(photo.digest(), partial),
        Err(ArtifactError::WrongImage(_))
    ));

    // Nor can it be rebound there. The unrelated image is no derivation of the mask's:
    // its pixels differ where the mask weighs nothing.
    let why = not_derived(Derivation::checked(&photo, &unrelated, &mask));
    assert!(why.contains("differ"), "{why}");
    // And a true derivation between two other images does not move this mask.
    let other_mask = full_mask(shown("another photo"));
    let elsewhere = Derivation::checked(
        &unrelated,
        &painted_through(&unrelated, &other_mask),
        &other_mask,
    )
    .unwrap();
    let why = refused(mask.rebind(&elsewhere));
    assert!(why.contains("the mask belongs to"), "{why}");
}

#[test]
fn a_mask_is_accepted_through_a_checked_rebind_on_a_composite_derived_from_its_image() {
    let photo = pixels("photo");
    let mask = full_mask(shown("photo"));
    let composite = painted_through(&photo, &mask);
    let derivation = Derivation::checked(&photo, &composite, &mask).unwrap();
    let rebound = mask.rebind(&derivation).unwrap();

    // It may now be used on the composite, and no longer on the photo.
    rebound.require_image(composite.digest(), space()).unwrap();
    assert!(matches!(
        rebound.require_image(photo.digest(), space()),
        Err(ArtifactError::WrongImage(_))
    ));
    // It belongs to the derivation's binding, which names both images.
    let binding = derivation.binding().clone();
    assert_eq!(*rebound.binding(), binding);
    assert_eq!(
        rebound.binding().destination(),
        Destination::Image(photo.digest())
    );
    // It is another mask, with the same planes and everything else it carried, moved
    // with it.
    assert_ne!(rebound.id(), mask.id());
    assert_eq!(rebound.core(), mask.core());
    assert_eq!(rebound.blend(), mask.blend());
    assert_eq!(rebound.protect(), mask.protect());
    assert_eq!(rebound.use_case(), mask.use_case());
    assert_eq!(rebound.source(), mask.source());
    let (profile, was) = (rebound.profile().unwrap(), mask.profile().unwrap());
    assert_eq!(*profile.band.binding(), binding);
    assert_eq!(*profile.sigma.binding(), binding);
    assert_eq!(profile.band.half(), was.band.half());
    assert_eq!(profile.band.line(), was.band.line());
    assert_eq!(profile.sigma.values(), was.sigma.values());
    assert_eq!(rebound.prescriptions().len(), 1);
    // The origin says what happened, and from which mask.
    let steps = rebound.origin().steps();
    assert_eq!(steps.len(), mask.origin().steps().len() + 1);
    let last = steps.last().unwrap();
    assert_eq!(last.action, "rebind");
    // Both are inputs, by identity: the mask it came from, and the derivation's binding.
    assert_eq!(
        last.inputs,
        [
            ("mask".to_owned(), mask.id().0),
            ("derivation".to_owned(), binding.id().0)
        ]
    );
    assert!(last.arguments.is_empty());

    // It is a mask like any other: it goes through its file, and on to the next
    // composite, which is checked against the rebound mask and the first composite.
    let read = Mask::from_bytes(&rebound.to_bytes(), &limits()).unwrap();
    assert_eq!(read, rebound);
    let second = painted_through(&composite, &rebound);
    let next = rebound
        .rebind(&Derivation::checked(&composite, &second, &rebound).unwrap())
        .unwrap();
    next.require_image(second.digest(), space()).unwrap();
    // The same rebind of the same mask is the same mask.
    let again = Derivation::checked(&photo, &composite, &mask).unwrap();
    assert_eq!(mask.rebind(&again).unwrap().id(), rebound.id());
    // And it is the mask made by hand on that binding, with everything it carries: the
    // fixed vector of a rebound mask (`vectors.rs`) is made that way.
    let line = line(shown("photo")).id();
    assert_eq!(rebound.id(), full_mask_moved(&mask, binding, line).id());
}

#[test]
fn a_derivation_is_made_only_of_two_pictures_that_differ_inside_a_mask_alone() {
    // The inverse of the test this replaced, which showed a derivation taken at its
    // word. A derivation is now made by looking: every pixel the mask weighs nothing
    // (outside its core and its blend, or protected) is the same four bytes in both.
    let photo = pixels("photo");
    let mask = full_mask(shown("photo"));
    let composite = painted_through(&photo, &mask);
    Derivation::checked(&photo, &composite, &mask).unwrap();

    // `full_mask`: the core is columns 3 and 4, the blend columns 2 and 5, and the
    // first row of columns 0 to 2 is protected. Column 7 is outside all of it.
    assert_eq!(mask.weight(7, 2), Some(0.0));
    assert_eq!(mask.weight(1, 0), Some(0.0));
    assert!(mask.protect()[1] != 0);
    for (x, y, channel, where_it_is) in [
        (7, 2, 0, "outside the mask"),
        (7, 2, 3, "outside the mask, in its alpha alone"),
        (1, 0, 1, "protected"),
        (2, 0, 2, "protected, where the blend would have reached"),
    ] {
        let changed = with_byte(&composite, x, y, channel);
        let why = not_derived(Derivation::checked(&photo, &changed, &mask));
        assert_eq!(
            why,
            format!(
                "1 pixel that the mask weighs nothing differs from the image it was made \
                 from, the first at {x}, {y}"
            ),
            "{where_it_is}"
        );
    }
    // Inside the core and inside the blend, any change is the composite's to make,
    // alpha included.
    for (x, y) in [(3, 2), (4, 5), (2, 3), (5, 0)] {
        assert!(mask.weight(x, y).unwrap() > 0.0);
        for channel in 0..4 {
            let changed = with_byte(&composite, x, y, channel);
            Derivation::checked(&photo, &changed, &mask).unwrap();
        }
    }
    // An unrelated picture of the same size: how many pixels, and the first.
    let why = not_derived(Derivation::checked(&photo, &pixels("another photo"), &mask));
    assert!(
        why.starts_with("25 pixels that the mask weighs nothing differ from the image"),
        "{why}"
    );
    assert!(why.ends_with("the first at 0, 0"), "{why}");

    // The same picture is derived from nothing.
    let why = not_derived(Derivation::checked(&photo, &photo, &mask));
    assert!(why.contains("the same picture"), "{why}");
    // A picture of another size.
    let small = CanonicalPixels::from_rgba(2, 2, vec![0; 16]).unwrap();
    let why = not_derived(Derivation::checked(&photo, &small, &mask));
    assert!(why.contains("2 by 2"), "{why}");
    // A mask that belongs to another image vouches for nothing about this one.
    let foreign = full_mask(shown("another photo"));
    assert!(matches!(
        Derivation::checked(&photo, &composite, &foreign),
        Err(ArtifactError::WrongImage(_))
    ));
}

#[test]
fn a_derivation_is_a_statement_of_its_own_and_adds_nothing_to_its_binding() {
    let photo = pixels("photo");
    let mask = rect_mask(shown("photo"));
    let composite = painted_through(&photo, &mask);
    let derivation = Derivation::checked(&photo, &composite, &mask).unwrap();
    // Its binding places the new image onto the one it was made from, by the identity,
    // in the mask's space: the binding anyone could write for the two images.
    let binding = Binding::new(
        composite.digest(),
        space(),
        photo.digest(),
        space(),
        Direction::SourceToDestination,
        Mapping::Identity,
    )
    .unwrap();
    assert_eq!(*derivation.binding(), binding);
    assert_eq!(derivation.image(), composite.digest());
    assert_eq!(derivation.parent(), photo.digest());
    // So the rebound mask is the mask made by hand on that binding: one artifact.
    let rebound = mask.rebind(&derivation).unwrap();
    assert_eq!(rebound.binding().id(), binding.id());
    assert_eq!(rebound.id(), rect_mask(binding).id());
}

#[test]
fn a_second_mask_on_the_image_is_rebound_through_the_first_masks_derivation() {
    // The ordinary case: one mask was painted through, and another mask on the same
    // image, a protect mask say, is then used on the result. The derivation is about
    // the two pictures, and any mask on the first may be moved by it.
    let photo = pixels("photo");
    let painted = rect_mask(shown("photo"));
    let other = full_mask(shown("photo"));
    let composite = painted_through(&photo, &painted);
    let derivation = Derivation::checked(&photo, &composite, &painted).unwrap();
    let moved = other.rebind(&derivation).unwrap();
    moved.require_image(composite.digest(), space()).unwrap();
    assert_eq!(moved.protect(), other.protect());
}

#[test]
fn a_derivation_in_another_space_of_as_many_samples_is_refused() {
    // The same pixels stated in two spaces of one size: a flat image, and a part of a
    // panorama. A derivation made with the mask of one does not move the mask of the
    // other.
    let photo = pixels("photo");
    let partial = Space::new(
        WIDTH,
        HEIGHT,
        Projection::Equirectangular,
        Topology::Bounded,
    )
    .unwrap();
    let flat = rect_mask(shown("photo"));
    let in_panorama = rect_mask(Binding::root(photo.digest(), partial));
    let composite = painted_through(&photo, &flat);

    let derivation = Derivation::checked(&photo, &composite, &flat).unwrap();
    assert_eq!(derivation.binding().source_space(), space());
    let why = refused(in_panorama.rebind(&derivation));
    assert!(why.contains("spaces"), "{why}");
    flat.rebind(&derivation).unwrap();

    let derivation = Derivation::checked(&photo, &composite, &in_panorama).unwrap();
    assert_eq!(derivation.binding().source_space(), partial);
    let why = refused(flat.rebind(&derivation));
    assert!(why.contains("spaces"), "{why}");
    in_panorama.rebind(&derivation).unwrap();
}
