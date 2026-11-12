//! Which image a mask may be used on, and the checked rebind onto an image derived from
//! its own (`SPEC.md` section 3).

use whizbang_core::artifact::{ArtifactError, Derivation, Mask};
use whizbang_core::identity::{
    Binding, Destination, Direction, Mapping, PixelDigest, Projection, Space, Topology,
};

use super::support::*;

/// The binding that places `image` onto `onto`, pixel for pixel: what the action that
/// derived `image` from `onto` states.
fn derived(image: &str, onto: PixelDigest) -> Binding {
    Binding::new(
        picture(image),
        space(),
        onto,
        space(),
        Direction::SourceToDestination,
        Mapping::Identity,
    )
    .unwrap()
}

/// States the binding as a derivation and rebinds the mask through it.
fn rebind(mask: &Mask, binding: &Binding) -> Result<Mask, ArtifactError> {
    mask.rebind(&Derivation::stated(binding.clone())?)
}

fn refused(result: Result<Mask, ArtifactError>) -> String {
    match result {
        Err(ArtifactError::Rebind(why)) => why,
        other => panic!("not refused as a rebind: {other:?}"),
    }
}

#[test]
fn a_mask_is_refused_on_an_unrelated_image_of_the_same_size() {
    let mask = full_mask(on("photo"));
    mask.require_image(picture("photo"), space()).unwrap();
    // Another image, of the same size and in the same space.
    let unrelated = picture("another photo");
    assert!(matches!(
        mask.require_image(unrelated, space()),
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
        mask.require_image(picture("photo"), partial),
        Err(ArtifactError::WrongImage(_))
    ));

    // Nor can it be rebound there: the unrelated image in its own space shows no
    // derivation, and its placement onto some third image shows none from the mask's.
    let why = refused(rebind(&mask, &Binding::root(unrelated, space())));
    assert!(why.contains("onto nothing"), "{why}");
    let why = refused(rebind(
        &mask,
        &derived("another photo", picture("a third photo")),
    ));
    assert!(why.contains("the mask belongs to"), "{why}");
    // And the binding that would show it the other way round does not serve either.
    let backwards = derived("photo", unrelated);
    assert!(refused(rebind(&mask, &backwards)).contains("the mask belongs to"));
}

#[test]
fn a_mask_is_accepted_through_a_checked_rebind_on_a_composite_derived_from_its_image() {
    let mask = full_mask(on("photo"));
    let derivation = derived("composite", picture("photo"));
    let rebound = rebind(&mask, &derivation).unwrap();

    // It may now be used on the composite, and no longer on the photo.
    rebound
        .require_image(picture("composite"), space())
        .unwrap();
    assert!(matches!(
        rebound.require_image(picture("photo"), space()),
        Err(ArtifactError::WrongImage(_))
    ));
    // It belongs to the derivation, which names both images.
    assert_eq!(*rebound.binding(), derivation);
    assert_eq!(
        rebound.binding().destination(),
        Destination::Image(picture("photo"))
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
    assert_eq!(*profile.band.binding(), derivation);
    assert_eq!(*profile.sigma.binding(), derivation);
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
            ("derivation".to_owned(), derivation.id().0)
        ]
    );
    assert!(last.arguments.is_empty());

    // It is a mask like any other: it goes through its file, and on to the next composite.
    let read = Mask::from_bytes(&rebound.to_bytes(), &limits()).unwrap();
    assert_eq!(read, rebound);
    let next = rebind(&rebound, &derived("second composite", picture("composite"))).unwrap();
    next.require_image(picture("second composite"), space())
        .unwrap();
    // The same rebind of the same mask is the same mask.
    assert_eq!(rebind(&mask, &derivation).unwrap().id(), rebound.id());
}

#[test]
fn a_rebind_keeps_every_pixel_in_its_place() {
    let mask = rect_mask(on("photo"));
    let photo = picture("photo");
    // A crop of the photo is derived from it, but its pixels are not the photo's pixels.
    let crop = Binding::new(
        picture("crop"),
        Space::flat(4, 3).unwrap(),
        photo,
        space(),
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 0.0,
            top: 0.0,
            right: 4.0,
            bottom: 3.0,
        },
    )
    .unwrap();
    assert!(refused(rebind(&mask, &crop)).contains("identity"));
    // A resize to the same size is a crop of the whole: still not the identity.
    let resize = Binding::new(
        picture("resized"),
        space(),
        photo,
        space(),
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 0.0,
            top: 0.0,
            right: f64::from(WIDTH),
            bottom: f64::from(HEIGHT),
        },
    )
    .unwrap();
    assert!(refused(rebind(&mask, &resize)).contains("identity"));
    let shifted = Binding::new(
        picture("shifted"),
        space(),
        photo,
        space(),
        Direction::SourceToDestination,
        Mapping::Homography {
            matrix: [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        },
    )
    .unwrap();
    assert!(refused(rebind(&mask, &shifted)).contains("identity"));
    // The photo placed onto itself derives nothing.
    assert!(refused(rebind(&mask, &derived("photo", photo))).contains("onto itself"));
    // Stated from the destination, the identity is still the identity.
    let stated_back = Binding::new(
        picture("composite"),
        space(),
        photo,
        space(),
        Direction::DestinationToSource,
        Mapping::Identity,
    )
    .unwrap();
    rebind(&mask, &stated_back).unwrap();
}

#[test]
fn a_derivation_in_another_space_of_as_many_samples_is_refused() {
    // The planes are 8 by 6. Each binding below names the mask's image as its destination
    // and is the identity, in a space that holds 48 samples too and is not the mask's.
    let mask = full_mask(on("photo"));
    let in_space = |space: Space| {
        Binding::new(
            picture("composite"),
            space,
            picture("photo"),
            space,
            Direction::SourceToDestination,
            Mapping::Identity,
        )
        .unwrap()
    };
    // Turned on its side: 6 by 8.
    let turned = Space::flat(HEIGHT, WIDTH).unwrap();
    let why = refused(rebind(&mask, &in_space(turned)));
    assert!(why.contains("spaces"), "{why}");
    // The same size, as part of a panorama, and as a complete one.
    let partial = Space::new(
        WIDTH,
        HEIGHT,
        Projection::Equirectangular,
        Topology::Bounded,
    )
    .unwrap();
    let why = refused(rebind(&mask, &in_space(partial)));
    assert!(why.contains("spaces"), "{why}");
    let panorama = Space::new(WIDTH, HEIGHT, Projection::Equirectangular, Topology::Wrap).unwrap();
    let why = refused(rebind(&mask, &in_space(panorama)));
    assert!(why.contains("spaces"), "{why}");
    // In the mask's own space it is accepted.
    rebind(&mask, &in_space(space())).unwrap();
}

#[test]
fn a_derivation_is_a_statement_of_its_own_and_adds_nothing_to_its_binding() {
    let photo = picture("photo");
    let binding = derived("composite", photo);
    let derivation = Derivation::stated(binding.clone()).unwrap();
    assert_eq!(*derivation.binding(), binding);
    assert_eq!(derivation.image(), picture("composite"));
    assert_eq!(derivation.parent(), photo);
    // Stating it changes no identity: the rebound mask belongs to the binding as it was.
    let rebound = full_mask(on("photo")).rebind(&derivation).unwrap();
    assert_eq!(rebound.binding().id(), binding.id());

    // A binding that cannot say "derived, pixel for pixel in place" is not stated as one.
    let not_one = |binding: Binding| match Derivation::stated(binding) {
        Err(ArtifactError::Rebind(why)) => why,
        other => panic!("stated as a derivation: {other:?}"),
    };
    assert!(not_one(on("composite")).contains("onto nothing"));
    assert!(not_one(derived("photo", photo)).contains("onto itself"));
    let crop = Binding::new(
        picture("crop"),
        Space::flat(4, 3).unwrap(),
        photo,
        space(),
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 0.0,
            top: 0.0,
            right: 4.0,
            bottom: 3.0,
        },
    )
    .unwrap();
    assert!(not_one(crop).contains("identity"));
}

#[test]
fn a_derivation_is_trusted_to_be_true() {
    // The boundary ADR 0002 states. The rebind checks the statement; it cannot see either
    // picture. A caller who states that an unrelated image of the same size was derived
    // from the mask's image is believed here, and the mask moves. Refusing a derivation
    // stated by hand belongs to the executor and the fill composite (the carry-over list).
    let mask = full_mask(on("photo"));
    let untrue = Derivation::stated(derived("another photo", picture("photo"))).unwrap();
    let moved = mask.rebind(&untrue).unwrap();
    moved
        .require_image(picture("another photo"), space())
        .unwrap();
    // What it cannot do is go unrecorded: the origin names the mask and the statement.
    let last = moved.origin().steps().last().unwrap();
    assert_eq!(last.action, "rebind");
    assert_eq!(
        last.inputs[1],
        ("derivation".to_owned(), untrue.binding().id().0)
    );
    // And the planes it carries protect the same places as before.
    assert_eq!(moved.protect(), mask.protect());
}
