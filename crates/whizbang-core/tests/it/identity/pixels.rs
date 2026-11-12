//! Image identity: the file digest and the canonical pixel digest, and exactly what
//! canonical means (ADR 0001).

use image::ExtendedColorType;
use whizbang_core::identity::{
    CanonicalPixels, DecodeLimits, Digest, IdentityError, Profile, Space, identify,
};

use super::support::{
    Curve, Extras, Grid, HEIGHT, SRGB_COLORANTS, WIDE_COLORANTS, WIDTH, exif_orientation,
    matrix_profile, matrix_tags, picture_grid, picture_rgba, png, profile_of, rgb_bmp, rgb_jpeg,
    rgb_png, rgb16_png, rgba_png,
};

fn limits() -> DecodeLimits {
    DecodeLimits::default()
}

fn with_profile(profile: Vec<u8>) -> Vec<u8> {
    rgb_png(
        &picture_grid(),
        Extras {
            profile: Some(profile),
            ..Extras::default()
        },
    )
}

// --- the two digests ----------------------------------------------------------------------

#[test]
fn two_different_images_of_the_same_size_have_different_identities_and_equal_spaces() {
    let mut other = picture_grid();
    other.pixels[5][1] ^= 1;
    let first = identify(&rgb_png(&picture_grid(), Extras::default()), &limits()).unwrap();
    let second = identify(&rgb_png(&other, Extras::default()), &limits()).unwrap();
    assert_ne!(first.identity, second.identity);
    assert_ne!(first.identity.file, second.identity.file);
    assert_ne!(first.identity.pixels, second.identity.pixels);
    let space = |pixels: &CanonicalPixels| Space::flat(pixels.width(), pixels.height()).unwrap();
    assert_eq!(space(&first.pixels), space(&second.pixels));
}

#[test]
fn the_same_pixels_in_two_containers_have_different_file_digests_and_equal_canonical_digests() {
    let png = rgb_png(&picture_grid(), Extras::default());
    let bmp = rgb_bmp(&picture_grid());
    assert_ne!(png, bmp);
    let from_png = identify(&png, &limits()).unwrap();
    let from_bmp = identify(&bmp, &limits()).unwrap();
    assert_ne!(from_png.identity.file, from_bmp.identity.file);
    assert_eq!(from_png.identity.pixels, from_bmp.identity.pixels);
    assert_ne!(from_png.identity, from_bmp.identity);
    assert_eq!(from_png.pixels, from_bmp.pixels);
    // The file digest is the plain SHA-256 of the bytes.
    assert_eq!(from_png.identity.file.0, Digest::of_bytes(&png));
    assert_eq!(from_bmp.identity.file.0, Digest::of_bytes(&bmp));
}

#[test]
fn canonical_pixels_are_upright_rgba_in_row_order() {
    let identified = identify(&rgb_png(&picture_grid(), Extras::default()), &limits()).unwrap();
    assert_eq!(
        (identified.pixels.width(), identified.pixels.height()),
        (WIDTH, HEIGHT)
    );
    assert_eq!(identified.pixels.rgba(), picture_rgba());
    assert_eq!(identified.identity.pixels, identified.pixels.digest());
    assert_eq!(
        (identified.stored.width, identified.stored.height),
        (WIDTH, HEIGHT)
    );
    assert_eq!(identified.stored.orientation, 1);
    assert!(!identified.stored.alpha);
    assert_eq!(identified.stored.profile, Profile::None);
}

#[test]
fn the_pixel_digest_is_fixed() {
    // Computed apart from this code, from the encoding ADR 0001 states: SHA-256 of
    // "whizbang:pixels/1", a zero byte, "u" and the width, "u" and the height (each eight
    // bytes, big-endian), "b", the length the same way, and the RGBA bytes.
    let pixels = CanonicalPixels::from_rgba(WIDTH, HEIGHT, picture_rgba()).unwrap();
    assert_eq!(
        pixels.digest().to_string(),
        "sha256:f009f1dc69fa814c300a824ebeaa5ef234deb9176f507ce6e9972c2658176e96"
    );
    // The dimensions are part of it: the same bytes as 3 by 4 are another picture.
    let turned = CanonicalPixels::from_rgba(HEIGHT, WIDTH, picture_rgba()).unwrap();
    assert_ne!(turned.digest(), pixels.digest());
}

#[test]
fn canonical_pixels_need_four_bytes_a_pixel_and_some_area() {
    assert!(CanonicalPixels::from_rgba(2, 2, vec![0; 16]).is_ok());
    for (width, height, bytes) in [(2, 2, 15), (2, 2, 12), (0, 2, 0), (2, 0, 0)] {
        assert!(matches!(
            CanonicalPixels::from_rgba(width, height, vec![0; bytes]),
            Err(IdentityError::Image(_))
        ));
    }
}

// --- orientation --------------------------------------------------------------------------

#[test]
fn every_orientation_tag_gives_the_upright_picture() {
    let upright = identify(&rgb_png(&picture_grid(), Extras::default()), &limits()).unwrap();
    let mut files = vec![];
    for orientation in 1..=8u8 {
        let stored = picture_grid().stored_under(orientation);
        if orientation > 1 {
            assert_ne!(stored, picture_grid(), "orientation {orientation}");
        }
        let file = rgb_png(
            &stored,
            Extras {
                exif: Some(exif_orientation(u16::from(orientation))),
                ..Extras::default()
            },
        );
        let identified = identify(&file, &limits()).unwrap();
        assert_eq!(
            identified.pixels.rgba(),
            picture_rgba(),
            "orientation {orientation}"
        );
        assert_eq!(identified.identity.pixels, upright.identity.pixels);
        assert_ne!(identified.identity.file, upright.identity.file);
        assert_eq!(identified.stored.orientation, orientation);
        assert_eq!(
            (identified.stored.width, identified.stored.height),
            (stored.width, stored.height)
        );
        assert_eq!(
            (identified.pixels.width(), identified.pixels.height()),
            (WIDTH, HEIGHT)
        );
        files.push(file);
    }
    files.sort();
    files.dedup();
    assert_eq!(files.len(), 8);
}

#[test]
fn the_same_stored_grid_under_another_tag_is_another_picture() {
    let tagged = |orientation| {
        let file = rgb_png(
            &picture_grid(),
            Extras {
                exif: Some(exif_orientation(orientation)),
                ..Extras::default()
            },
        );
        identify(&file, &limits()).unwrap()
    };
    let upright = tagged(1);
    for orientation in 2..=8 {
        assert_ne!(
            tagged(orientation).identity.pixels,
            upright.identity.pixels,
            "orientation {orientation}"
        );
    }
    // A tag outside 1 to 8 means nothing, and the grid is read as it is stored.
    for orientation in [0, 9] {
        let identified = tagged(orientation);
        assert_eq!(identified.identity.pixels, upright.identity.pixels);
        assert_eq!(identified.stored.orientation, 1);
    }
}

#[test]
fn a_jpeg_is_turned_by_its_tag_like_any_other_file() {
    // A lossy file has no fixed pixels across decoders, so this compares one decoder with
    // itself: the same compressed data, without the tag and with it.
    let grid = Grid::new(
        16,
        8,
        (0..128u32)
            .map(|n| [(n * 2) as u8, 255 - n as u8, (n % 16 * 16) as u8])
            .collect(),
    );
    let plain = identify(&rgb_jpeg(&grid, None), &limits()).unwrap();
    let turned = identify(&rgb_jpeg(&grid, Some(exif_orientation(6))), &limits()).unwrap();
    assert_eq!((plain.pixels.width(), plain.pixels.height()), (16, 8));
    assert_eq!((turned.pixels.width(), turned.pixels.height()), (8, 16));
    assert_eq!((turned.stored.width, turned.stored.height), (16, 8));
    assert_eq!(turned.stored.orientation, 6);
    // What the tagged file stores is what the plain one shows; under tag 6 the shown
    // picture is the one whose stored form that is.
    let shown = turned.pixels.rgba().as_chunks::<4>().0.to_vec();
    let stored = plain.pixels.rgba().as_chunks::<4>().0.to_vec();
    assert_eq!(
        Grid::new(8, 16, shown).stored_under(6),
        Grid::new(16, 8, stored)
    );
}

// --- channels, depth and alpha ------------------------------------------------------------

#[test]
fn a_grey_image_is_the_colour_image_with_three_equal_channels() {
    let grey: Vec<u8> = (0..12u8).map(|n| n * 20).collect();
    let colour: Vec<[u8; 3]> = grey.iter().map(|v| [*v; 3]).collect();
    let from_grey = identify(
        &png(
            WIDTH,
            HEIGHT,
            ExtendedColorType::L8,
            &grey,
            Extras::default(),
        ),
        &limits(),
    )
    .unwrap();
    let from_colour = identify(
        &rgb_png(&Grid::new(WIDTH, HEIGHT, colour), Extras::default()),
        &limits(),
    )
    .unwrap();
    assert_eq!(from_grey.identity.pixels, from_colour.identity.pixels);
    assert!(!from_grey.stored.alpha);
    // Grey with alpha keeps its alpha.
    let grey_alpha: Vec<u8> = grey.iter().flat_map(|v| [*v, 200]).collect();
    let from_grey_alpha = identify(
        &png(
            WIDTH,
            HEIGHT,
            ExtendedColorType::La8,
            &grey_alpha,
            Extras::default(),
        ),
        &limits(),
    )
    .unwrap();
    assert!(from_grey_alpha.stored.alpha);
    assert_eq!(
        &from_grey_alpha.pixels.rgba()[..8],
        [0, 0, 0, 200, 20, 20, 20, 200]
    );
}

#[test]
fn an_opaque_alpha_channel_is_no_alpha_channel_and_any_other_is_part_of_the_picture() {
    let plain = identify(&rgb_png(&picture_grid(), Extras::default()), &limits()).unwrap();
    let with_alpha = |alpha: &dyn Fn(usize) -> u8, colour: &dyn Fn(usize, [u8; 3]) -> [u8; 3]| {
        let pixels: Vec<[u8; 4]> = picture_grid()
            .pixels
            .iter()
            .enumerate()
            .map(|(i, rgb)| {
                let [r, g, b] = colour(i, *rgb);
                [r, g, b, alpha(i)]
            })
            .collect();
        identify(
            &rgba_png(&Grid::new(WIDTH, HEIGHT, pixels), Extras::default()),
            &limits(),
        )
        .unwrap()
    };
    let opaque = with_alpha(&|_| 255, &|_, rgb| rgb);
    assert!(opaque.stored.alpha);
    assert_eq!(opaque.identity.pixels, plain.identity.pixels);
    assert_ne!(opaque.identity.file, plain.identity.file);

    // One pixel a little see-through: another picture. Alpha is kept as stored, and the
    // colours are not multiplied by it.
    let faint = with_alpha(&|i| if i == 3 { 254 } else { 255 }, &|_, rgb| rgb);
    assert_ne!(faint.identity.pixels, plain.identity.pixels);
    let mut expected = picture_rgba();
    expected[15] = 254;
    assert_eq!(faint.pixels.rgba(), expected);

    // The colour under a fully transparent pixel is kept as stored: two files that differ
    // only there are two pictures.
    let clear = with_alpha(&|i| if i == 3 { 0 } else { 255 }, &|_, rgb| rgb);
    let clear_black = with_alpha(&|i| if i == 3 { 0 } else { 255 }, &|i, rgb| {
        if i == 3 { [0, 0, 0] } else { rgb }
    });
    assert_ne!(clear.identity.pixels, clear_black.identity.pixels);
}

#[test]
fn sixteen_bit_samples_are_rounded_to_eight() {
    // 257 times an 8-bit value is that value exactly.
    let wide: Vec<[u16; 3]> = picture_grid()
        .pixels
        .iter()
        .map(|rgb| rgb.map(|v| u16::from(v) * 257))
        .collect();
    let from_wide = identify(&rgb16_png(WIDTH, HEIGHT, &wide), &limits()).unwrap();
    let plain = identify(&rgb_png(&picture_grid(), Extras::default()), &limits()).unwrap();
    assert_eq!(from_wide.identity.pixels, plain.identity.pixels);
    // Between two 8-bit values, the nearer one: 128 and 129 of 65535 straddle one half of
    // a step of 257.
    let edge = [[0u16, 128, 129], [65535, 65407, 65406]];
    let identified = identify(&rgb16_png(2, 1, &edge), &limits()).unwrap();
    assert_eq!(identified.pixels.rgba(), [0, 0, 1, 255, 255, 255, 254, 255]);
}

// --- colour profiles ----------------------------------------------------------------------

#[test]
fn an_srgb_profile_is_no_profile() {
    let plain = identify(&rgb_png(&picture_grid(), Extras::default()), &limits()).unwrap();
    for curve in [
        Curve::SrgbParametric,
        Curve::SrgbTable(1024),
        Curve::SrgbTable(4096),
    ] {
        let file = with_profile(matrix_profile(&SRGB_COLORANTS, &curve));
        let identified = identify(&file, &limits()).unwrap();
        assert_eq!(identified.stored.profile, Profile::Srgb, "{curve:?}");
        assert_eq!(
            identified.identity.pixels, plain.identity.pixels,
            "{curve:?}"
        );
        assert_ne!(identified.identity.file, plain.identity.file);
    }
}

#[test]
fn the_same_samples_under_another_profile_are_another_picture() {
    let plain = identify(&rgb_png(&picture_grid(), Extras::default()), &limits()).unwrap();
    // The sRGB primaries under a plain power of 2.2: close to sRGB, and not sRGB.
    let power = identify(
        &with_profile(matrix_profile(&SRGB_COLORANTS, &Curve::Gamma(2.2))),
        &limits(),
    )
    .unwrap();
    assert_eq!(power.stored.profile, Profile::Converted);
    assert_ne!(power.identity.pixels, plain.identity.pixels);
    // Wider primaries under the sRGB curve.
    let wide = identify(
        &with_profile(matrix_profile(&WIDE_COLORANTS, &Curve::SrgbParametric)),
        &limits(),
    )
    .unwrap();
    assert_eq!(wide.stored.profile, Profile::Converted);
    assert_ne!(wide.identity.pixels, plain.identity.pixels);
    assert_ne!(wide.identity.pixels, power.identity.pixels);
}

#[test]
fn a_profile_is_applied_sample_by_sample() {
    // Greys stay grey under wider primaries with the same white and curve; a saturated
    // colour moves outward, and what sRGB cannot hold is clipped. The expected values were
    // computed apart from this code, from the steps ADR 0001 states.
    let samples: Vec<[u8; 3]> = vec![
        [0, 0, 0],
        [255, 255, 255],
        [128, 128, 128],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [200, 100, 50],
        [30, 60, 90],
    ];
    let file = rgb_png(
        &Grid::new(4, 2, samples),
        Extras {
            profile: Some(matrix_profile(&WIDE_COLORANTS, &Curve::SrgbParametric)),
            ..Extras::default()
        },
    );
    let identified = identify(&file, &limits()).unwrap();
    let expected: [[u8; 4]; 8] = [
        [0, 0, 0, 255],
        [255, 255, 255, 255],
        [128, 128, 128, 255],
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [215, 93, 31, 255],
        [17, 61, 93, 255],
    ];
    assert_eq!(identified.pixels.rgba(), expected.concat());

    // A power of 2.2 on the sRGB primaries moves only the tones.
    let file = rgb_png(
        &Grid::new(
            4,
            1,
            vec![[0, 0, 0], [255, 255, 255], [128, 128, 128], [10, 60, 200]],
        ),
        Extras {
            profile: Some(matrix_profile(&SRGB_COLORANTS, &Curve::Gamma(2.2))),
            ..Extras::default()
        },
    );
    let identified = identify(&file, &limits()).unwrap();
    let expected: [[u8; 4]; 4] = [
        [0, 0, 0, 255],
        [255, 255, 255, 255],
        [129, 129, 129, 255],
        [3, 57, 201, 255],
    ];
    assert_eq!(identified.pixels.rgba(), expected.concat());
}

#[test]
fn a_profile_is_applied_before_the_picture_is_turned_and_alpha_is_left_alone() {
    let profile = matrix_profile(&WIDE_COLORANTS, &Curve::SrgbParametric);
    let upright = identify(&with_profile(profile.clone()), &limits()).unwrap();
    let turned = identify(
        &rgb_png(
            &picture_grid().stored_under(6),
            Extras {
                exif: Some(exif_orientation(6)),
                profile: Some(profile.clone()),
            },
        ),
        &limits(),
    )
    .unwrap();
    assert_eq!(turned.identity.pixels, upright.identity.pixels);
    let pixels: Vec<[u8; 4]> = picture_grid()
        .pixels
        .iter()
        .map(|[r, g, b]| [*r, *g, *b, 77])
        .collect();
    let see_through = identify(
        &rgba_png(
            &Grid::new(WIDTH, HEIGHT, pixels),
            Extras {
                profile: Some(profile),
                ..Extras::default()
            },
        ),
        &limits(),
    )
    .unwrap();
    let (got, want) = (see_through.pixels.rgba(), upright.pixels.rgba());
    for (got, want) in got.as_chunks::<4>().0.iter().zip(want.as_chunks::<4>().0) {
        assert_eq!(*got, [want[0], want[1], want[2], 77]);
    }
}

#[test]
fn every_kind_of_curve_a_matrix_profile_may_hold_is_read() {
    // Each of these is the sRGB curve or the identity written another way.
    let (g, a, b, c, d) = (2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045);
    for (curve, profile) in [
        (Curve::Parametric(3, vec![g, a, b, c, d]), Profile::Srgb),
        (
            Curve::Parametric(4, vec![g, a, b, c, d, 0.0, 0.0]),
            Profile::Srgb,
        ),
        // Types 0 to 2 have no linear toe, so they are plain powers: not sRGB.
        (Curve::Parametric(0, vec![2.2]), Profile::Converted),
        (
            Curve::Parametric(1, vec![2.2, 1.0, 0.0]),
            Profile::Converted,
        ),
        (
            Curve::Parametric(2, vec![2.2, 1.0, 0.0, 0.0]),
            Profile::Converted,
        ),
        // No entries is the identity; two entries are a straight line: linear light.
        (Curve::Table(vec![]), Profile::Converted),
        (Curve::Table(vec![0, 65535]), Profile::Converted),
    ] {
        let file = with_profile(matrix_profile(&SRGB_COLORANTS, &curve));
        let identified = identify(&file, &limits()).unwrap();
        assert_eq!(identified.stored.profile, profile, "{curve:?}");
    }
    // The three plain powers of 2.2 agree with each other, and the two straight lines do.
    let digest = |curve: Curve| {
        identify(
            &with_profile(matrix_profile(&SRGB_COLORANTS, &curve)),
            &limits(),
        )
        .unwrap()
        .identity
        .pixels
    };
    assert_eq!(
        digest(Curve::Parametric(0, vec![2.2])),
        digest(Curve::Parametric(1, vec![2.2, 1.0, 0.0]))
    );
    assert_eq!(
        digest(Curve::Parametric(0, vec![2.2])),
        digest(Curve::Parametric(2, vec![2.2, 1.0, 0.0, 0.0]))
    );
    assert_eq!(
        digest(Curve::Table(vec![])),
        digest(Curve::Table(vec![0, 65535]))
    );
}

#[test]
fn a_profile_with_no_canonical_reading_is_refused_and_never_guessed() {
    let srgb = matrix_tags(&SRGB_COLORANTS, &Curve::SrgbParametric);
    let unsupported = |profile: Vec<u8>, what: &str| {
        let result = identify(&with_profile(profile), &limits());
        assert!(
            matches!(result, Err(IdentityError::Unsupported(_))),
            "{what}: {result:?}"
        );
    };
    // Not RGB data.
    unsupported(profile_of(b"CMYK", &srgb), "CMYK");
    unsupported(profile_of(b"GRAY", &srgb), "grey");
    // No colorants or no curves: a profile that works through lookup tables alone.
    unsupported(profile_of(b"RGB ", &srgb[..3]), "no curves");
    unsupported(profile_of(b"RGB ", &srgb[3..]), "no colorants");
    unsupported(
        profile_of(b"RGB ", &[(*b"A2B0", vec![0; 32])]),
        "lookup tables only",
    );
    // Not a profile at all.
    unsupported(vec![1, 2, 3], "three bytes");
    unsupported(vec![0; 128], "an empty header");
    let mut no_signature = matrix_profile(&SRGB_COLORANTS, &Curve::SrgbParametric);
    no_signature[36..40].copy_from_slice(b"xxxx");
    unsupported(no_signature, "no signature");
    // Connected through Lab, not XYZ.
    let mut lab = matrix_profile(&SRGB_COLORANTS, &Curve::SrgbParametric);
    lab[20..24].copy_from_slice(b"Lab ");
    unsupported(lab, "Lab");
    // Curves that are not curves: an unknown element, an unknown parametric type, a
    // parametric curve that divides by zero or takes the power of a negative number, and
    // colorants that make no invertible matrix.
    let mut odd = srgb.clone();
    odd[3].1 = b"mft2\0\0\0\0\0\0\0\0".to_vec();
    unsupported(profile_of(b"RGB ", &odd), "an unknown curve element");
    for (kind, parameters) in [
        (5, vec![2.2]),
        (1, vec![2.2, 0.0, 0.5]),
        (3, vec![2.5, -1.0, 0.0, 1.0, 0.0]),
    ] {
        unsupported(
            matrix_profile(
                &SRGB_COLORANTS,
                &Curve::Parametric(kind, parameters.clone()),
            ),
            &format!("parametric {kind} {parameters:?}"),
        );
    }
    let flat = [[0.4, 0.2, 0.1], [0.4, 0.2, 0.1], [0.2, 0.6, 0.7]];
    unsupported(
        matrix_profile(&flat, &Curve::SrgbParametric),
        "two equal colorants",
    );
    // A grey image with a profile: the profile cannot be an RGB one that fits it.
    let grey = png(
        WIDTH,
        HEIGHT,
        ExtendedColorType::L8,
        &[7; 12],
        Extras {
            profile: Some(matrix_profile(&SRGB_COLORANTS, &Curve::SrgbParametric)),
            ..Extras::default()
        },
    );
    assert!(matches!(
        identify(&grey, &limits()),
        Err(IdentityError::Unsupported(_))
    ));
}

#[test]
fn a_damaged_profile_is_refused_without_a_panic() {
    let whole = matrix_profile(&SRGB_COLORANTS, &Curve::SrgbTable(64));
    let plain = identify(&rgb_png(&picture_grid(), Extras::default()), &limits()).unwrap();
    // Cut short at every length.
    for length in 0..whole.len() {
        let result = identify(&with_profile(whole[..length].to_vec()), &limits());
        match result {
            Err(IdentityError::Unsupported(_)) => {}
            // An empty profile is no profile.
            Ok(identified) if length == 0 => {
                assert_eq!(identified.identity.pixels, plain.identity.pixels);
            }
            other => panic!("cut at {length}: {other:?}"),
        }
    }
    // Every byte of the header's size, the tag table and the curve's count overwritten
    // with the largest value: an offset, a size or a count that points past the end.
    let table_end = 128 + 4 + 12 * 6;
    let count_at = table_end + 3 * 20 + 8;
    for at in (0..4).chain(128..table_end).chain(count_at..count_at + 4) {
        let mut damaged = whole.clone();
        damaged[at] = 0xff;
        // Refused or read: either is an answer. A panic is not.
        let _ = identify(&with_profile(damaged), &limits());
    }
}

// --- limits and refusals ------------------------------------------------------------------

#[test]
fn a_file_over_a_limit_is_refused_before_it_is_decoded() {
    let file = rgb_png(&picture_grid(), Extras::default());
    let small = DecodeLimits {
        max_bytes: file.len() as u64 - 1,
        ..DecodeLimits::default()
    };
    assert!(matches!(
        identify(&file, &small),
        Err(IdentityError::Limit(_))
    ));
    let exact = DecodeLimits {
        max_bytes: file.len() as u64,
        max_pixels: 12,
    };
    identify(&file, &exact).unwrap();
    let few = DecodeLimits {
        max_pixels: 11,
        ..DecodeLimits::default()
    };
    assert!(matches!(
        identify(&file, &few),
        Err(IdentityError::Limit(_))
    ));
    // The limit is read from the header: a file that claims a vast picture and holds none
    // of it is over the limit, not a broken image.
    let mut vast = file.clone();
    // The width and height follow the eight-byte signature and the chunk's length and name.
    vast[16..20].copy_from_slice(&60_000u32.to_be_bytes());
    vast[20..24].copy_from_slice(&60_000u32.to_be_bytes());
    // The header chunk's checksum, over its name and data.
    let crc = crc32(&vast[12..29]);
    vast[29..33].copy_from_slice(&crc.to_be_bytes());
    assert!(matches!(
        identify(&vast, &limits()),
        Err(IdentityError::Limit(_))
    ));
    assert_eq!(limits().max_pixels, 50_000_000);
    assert_eq!(limits().max_bytes, 64 * 1024 * 1024);
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn bytes_that_are_not_an_image_are_refused() {
    let file = rgb_png(&picture_grid(), Extras::default());
    for bytes in [
        &b""[..],
        &b"not an image"[..],
        &file[..20],
        &file[..file.len() - 20],
    ] {
        assert!(
            matches!(identify(bytes, &limits()), Err(IdentityError::Image(_))),
            "{} bytes",
            bytes.len()
        );
    }
}
