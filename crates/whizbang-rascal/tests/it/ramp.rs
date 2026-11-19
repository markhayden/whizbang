//! `ramp` (plan m3, task O3, item 1): rascal's tone step on the picture of `fan`'s
//! vectors, measured on the step windows of the band `fan-v3` gives there, smoothed with
//! `ramp-v1`'s sigma of 0.02 of the height. The numbers are what rascal's own `tone_step`
//! (`crates/rascal-ops/src/ramp.rs` at the commit `docs/sources.toml` pins) gives.

use whizbang_core::artifact::Prescription;
use whizbang_core::contract::{Call, Value};

use super::canvases::{World, image, invalid, mask, put, ran};
use super::fan::{engine, picture};

/// Rascal's step, per channel, on every sixteenth row, and the sum of each channel.
const ROWS: [[f64; 3]; 8] = [
    [-4.002044270493322, -2.0, 0.0],
    [-5.579164972868187, -2.5779834184202812, 0.0],
    [-7.579164972868186, -3.5779834184202812, 0.0],
    [-9.579164972868188, -4.577983418420282, 0.0],
    [-11.30974220043885, -5.308560645990944, 0.26942277242933665],
    [-13.566647575679847, -6.56546602123194, 0.012517397188340013],
    [18.697423799012004, 22.880485445274317, 21.88441128288031],
    [-17.530694163408533, -8.531284940632494, 0.03311060163687164],
];
const SUMS: [f64; 3] = [-1026.7635474731544, -313.26354747315423, 274.23645252684554];

#[test]
fn the_ported_vectors_give_rascals_prescription() {
    let world = World::new("ramp");
    let engine = engine(&world, None);
    let pixels = picture(256, 128);
    let photo = image(&pixels, false);
    let id = put(&world, photo.clone());
    let made = |call: Call| ran(&engine, &call)[0].1;
    let line = made(
        Call::new("line")
            .argument("column", Value::Number(128.0))
            .argument("width_px", Value::Number(2.0))
            .input("image", id),
    );
    let band = made(Call::new("fan").input("image", id).input("line", line));
    let on_band = made(Call::new("profile").input("band", band));
    let ramped = world.mask(made(
        Call::new("ramp").input("image", id).input("mask", on_band),
    ));
    let [Prescription::ToneRamp { step, share, .. }] = ramped.prescriptions() else {
        panic!("{:?}", ramped.prescriptions());
    };
    assert_eq!(*share, 0.5, "ramp-v1's share");
    assert_eq!((step.channels(), step.rows().len()), (3, 128));
    let near = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs().max(1.0);
    for (i, expected) in ROWS.iter().enumerate() {
        let got = step.row(i as u32 * 16).unwrap();
        for ch in 0..3 {
            assert!(near(got[ch], expected[ch]), "row {} {ch}: {got:?}", i * 16);
        }
    }
    for (ch, sum) in SUMS.iter().enumerate() {
        let got: f64 = (0..128).map(|y| step.row(y).unwrap()[ch]).sum();
        assert!(near(got, *sum), "channel {ch}: {got}");
    }
    // The band mask keeps its planes and its profile; only the prescription is new.
    let before = world.mask(on_band);
    assert_eq!(
        (ramped.core(), ramped.blend()),
        (before.core(), before.blend())
    );
    assert_eq!(ramped.profile(), before.profile());

    // A share the call gives replaces ramp-v1's; a mask without a band profile is refused.
    let half = Call::new("ramp")
        .argument("share", Value::Number(0.25))
        .input("image", id)
        .input("mask", on_band);
    let halved = world.mask(made(half));
    let [Prescription::ToneRamp { share, .. }] = halved.prescriptions() else {
        panic!("one prescription");
    };
    assert_eq!(*share, 0.25);
    let plain = put(&world, mask(&photo, |x, _| x == 128, |_, _| false));
    let refused = Call::new("ramp").input("image", id).input("mask", plain);
    assert!(invalid(&engine, &refused).contains("no band profile"));
}
