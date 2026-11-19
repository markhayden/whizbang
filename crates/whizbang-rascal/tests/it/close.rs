//! `close` (plan m3, task R1, item 4): the core grown and then eroded by one disc, outside
//! the image counting as grown. The vectors are rascal's own
//! (`crates/rascal-ops/tests/it/grow_shrink_close.rs` at the commit `docs/sources.toml`
//! pins: `close_fills_a_gap_narrower_than_the_disc_and_keeps_the_extent` and
//! `close_keeps_the_extent_at_the_image_border_and_shrink_erodes_from_it`), on rascal's
//! picture of 120 by 80, held to rascal's pixels; and a reference by brute force over the
//! disc, apart from the code.

use whizbang_core::artifact::{Refinement, Step};
use whizbang_rascal::close::{NAME, VERSION, close, closed, eroded_within};

use super::grow::{mask_of, plane};

const W: usize = 120;
const H: usize = 80;

fn never() -> bool {
    false
}

/// True inside the half-open rectangle `[x0, x1) x [y0, y1)`, as rascal's test has it.
fn in_rect(x: usize, y: usize, r: [usize; 4]) -> bool {
    (r[0]..r[1]).contains(&x) && (r[2]..r[3]).contains(&y)
}

fn at(plane: &[u8], x: usize, y: usize) -> u8 {
    plane[y * W + x]
}

/// Closing by brute force over the disc: grown where a set pixel is within `r`, then kept
/// where every pixel of the image within `r` is grown.
fn brute_close(core: &[u8], r: f64) -> Vec<u8> {
    let reach = r.floor() as i64;
    let near = |x: usize, y: usize, hit: &dyn Fn(usize, usize) -> bool| {
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                if (dx * dx + dy * dy) as f64 > r * r {
                    continue;
                }
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if (0..W as i64).contains(&nx)
                    && (0..H as i64).contains(&ny)
                    && hit(nx as usize, ny as usize)
                {
                    return true;
                }
            }
        }
        false
    };
    let grown: Vec<u8> = (0..W * H)
        .map(|i| {
            if near(i % W, i / W, &|x, y| core[y * W + x] != 0) {
                255
            } else {
                0
            }
        })
        .collect();
    (0..W * H)
        .map(|i| {
            let outside = near(i % W, i / W, &|x, y| grown[y * W + x] == 0);
            if outside { 0 } else { 255 }
        })
        .collect()
}

#[test]
fn the_ported_vectors_give_rascals_masks() {
    // Two blocks 4 px apart, and a 2 px notch bitten from one of them.
    let blob = plane(W, H, |x, y| {
        let left = in_rect(x, y, [30, 50, 20, 60]);
        let right = in_rect(x, y, [54, 74, 20, 60]);
        let notch = in_rect(x, y, [38, 40, 20, 24]);
        (left || right) && !notch
    });
    let shut = closed(&blob, W, H, 3.0, &never).unwrap();
    assert_eq!(shut, brute_close(&blob, 3.0));
    assert_eq!(at(&shut, 51, 40), 255, "the gap filled");
    assert_eq!(at(&shut, 39, 21), 255, "the notch filled");
    assert_eq!(
        (at(&shut, 28, 40), at(&shut, 76, 40)),
        (0, 0),
        "the extent kept"
    );
    // The gap is wider than the disc: it stays.
    let open = closed(&blob, W, H, 1.5, &never).unwrap();
    assert_eq!(at(&open, 52, 40), 0);
    assert_eq!(open, brute_close(&blob, 1.5));

    // A box in the top-left corner: closing changes nothing, its border included.
    let corner = plane(W, H, |x, y| x < 50 && y < 40);
    assert_eq!(closed(&corner, W, H, 3.0, &never).unwrap(), corner);
    // A strip of 2 px along the top edge survives closing by 3: outside counts as set.
    let strip = plane(W, H, |_, y| y < 2);
    assert_eq!(closed(&strip, W, H, 3.0, &never).unwrap(), strip);
    // The erosion alone, in its convention: the corner box loses a margin along its own
    // edges and none along the image's.
    let eroded = eroded_within(&corner, W, H, 3.0, &never).unwrap();
    assert_eq!(
        (at(&eroded, 0, 0), at(&eroded, 46, 20), at(&eroded, 47, 20)),
        (255, 255, 0)
    );
    assert_eq!((at(&eroded, 20, 36), at(&eroded, 20, 37)), (255, 0));
    // Rascal's blob of radius 10, at every radius its test tried.
    let disc = plane(W, H, |x, y| {
        (x as i64 - 60).pow(2) + (y as i64 - 40).pow(2) <= 100
    });
    for r in [0.0, 1.0, 2.5, 7.0] {
        assert_eq!(
            closed(&disc, W, H, r, &never).unwrap(),
            brute_close(&disc, r),
            "{r}"
        );
    }

    // On a mask: a refinement, so the blend is cleared and protect is kept and wins; and
    // one more step in its origin. A protected pixel inside the gap stays out of the core.
    let mut protect = vec![0u8; W * H];
    protect[40 * W + 51] = 255;
    let mask = mask_of(W, H, blob.clone(), &protect);
    let from = mask.id();
    let made = close(mask, 3.0, &never).unwrap().unwrap();
    let mut expected = brute_close(&blob, 3.0);
    expected[40 * W + 51] = 0;
    assert_eq!(made.mask.core(), expected.as_slice());
    assert_eq!(made.mask.protect(), protect.as_slice());
    assert!(made.mask.blend().iter().all(|b| *b == 0));
    assert_eq!(
        made.cleared,
        Refinement {
            blend_cleared: true,
            profile_cleared: false
        }
    );
    assert_eq!(
        made.mask.origin().steps().last().unwrap(),
        &Step::new(NAME, VERSION)
            .argument("radius_px", "3")
            .input("mask", from.0)
    );
    // Told to stop, it stops.
    assert!(
        close(mask_of(W, H, blob, &protect), 3.0, &|| true)
            .unwrap()
            .is_none()
    );
}
