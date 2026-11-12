//! Fixed vectors of the canonical pixel digest (ADR 0001): files, as bytes, with the two
//! digests each must have. One picture is stored under each orientation tag, with and
//! without an alpha channel, with and without a colour profile, at several depths, through
//! a palette, interlaced, and in two containers. Re-encoding here means the same samples
//! stored another way, never a lossy recompression. The JPEG vectors fix what the pinned
//! decoder makes of a lossy file: a decoder that decodes one differently fails them.
//!
//! The files are kept as hex in `vectors_table.rs`, because this repo holds no image files.
//! `print_the_vectors` writes that table again from the builders in `support.rs` and the
//! files in `jpeg_sources.rs`; a change to a recorded pixel digest is a change to the
//! identity scheme, which needs a new ADR.

use std::collections::BTreeMap;

use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Digest, identify};

use super::jpeg_sources;
use super::support::{
    Curve, Extras, Grid, HEIGHT, RawPng, SRGB_COLORANTS, WIDE_COLORANTS, WIDTH, exif_orientation,
    hex, iccp, interlaced_rows, matrix_profile, packed, picture_grid, picture_rgba, profile_piece,
    rgb_bmp, rgb_png, rgb16_png, rgba_png, unhex, with_segment,
};
use super::vectors_table::VECTORS;

/// One file and the digests it must have.
pub struct Vector {
    pub name: &'static str,
    /// The picture the file holds. Files of one picture share a pixel digest, and files of
    /// two pictures do not. `picture` is the one `support::picture` makes.
    pub picture: &'static str,
    pub file: &'static str,
    pub pixels: &'static str,
    pub bytes: &'static str,
}

/// The grey samples of the low-depth vectors: twelve values of four bits.
pub fn grey_samples() -> Vec<u8> {
    (0..12u8).map(|n| (n * 7 + 3) % 16).collect()
}

/// The files, built: the name, the picture, and the bytes.
pub fn built() -> Vec<(String, &'static str, Vec<u8>)> {
    let picture = picture_grid();
    let mut files: Vec<(String, &'static str, Vec<u8>)> = Vec::new();
    let mut add = |name: &str, picture: &'static str, bytes: Vec<u8>| {
        files.push((name.to_owned(), picture, bytes));
    };
    add("plain", "picture", rgb_png(&picture, Extras::default()));
    for orientation in 1..=8u8 {
        add(
            &format!("orientation-{orientation}"),
            "picture",
            rgb_png(
                &picture.stored_under(orientation),
                Extras {
                    exif: Some(exif_orientation(u16::from(orientation))),
                    ..Extras::default()
                },
            ),
        );
    }
    // The tag after the image data, where the decoder does not look.
    add(
        "orientation-after-data",
        "picture",
        RawPng::rgb(&picture.stored_under(6))
            .after(b"eXIf", exif_orientation(6))
            .bytes(),
    );
    let with_alpha = |alpha: &dyn Fn(usize) -> u8| {
        let pixels: Vec<[u8; 4]> = picture
            .pixels
            .iter()
            .enumerate()
            .map(|(i, [r, g, b])| [*r, *g, *b, alpha(i)])
            .collect();
        rgba_png(&Grid::new(WIDTH, HEIGHT, pixels), Extras::default())
    };
    add("alpha-opaque", "picture", with_alpha(&|_| 255));
    add("alpha-graded", "graded", with_alpha(&|i| (i * 23) as u8));
    let with_profile = |colorants, curve| {
        rgb_png(
            &picture,
            Extras {
                profile: Some(matrix_profile(colorants, &curve)),
                ..Extras::default()
            },
        )
    };
    add(
        "profile-srgb",
        "picture",
        with_profile(&SRGB_COLORANTS, Curve::SrgbParametric),
    );
    add(
        "profile-wide",
        "wide",
        with_profile(&WIDE_COLORANTS, Curve::SrgbParametric),
    );
    add(
        "profile-power",
        "power",
        with_profile(&SRGB_COLORANTS, Curve::Gamma(2.2)),
    );
    add(
        "profile-after-data",
        "wide",
        RawPng::rgb(&picture)
            .after(
                b"iCCP",
                iccp(&matrix_profile(&WIDE_COLORANTS, &Curve::SrgbParametric)),
            )
            .bytes(),
    );
    let wide: Vec<[u16; 3]> = picture
        .pixels
        .iter()
        .map(|rgb| rgb.map(|v| u16::from(v) * 257))
        .collect();
    add("sixteen-bit", "picture", rgb16_png(WIDTH, HEIGHT, &wide));
    add("container-bmp", "picture", rgb_bmp(&picture));

    // Through a palette: the picture's twelve colours, and each pixel its own entry.
    let palette: Vec<u8> = picture.pixels.iter().flatten().copied().collect();
    let indices: Vec<Vec<u8>> = (0..HEIGHT)
        .map(|y| (0..WIDTH).map(|x| (y * WIDTH + x) as u8).collect())
        .collect();
    add(
        "palette",
        "picture",
        RawPng::new(WIDTH, HEIGHT, 8, 3, indices.clone())
            .before(b"PLTE", palette.clone())
            .bytes(),
    );
    // The same at four bits an index.
    add(
        "palette-four-bit",
        "picture",
        RawPng::new(
            WIDTH,
            HEIGHT,
            4,
            3,
            indices.iter().map(|row| packed(row, 4)).collect(),
        )
        .before(b"PLTE", palette.clone())
        .bytes(),
    );
    // A palette with alpha for its entries: the picture of `alpha-graded`.
    add(
        "palette-alpha",
        "graded",
        RawPng::new(WIDTH, HEIGHT, 8, 3, indices)
            .before(b"PLTE", palette)
            .before(b"tRNS", (0..12).map(|i| (i * 23) as u8).collect())
            .bytes(),
    );
    add(
        "interlaced",
        "picture",
        RawPng {
            interlaced: true,
            rows: interlaced_rows(&picture),
            ..RawPng::rgb(&picture)
        }
        .bytes(),
    );

    // Grey at four bits, and at eight with each sample 17 times as large: one picture.
    let grey = grey_samples();
    let rows = |depth: u8, scale: &dyn Fn(u8) -> u8| -> Vec<Vec<u8>> {
        grey.chunks(WIDTH as usize)
            .map(|row| {
                let row: Vec<u8> = row.iter().map(|v| scale(*v)).collect();
                if depth == 8 { row } else { packed(&row, depth) }
            })
            .collect()
    };
    add(
        "grey-four-bit",
        "grey",
        RawPng::new(WIDTH, HEIGHT, 4, 0, rows(4, &|v| v)).bytes(),
    );
    add(
        "grey-eight-bit",
        "grey",
        RawPng::new(WIDTH, HEIGHT, 8, 0, rows(8, &|v| v * 17)).bytes(),
    );
    add(
        "grey-two-bit",
        "grey-two-bit",
        RawPng::new(WIDTH, HEIGHT, 2, 0, rows(2, &|v| v % 4)).bytes(),
    );
    add(
        "grey-one-bit",
        "grey-one-bit",
        RawPng::new(WIDTH, HEIGHT, 1, 0, rows(1, &|v| v % 2)).bytes(),
    );

    // One colour named transparent, and the same picture with an alpha channel.
    let [r, g, b] = picture.pixels[3];
    add(
        "key-colour",
        "keyed",
        RawPng::rgb(&picture)
            .before(b"tRNS", vec![0, r, 0, g, 0, b])
            .bytes(),
    );
    add(
        "key-alpha",
        "keyed",
        with_alpha(&|i| if i == 3 { 0 } else { 255 }),
    );

    // The JPEG files, as they are and with a segment added.
    let jpeg_444 = unhex(jpeg_sources::BASELINE_444);
    add("jpeg-420", "jpeg-420", unhex(jpeg_sources::BASELINE_420));
    add("jpeg-444", "jpeg-444", jpeg_444.clone());
    // The same coefficients as `jpeg-420`, sent in another order: one picture.
    add(
        "jpeg-progressive",
        "jpeg-420",
        unhex(jpeg_sources::PROGRESSIVE),
    );
    add("jpeg-grey", "jpeg-grey", unhex(jpeg_sources::GREY));
    let mut exif = b"Exif\0\0".to_vec();
    exif.extend_from_slice(&exif_orientation(6));
    add(
        "jpeg-orientation-6",
        "jpeg-turned",
        with_segment(&jpeg_444, 0xe1, &exif),
    );
    add(
        "jpeg-profile-srgb",
        "jpeg-444",
        with_segment(
            &jpeg_444,
            0xe2,
            &profile_piece(
                1,
                1,
                &matrix_profile(&SRGB_COLORANTS, &Curve::SrgbParametric),
            ),
        ),
    );
    add(
        "jpeg-profile-wide",
        "jpeg-wide",
        with_segment(
            &jpeg_444,
            0xe2,
            &profile_piece(
                1,
                1,
                &matrix_profile(&WIDE_COLORANTS, &Curve::SrgbParametric),
            ),
        ),
    );
    files
}

/// The bytes of the vector with this name.
pub fn vector(name: &str) -> Vec<u8> {
    let found = VECTORS
        .iter()
        .find(|vector| vector.name == name)
        .unwrap_or_else(|| panic!("no vector {name}"));
    unhex(found.bytes)
}

#[test]
fn every_vector_has_its_two_digests() {
    assert_eq!(VECTORS.len(), 35);
    for vector in VECTORS {
        let bytes = unhex(vector.bytes);
        let identified = identify(&bytes, &DecodeLimits::default())
            .unwrap_or_else(|error| panic!("{}: {error}", vector.name));
        assert_eq!(
            identified.identity.file.to_string(),
            vector.file,
            "{}",
            vector.name
        );
        assert_eq!(identified.identity.file.0, Digest::of_bytes(&bytes));
        assert_eq!(
            identified.identity.pixels.to_string(),
            vector.pixels,
            "{}",
            vector.name
        );
    }
}

#[test]
fn the_vectors_of_one_picture_share_a_pixel_digest_and_no_file_digest() {
    let mut files = BTreeMap::new();
    let mut pictures: BTreeMap<&str, &str> = BTreeMap::new();
    let mut digests: BTreeMap<&str, &str> = BTreeMap::new();
    for vector in VECTORS {
        assert!(
            files.insert(vector.file, vector.name).is_none(),
            "{}",
            vector.name
        );
        // One picture, one digest; one digest, one picture.
        assert_eq!(
            *pictures.entry(vector.picture).or_insert(vector.pixels),
            vector.pixels,
            "{}",
            vector.name
        );
        assert_eq!(
            *digests.entry(vector.pixels).or_insert(vector.picture),
            vector.picture,
            "{}",
            vector.name
        );
    }
    let picture = CanonicalPixels::from_rgba(WIDTH, HEIGHT, picture_rgba())
        .unwrap()
        .digest()
        .to_string();
    assert_eq!(pictures["picture"], picture);
    // What the plan asks for is here: each orientation tag, with and without alpha, with
    // and without a profile.
    let of = |picture: &str| -> Vec<&str> {
        VECTORS
            .iter()
            .filter(|vector| vector.picture == picture)
            .map(|vector| vector.name)
            .collect()
    };
    assert_eq!(
        of("picture"),
        [
            "plain",
            "orientation-1",
            "orientation-2",
            "orientation-3",
            "orientation-4",
            "orientation-5",
            "orientation-6",
            "orientation-7",
            "orientation-8",
            "orientation-after-data",
            "alpha-opaque",
            "profile-srgb",
            "sixteen-bit",
            "container-bmp",
            "palette",
            "palette-four-bit",
            "interlaced",
        ]
    );
    assert_eq!(of("graded"), ["alpha-graded", "palette-alpha"]);
    assert_eq!(of("wide"), ["profile-wide", "profile-after-data"]);
    assert_eq!(of("power"), ["profile-power"]);
    assert_eq!(of("grey"), ["grey-four-bit", "grey-eight-bit"]);
    assert_eq!(of("keyed"), ["key-colour", "key-alpha"]);
    assert_eq!(of("jpeg-420"), ["jpeg-420", "jpeg-progressive"]);
    assert_eq!(of("jpeg-444"), ["jpeg-444", "jpeg-profile-srgb"]);
    assert_eq!(pictures.len(), 13);
}

#[test]
fn the_builders_still_make_the_pictures_the_vectors_hold() {
    // The encoders may change the bytes they write; the pictures do not change.
    let built = built();
    assert_eq!(built.len(), VECTORS.len());
    for ((name, picture, bytes), vector) in built.iter().zip(VECTORS) {
        assert_eq!(name, vector.name);
        assert_eq!(*picture, vector.picture);
        let identified = identify(bytes, &DecodeLimits::default()).unwrap();
        assert_eq!(
            identified.identity.pixels.to_string(),
            vector.pixels,
            "{name}"
        );
    }
}

/// Writes `vectors_table.rs` again. Run it by name with `--run-ignored only`.
#[test]
#[ignore = "rewrites the committed vectors"]
fn print_the_vectors() {
    let mut out = String::from(
        "//! The vectors of `vectors.rs`, written by its `print_the_vectors`. Not edited by hand.\n\n\
         use super::vectors::Vector;\n\npub const VECTORS: &[Vector] = &[\n",
    );
    for (name, picture, bytes) in built() {
        let identified = identify(&bytes, &DecodeLimits::default())
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        out.push_str(&format!(
            "    Vector {{\n        name: {name:?},\n        picture: {picture:?},\n        \
             file: \"{}\",\n        pixels: \"{}\",\n        bytes: \"{}\",\n    }},\n",
            identified.identity.file,
            identified.identity.pixels,
            hex(&bytes)
        ));
    }
    out.push_str("];\n");
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/it/identity/vectors_table.rs");
    std::fs::write(path, out).unwrap();
}
