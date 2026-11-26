//! `select` (plan 2A, task R2): the union of the kept instances' cores, minus the exclude
//! boxes, as one new mask; and refused when the masks are on two images or nothing is
//! left.

use whizbang_core::artifact::{Mask, Source, UseCase};
use whizbang_core::identity::{Binding, Digest, PixelDigest, Space};
use whizbang_rascal::select::{SelectError, boxes, holding, point, select};

use super::grow::{made_by, mask_of, plane};

fn places(core: &[u8], width: usize) -> Vec<(usize, usize)> {
    let set = core.iter().enumerate().filter(|(_, s)| **s != 0);
    set.map(|(at, _)| (at % width, at / width)).collect()
}

#[test]
fn the_kept_instances_union_minus_an_exclude_box_is_the_core() {
    let (w, h) = (20, 12);
    // Two instances that overlap, and a third that is not kept, so not given.
    let a = mask_of(
        w,
        h,
        plane(w, h, |x, y| (2..8).contains(&x) && (2..6).contains(&y)),
        &[0; 240],
    );
    let b = mask_of(
        w,
        h,
        plane(w, h, |x, y| (6..12).contains(&x) && (4..9).contains(&y)),
        &[0; 240],
    );
    // A box from 7 to 9.5 across and 3.5 to 5 down: the pixels whose centres are at or
    // past the left and top edges and before the right and bottom ones, columns 7 to 8
    // (8.5 < 9.5, 9.5 is not) and row 4 (3.5 is row 3's centre, so row 3 too; 4.5 < 5).
    let exclude = boxes("7 3.5 9.5 5").unwrap();
    assert_eq!(exclude, [[7.0, 3.5, 9.5, 5.0]]);
    let mask = select(&[a.clone(), b.clone()], &exclude, UseCase::Remove).unwrap();
    let expected = plane(w, h, |x, y| {
        let in_a = (2..8).contains(&x) && (2..6).contains(&y);
        let in_b = (6..12).contains(&x) && (4..9).contains(&y);
        let cut = (7..9).contains(&x) && (3..5).contains(&y);
        (in_a || in_b) && !cut
    });
    assert_eq!(places(mask.core(), w), places(&expected, w));
    // A new mask on the same binding, with no blend and nothing protected.
    assert_eq!(mask.binding(), a.binding());
    assert!(mask.blend().iter().all(|s| *s == 0));
    assert!(mask.protect().iter().all(|s| *s == 0));
    assert_eq!(mask.use_case(), UseCase::Remove);
    // Its origin is one step: the action, its arguments, and every mask in order.
    let step = &mask.origin().steps()[0];
    assert_eq!(mask.origin().steps().len(), 1);
    assert_eq!(
        *step,
        whizbang_core::artifact::Step::new("select", "1.0.0")
            .argument("exclude", "7 3.5 9.5 5")
            .argument("use_case", "remove")
            .input("mask", a.id().0)
            .input("mask", b.id().0)
    );
    // Without a box, the union whole; the order of the masks changes no pixel.
    let whole = select(&[b, a], &[], UseCase::Custom).unwrap();
    let union = plane(w, h, |x, y| {
        ((2..8).contains(&x) && (2..6).contains(&y))
            || ((6..12).contains(&x) && (4..9).contains(&y))
    });
    assert_eq!(whole.core(), union.as_slice());
}

#[test]
fn select_refuses_masks_on_two_images_nothing_left_and_a_box_that_is_not_four_numbers() {
    let (w, h) = (8, 8);
    let a = mask_of(w, h, plane(w, h, |x, _| x < 2), &[0; 64]);
    let other = Mask::from_core(
        Binding::root(
            PixelDigest(Digest::of_bytes(b"another")),
            Space::flat(8, 8).unwrap(),
        ),
        UseCase::Remove,
        plane(w, h, |x, _| x < 2),
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    let Err(SelectError::Invalid(why)) = select(&[a.clone(), other], &[], UseCase::Remove) else {
        panic!("two images were taken");
    };
    assert!(why.contains("two images"), "{why}");
    let Err(SelectError::Invalid(why)) = select(&[a], &[[0.0, 0.0, 8.0, 8.0]], UseCase::Remove)
    else {
        panic!("an empty core was made");
    };
    assert!(why.contains("no pixel is left"), "{why}");
    assert!(matches!(
        select(&[], &[], UseCase::Remove),
        Err(SelectError::Invalid(_))
    ));
    for text in ["1 2 3", "1 2 3 x", "1 2 3 inf", "1 2 3 4 5"] {
        assert!(boxes(text).is_err(), "{text}");
    }
    assert_eq!(boxes("").unwrap(), Vec::<[f64; 4]>::new());
    assert_eq!(boxes("0,0,1,1; 2 2 3 3").unwrap().len(), 2);
}

#[test]
fn a_pixel_whose_centre_is_on_an_exclude_boxs_edge_is_cleared() {
    // A box whose left and top edges are on the centre of pixel (1, 1), and whose right
    // and bottom edges are on the centres of the next column and row. A centre on the
    // left or top edge is inside the box, so (1, 1) is cleared; one on the right or
    // bottom edge is not, so (2, 1) and (1, 2) stay.
    let (w, h) = (4, 4);
    let a = mask_of(w, h, plane(w, h, |_, _| true), &[0; 16]);
    let mask = select(&[a], &[[1.5, 1.5, 2.5, 2.5]], UseCase::Remove).unwrap();
    let expected = plane(w, h, |x, y| (x, y) != (1, 1));
    assert_eq!(places(mask.core(), w), places(&expected, w));
}

#[test]
fn holding_keeps_the_masks_whose_core_holds_the_pixel_in_the_order_given() {
    let (w, h) = (20, 12);
    let a = mask_of(
        w,
        h,
        plane(w, h, |x, y| (2..8).contains(&x) && (2..6).contains(&y)),
        &[0; 240],
    );
    let b = mask_of(
        w,
        h,
        plane(w, h, |x, y| (6..12).contains(&x) && (4..9).contains(&y)),
        &[0; 240],
    );
    let c = mask_of(
        w,
        h,
        plane(w, h, |x, y| (15..18).contains(&x) && (0..3).contains(&y)),
        &[0; 240],
    );
    let given = || vec![a.clone(), b.clone(), c.clone()];
    let ids = |kept: Vec<Mask>| kept.iter().map(Mask::id).collect::<Vec<_>>();
    // The pixel at 6.5 5.0 is in both a and b; at 2 2, a alone; at 16.9 2.99, c alone.
    assert_eq!(ids(holding(given(), [6.5, 5.0]).unwrap()), [a.id(), b.id()]);
    assert_eq!(ids(holding(given(), [2.0, 2.0]).unwrap()), [a.id()]);
    assert_eq!(ids(holding(given(), [16.9, 2.99]).unwrap()), [c.id()]);
    // The order is the order given.
    assert_eq!(
        ids(holding(vec![b.clone(), a.clone()], [6.5, 5.0]).unwrap()),
        [b.id(), a.id()]
    );
    // None that holds it, or a place outside the image, is refused naming the count.
    for at in [[0.0, 0.0], [20.0, 1.0], [1.0, 12.0], [-0.5, 1.0]] {
        let Err(SelectError::Invalid(why)) = holding(given(), at) else {
            panic!("kept something at {at:?}");
        };
        assert!(
            why.contains("of the 3 masks") || why.contains("none of the 3 masks"),
            "{why}"
        );
    }
    // The argument is two numbers, apart by spaces or commas.
    assert_eq!(point("4, 5.5").unwrap(), [4.0, 5.5]);
    assert!(point("4").is_err() && point("4 5 6").is_err() && point("x 1").is_err());
}

#[test]
fn the_ported_vectors_give_rascals_cores() {
    // Rascal's three planes (`crates/rascal-ops/tests/it/subtract_select_invert.rs`,
    // `select_keeps_drops_and_excludes_over_three_planes`, at the commit `docs/sources.toml`
    // pins), on its picture of 100 by 60, each an instance mask here. Rascal's `keep` and
    // `drop` are the masks given; its exclude box in fractions, 0.65 0 1 1, is in pixels.
    let (w, h) = (100usize, 60usize);
    let masks: Vec<Mask> = [[10, 30], [40, 60], [70, 90]]
        .iter()
        .map(|[x0, x1]| {
            let core = plane(w, h, |x, y| {
                (*x0..*x1).contains(&x) && (10..30).contains(&y)
            });
            mask_of(w, h, core, &[0; 6000])
        })
        .collect();
    let count = |m: &Mask| m.core().iter().filter(|s| **s != 0).count();
    let all = select(&masks, &[], UseCase::Remove).unwrap();
    assert_eq!(count(&all), 1200);
    assert!(all.blend().iter().all(|s| *s == 0) && all.protect().iter().all(|s| *s == 0));
    let kept = select(&[masks[0].clone(), masks[2].clone()], &[], UseCase::Remove).unwrap();
    assert_eq!((count(&kept), kept.core()[20 * w + 50]), (800, 0));
    let excluded = select(&masks, &[[65.0, 0.0, 100.0, 60.0]], UseCase::Remove).unwrap();
    assert_eq!((count(&excluded), excluded.core()[20 * w + 80]), (800, 0));
    // Everything excluded: rascal said its op did not apply; here the call is refused.
    let nothing = select(&masks, &[[0.0, 0.0, 100.0, 60.0]], UseCase::Remove);
    assert!(matches!(nothing, Err(SelectError::Invalid(_))));
}

/// A run joins its masks one at a time (plan mE, task L2, item 3): masks added to a union
/// one after the other, each let go once added, give the mask `select` makes of them all
/// at once; and with a pixel to keep by, the mask of those `holding` keeps.
#[test]
fn a_union_made_one_mask_at_a_time_is_the_selection_of_them_all() {
    use whizbang_rascal::select::Union;
    let (w, h) = (16, 10);
    let made = |x0: usize, x1: usize| {
        mask_of(
            w,
            h,
            plane(w, h, |x, y| (x0..x1).contains(&x) && (2..7).contains(&y)),
            &[0; 160],
        )
    };
    let all = [made(1, 5), made(4, 9), made(10, 14)];
    let exclude = boxes("3 3 11 4").unwrap();
    let whole = select(&all, &exclude, UseCase::Remove).unwrap();
    let mut union = Union::new(None);
    for mask in &all {
        // Each mask made afresh, added, and dropped before the next.
        let one = mask.clone();
        union.add(&one).unwrap();
        drop(one);
    }
    let joined = union.finish(&exclude, UseCase::Remove).unwrap();
    assert_eq!(joined.core(), whole.core());
    assert_eq!(joined.origin(), whole.origin());
    // Kept by a pixel: only the masks that hold it.
    let kept = holding(all.to_vec(), [4.5, 3.0]).unwrap();
    let held = select(&kept, &[], UseCase::Remove).unwrap();
    let mut union = Union::new(Some([4.5, 3.0]));
    for mask in &all {
        union.add(mask).unwrap();
    }
    let joined = union.finish(&[], UseCase::Remove).unwrap();
    assert_eq!(
        (joined.core(), joined.origin()),
        (held.core(), held.origin())
    );
    // None that holds it is refused, naming how many were given.
    let mut union = Union::new(Some([15.0, 9.0]));
    for mask in &all {
        union.add(mask).unwrap();
    }
    let Err(SelectError::Invalid(why)) = union.finish(&[], UseCase::Remove) else {
        panic!("a pixel no mask holds");
    };
    assert!(why.contains("none of the 3 masks"), "{why}");
}
