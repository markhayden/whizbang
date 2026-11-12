//! What is read from the container apart from the decoder (ADR 0001): a profile or a tag
//! the decoder would pass over in silence, a JPEG that ends early, and a colour model with
//! no canonical reading.

use whizbang_core::identity::{DecodeLimits, IdentityError, Profile, identify};

use super::support::{
    Curve, Extras, Grid, RawPng, WIDE_COLORANTS, chunk, exif_orientation, flat_jpeg, iccp,
    matrix_profile, picture_grid, picture_rgba, profile_piece, rgb_jpeg, rgb_png, with_segment,
    zlib_stored,
};
use super::vectors::vector;

fn limits() -> DecodeLimits {
    DecodeLimits::default()
}

fn wide() -> Vec<u8> {
    matrix_profile(&WIDE_COLORANTS, &Curve::SrgbParametric)
}

fn unsupported(bytes: &[u8], what: &str) {
    let result = identify(bytes, &limits());
    assert!(
        matches!(result, Err(IdentityError::Unsupported(_))),
        "{what}: {result:?}"
    );
}

// --- a PNG's colour profile ---------------------------------------------------------------

#[test]
fn a_png_profile_that_cannot_be_read_is_refused_and_not_passed_over() {
    let picture = picture_grid();
    let good = iccp(&wide());
    // As it should be: read, and applied.
    let read = identify(
        &RawPng::rgb(&picture).before(b"iCCP", good.clone()).bytes(),
        &limits(),
    )
    .unwrap();
    assert_eq!(read.stored.profile, Profile::Converted);

    // A stream that is not zlib; one cut short; one whose check value is wrong.
    let mut not_zlib = b"icc\0\0".to_vec();
    not_zlib.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef, 0x00, 0x11]);
    let cut = good[..good.len() - 9].to_vec();
    let mut wrong_check = good.clone();
    *wrong_check.last_mut().unwrap() ^= 1;
    // An unknown compression method, no name, a name that never ends, and nothing at all.
    let mut method = good.clone();
    method[4] = 1;
    let no_name = good[3..].to_vec();
    let endless = vec![b'a'; 100];
    for (data, what) in [
        (not_zlib, "not zlib"),
        (cut, "cut short"),
        (wrong_check, "wrong check value"),
        (method, "unknown method"),
        (no_name, "no name"),
        (endless, "a name that never ends"),
        (Vec::new(), "empty chunk"),
    ] {
        unsupported(&RawPng::rgb(&picture).before(b"iCCP", data).bytes(), what);
    }

    // The chunk's own checksum fails: the decoder skips such a chunk and decodes the rest.
    let whole = RawPng::rgb(&picture).before(b"iCCP", good.clone()).bytes();
    let at = whole.windows(4).position(|w| w == b"iCCP").unwrap();
    let mut damaged = whole.clone();
    damaged[at + 10] ^= 0x40;
    unsupported(&damaged, "a chunk whose checksum fails");
}

#[test]
fn a_png_profile_is_read_wherever_its_chunk_sits_and_only_the_first_counts() {
    let picture = picture_grid();
    let before = identify(
        &RawPng::rgb(&picture).before(b"iCCP", iccp(&wide())).bytes(),
        &limits(),
    )
    .unwrap();
    let after = identify(
        &RawPng::rgb(&picture).after(b"iCCP", iccp(&wide())).bytes(),
        &limits(),
    )
    .unwrap();
    assert_eq!(after.stored.profile, Profile::Converted);
    assert_eq!(after.identity.pixels, before.identity.pixels);
    // A second profile is not read: not even to be refused.
    let two = RawPng::rgb(&picture)
        .before(b"iCCP", iccp(&wide()))
        .before(b"iCCP", b"icc\0\0junk".to_vec())
        .bytes();
    assert_eq!(
        identify(&two, &limits()).unwrap().identity.pixels,
        before.identity.pixels
    );
    // A profile of no bytes is no profile.
    let empty = RawPng::rgb(&picture).before(b"iCCP", iccp(&[])).bytes();
    let identified = identify(&empty, &limits()).unwrap();
    assert_eq!(identified.stored.profile, Profile::None);
    assert_eq!(identified.pixels.rgba(), picture_rgba());
    // The chunks a PNG may carry beside a profile change nothing: gamma, chromaticities,
    // the sRGB flag and the coding-independent code points are not read.
    let plain = identify(&RawPng::rgb(&picture).bytes(), &limits()).unwrap();
    let dressed = RawPng::rgb(&picture)
        .before(b"gAMA", 100_000u32.to_be_bytes().to_vec())
        .before(b"cHRM", [0, 0, 0x7a, 0x26].repeat(8))
        .before(b"sRGB", vec![0])
        .before(b"cICP", vec![9, 16, 0, 1])
        .bytes();
    let identified = identify(&dressed, &limits()).unwrap();
    assert_eq!(identified.stored.profile, Profile::None);
    assert_eq!(identified.identity.pixels, plain.identity.pixels);
}

#[test]
fn a_profile_over_the_limit_is_refused_before_it_is_held() {
    let picture = picture_grid();
    // Sixteen mebibytes of zeros in a chunk of a few kilobytes: the encoder compresses it.
    let bomb = rgb_png(
        &picture,
        Extras {
            profile: Some(vec![0; 16 * 1024 * 1024]),
            ..Extras::default()
        },
    );
    assert!(bomb.len() < 100_000, "{}", bomb.len());
    assert!(matches!(
        identify(&bomb, &limits()),
        Err(IdentityError::Limit(_))
    ));
    assert_eq!(limits().max_profile_bytes, 1024 * 1024);

    // The limit is on the inflated bytes, to the byte.
    let profile = wide();
    let file = RawPng::rgb(&picture)
        .before(b"iCCP", iccp(&profile))
        .bytes();
    let at = |max_profile_bytes: u64| {
        identify(
            &file,
            &DecodeLimits {
                max_profile_bytes,
                ..DecodeLimits::default()
            },
        )
    };
    at(profile.len() as u64).unwrap();
    assert!(matches!(
        at(profile.len() as u64 - 1),
        Err(IdentityError::Limit(_))
    ));
    // A JPEG's profile is held to the same limit.
    let jpeg = with_segment(
        &rgb_jpeg(&Grid::new(8, 8, vec![[90, 120, 30]; 64]), None),
        0xe2,
        &profile_piece(1, 1, &profile),
    );
    identify(&jpeg, &limits()).unwrap();
    let small = DecodeLimits {
        max_profile_bytes: profile.len() as u64 - 1,
        ..DecodeLimits::default()
    };
    assert!(matches!(
        identify(&jpeg, &small),
        Err(IdentityError::Limit(_))
    ));
}

// --- a PNG's orientation tag --------------------------------------------------------------

#[test]
fn a_png_orientation_tag_counts_before_the_image_data_and_after_it() {
    let picture = picture_grid();
    for orientation in 1..=8u8 {
        let stored = picture.stored_under(orientation);
        let exif = exif_orientation(u16::from(orientation));
        for file in [
            RawPng::rgb(&stored).before(b"eXIf", exif.clone()).bytes(),
            RawPng::rgb(&stored).after(b"eXIf", exif.clone()).bytes(),
        ] {
            let identified = identify(&file, &limits()).unwrap();
            assert_eq!(identified.pixels.rgba(), picture_rgba(), "{orientation}");
            assert_eq!(identified.stored.orientation, orientation);
        }
    }
    // The first tag counts; a tag whose chunk fails its checksum is no tag.
    let two = RawPng::rgb(&picture.stored_under(6))
        .before(b"eXIf", exif_orientation(6))
        .after(b"eXIf", exif_orientation(3))
        .bytes();
    assert_eq!(
        identify(&two, &limits()).unwrap().pixels.rgba(),
        picture_rgba()
    );
    let whole = RawPng::rgb(&picture)
        .after(b"eXIf", exif_orientation(6))
        .bytes();
    let at = whole.windows(4).position(|w| w == b"eXIf").unwrap();
    let mut damaged = whole.clone();
    // The checksum itself, so that the tag still says 6.
    damaged[at + 4 + 26] ^= 1;
    let identified = identify(&damaged, &limits()).unwrap();
    assert_eq!(identified.stored.orientation, 1);
    assert_eq!(identified.pixels.rgba(), picture_rgba());
}

// --- a JPEG's end, components and profile -------------------------------------------------

#[test]
fn a_jpeg_cut_short_anywhere_is_refused() {
    for name in [
        "jpeg-420",
        "jpeg-444",
        "jpeg-progressive",
        "jpeg-grey",
        "jpeg-orientation-6",
    ] {
        let whole = vector(name);
        identify(&whole, &limits()).unwrap();
        for length in 0..whole.len() {
            let result = identify(&whole[..length], &limits());
            assert!(
                matches!(result, Err(IdentityError::Image(_))),
                "{name} cut at {length} of {}: {result:?}",
                whole.len()
            );
        }
        // What follows the end marker is not part of the picture.
        let mut padded = whole.clone();
        padded.extend_from_slice(&[0, 0, 0xff, 0xd9, 7]);
        assert_eq!(
            identify(&padded, &limits()).unwrap().identity.pixels,
            identify(&whole, &limits()).unwrap().identity.pixels
        );
    }
}

#[test]
fn a_jpeg_of_four_components_is_refused() {
    // One and three components are grey and colour, and are read.
    for components in [1, 3] {
        let identified = identify(&flat_jpeg(components, 8), &limits()).unwrap();
        assert_eq!(
            (identified.pixels.width(), identified.pixels.height()),
            (8, 8)
        );
    }
    // Four are CMYK, with or without the marker that says how they are stored.
    unsupported(&flat_jpeg(4, 8), "four components");
    let mut adobe = b"Adobe".to_vec();
    adobe.extend_from_slice(&[0, 100, 0, 0, 0, 0, 2]);
    unsupported(
        &with_segment(&flat_jpeg(4, 8), 0xee, &adobe),
        "four components, marked",
    );
    unsupported(&flat_jpeg(2, 8), "two components");
    unsupported(&flat_jpeg(3, 12), "twelve bits");
}

#[test]
fn a_jpeg_profile_is_whole_or_refused() {
    let jpeg = vector("jpeg-444");
    let profile = wide();
    let (first, second) = profile.split_at(200);
    let add = |pieces: &[Vec<u8>]| {
        let mut out = jpeg.clone();
        // Each is put straight after the start marker, so the last added comes first.
        for piece in pieces.iter().rev() {
            out = with_segment(&out, 0xe2, piece);
        }
        out
    };
    let whole = identify(&vector("jpeg-profile-wide"), &limits()).unwrap();
    assert_eq!(whole.stored.profile, Profile::Converted);
    // In two pieces, in either order.
    for pieces in [
        [profile_piece(1, 2, first), profile_piece(2, 2, second)],
        [profile_piece(2, 2, second), profile_piece(1, 2, first)],
    ] {
        let identified = identify(&add(&pieces), &limits()).unwrap();
        assert_eq!(identified.identity.pixels, whole.identity.pixels);
    }
    // A piece missing, a piece twice, pieces that disagree on how many there are, and a
    // piece numbered zero.
    for (pieces, what) in [
        (vec![profile_piece(1, 2, first)], "the second piece missing"),
        (vec![profile_piece(2, 2, second)], "the first piece missing"),
        (
            vec![
                profile_piece(1, 2, first),
                profile_piece(1, 2, first),
                profile_piece(2, 2, second),
            ],
            "a piece twice",
        ),
        (
            vec![profile_piece(1, 2, first), profile_piece(2, 3, second)],
            "two counts",
        ),
        (vec![profile_piece(0, 1, &profile)], "piece zero"),
        (vec![profile_piece(1, 0, &profile)], "a count of zero"),
    ] {
        unsupported(&add(&pieces), what);
    }
    // A segment of another kind under the same marker is not a profile.
    let other = identify(&add(&[b"MPF\0whatever".to_vec()]), &limits()).unwrap();
    assert_eq!(other.stored.profile, Profile::None);
}

// --- limits -------------------------------------------------------------------------------

#[test]
fn an_image_the_decoder_will_not_hold_is_over_a_limit_and_not_a_broken_image() {
    // A header that claims 60,000 by 60,000 pixels, with the pixel limit out of the way:
    // the decoder's own allocation limit refuses it.
    let mut vast = rgb_png(&picture_grid(), Extras::default());
    vast[16..20].copy_from_slice(&60_000u32.to_be_bytes());
    vast[20..24].copy_from_slice(&60_000u32.to_be_bytes());
    let header = chunk(b"IHDR", &vast[16..29]);
    vast[8..33].copy_from_slice(&header);
    let open = DecodeLimits {
        max_pixels: u64::MAX,
        ..DecodeLimits::default()
    };
    let result = identify(&vast, &open);
    assert!(matches!(result, Err(IdentityError::Limit(_))), "{result:?}");
}

#[test]
fn the_support_writes_what_it_says() {
    // The stored zlib stream of nothing, and of more than one block.
    assert_eq!(
        zlib_stored(&[]),
        [0x78, 0x01, 1, 0, 0, 0xff, 0xff, 0, 0, 0, 1]
    );
    let long = vec![7u8; 70_000];
    let file = RawPng::rgb(&picture_grid())
        .before(b"iCCP", iccp(&long))
        .bytes();
    // Seventy thousand sevens are no profile, but they inflate: the refusal is the
    // profile's, not the stream's.
    let result = identify(&file, &limits());
    let Err(IdentityError::Unsupported(why)) = result else {
        panic!("{result:?}");
    };
    assert!(why.contains("cannot be applied"), "{why}");
}
