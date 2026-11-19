//! What rascal's `mask.rs` held beside the record (plan m3, task O1, item 2): the counts
//! per zone, the box of the pixels that weigh more than 0, and whether a mask is empty.
//! The vector is rascal's own (`crates/rascal-core/tests/it/mask.rs`,
//! `the_weight_rule_and_protect_wins`, at the commit `docs/sources.toml` pins): on a
//! picture of 40 by 30, a core from (10, 10) to (20, 20), a blend of 51 at (5, 5) and of
//! 200 at (15, 15), and then (15, 15) and (5, 5) protected.

use whizbang_core::artifact::{Mask, Source, UseCase, ZoneStatistics};
use whizbang_core::identity::{Binding, Digest, PixelDigest, Space};

use super::support::made_by;

const W: usize = 40;
const H: usize = 30;

fn plane(set: &[(usize, usize, u8)]) -> Vec<u8> {
    let mut plane = vec![0; W * H];
    for (x, y, value) in set {
        plane[y * W + x] = *value;
    }
    plane
}

fn rascals() -> Mask {
    let binding = Binding::root(
        PixelDigest(Digest::of_bytes(b"forty by thirty")),
        Space::flat(W as u32, H as u32).unwrap(),
    );
    let core = (0..W * H)
        .map(|at| {
            let (x, y) = (at % W, at / W);
            if (10..20).contains(&x) && (10..20).contains(&y) {
                255
            } else {
                0
            }
        })
        .collect();
    let mut mask = Mask::from_core(
        binding,
        UseCase::Custom,
        core,
        Source::default(),
        made_by("rect"),
    )
    .unwrap();
    mask.set_blend(plane(&[(5, 5, 51), (15, 15, 200)])).unwrap();
    assert_eq!(
        (mask.weight(15, 15), mask.weight(5, 5)),
        (Some(1.0), Some(0.2))
    );
    mask.protect_union(&plane(&[(15, 15, 255), (5, 5, 255)]))
        .unwrap();
    mask
}

#[test]
fn counts_per_zone_and_the_box_of_weighted_pixels_are_rascals() {
    let mask = rascals();
    // Rascal's two numbers.
    assert_eq!(mask.statistics().protect, 2);
    assert_eq!(mask.support(), Some([10, 10, 20, 20]));
    // The rest of the counts: the core's hundred less the protected pixel, no blend left
    // (both blended pixels are protected or in the core), and every core pixel weighs 1.
    assert_eq!(
        mask.statistics(),
        ZoneStatistics {
            width: 40,
            height: 30,
            core: 99,
            blend: 0,
            protect: 2,
            painted: 99,
            weight_sum: 99.0,
            support: Some([10, 10, 20, 20]),
        }
    );
    assert!(!mask.is_empty());
    // A blended pixel outside the core widens the box and adds its weight, as a 32-bit
    // float.
    let mut blended = mask.clone();
    blended.set_blend(plane(&[(25, 27, 51)])).unwrap();
    let counts = blended.statistics();
    assert_eq!(counts.support, Some([10, 10, 26, 28]));
    assert_eq!((counts.blend, counts.painted), (1, 100));
    assert_eq!(counts.weight_sum, 99.0 + f64::from(51.0_f32 / 255.0));
    // Everything protected: nothing weighs anything, and there is no box.
    let mut covered = mask;
    covered.protect_union(&vec![255; W * H]).unwrap();
    assert!(covered.is_empty());
    assert_eq!(covered.support(), None);
    let counts = covered.statistics();
    assert_eq!(
        (
            counts.core,
            counts.protect,
            counts.painted,
            counts.weight_sum
        ),
        (0, 1200, 0, 0.0)
    );
}
