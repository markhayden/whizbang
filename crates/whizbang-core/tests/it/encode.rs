//! `encode::…`: the one encoder of an image artifact's file (plan m3, task F1), held
//! still. Its pins moved here from `whizbang-squint` with it; the crops of `view`, whose
//! file digests `whizbang-squint`'s `vectors::the_crops_are_the_fixed_vectors` holds, are
//! written by it, and this test holds one picture's file to the digest that encoder wrote
//! before the move.

use whizbang_core::identity::encode::{self, Channels};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Digest, identify};

/// The package of a name and version in the lock file: its lines, from its name on.
fn locked<'a>(lock: &'a str, name: &str, version: &str) -> &'a str {
    lock.split("[[package]]\n")
        .find(|package| {
            package.starts_with(&format!("name = \"{name}\"\nversion = \"{version}\"\n"))
        })
        .unwrap_or_else(|| panic!("the lock file holds no {name} {version}"))
}

/// A picture of 37 by 11 with some of everything in it, alpha too.
fn noisy() -> CanonicalPixels {
    let rgba = (0..37u32 * 11)
        .flat_map(|at| {
            let mixed = at.wrapping_mul(2_654_435_761) >> 7;
            [
                mixed as u8,
                (mixed >> 8) as u8,
                (at * 7) as u8,
                (255 - at % 61) as u8,
            ]
        })
        .collect();
    CanonicalPixels::from_rgba(37, 11, rgba).unwrap()
}

#[test]
fn the_core_encoder_keeps_the_four_pins_and_the_squint_vectors() {
    // `image`'s PNG encoder is the `png` crate, whose compressor at the setting used is
    // `flate2` over `miniz_oxide` 0.9.1; each is pinned in this crate's manifest to the
    // version the lock file resolves, so raising one is an edit of the manifest, which
    // the file digests then judge.
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(folder.join("Cargo.toml")).unwrap();
    let lock = std::fs::read_to_string(folder.join("../../Cargo.lock")).unwrap();
    for (key, name, version) in [
        ("image", "image", "0.25.10"),
        ("png", "png", "0.18.1"),
        ("flate2", "flate2", "1.1.11"),
        ("miniz_oxide_compress", "miniz_oxide", "0.9.1"),
    ] {
        let pinned = manifest.lines().any(|line| {
            line.starts_with(&format!("{key} = "))
                && line.contains(&format!("\"={version}\""))
                && (key == name || line.contains(&format!("package = \"{name}\"")))
        });
        assert!(
            pinned,
            "{name} is pinned to exactly {version} in the manifest"
        );
        locked(&lock, name, version);
    }
    assert!(locked(&lock, "image", "0.25.10").contains("\n \"png\",\n"));
    assert!(locked(&lock, "png", "0.18.1").contains("\n \"flate2\",\n"));
    assert!(locked(&lock, "flate2", "1.1.11").contains("\n \"miniz_oxide 0.9.1\",\n"));

    // One picture's file, four channels and grey, is what the encoder wrote before it
    // moved; and it reads back to the same pixels.
    let pixels = noisy();
    let file = encode::png(&pixels).unwrap();
    assert_eq!(
        identify(&file, &DecodeLimits::default()).unwrap().pixels,
        pixels
    );
    let grey: Vec<u8> = pixels.rgba().chunks(4).map(|p| p[0]).collect();
    let grey = encode::png_of(&grey, 37, 11, Channels::Grey).unwrap();
    let digests = [
        Digest::of_bytes(&file).to_string(),
        Digest::of_bytes(&grey).to_string(),
    ];
    assert_eq!(
        digests,
        [
            "sha256:e0991ed831b84a5ed4491639233a29d68d14fb3b529944e7e9c02e9718e86277",
            "sha256:f409ee181d964e72d25e3af4b28b4308329f46328bd34da6b28c6e4f653ac548",
        ]
    );
    assert!(encode::png_of(&[0; 5], 2, 2, Channels::Rgba).is_err());
}
