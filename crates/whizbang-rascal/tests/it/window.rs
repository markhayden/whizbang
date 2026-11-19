//! `window` (plan m3, task O2, item 1): rascal's window contract to the pixel
//! (`crates/rascal-ops/tests/it/window.rs` at the commit `docs/sources.toml` pins), and
//! the action through the executor: the crop, and the mask cut by the same box onto it,
//! placing it onto the image.

use whizbang_core::artifact::Mask;
use whizbang_core::contract::{Call, Value};
use whizbang_core::identity::{Destination, Direction, Mapping, Space};
use whizbang_rascal::window::{TooLarge, window_box};

use super::canvases::{World, at, engine, image, invalid, pano, pixels_of, put, ran};
use super::grow::plane;

/// A support, the image's width and height, `max_side`, the context, and the window.
type Case = ([u32; 4], u32, u32, Option<u32>, f64, [u32; 4]);

#[test]
fn the_ported_vectors_give_rascals_window() {
    // Rascal's cases, each with its numbers.
    let cases: [Case; 10] = [
        (
            [190, 0, 210, 100],
            400,
            100,
            Some(1024),
            0.5,
            [100, 0, 300, 100],
        ),
        ([90, 0, 110, 100], 200, 100, None, 0.5, [0, 0, 200, 100]),
        ([0, 40, 20, 60], 200, 100, Some(60), 0.5, [0, 30, 40, 70]),
        (
            [180, 40, 200, 60],
            200,
            100,
            Some(60),
            0.5,
            [160, 30, 200, 70],
        ),
        ([90, 0, 110, 20], 200, 100, Some(60), 0.5, [80, 0, 120, 40]),
        (
            [90, 80, 110, 100],
            200,
            100,
            Some(60),
            0.5,
            [80, 60, 120, 100],
        ),
        (
            [80, 40, 120, 60],
            200,
            100,
            Some(44),
            0.5,
            [78, 28, 122, 72],
        ),
        ([50, 50, 65, 60], 200, 100, None, 0.3, [45, 43, 70, 68]),
        ([50, 50, 60, 60], 200, 100, Some(13), 0.5, [49, 49, 62, 62]),
        ([10, 10, 20, 14], 200, 100, None, 0.0, [10, 7, 20, 17]),
    ];
    for (support, w, h, max_side, context, expected) in cases {
        assert_eq!(
            window_box(support, w, h, max_side, context),
            Ok(expected),
            "{support:?}"
        );
    }
    assert_eq!(
        window_box([0, 0, 150, 10], 200, 100, Some(100), 0.5),
        Err(TooLarge {
            support: [0, 0, 150, 10],
            max_side: 100
        })
    );
    // Whatever the inputs, the support is inside the window and the window inside the
    // image.
    for (s, m, c) in [
        ([3, 7, 9, 90], Some(95), 0.5),
        ([0, 0, 1, 1], Some(1), 2.0),
        ([100, 0, 200, 100], None, 1.0),
        ([17, 33, 18, 34], Some(7), 0.25),
    ] {
        let [x0, y0, x1, y1] = window_box(s, 200, 100, m, c).unwrap();
        assert!(
            x0 <= s[0] && y0 <= s[1] && x1 >= s[2] && y1 >= s[3],
            "{s:?}"
        );
        assert!(x1 <= 200 && y1 <= 100);
        if let Some(m) = m {
            assert!(x1 - x0 <= m && y1 - y0 <= m);
        }
    }

    // The action, on rascal's floor at half its size: 200 by 50 with a band of 10 columns
    // at the centre, a blend of one column each side and one pixel protected. The support
    // is 12 by 50; with window-v1's context of 0.5 a square of 100, capped to the height:
    // [50, 0, 150, 50).
    let world = World::new("window");
    let photo = image(&pano(200, 50), false);
    let mut band = Mask::from_core(
        photo.binding(),
        whizbang_core::artifact::UseCase::Seam,
        plane(200, 50, |x, _| (95..105).contains(&x)),
        Default::default(),
        super::grow::made_by("fixture"),
    )
    .unwrap();
    band.set_blend(
        plane(200, 50, |x, _| x == 94 || x == 105)
            .iter()
            .map(|v| v / 2)
            .collect(),
    )
    .unwrap();
    band.protect_union(&plane(200, 50, |x, y| (x, y) == (100, 25)))
        .unwrap();
    let ids = (put(&world, photo.clone()), put(&world, band.clone()));
    let engine = engine(&world, None);
    let call = |extra: Option<(&str, Value)>| {
        let call = Call::new("window")
            .input("image", ids.0)
            .input("mask", ids.1);
        match extra {
            Some((name, value)) => call.argument(name, value),
            None => call,
        }
    };
    let made = ran(&engine, &call(None));
    let (crop, cropped) = (world.image(made[0].1), world.mask(made[1].1));
    assert_eq!(crop.space(), Space::flat(100, 50).unwrap());
    let binding = cropped.binding();
    assert_eq!(binding.source(), crop.identity().pixels);
    assert_eq!(
        binding.destination(),
        Destination::Image(photo.identity().pixels)
    );
    assert_eq!(binding.direction(), Direction::SourceToDestination);
    assert_eq!(
        *binding.mapping(),
        Mapping::Crop {
            left: 50.0,
            top: 0.0,
            right: 150.0,
            bottom: 50.0
        }
    );
    // The pixels and the planes are cut by the same box: weights agree pixel for pixel.
    let (whole, part) = (pixels_of(&photo), pixels_of(&crop));
    for y in 0..50 {
        for x in 0..100 {
            assert_eq!(at(&part, x, y), at(&whole, x + 50, y));
            assert_eq!(cropped.weight(x, y), band.weight(x + 50, y), "{x},{y}");
        }
    }
    assert_eq!(cropped.use_case(), band.use_case());
    let steps: Vec<&str> = cropped
        .origin()
        .steps()
        .iter()
        .map(|s| s.action.as_str())
        .collect();
    assert_eq!(steps, ["fixture", "window"]);

    // A context of 0 makes the window the support made square: [75, 0, 125, 50).
    let made = ran(&engine, &call(Some(("context", Value::Number(0.0)))));
    let Mapping::Crop { left, right, .. } = *world.mask(made[1].1).binding().mapping() else {
        panic!("no crop");
    };
    assert_eq!((left, right), (75.0, 125.0));
    // A support over max_side is refused with its sizes; an empty one is refused.
    let why = invalid(&engine, &call(Some(("max_side", Value::Integer(10)))));
    assert!(why.contains("12x50") && why.contains("10"), "{why}");
    let empty = Mask::from_core(
        photo.binding(),
        whizbang_core::artifact::UseCase::Remove,
        vec![0; 200 * 50],
        Default::default(),
        super::grow::made_by("fixture"),
    )
    .unwrap();
    let empty = put(&world, empty);
    let none = Call::new("window")
        .input("image", ids.0)
        .input("mask", empty);
    assert!(invalid(&engine, &none).contains("empty"));
}
