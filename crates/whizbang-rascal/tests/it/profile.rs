//! `profile` (plan m3, task O3, item 1): rascal's band mask of rascal's band, the one of
//! `crates/rascal-ops/tests/it/profile_feather.rs` at the commit `docs/sources.toml` pins
//! (120 by 40, the line at column 60, the half-width from 3 at the top to 15 at the
//! bottom), with `profile-v1`'s ratio of 0.55. The numbers are what rascal's own
//! `band_core` and `feather_blend` give on it.

use whizbang_core::artifact::{ArtifactId, BandProfile, Step, UseCase, Windows};
use whizbang_core::contract::{Call, Value};
use whizbang_core::identity::Digest;
use whizbang_rascal::profile::band_mask;

use super::canvases::{World, put, ran};
use super::grow::{made_by, on};

fn band() -> BandProfile {
    let half: Vec<f64> = (0..40).map(|y| 3.0 + 12.0 * f64::from(y) / 39.0).collect();
    let windows = Windows {
        gap: 1,
        step_columns: 2,
        texture_columns: 12,
    };
    let line = ArtifactId(Digest::of_bytes(b"line"));
    BandProfile::new(
        on(120, 40),
        line,
        60,
        3.0,
        half,
        windows,
        "fan-v1",
        made_by("fan"),
    )
    .unwrap()
}

/// Rascal's blend right of the line, from the first pixel outside the core, on rows 0,
/// 10, 20, 30 and 39, and the sum of each of those rows.
const ROWS: [(usize, u64, &[u8]); 5] = [
    (0, 1134, &[244, 169, 94, 43, 14, 3]),
    (10, 2362, &[252, 231, 200, 166, 126, 88, 56, 33, 17, 8, 4]),
    (
        20,
        3522,
        &[
            254, 244, 229, 211, 187, 159, 131, 103, 78, 57, 40, 27, 18, 12, 7, 4,
        ],
    ),
    (
        30,
        4784,
        &[
            254, 253, 247, 236, 221, 202, 181, 158, 136, 114, 93, 75, 59, 45, 35, 26, 19, 14, 10,
            7, 4, 3,
        ],
    ),
    (
        39,
        5266,
        &[
            255, 251, 244, 233, 220, 204, 187, 169, 150, 131, 113, 97, 81, 67, 54, 44, 35, 27, 21,
            16, 12, 9, 6, 4, 3,
        ],
    ),
];

#[test]
fn the_ported_vectors_give_rascals_mask() {
    let band = band();
    let step = Step::new("profile", "1");
    let mask = band_mask(&band, 0.55, step, &|| false).unwrap().unwrap();
    let count = |plane: &[u8]| plane.iter().filter(|v| **v != 0).count();
    assert_eq!(count(mask.core()), 720);
    assert_eq!(count(mask.blend()), 1258);
    let sum = |plane: &[u8]| plane.iter().map(|v| u64::from(*v)).sum::<u64>();
    assert_eq!(sum(mask.blend()), 136_554);
    for (y, row_sum, right) in ROWS {
        let row = &mask.blend()[y * 120..(y + 1) * 120];
        assert_eq!(sum(row), row_sum, "row {y}");
        let outside: Vec<u8> = row[60..].iter().copied().filter(|v| *v != 0).collect();
        assert_eq!(outside, right, "row {y}");
        // The core is the band at the row's half-width, symmetric about the line.
        let half = band.half()[y];
        for x in 0..120 {
            let inside = (x as f64 + 0.5 - 60.0).abs() <= half;
            assert_eq!(mask.core()[y * 120 + x] != 0, inside, "{x},{y}");
            assert_eq!(row[x], row[119 - x], "{x},{y}");
        }
    }
    // The mask is a seam's, drawn from the line, on the band's binding, with the band
    // and the sigma of each row kept.
    assert_eq!(mask.use_case(), UseCase::Seam);
    assert_eq!(mask.source().locations, vec![band.line()]);
    assert_eq!(mask.binding(), band.binding());
    let profile = mask.profile().unwrap();
    assert_eq!(profile.band, band);
    for (sigma, half) in profile.sigma.values().iter().zip(band.half()) {
        assert_eq!(*sigma, 0.55 * half);
    }

    // The action makes the same mask, with profile-v1's ratio when the call gives none.
    let world = World::new("profile");
    let engine = super::fan::engine(&world, None);
    let id = put(&world, band.clone());
    let made = ran(&engine, &Call::new("profile").input("band", id))[0].1;
    let stored = world.mask(made);
    assert_eq!((stored.core(), stored.blend()), (mask.core(), mask.blend()));
    let wider = Call::new("profile")
        .argument("feather_ratio", Value::Number(1.0))
        .input("band", id);
    let wider = world.mask(ran(&engine, &wider)[0].1);
    assert!(count(wider.blend()) > 1258);
}
