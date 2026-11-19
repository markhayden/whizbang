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

// --- Rascal's vectors -------------------------------------------------------------------------

/// Rascal's rule written out plainly, apart from the code (rascal's
/// `crates/rascal-ops/tests/it/plug.rs`, `reference`, at the commit `docs/sources.toml`
/// pins): flood every region, judge every hole from the labels of the core given, then
/// fill. The share is compared as rascal compared it, as doubles multiply.
fn reference(core: &[u8], w: usize, h: usize, max_share: f64) -> Vec<u8> {
    let on = |x: i64, y: i64| {
        x >= 0 && y >= 0 && x < w as i64 && y < h as i64 && core[y as usize * w + x as usize] != 0
    };
    let flood = |want: bool, eight: bool| {
        let (mut label, mut areas) = (vec![0usize; w * h], vec![0usize]);
        for start in 0..w * h {
            let (sx, sy) = ((start % w) as i64, (start / w) as i64);
            if on(sx, sy) != want || label[start] != 0 {
                continue;
            }
            let id = areas.len();
            areas.push(0);
            label[start] = id;
            let mut queue = std::collections::VecDeque::from([(sx, sy)]);
            while let Some((x, y)) = queue.pop_front() {
                areas[id] += 1;
                for (dx, dy) in (-1i64..=1).flat_map(|dy| (-1i64..=1).map(move |dx| (dx, dy))) {
                    let (nx, ny) = (x + dx, y + dy);
                    let skip = (dx == 0 && dy == 0) || (!eight && dx != 0 && dy != 0);
                    if skip || nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let k = ny as usize * w + nx as usize;
                    if on(nx, ny) == want && label[k] == 0 {
                        label[k] = id;
                        queue.push_back((nx, ny));
                    }
                }
            }
        }
        (label, areas)
    };
    let (fg, fg_areas) = flood(true, true);
    let (bg, bg_areas) = flood(false, false);
    let mut out: Vec<u8> = core.iter().map(|s| if *s != 0 { 255 } else { 0 }).collect();
    for hole in 1..bg_areas.len() {
        let pixels: Vec<(i64, i64)> = (0..w * h)
            .filter(|k| bg[*k] == hole)
            .map(|k| ((k % w) as i64, (k / w) as i64))
            .collect();
        let edge =
            |(x, y): &(i64, i64)| *x == 0 || *y == 0 || *x == w as i64 - 1 || *y == h as i64 - 1;
        if pixels.iter().any(edge) {
            continue;
        }
        let mut owner: Option<usize> = None;
        for (x, y) in &pixels {
            for (dx, dy) in (-1i64..=1).flat_map(|dy| (-1i64..=1).map(move |dx| (dx, dy))) {
                if on(x + dx, y + dy) {
                    let id = fg[(y + dy) as usize * w + (x + dx) as usize];
                    owner = Some(match owner {
                        Some(o)
                            if fg_areas[o] > fg_areas[id]
                                || (fg_areas[o] == fg_areas[id] && o < id) =>
                        {
                            o
                        }
                        _ => id,
                    });
                }
            }
        }
        if pixels.len() as f64 <= max_share * fg_areas[owner.unwrap()] as f64 {
            for (x, y) in pixels {
                out[y as usize * w + x as usize] = 255;
            }
        }
    }
    out
}

#[test]
fn the_ported_vectors_give_rascals_cores() {
    // Rascal's own cases (`crates/rascal-ops/tests/it/plug.rs` at the pinned commit), on
    // its picture of 80 by 80, with its numbers.
    const S: usize = 80;
    let rect =
        |x: usize, y: usize, r: [usize; 4]| (r[0]..r[1]).contains(&x) && (r[2]..r[3]).contains(&y);
    let count = |p: &[u8]| p.iter().filter(|s| **s != 0).count();
    let run = |core: &[u8], share: f64| plugged(core, S, S, share, &never).unwrap();
    // A small hole in a ring is filled at rascal's default share.
    let ring = plane(S, S, |x, y| {
        rect(x, y, [20, 60, 20, 60]) && !rect(x, y, [38, 42, 38, 42])
    });
    let out = run(&ring, 0.1);
    assert_eq!((count(&out.core), out.found, out.filled), (1600, 1, 1));
    // A frame keeps its large hole; a hole touching the edge is no hole.
    let frame = plane(S, S, |x, y| {
        rect(x, y, [10, 70, 10, 70]) && !rect(x, y, [14, 66, 14, 66])
    });
    let out = run(&frame, 0.1);
    assert_eq!(
        (out.core.clone(), out.found, out.filled),
        (frame.clone(), 1, 0)
    );
    assert_eq!(
        run(&frame, 1.0).core,
        frame,
        "the hole, 2704, is larger than the frame, 896"
    );
    let edge = plane(S, S, |x, y| {
        rect(x, y, [0, 30, 0, 30]) && !rect(x, y, [0, 5, 10, 15])
    });
    assert_eq!(
        (run(&edge, 0.1).core, run(&edge, 0.1).found),
        (edge.clone(), 0)
    );
    // A pocket open only across a corner is a hole; one open along a side is not.
    let diagonal = plane(S, S, |x, y| {
        rect(x, y, [5, 25, 5, 25]) && (x, y) != (5, 5) && (x, y) != (6, 6)
    });
    let out = run(&diagonal, 0.1);
    assert_eq!(
        (out.filled, out.core[6 * S + 6], out.core[5 * S + 5]),
        (1, 255, 0)
    );
    let open = plane(S, S, |x, y| {
        rect(x, y, [5, 25, 5, 25]) && ![(5, 5), (6, 6), (6, 5)].contains(&(x, y))
    });
    assert_eq!(
        (run(&open, 0.1).found, run(&open, 0.1).core[6 * S + 6]),
        (0, 0)
    );
    // Merged blobs with several holes, and a small blob whose hole is judged against its
    // own area: as the reference, at every share rascal tried.
    let merged = plane(S, S, |x, y| {
        let big = rect(x, y, [5, 45, 5, 45]) || rect(x, y, [30, 70, 30, 70]);
        let small = rect(x, y, [55, 75, 2, 22]);
        let holes = rect(x, y, [10, 12, 10, 12])
            || rect(x, y, [20, 40, 20, 26])
            || rect(x, y, [50, 62, 50, 62])
            || rect(x, y, [62, 66, 8, 14]);
        (big || small) && !holes
    });
    for share in [0.0, 0.05, 0.1, 0.3, 1.0] {
        assert_eq!(
            run(&merged, share).core,
            reference(&merged, S, S, share),
            "{share}"
        );
    }
    // A share of 0 fills nothing; a protected hole stays empty.
    assert_eq!(count(&run(&ring, 0.0).core), 1600 - 16);
    let hole = plane(S, S, |x, y| rect(x, y, [38, 42, 38, 42]));
    let (mask, _) = plug(mask_of(S, S, ring, &hole), 0.1, &never)
        .unwrap()
        .unwrap();
    assert_eq!(
        (count(mask.core()), mask.core()[39 * S + 39]),
        (1600 - 16, 0)
    );
}
