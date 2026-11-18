//! `plug` (plan 2A, task R2): a hole is filled when its area is at most the share of its
//! owner's, compared exactly; a hole that touches the image's edge never is; protect
//! wins; and the refinement is cleared.

use whizbang_rascal::plug::{at_most_share, plug, plugged};

use super::grow::{mask_of, plane};

fn never() -> bool {
    false
}

/// A ring: the square of side `outer` at (2, 2), with a hole of side `inner` in its
/// middle, in an image `size` square.
fn ring(size: usize, outer: usize, inner: usize) -> Vec<u8> {
    let first = 2 + (outer - inner) / 2;
    plane(size, size, |x, y| {
        let in_outer = (2..2 + outer).contains(&x) && (2..2 + outer).contains(&y);
        let in_hole = (first..first + inner).contains(&x) && (first..first + inner).contains(&y);
        in_outer && !in_hole
    })
}

#[test]
fn a_hole_up_to_its_share_of_its_owner_is_filled_and_a_larger_one_is_not() {
    // A 10 by 10 square with a 4 by 4 hole: the ring is 84 pixels, the hole 16, a share
    // of 0.1904...: filled at 0.25, kept at 0.19 and at rascal's default of 0.1.
    let core = ring(16, 10, 4);
    let square = plane(16, 16, |x, y| (2..12).contains(&x) && (2..12).contains(&y));
    let filled = plugged(&core, 16, 16, 0.25, &never).unwrap();
    assert_eq!((filled.found, filled.filled), (1, 1));
    assert_eq!(filled.core, square);
    for share in [0.19, 0.1] {
        let kept = plugged(&core, 16, 16, share, &never).unwrap();
        assert_eq!((kept.found, kept.filled), (1, 0), "{share}");
        assert_eq!(kept.core, core);
    }
    // On the bound: a 6 by 6 square with a 2 by 2 hole is a ring of 32 and a hole of 4,
    // a share of exactly 0.125. That fills it; the double just under does not.
    let small = ring(10, 6, 2);
    assert_eq!(plugged(&small, 10, 10, 0.125, &never).unwrap().filled, 1);
    let under = f64::from_bits(0.125_f64.to_bits() - 1);
    assert_eq!(plugged(&small, 10, 10, under, &never).unwrap().filled, 0);

    // The comparison in whole numbers: a share of a half holds 2 of 4 and not 3 of 4;
    // 0.1 as a double is over a tenth, so it holds 1 of 10, and 0.3 is under 3/10.
    assert!(at_most_share(2, 4, 0.5) && !at_most_share(3, 4, 0.5));
    assert!(at_most_share(1, 10, 0.1));
    assert!(!at_most_share(3, 10, 0.3));
    assert!(at_most_share(0, 0, 0.0) && !at_most_share(1, 100, 0.0));
    assert!(at_most_share(u64::MAX, 1, 1e300) && !at_most_share(u64::MAX, 1, 5e-324));

    // On a mask: the hole filled, its blend cleared, protect kept and still winning
    // over a protected pixel of the hole, and one more step in its origin.
    let mut protect = vec![0u8; 256];
    protect[6 * 16 + 6] = 255;
    let mask = mask_of(16, 16, core.clone(), &protect);
    let from = mask.id();
    let (out, said) = plug(mask, 0.25, &never).unwrap().unwrap();
    assert_eq!(said.filled, 1);
    let mut expected = square.clone();
    expected[6 * 16 + 6] = 0;
    assert_eq!(out.core(), expected.as_slice());
    assert_eq!(out.protect(), protect.as_slice());
    let last = out.origin().steps().last().unwrap();
    assert_eq!(
        *last,
        whizbang_core::artifact::Step::new("plug", "1.0.0")
            .argument("max_share", "0.25")
            .input("mask", from.0)
    );
}

#[test]
fn a_hole_touching_the_edge_is_not_filled() {
    // A U against the bottom edge: its middle is open to the edge, so it is no hole,
    // whatever the share. A gap joined to the edge only through a corner is not open
    // to it: gaps join through sides alone.
    let (w, h) = (10, 8);
    let u = plane(w, h, |x, y| {
        (2..8).contains(&x) && y >= 2 && !((4..6).contains(&x) && y >= 4)
    });
    let out = plugged(&u, w, h, 1.0, &never).unwrap();
    assert_eq!((out.found, out.filled), (0, 0));
    assert_eq!(out.core, u);
    // The same U with a pixel of core closing it at the bottom row: now a hole.
    let closed = plane(w, h, |x, y| {
        (2..8).contains(&x) && y >= 2 && !((4..6).contains(&x) && (4..7).contains(&y))
    });
    let out = plugged(&closed, w, h, 1.0, &never).unwrap();
    assert_eq!((out.found, out.filled), (1, 1));
    // A hole at the image's first column is open to the edge.
    let left = plane(w, h, |x, y| {
        x < 4 && (1..6).contains(&y) && !(x == 0 && y == 3)
    });
    let out = plugged(&left, w, h, 1.0, &never).unwrap();
    assert_eq!(out.filled, 0);
    // And plugging stops when told to.
    assert!(plugged(&u, w, h, 1.0, &|| true).is_none());
}

#[test]
fn a_hole_of_exactly_its_share_is_filled() {
    // A frame one pixel wide, 6 across and 8 down, around a hole 4 by 6: the frame is
    // 24 pixels and the hole 24, a share of exactly 1. That fills it; the double just
    // under 1 does not.
    let (w, h) = (8, 10);
    let frame = plane(w, h, |x, y| {
        let outer = (1..7).contains(&x) && (1..9).contains(&y);
        let hole = (2..6).contains(&x) && (2..8).contains(&y);
        outer && !hole
    });
    let full = plane(w, h, |x, y| (1..7).contains(&x) && (1..9).contains(&y));
    let out = plugged(&frame, w, h, 1.0, &never).unwrap();
    assert_eq!((out.found, out.filled), (1, 1));
    assert_eq!(out.core, full);
    let under = f64::from_bits(1.0_f64.to_bits() - 1);
    assert_eq!(plugged(&frame, w, h, under, &never).unwrap().filled, 0);
}
