//! `cut` (plan m3, task O2, item 1): rascal's strip widths and canvases
//! (`crates/rascal-ops/tests/it/cut_put.rs` at the commit `docs/sources.toml` pins), and
//! the action's canvas and plan through the executor.

use whizbang_core::artifact::{self, Artifact, ArtifactType};
use whizbang_core::contract::{Call, Executor, Value};
use whizbang_core::identity::{DecodeLimits, Space};
use whizbang_rascal::cut::{canvas_of, strip_width};

use super::canvases::{World, at, engine, image, invalid, pano, pixels_of, put, ran};

#[test]
fn the_ported_vectors_give_rascals_canvas_and_plan() {
    // Rascal's widths: s = min(max(2, 2 round_half_up(f W / 2)), 2 floor(W / 4)).
    for (w, f, s) in [
        (4_u32, 0.07, 2_u32),
        (5, 0.07, 2),
        (10, 0.07, 2),
        (10, 0.3, 4),
        (5760, 0.07, 404),
    ] {
        assert_eq!(strip_width(w, f), s, "W = {w}, f = {f}");
        let h = 6;
        let panorama = pano(w, h);
        let canvas = canvas_of(&panorama, s).unwrap();
        assert_eq!((canvas.width(), canvas.height()), (2 * s, h));
        // The canvas's left half is the panorama's last s columns; its right half the
        // first.
        for y in 0..h {
            for x in 0..s {
                assert_eq!(
                    at(&canvas, x, y),
                    at(&panorama, w - s + x, y),
                    "W {w} ({x},{y})"
                );
                assert_eq!(
                    at(&canvas, s + x, y),
                    at(&panorama, x, y),
                    "W {w} ({x},{y})"
                );
            }
        }
    }

    // The action, on rascal's panorama of 5760 by 6: a canvas of 808 by 6 and a plan of
    // strip 404 made for this panorama.
    let world = World::new("cut");
    let panorama = image(&pano(5760, 6), true);
    let id = put(&world, panorama.clone());
    let engine = engine(&world, None);
    let made = ran(&engine, &Call::new("cut").input("image", id));
    let ports: Vec<&str> = made.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(ports, ["image", "plan"]);
    let canvas = world.image(made[0].1);
    assert_eq!(canvas.space(), Space::flat(808, 6).unwrap());
    assert_eq!(
        pixels_of(&canvas),
        canvas_of(&pixels_of(&panorama), 404).unwrap()
    );
    let read = artifact::get(
        &*world.store,
        ArtifactType::Plan,
        made[1].1,
        &DecodeLimits::default(),
    );
    let Artifact::Plan(plan) = read.unwrap() else {
        panic!("no plan");
    };
    assert_eq!(plan.strip(), 404);
    assert_eq!(*plan.binding(), panorama.binding());
    assert_eq!(
        (plan.left_span(), plan.right_span()),
        ([5356, 5760], [0, 404])
    );
    assert_eq!(canvas.origin().steps()[0].action, "cut");

    // A flat picture is no panorama, a panorama 2 wide is too narrow, and a strip over
    // half is refused before anything runs.
    let flat = put(&world, image(&pano(40, 6), false));
    assert!(invalid(&engine, &Call::new("cut").input("image", flat)).contains("panorama"));
    let narrow = put(&world, image(&pano(2, 6), true));
    assert!(invalid(&engine, &Call::new("cut").input("image", narrow)).contains("at least 4"));
    let wide = Call::new("cut")
        .argument("strip", Value::Number(0.6))
        .input("image", id);
    assert!(engine.submit(&wide).is_err());
}
