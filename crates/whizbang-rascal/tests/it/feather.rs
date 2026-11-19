//! `feather` (plan 2A, task R2): the blend is `255 * exp(-d^2 / 2 sigma^2)` at the
//! distance `d` from the core's edge, out to three sigmas; it falls off outward, is wider
//! with a wider sigma, is 0 in the core and under protect, and the distances are exact.
//!
//! The expected numbers were worked apart from the code, with Python's `math.exp`, for a
//! point of core: `k` is the squared distance between centres and `d` is `sqrt(k) - 0.5`.

use whizbang_rascal::feather::{blend_at, exp_neg, feather, squared_distances};

use super::grow::{mask_of, plane};

fn never() -> bool {
    false
}

#[test]
fn the_blend_falls_off_outward_with_sigma_and_protect_wins() {
    // Sigma 2: out to d = 6, so to k = 42 and not 43.
    for (k, blend) in [
        (1, 247),
        (2, 230),
        (4, 192),
        (9, 117),
        (36, 6),
        (42, 3),
        (43, 0),
    ] {
        assert_eq!(blend_at(k, 2.0), blend, "k {k}");
    }
    // Sigma 4: wider.
    for (k, blend) in [(1, 253), (4, 238), (9, 210)] {
        assert_eq!(blend_at(k, 4.0), blend, "k {k}");
    }
    // The exponential is this crate's, and agrees with the platform's to within 8 units
    // in the last place; over every distance a feather can reach, the bytes are the same.
    for x in 0..=4500 {
        let x = f64::from(x) / 1000.0;
        let (ours, theirs) = (exp_neg(x), (-x).exp());
        assert!((ours - theirs).abs() <= 8.0 * f64::EPSILON * theirs, "{x}");
    }

    // A point of core in the middle of a 21 by 21 image, and one pixel protected two to
    // its right.
    let (w, h, c) = (21, 21, 10);
    let mut protect = vec![0u8; w * h];
    protect[c * w + c + 2] = 255;
    let mask = mask_of(w, h, plane(w, h, |x, y| (x, y) == (c, c)), &protect);
    let from = mask.id();
    let out = feather(mask, 2.0, &never).unwrap().unwrap();
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as i64 - c as i64, y as i64 - c as i64);
            let k = (dx * dx + dy * dy) as u64;
            let expected = if (x, y) == (c, c) || protect[y * w + x] != 0 {
                0
            } else {
                blend_at(k, 2.0)
            };
            assert_eq!(out.blend()[y * w + x], expected, "({x}, {y})");
        }
    }
    // Outward along the row it falls: 247 beside the core, then (protected), 117, 55, 20 and 6.
    let row: Vec<u8> = (c + 1..w).map(|x| out.blend()[c * w + x]).collect();
    assert_eq!(row[..6], [247, 0, 117, 55, 20, 6]);
    assert_eq!(out.core()[c * w + c], 255);
    assert_eq!(out.protect(), protect.as_slice());
    let last = out.origin().steps().last().unwrap();
    assert_eq!(
        *last,
        whizbang_core::artifact::Step::new("feather", "1.0.0")
            .argument("sigma_px", "2")
            .input("mask", from.0)
    );
}

#[test]
fn the_squared_distances_are_exact_and_none_without_a_core() {
    let (w, h) = (23, 17);
    let core = plane(w, h, |x, y| (x * 7 + y * 3) % 19 == 0 && x % 3 != 1);
    let found = squared_distances(&core, w, h, &never).unwrap();
    let set: Vec<(i64, i64)> = (0..w * h)
        .filter(|at| core[*at] != 0)
        .map(|at| ((at % w) as i64, (at / w) as i64))
        .collect();
    for (at, found) in found.iter().enumerate() {
        let (x, y) = ((at % w) as i64, (at / w) as i64);
        let nearest = set
            .iter()
            .map(|(a, b)| ((a - x).pow(2) + (b - y).pow(2)) as u64)
            .min();
        assert_eq!(*found, nearest, "({x}, {y})");
    }
    let empty = squared_distances(&vec![0; w * h], w, h, &never).unwrap();
    assert!(empty.iter().all(Option::is_none));
    assert!(squared_distances(&core, w, h, &|| true).is_none());
}

#[test]
fn protect_is_kept_where_the_blend_would_lower_it() {
    // A point of core with a protected pixel on its right, where the feather would put
    // 247. Protect wins there: the pixel stays protected and its blend is 0, while the
    // pixel on the core's left, which is not protected, gets the 247.
    let (w, h, c) = (5, 5, 2);
    let mut protect = vec![0u8; w * h];
    protect[c * w + c + 1] = 255;
    let mask = mask_of(w, h, plane(w, h, |x, y| (x, y) == (c, c)), &protect);
    let out = feather(mask, 2.0, &never).unwrap().unwrap();
    assert_eq!(out.protect()[c * w + c + 1], 255);
    assert_eq!(out.blend()[c * w + c + 1], 0);
    assert_eq!(out.blend()[c * w + c - 1], 247);
    assert_eq!(out.protect(), protect.as_slice());
}

#[test]
fn the_ported_vectors_give_rascals_blends() {
    // Rascal's blob (`crates/rascal-ops/tests/it/profile_feather.rs`,
    // `feather_on_a_blob_matches_the_brute_force_and_keeps_protect_and_the_profile`, at the
    // commit `docs/sources.toml` pins): an ellipse and a corner patch on 40 by 30, its last
    // two rows protected, sigma 2.5. Every blend is rascal's brute force: the distance to
    // the nearest core pixel's edge over every core pixel, and the platform's `exp`.
    let (w, h) = (40usize, 30usize);
    let core = plane(w, h, |x, y| {
        let (dx, dy) = (x as f64 - 19.0, y as f64 - 14.0);
        dx * dx / 64.0 + dy * dy / 25.0 <= 1.0 || (x > 30 && y < 6)
    });
    let protect = plane(w, h, |_, y| y >= 28);
    let mask = feather(mask_of(w, h, core.clone(), &protect), 2.5, &never)
        .unwrap()
        .unwrap();
    assert_eq!(mask.protect(), &protect[..]);
    let edge = |x: usize, y: usize| {
        let near = (0..w * h).filter(|at| mask.core()[*at] != 0).map(|at| {
            let (cx, cy) = ((at % w) as f64, (at / w) as f64);
            ((cx - x as f64).powi(2) + (cy - y as f64).powi(2)).sqrt()
        });
        near.fold(f64::INFINITY, f64::min) - 0.5
    };
    let mut painted = 0;
    for y in 0..h {
        for x in 0..w {
            let at = y * w + x;
            let d = edge(x, y);
            let expected = if mask.core()[at] != 0 || protect[at] != 0 || !(d > 0.0 && d <= 7.5) {
                0
            } else {
                (255.0 * (-d * d / 12.5).exp()).round() as u8
            };
            assert_eq!(mask.blend()[at], expected, "({x}, {y}) at {d}");
            painted += usize::from(expected != 0);
        }
    }
    assert!(painted > 100, "{painted}");
    // Rascal's contour: walking out from a straight edge, the blend crosses half between
    // the last pixel at or over 128 and the first under it, within half a pixel of
    // 1.1774 sigma, and is 0 past three sigmas.
    let sigma = 4.0;
    let straight = feather(
        mask_of(60, 4, plane(60, 4, |x, _| x < 20), &[0; 240]),
        sigma,
        &never,
    )
    .unwrap()
    .unwrap();
    let row: Vec<f64> = (0..60).map(|x| f64::from(straight.blend()[x])).collect();
    let over = (20..60).rev().find(|x| row[*x] >= 128.0).unwrap();
    let (da, db) = (over as f64 - 19.5, over as f64 - 18.5);
    let (va, vb) = (row[over], row[over + 1]);
    let half = da + (va - 127.5) / (va - vb) * (db - da);
    assert!(
        (half - sigma * (2.0 * 2.0f64.ln()).sqrt()).abs() <= 0.5,
        "{half}"
    );
    assert!(row[20 + 12..].iter().all(|v| *v == 0.0));
}
