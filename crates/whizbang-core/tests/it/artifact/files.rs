//! The canonical file: every artifact goes through it without loss, the same artifact
//! gives the same bytes, and anything that is not the one way to write an artifact is
//! refused.

use whizbang_core::artifact::{
    Artifact, ArtifactError, ArtifactType, BandProfile, Image, ImageRole, Location, Mask, Plan,
    Series, Shape,
};
use whizbang_core::identity::{Binding, DecodeLimits, Direction, IdentityError, Mapping, Space};

use super::support::*;

#[test]
fn each_type_goes_through_its_file_and_comes_back_the_same() {
    for artifact in one_of_each() {
        let bytes = artifact.to_bytes();
        let read = Artifact::from_bytes(&bytes, &limits())
            .unwrap_or_else(|error| panic!("{}: {error}", artifact.artifact_type()));
        assert_eq!(read, artifact);
        assert_eq!(read.id(), artifact.id());
        assert_eq!(read.origin(), artifact.origin());
        // The same artifact, written again, is the same bytes.
        assert_eq!(read.to_bytes(), bytes);
        // The file says what it is before anything else.
        let start = format!("whizbang:{}/1\0", artifact.artifact_type().name());
        assert!(bytes.starts_with(start.as_bytes()), "{start:?}");
    }
}

#[test]
fn a_mask_that_carries_a_profile_and_prescriptions_round_trips_with_them() {
    let mask = full_mask(on("photo"));
    let read = Mask::from_bytes(&mask.to_bytes(), &limits()).unwrap();
    assert_eq!(read, mask);
    assert_eq!(read.id(), mask.id());
    let (profile, was) = (read.profile().unwrap(), mask.profile().unwrap());
    // The band and the feather are held whole, with their own identities and origins.
    assert_eq!(profile.band, was.band);
    assert_eq!(profile.band.id(), was.band.id());
    assert_eq!(profile.band.origin(), was.band.origin());
    assert_eq!(profile.sigma, was.sigma);
    assert_eq!(read.prescriptions(), mask.prescriptions());
    assert_eq!(read.source(), mask.source());
    assert_eq!(read.origin().steps().len(), 2);
    assert_eq!(read.origin(), mask.origin());
    assert_eq!((read.core(), read.blend()), (mask.core(), mask.blend()));
    assert_eq!(read.protect(), mask.protect());
    // The band read out of the mask is the band's own file, too.
    let alone = BandProfile::from_bytes(&was.band.to_bytes(), &limits()).unwrap();
    assert_eq!(alone, profile.band);
}

#[test]
fn a_binding_of_every_kind_goes_through_a_file() {
    let flat = Space::flat(320, 200).unwrap();
    let panorama = Space::panorama(400, 200).unwrap();
    let turn = {
        let (s, c) = 30f64.to_radians().sin_cos();
        [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0]
    };
    let bindings = [
        Binding::root(picture("a"), flat),
        Binding::new(
            picture("crop"),
            Space::flat(64, 60).unwrap(),
            picture("a"),
            flat,
            Direction::SourceToDestination,
            Mapping::Crop {
                left: 64.1,
                top: 40.0,
                right: 128.0,
                bottom: 100.7,
            },
        )
        .unwrap(),
        Binding::new(
            picture("aligned"),
            flat,
            picture("a"),
            flat,
            Direction::DestinationToSource,
            Mapping::Homography {
                matrix: [1.0, 0.01, 12.0, -0.02, 1.0, 0.5, 0.0, 0.0, 1.0],
            },
        )
        .unwrap(),
        Binding::new(
            picture("turned"),
            panorama,
            picture("panorama"),
            panorama,
            Direction::SourceToDestination,
            Mapping::Rotation { matrix: turn },
        )
        .unwrap(),
        Binding::new(
            picture("canvas"),
            Space::flat(80, 200).unwrap(),
            picture("panorama"),
            panorama,
            Direction::SourceToDestination,
            Mapping::Wrap { strip: 40 },
        )
        .unwrap(),
    ];
    for binding in bindings {
        let location = Location::new(binding.clone(), Shape::WholeImage, made_by("find")).unwrap();
        let read = Location::from_bytes(&location.to_bytes(), &limits()).unwrap();
        assert_eq!(*read.binding(), binding);
        assert_eq!(read.binding().id(), binding.id());
        assert_eq!(read.id(), location.id());
    }
}

#[test]
fn a_file_of_one_type_is_refused_where_another_is_required() {
    let artifacts = one_of_each();
    for artifact in &artifacts {
        let bytes = artifact.to_bytes();
        for expected in ArtifactType::ALL {
            let result = Artifact::from_bytes_as(expected, &bytes, &limits());
            if expected == artifact.artifact_type() {
                result.unwrap();
            } else {
                assert!(
                    matches!(
                        &result,
                        Err(ArtifactError::WrongType { expected: wanted, .. }) if *wanted == expected
                    ),
                    "{} as {expected}: {result:?}",
                    artifact.artifact_type()
                );
            }
        }
    }
    let series = artifacts[4].to_bytes();
    assert!(matches!(
        Plan::from_bytes(&series, &limits()),
        Err(ArtifactError::WrongType { .. })
    ));
    assert!(Series::from_bytes(&series, &limits()).is_ok());
}

#[test]
fn a_rendering_is_refused_where_a_mask_is_required() {
    let mask = rect_mask(on("photo"));
    let rendering = rendering_of(&mask);
    assert_eq!(rendering.role(), ImageRole::Rendering);

    // As an artifact: it is an image, whatever it pictures.
    let refusal = Artifact::from(rendering.clone()).into_mask().unwrap_err();
    assert!(
        matches!(&refusal, ArtifactError::WrongType { expected: ArtifactType::Mask, found }
            if found.contains("rendering")),
        "{refusal:?}"
    );
    assert_eq!(
        refusal.to_string(),
        "a mask is required, and this is a rendering, which is an image"
    );
    assert_eq!(Artifact::from(mask.clone()).into_mask().unwrap(), mask);
    // As a file: its artifact file is an image's.
    let refusal = Mask::from_bytes(&rendering.to_bytes(), &limits()).unwrap_err();
    assert!(
        matches!(&refusal, ArtifactError::WrongType { expected: ArtifactType::Mask, found }
            if found == "an image"),
        "{refusal:?}"
    );
    // As a picture: the PNG itself, which is what the old tools would have read as a mask.
    let refusal = Mask::from_bytes(rendering.file(), &limits()).unwrap_err();
    assert!(
        matches!(&refusal, ArtifactError::WrongType { expected: ArtifactType::Mask, found }
            if found.contains("PNG")),
        "{refusal:?}"
    );
    // A picture is no artifact file of any type.
    assert!(matches!(
        Artifact::from_bytes(rendering.file(), &limits()),
        Err(ArtifactError::Format(why)) if why.contains("PNG")
    ));
    assert!(matches!(
        Image::from_bytes(rendering.file(), &limits()),
        Err(ArtifactError::WrongType { .. })
    ));
}

/// The file of the plain mask, and where its first plane's runs start.
fn plain_mask_file() -> (Vec<u8>, usize) {
    let mask = rect_mask(on("photo"));
    let bytes = mask.to_bytes();
    // The first plane: `p`, 48 samples, then the count of its runs.
    let mut mark = vec![b'p'];
    mark.extend_from_slice(&48u64.to_be_bytes());
    let at = bytes
        .windows(mark.len())
        .position(|window| window == mark)
        .unwrap();
    (bytes, at + mark.len())
}

#[test]
fn a_plane_is_written_as_runs_as_long_as_they_can_be() {
    let (bytes, at) = plain_mask_file();
    // The core is the box from (2, 1) to (5, 4) of an 8 by 6 image: 10 samples of 0, then
    // three rows of 3 samples of 255 and 5 of 0, the last run reaching the end.
    let runs = u64::from_be_bytes(bytes[at..at + 8].try_into().unwrap());
    assert_eq!(runs, 7);
    let mut found = Vec::new();
    for run in bytes[at + 8..at + 8 + 35].chunks(5) {
        found.push((run[0], u32::from_be_bytes(run[1..].try_into().unwrap())));
    }
    assert_eq!(
        found,
        [
            (0, 10),
            (255, 3),
            (0, 5),
            (255, 3),
            (0, 5),
            (255, 3),
            (0, 19)
        ]
    );
    // The blend and protect planes are empty: one run each.
    let empty = [
        &b"p"[..],
        &48u64.to_be_bytes(),
        &1u64.to_be_bytes(),
        &[0],
        &48u32.to_be_bytes(),
    ]
    .concat();
    let rest = &bytes[at + 8 + 35..];
    assert!(rest.starts_with(&[&empty[..], &empty[..]].concat()));
}

#[test]
fn a_file_that_is_not_the_one_way_to_write_its_artifact_is_refused() {
    let (bytes, at) = plain_mask_file();
    let not_canonical = |bytes: &[u8], what: &str| {
        let result = Mask::from_bytes(bytes, &limits());
        assert!(
            matches!(&result, Err(ArtifactError::Format(_))),
            "{what}: {result:?}"
        );
    };
    // The first run of 10 zeros, split into two runs of 4 and 6: the same plane.
    let mut split = bytes.clone();
    split[at..at + 8].copy_from_slice(&8u64.to_be_bytes());
    split[at + 9..at + 13].copy_from_slice(&4u32.to_be_bytes());
    let mut second = vec![0];
    second.extend_from_slice(&6u32.to_be_bytes());
    split.splice(at + 13..at + 13, second);
    not_canonical(&split, "a split run");
    // Runs that add up to more, or to less, than the plane.
    let mut long = bytes.clone();
    long[at + 9..at + 13].copy_from_slice(&11u32.to_be_bytes());
    not_canonical(&long, "a run too long");
    let mut short = bytes.clone();
    short[at + 9..at + 13].copy_from_slice(&9u32.to_be_bytes());
    not_canonical(&short, "a run too short");
    let mut empty = bytes.clone();
    empty[at + 9..at + 13].copy_from_slice(&0u32.to_be_bytes());
    not_canonical(&empty, "an empty run");
    // Bytes after the artifact.
    let mut more = bytes.clone();
    more.push(0);
    not_canonical(&more, "a byte more");
    // A kind this build does not read.
    let newer = [&b"whizbang:mask/2\0"[..], &bytes[16..]].concat();
    not_canonical(&newer, "another version");
    let unknown = [&b"whizbang:shape/1\0"[..], &bytes[16..]].concat();
    not_canonical(&unknown, "another kind");
    not_canonical(b"", "nothing");
    not_canonical(b"whizbang:", "a start and nothing more");
    not_canonical(b"{\"mask\": 1}", "something else");
    // The file as it is, is read.
    Mask::from_bytes(&bytes, &limits()).unwrap();
}

#[test]
fn a_number_has_one_way_to_be_written() {
    let location = Location::new(
        on("photo"),
        Shape::Box {
            left: 0.0,
            top: 0.0,
            right: 4.0,
            bottom: 3.0,
        },
        made_by("find"),
    )
    .unwrap();
    let bytes = location.to_bytes();
    // A zero written with its sign set is the same number, and not the same file. In
    // memory the two are one location with one identity.
    let negative = Location::new(
        on("photo"),
        Shape::Box {
            left: -0.0,
            top: 0.0,
            right: 4.0,
            bottom: 3.0,
        },
        made_by("find"),
    )
    .unwrap();
    assert_eq!(negative.id(), location.id());
    assert_eq!(negative.to_bytes(), bytes);
    let mut zero = vec![b'f'];
    zero.extend_from_slice(&0f64.to_bits().to_be_bytes());
    let at = bytes
        .windows(zero.len())
        .position(|window| window == zero)
        .unwrap();
    let mut signed = bytes.clone();
    signed[at + 1] = 0x80;
    assert!(matches!(
        Location::from_bytes(&signed, &limits()),
        Err(ArtifactError::Format(_))
    ));
    // A number that is not finite is refused as it is read.
    let mut infinite = bytes.clone();
    infinite[at + 1..at + 9].copy_from_slice(&f64::INFINITY.to_bits().to_be_bytes());
    assert!(Location::from_bytes(&infinite, &limits()).is_err());
}

#[test]
fn a_file_cut_short_or_damaged_anywhere_is_refused_and_never_read_as_something_else() {
    for artifact in one_of_each() {
        let bytes = artifact.to_bytes();
        let kind = artifact.artifact_type();
        // Cut at every length.
        for length in 0..bytes.len() {
            let result = Artifact::from_bytes(&bytes[..length], &limits());
            assert!(result.is_err(), "{kind} cut to {length} bytes was read");
        }
        // One byte changed, at every place a small file has and at a spread of places in
        // a large one. What is read, if anything is, is not the artifact that was written:
        // some part of it differs, in its content, in its origin, or in the origin of
        // something it holds.
        let step = (bytes.len() / 600).max(1);
        for at in (0..bytes.len()).step_by(step) {
            let mut damaged = bytes.clone();
            damaged[at] ^= 0x01;
            match Artifact::from_bytes(&damaged, &limits()) {
                Err(_) => {}
                Ok(read) => {
                    assert_ne!(read, artifact, "{kind}: a change at byte {at} was not seen");
                    assert_eq!(read.to_bytes(), damaged, "{kind}: byte {at}");
                }
            }
        }
    }
}

#[test]
fn an_artifact_file_without_an_origin_is_refused() {
    let location = Location::new(on("photo"), Shape::WholeImage, made_by("find")).unwrap();
    let bytes = location.to_bytes();
    // The origin: `o`, version 1, one step.
    let mut mark = vec![b'o', b'u'];
    mark.extend_from_slice(&1u64.to_be_bytes());
    mark.push(b'u');
    mark.extend_from_slice(&1u64.to_be_bytes());
    let at = bytes
        .windows(mark.len())
        .position(|window| window == mark)
        .unwrap();
    // No origin at all, and an origin of no steps.
    assert!(Location::from_bytes(&bytes[..at], &limits()).is_err());
    let mut none = bytes[..at + mark.len()].to_vec();
    let last = none.len() - 1;
    none[last] = 0;
    let refusal = Location::from_bytes(&none, &limits()).unwrap_err();
    assert!(
        matches!(&refusal, ArtifactError::Origin(why) if why.contains("no step")),
        "{refusal}"
    );
    // An origin of a version this build does not read.
    let mut newer = bytes.clone();
    newer[at + 9] = 2;
    let refusal = Location::from_bytes(&newer, &limits()).unwrap_err();
    assert!(refusal.to_string().contains("version 2"), "{refusal}");
}

#[test]
fn an_image_file_whose_record_does_not_describe_its_picture_is_refused() {
    let image = image_of(photo(), ImageRole::Picture);
    let bytes = image.to_bytes();
    // The width in the record, after the picture and its pixel digest: 8, said to be 9.
    let mut mark = vec![b'u'];
    mark.extend_from_slice(&u64::from(WIDTH).to_be_bytes());
    mark.push(b'u');
    mark.extend_from_slice(&u64::from(HEIGHT).to_be_bytes());
    let at = bytes.len()
        - bytes
            .windows(mark.len())
            .rev()
            .position(|window| window == mark)
            .unwrap()
        - mark.len();
    let mut wider = bytes.clone();
    wider[at + 8] = 9;
    assert!(matches!(
        Image::from_bytes(&wider, &limits()),
        Err(ArtifactError::Format(why)) if why.contains("does not describe")
    ));
    // The pixel digest in the record, one bit off.
    let mut other = bytes.clone();
    other[at - 1] ^= 1;
    assert!(matches!(
        Image::from_bytes(&other, &limits()),
        Err(ArtifactError::Format(why)) if why.contains("does not describe")
    ));
    Image::from_bytes(&bytes, &limits()).unwrap();
}

#[test]
fn reading_holds_to_the_decode_limits() {
    let tight = DecodeLimits {
        max_pixels: 47,
        ..DecodeLimits::default()
    };
    let mask = rect_mask(on("photo"));
    assert!(matches!(
        Mask::from_bytes(&mask.to_bytes(), &tight),
        Err(ArtifactError::Identity(IdentityError::Limit(_)))
    ));
    let image = image_of(photo(), ImageRole::Picture);
    assert!(matches!(
        Image::from_bytes(&image.to_bytes(), &tight),
        Err(ArtifactError::Identity(IdentityError::Limit(_)))
    ));
    // A mask whose binding says it is vast, with no planes behind it, is refused by the
    // limit before anything is set aside for it.
    let vast = Binding::root(picture("vast"), Space::flat(60_000, 60_000).unwrap());
    let location = Location::new(vast, Shape::WholeImage, made_by("find")).unwrap();
    let as_mask = [
        &b"whizbang:mask/1\0"[..],
        &location.to_bytes()[b"whizbang:location/1\0".len()..],
    ]
    .concat();
    let result = Mask::from_bytes(&as_mask, &limits());
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_mask_file_whose_planes_break_an_invariant_is_refused() {
    let (bytes, at) = plain_mask_file();
    let refused = |bytes: &[u8], invariant: &str| {
        let result = Mask::from_bytes(bytes, &limits());
        assert!(
            matches!(
                &result,
                Err(ArtifactError::Invalid { artifact: ArtifactType::Mask, why })
                    if why.contains(invariant)
            ),
            "{invariant}: {result:?}"
        );
    };
    // The runs of the core start 8 bytes after the count of them; the second is the first
    // row of the box, and its value is the byte that starts it.
    let core_run = at + 8 + 5;
    assert_eq!(bytes[core_run], 255);
    let mut grey = bytes.clone();
    grey[core_run] = 7;
    refused(&grey, "the core is binary");
    // The blend plane follows the core's seven runs: `p`, two counts, then its one run of
    // 48 zeros. A blend of 9 everywhere is not zero inside the core.
    let blend_run = at + 8 + 35 + 17;
    assert_eq!(bytes[blend_run..blend_run + 5], [0, 0, 0, 0, 48]);
    let mut soft = bytes.clone();
    soft[blend_run] = 9;
    refused(&soft, "the blend is zero inside the core");
    // Protect follows the same way. A protect of 3 everywhere is not binary.
    let protect_run = blend_run + 5 + 17;
    assert_eq!(bytes[protect_run..protect_run + 5], [0, 0, 0, 0, 48]);
    let mut odd = bytes.clone();
    odd[protect_run] = 3;
    refused(&odd, "protect is binary");
    // A protect that covers everything, over the core's box.
    let mut over = bytes.clone();
    over[protect_run] = 255;
    refused(&over, "the core is zero under protect");
    // A blend at the first pixel, which is outside the core, under a protect that covers
    // everything.
    let mut under = bytes.clone();
    under[protect_run] = 255;
    under.splice(blend_run..blend_run + 5, [9, 0, 0, 0, 1, 0, 0, 0, 0, 47]);
    under[blend_run - 8..blend_run].copy_from_slice(&2u64.to_be_bytes());
    refused(&under, "the blend is zero under protect");
}

/// Where `mark` starts in `bytes`.
fn find(bytes: &[u8], mark: &[u8]) -> usize {
    bytes
        .windows(mark.len())
        .position(|window| window == mark)
        .unwrap()
}

fn whole(value: u64) -> Vec<u8> {
    [&b"u"[..], &value.to_be_bytes()].concat()
}

#[test]
fn a_run_past_the_end_of_its_plane_is_refused_before_it_is_set_aside() {
    let (bytes, at) = plain_mask_file();
    // The first run of the core, 10 samples long, said to be 4,294,967,295: the plane is
    // 48 samples, and nothing is set aside for a run that passes its end.
    let mut vast = bytes.clone();
    vast[at + 9..at + 13].copy_from_slice(&u32::MAX.to_be_bytes());
    let refusal = Mask::from_bytes(&vast, &limits()).unwrap_err();
    assert!(
        matches!(&refusal, ArtifactError::Format(why) if why.contains("past the end")),
        "{refusal}"
    );
    // A run that stops short leaves runs that do not add up, which is another refusal.
    let mut short = bytes.clone();
    short[at + 9..at + 13].copy_from_slice(&9u32.to_be_bytes());
    let refusal = Mask::from_bytes(&short, &limits()).unwrap_err();
    assert!(
        matches!(&refusal, ArtifactError::Format(why) if why.contains("add up")),
        "{refusal}"
    );
}

#[test]
fn a_tone_ramp_of_a_version_this_build_does_not_read_is_refused() {
    let mask = full_mask(on("photo"));
    let bytes = mask.to_bytes();
    // The one prescription: kind 0, version 1, then the step, a series inside the mask.
    let mark = [&whole(0)[..], &whole(1), b"awhizbang:series/1\0"].concat();
    let version = find(&bytes, &mark) + 9 + 8;
    assert_eq!(bytes[version], 1);
    let mut newer = bytes.clone();
    newer[version] = 2;
    let refusal = Mask::from_bytes(&newer, &limits()).unwrap_err();
    assert!(
        matches!(&refusal, ArtifactError::Invalid { artifact: ArtifactType::Mask, why }
            if why.contains("version 2")),
        "{refusal}"
    );
    Mask::from_bytes(&bytes, &limits()).unwrap();
}

#[test]
fn a_binding_inside_a_file_that_breaks_its_rules_is_refused() {
    // A location on a composite placed onto a photo by the identity, both 8 by 6.
    let binding = Binding::new(
        picture("composite"),
        space(),
        picture("photo"),
        space(),
        Direction::SourceToDestination,
        Mapping::Identity,
    )
    .unwrap();
    let location = Location::new(binding, Shape::WholeImage, made_by("find")).unwrap();
    let bytes = location.to_bytes();
    // The binding starts after the file's kind, `a` and its own kind. Its fields: the
    // source (`d`, 33 bytes), the source space (four `u`, 36 bytes), the destination (`u`
    // and `d`, 42 bytes), the destination space, the direction, the mapping's kind.
    let binding_at = b"whizbang:location/1\0".len() + 1 + b"whizbang:binding/1\0".len();
    let destination_space = binding_at + 33 + 36 + 42;
    assert_eq!(
        bytes[destination_space..destination_space + 9],
        whole(8)[..]
    );
    let direction = destination_space + 36;
    assert_eq!(bytes[direction..direction + 9], whole(0)[..]);
    let refused = |bytes: &[u8], what: &str| {
        let result = Location::from_bytes(bytes, &limits());
        assert!(
            matches!(
                &result,
                Err(ArtifactError::Identity(IdentityError::Binding(_)))
            ),
            "{what}: {result:?}"
        );
    };
    // The identity between two spaces that are not equal: the destination 9 wide.
    let mut wider = bytes.clone();
    wider[destination_space + 8] = 9;
    refused(&wider, "an identity between unequal spaces");
    // A direction that is neither of the two.
    let mut neither = bytes.clone();
    neither[direction + 8] = 2;
    refused(&neither, "a third direction");
    // A mapping of a kind ADR 0001 does not list.
    let mut unknown = bytes.clone();
    unknown[direction + 9 + 8] = 9;
    refused(&unknown, "an unknown mapping");
    // A wrap between two flat images: a mapping that does not fit its spaces. The strip
    // follows the kind, so the file grows by one field.
    let mut wrap = bytes.clone();
    wrap[direction + 9 + 8] = 4;
    wrap.splice(direction + 18..direction + 18, whole(2));
    refused(&wrap, "a wrap between flat images");
    Location::from_bytes(&bytes, &limits()).unwrap();
}

#[test]
fn a_refusal_inside_a_binding_says_what_is_wrong_once() {
    let location = Location::new(on("photo"), Shape::WholeImage, made_by("find")).unwrap();
    let bytes = location.to_bytes();
    // Cut inside the binding: the file is short, and that is all the refusal says.
    let binding_at = b"whizbang:location/1\0".len() + 1 + b"whizbang:binding/1\0".len();
    let refusal = Location::from_bytes(&bytes[..binding_at + 40], &limits()).unwrap_err();
    assert!(matches!(refusal, ArtifactError::Format(_)), "{refusal:?}");
    let said = refusal.to_string();
    assert!(
        said.starts_with("not a canonical artifact file: the file ends at byte"),
        "{said}"
    );
    // A field of the wrong kind inside the binding is the file's fault too.
    let mut wrong = bytes.clone();
    wrong[binding_at] = b'u';
    let refusal = Location::from_bytes(&wrong, &limits()).unwrap_err();
    assert!(matches!(refusal, ArtifactError::Format(_)), "{refusal:?}");
    assert!(!refusal.to_string().contains("binding:"), "{refusal}");
    // The same holds for the space an image's record states: its width, which follows
    // the picture and its pixel digest, written as a number of another kind.
    let image = image_of(photo(), ImageRole::Picture).to_bytes();
    let mark = [&whole(u64::from(WIDTH))[..], &whole(u64::from(HEIGHT))].concat();
    let width = image.len()
        - image
            .windows(mark.len())
            .rev()
            .position(|window| window == mark)
            .unwrap()
        - mark.len();
    let mut wrong = image.clone();
    wrong[width] = b'f';
    let refusal = Image::from_bytes(&wrong, &limits()).unwrap_err();
    assert!(matches!(refusal, ArtifactError::Format(_)), "{refusal:?}");
    assert!(!refusal.to_string().contains("binding:"), "{refusal}");
}
