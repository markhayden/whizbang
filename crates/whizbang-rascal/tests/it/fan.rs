//! `fan` (plan m3, task O3, item 1): rascal's band, for each of its four sets, on a
//! picture of 256 by 128 (a step across column 128 that grows down the rows, texture in
//! the lower half, a dark disc beside the line and one across it). The numbers are what
//! rascal's own `fan_profile` (`crates/rascal-ops/src/fan.rs` at the commit
//! `docs/sources.toml` pins) gives on that picture, rascal's column 0.5 taken to pixels;
//! and the action through the executor, with its refusals. What the seam's tests share is
//! here too.

use std::sync::Arc;

use whizbang_core::artifact::{Artifact, ArtifactId, ArtifactType, BandProfile, Windows};
use whizbang_core::capability::Filler;
use whizbang_core::contract::{Call, Capabilities, Engine, NoBackends, Registry, Value};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Digest};
use whizbang_rascal::fan::{Settings, fan_profile};
use whizbang_rascal::recipes;
use whizbang_testkit::capabilities::roster;

use super::canvases::{World, image, invalid, put, ran};

/// The executor over the seam's actions and the recipe `seam`, with this filler when
/// there is one.
pub fn engine(world: &World, filler: Option<Arc<dyn Filler>>) -> Engine {
    let mut registry = Registry::new();
    whizbang_rascal::cut::register(&mut registry).unwrap();
    whizbang_rascal::line::register(&mut registry).unwrap();
    whizbang_rascal::fan::register(&mut registry).unwrap();
    whizbang_rascal::profile::register(&mut registry).unwrap();
    whizbang_rascal::ramp::register(&mut registry).unwrap();
    whizbang_rascal::apply::register(&mut registry).unwrap();
    whizbang_rascal::fill::register(&mut registry).unwrap();
    whizbang_rascal::put::register(&mut registry).unwrap();
    let seam = recipes::action_of(recipes::SEAM, &registry).unwrap();
    registry.register(Arc::new(seam)).unwrap();
    let over = |backends, capabilities| {
        Engine::over(
            registry,
            backends,
            world.store.clone(),
            Digest::of_bytes(b""),
        )
        .with_capabilities(capabilities)
    };
    match filler {
        Some(filler) => over(
            Arc::new(roster(None, Some(filler.as_ref()), None)),
            Capabilities::none().with_fill(filler.clone()),
        ),
        None => over(Arc::new(NoBackends), Capabilities::none()),
    }
}

/// The picture of the vectors, `w` by `h`, its step across column `w / 2`.
pub fn picture(w: u32, h: u32) -> CanonicalPixels {
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let step = if x < w / 2 { 4 + y as i32 / 8 } else { 0 };
            let n = if y >= 64 {
                3 * (((x * 7 + y * 13) % 5) as i32 - 2)
            } else {
                0
            };
            let inside = |cx: i32, cy: i32, r: i32| {
                let (dx, dy) = (2 * x as i32 + 1 - 2 * cx, 2 * y as i32 + 1 - 2 * cy);
                dx * dx + dy * dy <= 4 * r * r
            };
            let c = w as i32 / 2;
            let p = if inside(c + 16, 24, 6) || inside(c, 100, 6) {
                [40, 40, 40]
            } else {
                [140 + step + n, 146 + step / 2 + n, 131 + n]
            };
            rgba.extend(p.iter().map(|v| *v as u8));
            rgba.push(255);
        }
    }
    CanonicalPixels::from_rgba(w, h, rgba).unwrap()
}

/// The band profile of this identity.
pub fn band(world: &World, id: ArtifactId) -> BandProfile {
    let read = whizbang_core::artifact::get(
        &*world.store,
        ArtifactType::BandProfile,
        id,
        &DecodeLimits::default(),
    );
    match read.unwrap() {
        Artifact::BandProfile(band) => band,
        other => panic!("{other:?}"),
    }
}

/// Rascal's numbers for one set: rows widened, rows capped, the sum of the half-widths,
/// and the half-width of every eighth row.
type Vector = (&'static str, usize, usize, f64, [f64; 16]);

const VECTORS: [Vector; 4] = [
    (
        "fan-v3",
        111,
        120,
        2491.628839333387,
        [
            17.843755676032252,
            18.88680240749622,
            6.093548735814353,
            2.4157720987543003,
            12.492582626802012,
            38.07080036717745,
            38.399999999999984,
            38.164614248928075,
            26.486245544855425,
            22.545171046670536,
            22.530656698330947,
            17.06883119339465,
            2.6736261439190856,
            3.234101674539175,
            19.285956632275038,
            22.630259467371403,
        ],
    ),
    (
        "fan-v1",
        117,
        0,
        2718.14677968945,
        [
            17.843755676032252,
            19.760048687119692,
            11.087614633081694,
            2.432119756892528,
            22.712288356970067,
            38.38225652550459,
            38.399999999999984,
            38.381768326019504,
            30.618884785530124,
            22.58294425863027,
            22.547547817139254,
            20.87365664222289,
            3.393429351670778,
            4.471131030887768,
            21.898775824744042,
            22.82027202173664,
        ],
    ),
    (
        "fan-thin-v1",
        0,
        0,
        303.407407407408,
        [2.3703703703703702; 16],
    ),
    (
        "fan-v2",
        109,
        10,
        2691.8374226808614,
        [
            17.843755676032252,
            19.760048687119692,
            11.087614633081694,
            2.432119756892528,
            22.712288356970067,
            38.38225652550459,
            38.399999999999984,
            38.381768326019504,
            30.618884785530124,
            22.58294425863027,
            22.547547817139254,
            20.87365664222289,
            2.3703703703703702,
            2.3703703703703702,
            21.898775824744042,
            22.82027202173664,
        ],
    ),
];

#[test]
fn the_ported_vectors_give_rascals_band_for_each_set() {
    let pixels = picture(256, 128);
    let near = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs().max(1.0);
    for (set, widened, capped, sum, rows) in VECTORS {
        let p = fan_profile(&pixels, 128.0, &Settings::of(set).unwrap()).unwrap();
        assert_eq!(p.column, 128, "{set}");
        assert!(near(p.thin, 2.3703703703703702), "{set}: thin {}", p.thin);
        let geometry = Windows {
            gap: 1,
            step_columns: 2,
            texture_columns: 25,
        };
        assert_eq!(p.windows, geometry, "{set}");
        let wide = p.half.iter().filter(|h| **h > p.thin + 0.5).count();
        assert_eq!(wide, widened, "{set}: rows widened");
        let capped_here = p.cap.iter().filter(|c| c.is_finite()).count();
        assert_eq!(capped_here, capped, "{set}: rows capped");
        assert!(near(p.half.iter().sum(), sum), "{set}: sum");
        for (i, expected) in rows.iter().enumerate() {
            let got = p.half[i * 8];
            assert!(
                near(got, *expected),
                "{set} row {}: {got} {expected}",
                i * 8
            );
        }
    }

    // The action gives that band, under the set a call names, on a line made at the
    // middle of the image by its share of the width.
    let world = World::new("fan");
    let engine = engine(&world, None);
    let id = put(&world, image(&pixels, false));
    let line = ran(
        &engine,
        &Call::new("line")
            .argument("column_share", Value::Number(0.5))
            .argument("width_px", Value::Number(2.0))
            .input("image", id),
    )[0]
    .1;
    let fan = |set: &str| {
        Call::new("fan")
            .argument("constants", Value::text(set))
            .input("image", id)
            .input("line", line)
    };
    let made = band(&world, ran(&engine, &fan("fan-v2"))[0].1);
    let direct = fan_profile(&pixels, 128.0, &Settings::of("fan-v2").unwrap()).unwrap();
    assert_eq!(made.half(), direct.half.as_slice());
    assert_eq!((made.column(), made.constants()), (128, "fan-v2"));
    assert_eq!(made.line(), line);
    // The default set is fan-v3; a tunable replaces its set's value.
    let default = Call::new("fan").input("image", id).input("line", line);
    assert_eq!(
        band(&world, ran(&engine, &default)[0].1).constants(),
        "fan-v3"
    );
    let fixed = fan("fan-v1").argument("wide_max", Value::Number(0.0));
    let fixed = band(&world, ran(&engine, &fixed)[0].1);
    assert!(fixed.half().iter().all(|h| *h == fixed.thin()));
}

#[test]
fn fan_refuses_a_path_a_panorama_and_a_line_too_near_the_edge() {
    let world = World::new("fan-refusals");
    let engine = engine(&world, None);
    let pixels = picture(256, 128);
    let flat = put(&world, image(&pixels, false));
    let line = |at: &str, value: Value| {
        let call = Call::new("line")
            .argument(at, value)
            .argument("width_px", Value::Number(2.0))
            .input("image", flat);
        ran(&engine, &call)[0].1
    };
    let fan = |image, line| Call::new("fan").input("image", image).input("line", line);
    let path = line("path", Value::text("128 0; 128 127"));
    assert!(invalid(&engine, &fan(flat, path)).contains("whole column"));
    let edge = line("column", Value::Number(1.0));
    assert!(invalid(&engine, &fan(flat, edge)).contains("left step window clips"));
    let tex = Call::new("fan")
        .argument("tex_low", Value::Number(3.0))
        .input("image", flat)
        .input("line", line("column", Value::Number(128.0)));
    assert!(invalid(&engine, &tex).contains("tex_high"));
    let pano = put(&world, image(&pixels, true));
    let on_pano = ran(
        &engine,
        &Call::new("line")
            .argument("column", Value::Number(128.0))
            .argument("width_px", Value::Number(2.0))
            .input("image", pano),
    )[0]
    .1;
    assert!(invalid(&engine, &fan(pano, on_pano)).contains("wraps"));
}

/// The steps after `ramp` made by hand, one action at a time through the executor, from
/// the mask the recipe handed back.
pub struct ByHand {
    pub cut: ArtifactId,
    pub working: ArtifactId,
    pub rebound: ArtifactId,
    pub put: ArtifactId,
}

/// A panorama of 256 by 128 whose wrap edge is a step, in a store, and the recipe `seam`
/// run on it under a filler that repaints every pixel it is sent.
pub struct Seam {
    pub world: World,
    pub engine: Engine,
    pub panorama: ArtifactId,
    /// The recipe's panorama and mask: the seam's mask, on the cut, with its tone ramp.
    pub ran: (ArtifactId, ArtifactId),
}

/// The strip a cut of the seam's panorama takes: rascal's 0.07 of 256 columns.
pub const STRIP: u32 = 18;

impl Seam {
    pub fn new(label: &str) -> Seam {
        use whizbang_testkit::capabilities::{FakeFiller, Paint};
        let world = World::new(label);
        let filler: Arc<dyn Filler> = Arc::new(FakeFiller::new(Paint::Inverse));
        let engine = engine(&world, Some(filler));
        let rgba = (0..256 * 128)
            .flat_map(|at| {
                let x = at % 256;
                [(100 + x / 8) as u8, (120 + x / 16) as u8, 90, 255]
            })
            .collect();
        let pixels = CanonicalPixels::from_rgba(256, 128, rgba).unwrap();
        let panorama = put(&world, image(&pixels, true));
        let ran_seam = ran(&engine, &Call::new("seam").input("image", panorama));
        assert_eq!(
            (ran_seam[0].0.as_str(), ran_seam[1].0.as_str()),
            ("image", "mask")
        );
        Seam {
            world,
            engine,
            panorama,
            ran: (ran_seam[0].1, ran_seam[1].1),
        }
    }

    /// The cut, `apply`, `fill` and `put`, by hand, through the recipe's mask.
    pub fn by_hand(&self) -> ByHand {
        let made = |call: Call| ran(&self.engine, &call);
        let cut = made(Call::new("cut").input("image", self.panorama));
        let applied = made(
            Call::new("apply")
                .input("image", cut[0].1)
                .input("mask", self.ran.1),
        );
        let filled = made(
            Call::new("fill")
                .input("image", applied[0].1)
                .input("mask", applied[1].1),
        );
        let back = made(
            Call::new("put")
                .input("image", self.panorama)
                .input("canvas", filled[0].1)
                .input("plan", cut[1].1)
                .input("mask", filled[1].1),
        )[0]
        .1;
        ByHand {
            cut: cut[0].1,
            working: applied[0].1,
            rebound: applied[1].1,
            put: back,
        }
    }

    /// The canvas a cut makes of the panorama `id`.
    pub fn canvas_of(&self, id: ArtifactId) -> CanonicalPixels {
        let pixels = super::canvases::pixels_of(&self.world.image(id));
        whizbang_rascal::cut::canvas_of(&pixels, STRIP).unwrap()
    }
}

#[test]
fn on_a_small_panorama_the_windows_are_the_scale_meant_one_column_a_side() {
    // The seam's canvas of a panorama 800 wide, strip 80, is 160 wide: a tenth of
    // `ref_width`. `fan` and `ramp` read their windows at that scale on purpose (plan
    // mE, task P1, item 7): the band is measured on one column each side of the line,
    // and the texture on sixteen, as the records of `fan` and `ramp` say.
    let settings = whizbang_rascal::fan::Settings::of("fan-v3").unwrap();
    let profile = whizbang_rascal::fan::fan_profile(&picture(160, 32), 80.0, &settings).unwrap();
    let w = profile.windows;
    assert_eq!((w.gap, w.step_columns, w.texture_columns), (1, 1, 16));
    assert!((profile.thin - 15.0 * 160.0 / 1620.0).abs() < 1e-12);
    assert_eq!(
        whizbang_rascal::fan::windows(80, profile.thin, w, 160),
        [
            ("left step", 77, 78),
            ("right step", 83, 84),
            ("left texture", 63, 79),
            ("right texture", 82, 98),
        ]
    );
}
