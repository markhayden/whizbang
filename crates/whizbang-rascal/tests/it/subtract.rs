//! `subtract` of a mask (plan m3, task R1, item 4): the second mask's support, its core or
//! its blend, grown by `grow_px`, joins protect, and the core and the blend are cleared
//! there; with no second mask the mask is handed back unchanged. The vectors are rascal's
//! own (`crates/rascal-ops/tests/it/subtract_select_invert.rs` at the commit
//! `docs/sources.toml` pins: `subtract_protects_from_a_mask_or_a_location_grown_and_keeps_the_profile`
//! and `subtract_refuses_another_frame_and_a_grown_box_clips_at_the_edge`, the forms with
//! a mask), on rascal's picture of 100 by 60, held to rascal's pixels.

use whizbang_core::artifact::{Mask, Source, Step, UseCase};
use whizbang_core::identity::{Binding, Digest, PixelDigest, Space};
use whizbang_rascal::subtract::{NAME, SubtractError, VERSION, subtract};

use super::grow::{made_by, mask_of, plane};

const W: usize = 100;
const H: usize = 60;

fn never() -> bool {
    false
}

fn in_rect(x: usize, y: usize, r: [usize; 4]) -> bool {
    (r[0]..r[1]).contains(&x) && (r[2]..r[3]).contains(&y)
}

/// Rascal's band: a core of the columns 40 to 59 and a blend on either side of it.
fn band() -> Mask {
    let mut mask = mask_of(W, H, plane(W, H, |x, _| (40..60).contains(&x)), &[0; W * H]);
    mask.set_blend(plane(W, H, |x, _| {
        (36..40).contains(&x) || (60..64).contains(&x)
    }))
    .unwrap();
    mask
}

fn count(plane: &[u8]) -> usize {
    plane.iter().filter(|s| **s != 0).count()
}

#[test]
fn the_ported_vectors_of_a_mask_give_rascals_masks() {
    let mask = band();
    let before = count(mask.protect());
    // From a mask: its core and its blend both count as support; grown by 2.
    let mut other = mask_of(
        W,
        H,
        plane(W, H, |x, y| in_rect(x, y, [45, 52, 10, 20])),
        &[0; W * H],
    );
    other
        .set_blend(plane(W, H, |x, y| in_rect(x, y, [52, 55, 10, 20])))
        .unwrap();
    let made = subtract(mask.clone(), Some(&other), 2.0, &never)
        .unwrap()
        .unwrap();
    let m = &made.mask;
    let p = |x: usize, y: usize| m.protect()[y * W + x];
    assert!(count(m.protect()) > before);
    assert_eq!(
        (made.protect_before as usize, made.protect_after as usize),
        (before, count(m.protect()))
    );
    // Protect covers the support grown by 2 (a point 2 px right of the blend's edge is in,
    // 3 px is out), and wins: core and blend are 0 there.
    assert!(p(56, 15) != 0 && p(58, 15) == 0);
    assert!(p(43, 15) != 0 && p(42, 15) == 0);
    for (at, protect) in m.protect().iter().enumerate() {
        if *protect != 0 {
            assert_eq!((m.core()[at], m.blend()[at]), (0, 0), "{at}");
        }
    }
    // Outside the new protect, core and blend are untouched.
    assert_eq!(m.core()[40 * W + 50], 255);
    assert_eq!(m.blend()[40 * W + 37], mask.blend()[40 * W + 37]);
    assert_eq!(
        m.origin().steps().last().unwrap(),
        &Step::new(NAME, VERSION)
            .argument("grow_px", "2")
            .input("mask", mask.id().0)
            .input("subtract", other.id().0)
    );

    // Against the right edge, grown by 3: protect reaches the last column and no further.
    let full = mask_of(W, H, plane(W, H, |_, _| true), &[0; W * H]);
    let edge = mask_of(
        W,
        H,
        plane(W, H, |x, y| in_rect(x, y, [90, 100, 24, 36])),
        &[0; W * H],
    );
    let m = subtract(full, Some(&edge), 3.0, &never)
        .unwrap()
        .unwrap()
        .mask;
    assert_ne!(m.protect()[30 * W + W - 1], 0);
    assert_ne!(m.protect()[30 * W + 87], 0, "3 px left of the box");
    assert_eq!(m.protect()[30 * W + 86], 0);

    // A second mask on another image is refused, as rascal refused one in another frame.
    let elsewhere = Mask::from_core(
        Binding::root(
            PixelDigest(Digest::of_bytes(b"another")),
            Space::flat(W as u32, H as u32).unwrap(),
        ),
        UseCase::Remove,
        plane(W, H, |x, _| x < 10),
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    let refused = subtract(band(), Some(&elsewhere), 0.0, &never);
    assert!(
        matches!(refused, Err(SubtractError::Invalid(_))),
        "{refused:?}"
    );
}

#[test]
fn with_no_mask_to_subtract_the_mask_is_handed_back_unchanged() {
    let mask = band();
    let made = subtract(mask.clone(), None, 2.0, &never).unwrap().unwrap();
    // The same artifact: the same identity, and the same origin.
    assert_eq!(made.mask, mask);
    assert_eq!(made.mask.id(), mask.id());
    assert_eq!(made.protect_before, made.protect_after);
}
