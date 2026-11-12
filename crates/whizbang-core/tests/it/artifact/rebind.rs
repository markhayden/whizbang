//! Which image a mask may be used on, and the checked rebind onto an image derived from
//! its own (`SPEC.md` section 3).

use whizbang_core::artifact::{ArtifactError, Mask};
use whizbang_core::identity::{Binding, Destination, Direction, Mapping, PixelDigest, Space};

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
        whizbang_core::identity::Projection::Equirectangular,
        whizbang_core::identity::Topology::Bounded,
    )
    .unwrap();
    assert!(matches!(
        mask.require_image(picture("photo"), partial),
        Err(ArtifactError::WrongImage(_))
    ));

    // Nor can it be rebound there: the unrelated image in its own space shows no
    // derivation, and its placement onto some third image shows none from the mask's.
    let why = refused(mask.rebind(&Binding::root(unrelated, space())));
    assert!(why.contains("onto nothing"), "{why}");
    let why = refused(mask.rebind(&derived("another photo", picture("a third photo"))));
    assert!(why.contains("the mask belongs to"), "{why}");
    // And the binding that would show it the other way round does not serve either.
    let backwards = derived("photo", unrelated);
    assert!(refused(mask.rebind(&backwards)).contains("the mask belongs to"));
}

#[test]
fn a_mask_is_accepted_through_a_checked_rebind_on_a_composite_derived_from_its_image() {
    let mask = full_mask(on("photo"));
    let derivation = derived("composite", picture("photo"));
    let rebound = mask.rebind(&derivation).unwrap();

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
    assert_eq!(last.inputs, [("mask".to_owned(), mask.id().0)]);
    assert_eq!(
        last.arguments,
        [("binding".to_owned(), derivation.id().to_string())]
    );

    // It is a mask like any other: it goes through its file, and on to the next composite.
    let read = Mask::from_bytes(&rebound.to_bytes(), &limits()).unwrap();
    assert_eq!(read, rebound);
    let next = rebound
        .rebind(&derived("second composite", picture("composite")))
        .unwrap();
    next.require_image(picture("second composite"), space())
        .unwrap();
    // The same rebind of the same mask is the same mask.
    assert_eq!(mask.rebind(&derivation).unwrap().id(), rebound.id());
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
    assert!(refused(mask.rebind(&crop)).contains("identity"));
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
    assert!(refused(mask.rebind(&resize)).contains("identity"));
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
    assert!(refused(mask.rebind(&shifted)).contains("identity"));
    // The photo placed onto itself derives nothing.
    assert!(refused(mask.rebind(&derived("photo", photo))).contains("own image"));
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
    mask.rebind(&stated_back).unwrap();
}
