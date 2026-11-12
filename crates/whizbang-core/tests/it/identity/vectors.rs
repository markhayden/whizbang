//! Fixed vectors of the canonical pixel digest (ADR 0001): files, as bytes, with the two
//! digests each must have. One picture is stored under each orientation tag, with and
//! without an alpha channel, with and without a colour profile, at two depths and in two
//! containers. Re-encoding here means the same samples stored another way, never a lossy
//! recompression.
//!
//! The files are kept as hex in `vectors_table.rs`, because this repo holds no image files.
//! `print_the_vectors` writes that table again from the builders in `support.rs`; a change
//! to a recorded pixel digest is a change to the identity scheme, which needs a new ADR.

use std::collections::BTreeSet;

use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Digest, identify};

use super::support::{
    Curve, Extras, Grid, HEIGHT, SRGB_COLORANTS, WIDE_COLORANTS, WIDTH, exif_orientation, hex,
    matrix_profile, picture_grid, picture_rgba, rgb_bmp, rgb_png, rgb16_png, rgba_png, unhex,
};
use super::vectors_table::VECTORS;

/// One file and the digests it must have.
pub struct Vector {
    pub name: &'static str,
    /// What the picture is, against the plain one.
    pub same_picture: bool,
    pub file: &'static str,
    pub pixels: &'static str,
    pub bytes: &'static str,
}

/// The files, built: the name, whether the picture is the plain one, and the bytes.
fn built() -> Vec<(String, bool, Vec<u8>)> {
    let picture = picture_grid();
    let mut files = vec![(
        "plain".to_owned(),
        true,
        rgb_png(&picture, Extras::default()),
    )];
    for orientation in 1..=8u8 {
        files.push((
            format!("orientation-{orientation}"),
            true,
            rgb_png(
                &picture.stored_under(orientation),
                Extras {
                    exif: Some(exif_orientation(u16::from(orientation))),
                    ..Extras::default()
                },
            ),
        ));
    }
    let with_alpha = |alpha: &dyn Fn(usize) -> u8| {
        let pixels: Vec<[u8; 4]> = picture
            .pixels
            .iter()
            .enumerate()
            .map(|(i, [r, g, b])| [*r, *g, *b, alpha(i)])
            .collect();
        rgba_png(&Grid::new(WIDTH, HEIGHT, pixels), Extras::default())
    };
    files.push(("alpha-opaque".to_owned(), true, with_alpha(&|_| 255)));
    files.push((
        "alpha-graded".to_owned(),
        false,
        with_alpha(&|i| (i * 23) as u8),
    ));
    let with_profile = |colorants, curve| {
        rgb_png(
            &picture,
            Extras {
                profile: Some(matrix_profile(colorants, &curve)),
                ..Extras::default()
            },
        )
    };
    files.push((
        "profile-srgb".to_owned(),
        true,
        with_profile(&SRGB_COLORANTS, Curve::SrgbParametric),
    ));
    files.push((
        "profile-wide".to_owned(),
        false,
        with_profile(&WIDE_COLORANTS, Curve::SrgbParametric),
    ));
    files.push((
        "profile-power".to_owned(),
        false,
        with_profile(&SRGB_COLORANTS, Curve::Gamma(2.2)),
    ));
    let wide: Vec<[u16; 3]> = picture
        .pixels
        .iter()
        .map(|rgb| rgb.map(|v| u16::from(v) * 257))
        .collect();
    files.push((
        "sixteen-bit".to_owned(),
        true,
        rgb16_png(WIDTH, HEIGHT, &wide),
    ));
    files.push(("container-bmp".to_owned(), true, rgb_bmp(&picture)));
    files
}

#[test]
fn every_vector_has_its_two_digests() {
    assert_eq!(VECTORS.len(), 16);
    for vector in VECTORS {
        let bytes = unhex(vector.bytes);
        let identified = identify(&bytes, &DecodeLimits::default()).unwrap();
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
    let picture = CanonicalPixels::from_rgba(WIDTH, HEIGHT, picture_rgba())
        .unwrap()
        .digest()
        .to_string();
    let mut files = BTreeSet::new();
    let mut others = BTreeSet::new();
    for vector in VECTORS {
        assert!(files.insert(vector.file), "{}", vector.name);
        if vector.same_picture {
            assert_eq!(vector.pixels, picture, "{}", vector.name);
        } else {
            assert_ne!(vector.pixels, picture, "{}", vector.name);
            assert!(others.insert(vector.pixels), "{}", vector.name);
        }
    }
    let names: Vec<&str> = VECTORS.iter().map(|vector| vector.name).collect();
    for needed in [
        "plain",
        "orientation-1",
        "orientation-2",
        "orientation-3",
        "orientation-4",
        "orientation-5",
        "orientation-6",
        "orientation-7",
        "orientation-8",
        "alpha-opaque",
        "alpha-graded",
        "profile-srgb",
        "profile-wide",
        "profile-power",
        "sixteen-bit",
        "container-bmp",
    ] {
        assert!(names.contains(&needed), "{needed}");
    }
}

#[test]
fn the_builders_still_make_the_pictures_the_vectors_hold() {
    // The encoders may change the bytes they write; the pictures do not change.
    let built = built();
    assert_eq!(built.len(), VECTORS.len());
    for ((name, same_picture, bytes), vector) in built.iter().zip(VECTORS) {
        assert_eq!(name, vector.name);
        assert_eq!(*same_picture, vector.same_picture);
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
    for (name, same_picture, bytes) in built() {
        let identified = identify(&bytes, &DecodeLimits::default()).unwrap();
        out.push_str(&format!(
            "    Vector {{\n        name: {name:?},\n        same_picture: {same_picture},\n        \
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
