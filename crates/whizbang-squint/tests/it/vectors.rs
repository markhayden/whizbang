//! Fixed vectors: the crops `view` must give, on every machine.
//!
//! The pixel digests were computed apart from this crate, by a separate statement of the
//! record's method (`docs/reference/tools/view.md`) in whole-number arithmetic: the box
//! snapped outward, the size, and for each output sample the area-weighted sum over the
//! source pixels its rectangle touches, divided once and rounded half up; then ADR 0001's
//! `pixels/1` over the result. The pictures are `support::noisy`, which that statement
//! repeats.
//!
//! The file digests are what the pinned PNG encoder writes for those pixels. They were
//! taken from a run, and they hold the encoder still: a crop's file is part of its
//! artifact's identity (ADR 0002), so an encoder that writes another file for the same
//! pixels changes every crop's identity, and must be a deliberate change.

use whizbang_core::identity::Digest;

use super::support::*;

pub struct Vector {
    pub name: &'static str,
    pub size: (u32, u32),
    pub seed: u32,
    pub alpha: bool,
    pub bbox: [f64; 4],
    pub cut: [f64; 4],
    pub shown: (u32, u32),
    pub pixels: &'static str,
    pub file: &'static str,
}

pub const VECTORS: &[Vector] = &[
    // Twice as wide as the bound and a little more: both ratios are fractions.
    Vector {
        name: "wide",
        size: (2050, 9),
        seed: 11,
        alpha: false,
        bbox: [0.0, 0.0, 1.0, 1.0],
        cut: [0.0, 0.0, 2050.0, 9.0],
        shown: (1024, 4),
        pixels: "sha256:72bc845fcdf488b8c0c5d945f7646d3b60991fed5b55a22884f3def8c1fb175b",
        file: "sha256:1aebc63f3ef4b44b312b4341fa1d6d9214338b2a0693687eb7ba7ecbb177fec8",
    },
    Vector {
        name: "tall",
        size: (9, 3000),
        seed: 12,
        alpha: true,
        bbox: [0.0, 0.0, 1.0, 1.0],
        cut: [0.0, 0.0, 9.0, 3000.0],
        shown: (3, 1024),
        pixels: "sha256:a36af55b50d0e73d98ea70e7ff5c4060dc0e30cca892b7b88b8027f148244ed0",
        file: "sha256:fe4c8a2b43b75ebaacefa7e53c64f8326eaa5eceb7c4067ed832f210e47d8419",
    },
    // A box inside the image, snapped outward on all four sides, then scaled.
    Vector {
        name: "inner",
        size: (1500, 110),
        seed: 13,
        alpha: true,
        bbox: [0.013, 0.02, 0.991, 0.987],
        cut: [19.0, 2.0, 1487.0, 109.0],
        shown: (1024, 75),
        pixels: "sha256:0c94e5c22ca385b105b1c9ef87aae2f795f23a13c91a5e2228cc44684ab55880",
        file: "sha256:720e731ca75903f1f1e945a8d58ed44f9ccc1dae36cc01eb5e9c08be5301811b",
    },
    // One pixel over the bound: every output pixel but the ends straddles two.
    Vector {
        name: "barely",
        size: (1025, 40),
        seed: 14,
        alpha: false,
        bbox: [0.0, 0.1, 1.0, 0.9],
        cut: [0.0, 4.0, 1025.0, 36.0],
        shown: (1024, 32),
        pixels: "sha256:0423afb95b17c7fae95e277de456a6d37f5df37439f2bdc72a9993ab94a065cc",
        file: "sha256:caebc62f193f97e29b31dd1592ea55a837c52e67b182c0c42f023ad78448d444",
    },
    // Under the bound: cut, and not scaled.
    Vector {
        name: "uncut",
        size: (300, 200),
        seed: 15,
        alpha: true,
        bbox: [0.101, 0.203, 0.907, 0.611],
        cut: [30.0, 40.0, 273.0, 123.0],
        shown: (243, 83),
        pixels: "sha256:ac039282827fe9cd35256aa966fa555923dadc86b0b9b5c2082484ef3cee36c6",
        file: "sha256:8892c2dbac0394a53bd6c31a0801b54492ade36fbbd9fcd7fcd7c9ea7861293f",
    },
];

#[test]
fn the_crops_are_the_fixed_vectors() {
    for vector in VECTORS {
        let (width, height) = vector.size;
        let image = picture(width, height, noisy(vector.seed, vector.alpha));
        let crop = look(&image, vector.bbox);
        let space = crop.image.space();
        assert_eq!(
            cut_of(&crop.binding),
            vector.cut,
            "{}: the cut",
            vector.name
        );
        assert_eq!(
            (space.width(), space.height()),
            vector.shown,
            "{}: the size",
            vector.name
        );
        assert_eq!(
            crop.image.identity().pixels.to_string(),
            vector.pixels,
            "{}: the pixels",
            vector.name
        );
        assert_eq!(
            Digest::of_bytes(crop.image.file()).to_string(),
            vector.file,
            "{}: the file",
            vector.name
        );
        assert_eq!(
            crop.image.identity().file.to_string(),
            vector.file,
            "{}: the file's digest in the identity",
            vector.name
        );
    }
}
