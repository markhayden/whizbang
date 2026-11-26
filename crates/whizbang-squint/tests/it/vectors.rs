//! Fixed vectors: the crops `view` must give, on every machine.
//!
//! The pixel digests were computed apart from this crate, by a separate statement of the
//! record's method (`docs/reference/tools/view.md`) in whole-number arithmetic: the box
//! snapped outward, the size, and for each output sample the area-weighted sum over the
//! source pixels its rectangle touches, divided once and rounded half up, each colour
//! weighted by alpha as well where any alpha is over 0; then ADR 0001's `pixels/1` over
//! the result. The pictures are `support::noisy`, which that statement repeats.
//!
//! Weighting by alpha (plan mE, task P1, item 6) changed the two reduced vectors with
//! alpha, `tall` and `inner`. Their pixel digests were worked out again by that separate
//! statement, a short program outside the repository that first gave all five of the
//! digests before the change, and only then the two new ones; the test agreed with them.
//!
//! The file digests are what the pinned PNG encoder writes for those pixels: the core's
//! one encoder (`whizbang_core::identity::encode`), whose pins its own test holds
//! (`encode::the_core_encoder_keeps_the_three_pins_and_the_squint_vectors`). They were
//! taken from a run, and they hold the encoder still: a crop's file is part of its
//! artifact's identity (ADR 0002), so an encoder that writes another file for the same
//! pixels changes every crop's identity, and must be a deliberate change.
//!
//! The record's table of these vectors is held here too, by the test that makes the
//! crops anyway.

use whizbang_core::identity::Digest;
use whizbang_squint::view::Crop;

use super::record;
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
        file: "sha256:21613d6eb49bfe933c8d88478a36511435a49742913170f5aed47cb054deee77",
    },
    Vector {
        name: "tall",
        size: (9, 3000),
        seed: 12,
        alpha: true,
        bbox: [0.0, 0.0, 1.0, 1.0],
        cut: [0.0, 0.0, 9.0, 3000.0],
        shown: (3, 1024),
        pixels: "sha256:a485fc9f2a3f791afbc13c238e68bd21de1fbde6edfe75dcf3fdfe6111fb6deb",
        file: "sha256:1d2501dc1c7853f26b211db67c8209850820fa1bb5d88bd548a8600d8a8788e8",
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
        pixels: "sha256:0907d1e380ee24b9a4280dc0190df66e17833f2a5cfe68e496527781cd18119e",
        file: "sha256:4cf5e1120988d0fd9fea9c6c63b482f32fea2c549a9f3ac2a42e55bc4b9e4679",
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
        file: "sha256:807776e8b9ef73814e2d45db8334406593197040d164552d9c58626a27174832",
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
        file: "sha256:f83ba461e385f87ee923cbb9551c6e3d1d6c1d65901f08806ad4679a682292a3",
    },
];

/// The crop `view` gives for each vector, in their order.
pub fn crops() -> Vec<Crop> {
    VECTORS
        .iter()
        .map(|vector| {
            let (width, height) = vector.size;
            let image = picture(width, height, noisy(vector.seed, vector.alpha));
            look(&image, vector.bbox)
        })
        .collect()
}

#[test]
fn the_crops_are_the_fixed_vectors() {
    let crops = crops();
    for (vector, crop) in VECTORS.iter().zip(&crops) {
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
    // The record's table of the vectors is what these crops are.
    record::holds(1, &record::vector_table(&crops));
}
