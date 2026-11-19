//! `shrink` (plan m3, task O1, item 1). The vectors are rascal's own
//! (`crates/rascal-ops/tests/it/grow_shrink_close.rs` at the commit `docs/sources.toml`
//! pins: `shrink_matches_brute_force_and_a_core_eroded_to_nothing_is_not_applicable` and
//! `close_keeps_the_extent_at_the_image_border_and_shrink_erodes_from_it`), on rascal's
//! picture of 120 by 80, held to rascal's pixels and to rascal's reference by brute force
//! over the disc.

use whizbang_core::artifact::{Refinement, Step};
use whizbang_rascal::shrink::{NAME, ShrinkError, VERSION, eroded, reach, shrink};

use super::grow::{mask_of, plane};

const W: usize = 120;
const H: usize = 80;

fn never() -> bool {
    false
}

fn at(plane: &[u8], x: usize, y: usize) -> u8 {
    plane[y * W + x]
}

/// Rascal's reference: a pixel survives when every pixel within the disc of `r` is set,
/// and a pixel of the disc outside the image is not.
fn brute_erode(core: &[u8], r: f64) -> Vec<u8> {
    let reach = r.floor() as i64;
    (0..W * H)
        .map(|i| {
            let (x, y) = ((i % W) as i64, (i / W) as i64);
            if core[i] == 0 {
                return 0;
            }
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    if (dx * dx + dy * dy) as f64 > r * r {
                        continue;
                    }
                    let (nx, ny) = (x + dx, y + dy);
                    let inside = (0..W as i64).contains(&nx) && (0..H as i64).contains(&ny);
                    if !inside || core[ny as usize * W + nx as usize] == 0 {
                        return 0;
                    }
                }
            }
            255
        })
        .collect()
}

#[test]
fn the_ported_vectors_give_rascals_masks() {
    // A disc of radius 12 about (50, 40), shrunk by 4.
    let blob = plane(W, H, |x, y| {
        (x as i64 - 50).pow(2) + (y as i64 - 40).pow(2) <= 144
    });
    let shrunk = eroded(&blob, W, H, 4.0, &never).unwrap();
    assert_eq!(shrunk, brute_erode(&blob, 4.0));
    assert_eq!(
        (
            at(&shrunk, 50, 40),
            at(&shrunk, 50, 32),
            at(&shrunk, 50, 31)
        ),
        (255, 255, 0)
    );
    for r in [0.0, 1.0, 2.5, 7.0] {
        assert_eq!(
            eroded(&blob, W, H, r, &never).unwrap(),
            brute_erode(&blob, r),
            "{r}"
        );
    }
    // By 13 nothing is left: refused, as rascal said it did not apply.
    let protect = vec![0u8; W * H];
    match shrink(mask_of(W, H, blob.clone(), &protect), 13.0, &never) {
        Err(ShrinkError::Nothing { px, before }) => {
            assert_eq!(px, 13.0);
            assert_eq!(before as usize, blob.iter().filter(|s| **s != 0).count());
        }
        other => panic!("{other:?}"),
    }
    // A strip of 2 px along the top edge erodes to nothing by 3; a box in the corner
    // loses a margin along the image's border and along its own edge.
    let strip = plane(W, H, |_, y| y < 2);
    assert!(
        eroded(&strip, W, H, 3.0, &never)
            .unwrap()
            .iter()
            .all(|s| *s == 0)
    );
    let corner = plane(W, H, |x, y| x < 50 && y < 40);
    let shrunk = eroded(&corner, W, H, 3.0, &never).unwrap();
    assert_eq!(at(&shrunk, 0, 20), 0, "eroded from the left border");
    assert_eq!(at(&shrunk, 3, 20), 255);
    assert_eq!(at(&shrunk, 46, 20), 255);
    assert_eq!(at(&shrunk, 47, 20), 0, "eroded from the box's own edge");
    let brute = brute_erode(&corner, 3.0);
    let differ: Vec<(usize, usize, u8)> = (0..W * H)
        .filter(|i| shrunk[*i] != brute[*i])
        .map(|i| (i % W, i / W, shrunk[i]))
        .take(8)
        .collect();
    assert!(differ.is_empty(), "{differ:?}");
    // How far the disc reaches along a row.
    assert_eq!(
        [0.0, 0.9, 1.0, 2.5, 3.0, 3.99, 4.0].map(reach),
        [0, 0, 1, 2, 3, 3, 4]
    );

    // On a mask: a refinement, protect kept and winning, one more step in its origin.
    // A protected pixel at the edge of what survives: the core is cleared under it when
    // the mask is made, so the erosion works on the core with that hole.
    let mut protect = vec![0u8; W * H];
    protect[32 * W + 50] = 255;
    let mask = mask_of(W, H, blob.clone(), &protect);
    assert_eq!(mask.core()[32 * W + 50], 0);
    let from = mask.id();
    let expected = brute_erode(mask.core(), 4.0);
    let made = shrink(mask, 4.0, &never).unwrap().unwrap();
    assert_eq!(expected[36 * W + 50], 0, "the hole eroded what was near it");
    assert_eq!(made.mask.core(), expected.as_slice());
    assert_eq!(made.mask.protect(), protect.as_slice());
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
            .argument("px", "4")
            .input("mask", from.0)
    );
    assert!(
        shrink(mask_of(W, H, blob, &protect), 4.0, &|| true)
            .unwrap()
            .is_none()
    );
}
