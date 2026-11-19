//! `paste`'s mode `poisson` (plan m3, task O4, item 3). The vectors are rascal's own
//! (`crates/rascal-ops/tests/it/poisson.rs` at the commit `docs/sources.toml` pins), on
//! rascal's picture of 48 by 32: a fill equal to the base gives the base in no iteration;
//! a constant fill on a constant base gives the base exactly, at the border too; one
//! editable pixel from a wrong guess; a linear fill on a matching linear base to one level;
//! an offset fill that loses its seam where feather keeps it; a domain with nothing to
//! anchor to; a protected hole the solve anchors to; the ceiling of iterations reached; a
//! wrapping picture refused; and an alpha kept. The benchmark is ignored.

use std::sync::Arc;

use whizbang_core::artifact::{Image, Mask, Source, UseCase};
use whizbang_core::capability::Picture;
use whizbang_core::contract::{Call, Capabilities, Executor, NoBackends, Value};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Projection, Topology, encode};
use whizbang_rascal::composite::{Mode, composite_in, composite_solved};
use whizbang_rascal::constants::POISSON_V1;
use whizbang_rascal::poisson::{Settings, Solve, solve};

use super::grow::{made_by, plane};
use super::paste::{World, outputs};

const W: u32 = 48;
const H: u32 = 32;

/// Pixels of `w` by `h`, opaque, each `f(x, y)`.
fn rgb(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> CanonicalPixels {
    let rgba = (0..w * h)
        .flat_map(|at| {
            let [r, g, b] = f(at % w, at / w);
            [r, g, b, 255]
        })
        .collect();
    CanonicalPixels::from_rgba(w, h, rgba).unwrap()
}

fn constant(v: u8) -> CanonicalPixels {
    rgb(W, H, |_, _| [v, v, v])
}

fn linear() -> CanonicalPixels {
    rgb(W, H, |x, y| {
        let v = (2 * x + 2 * y) as u8;
        [v, v / 2 + 10, 200 - v]
    })
}

fn offset(pixels: &CanonicalPixels, by: u8) -> CanonicalPixels {
    let rgba = pixels
        .rgba()
        .chunks(4)
        .flat_map(|p| [p[0] + by, p[1] + by, p[2] + by, p[3]])
        .collect();
    CanonicalPixels::from_rgba(pixels.width(), pixels.height(), rgba).unwrap()
}

fn image(pixels: &CanonicalPixels, projection: Projection, topology: Topology) -> Image {
    Image::new(
        encode::png(pixels).unwrap(),
        projection,
        topology,
        whizbang_core::artifact::ImageRole::Picture,
        made_by("import"),
        &DecodeLimits::default(),
    )
    .unwrap()
}

fn flat(pixels: &CanonicalPixels) -> Image {
    image(pixels, Projection::Flat, Topology::Bounded)
}

/// Rascal's block mask: the core is the block, with a blend ring of 170 and 85 round it.
fn block_mask(on: &Image, x0: u32, y0: u32, x1: u32, y1: u32) -> Mask {
    let (w, h) = (on.space().width(), on.space().height());
    let core = plane(w as usize, h as usize, |x, y| {
        (x0 as usize..x1 as usize).contains(&x) && (y0 as usize..y1 as usize).contains(&y)
    });
    let mut mask = Mask::from_core(
        on.binding(),
        UseCase::Remove,
        core,
        Source::default(),
        made_by("m"),
    )
    .unwrap();
    let blend = (0..w * h)
        .map(|at| {
            let (x, y) = ((at % w) as i32, (at / w) as i32);
            let ring = (y0 as i32 - y)
                .max(y - (y1 as i32 - 1))
                .max(x0 as i32 - x)
                .max(x - (x1 as i32 - 1));
            match ring {
                1 => 170,
                2 => 85,
                _ => 0,
            }
        })
        .collect();
    mask.set_blend(blend).unwrap();
    mask
}

fn weight(mask: &Mask, at: usize) -> f32 {
    whizbang_core::artifact::effective_weight(mask.core()[at], mask.blend()[at], mask.protect()[at])
}

fn v1() -> Settings {
    Settings::of(POISSON_V1.1).unwrap()
}

fn solved(base: &CanonicalPixels, fill: &CanonicalPixels, mask: &Mask) -> (Vec<f32>, usize) {
    match solve(base.rgba(), fill.rgba(), mask, &v1(), &|| false)
        .unwrap()
        .unwrap()
    {
        Solve::Solved {
            solution,
            iterations,
            ..
        } => (solution, iterations),
        Solve::Unanchored { .. } => panic!("unanchored"),
    }
}

/// The poisson composite of `fill` onto `base` under `mask`.
fn pasted(base: &CanonicalPixels, fill: &CanonicalPixels, mask: &Mask) -> CanonicalPixels {
    let (solution, _) = solved(base, fill, mask);
    let picture = Picture::new(base.clone()).unwrap();
    composite_solved(&picture, &solution, mask)
        .unwrap()
        .pixels()
        .clone()
}

#[test]
fn a_fill_equal_to_the_base_gives_the_base_in_no_iteration() {
    let base = linear();
    let mask = block_mask(&flat(&base), 14, 10, 34, 22);
    let (_, iterations) = solved(&base, &base, &mask);
    assert_eq!(iterations, 0, "the initial guess is the solution");
    assert_eq!(pasted(&base, &base, &mask), base);
}

#[test]
fn a_constant_fill_on_a_constant_base_gives_the_base_exactly_at_the_border_too() {
    let (base, fill) = (constant(50), constant(200));
    let on = flat(&base);
    for mask in [
        block_mask(&on, 14, 10, 34, 22),
        block_mask(&on, 0, 0, 12, 9),
    ] {
        assert_eq!(pasted(&base, &fill, &mask), base);
    }
    // One core pixel and no blend: the right-hand side is 0 there, and the guess 200.
    let mut one = block_mask(&on, 20, 15, 21, 16);
    one.set_blend(vec![0; (W * H) as usize]).unwrap();
    let black = constant(0);
    let one = Mask::from_core(
        flat(&black).binding(),
        UseCase::Remove,
        one.core().to_vec(),
        Source::default(),
        made_by("m"),
    )
    .unwrap();
    assert_eq!(pasted(&black, &fill, &one), black);
}

#[test]
fn a_linear_fill_on_a_matching_linear_base_is_reproduced_to_one_level() {
    let base = linear();
    let mask = block_mask(&flat(&base), 14, 10, 34, 22);
    let fill = offset(&base, 40);
    let (solution, _) = solved(&base, &fill, &mask);
    for at in 0..(W * H) as usize {
        for c in 0..3 {
            let got = solution[at * 3 + c].round() as i32;
            if weight(&mask, at) > 0.0 {
                let b = i32::from(base.rgba()[at * 4 + c]);
                assert!((got - b).abs() <= 1, "{at} channel {c}: {got} vs {b}");
            } else {
                assert_eq!(got, i32::from(fill.rgba()[at * 4 + c]), "outside: the fill");
            }
        }
    }
}

#[test]
fn an_offset_fill_loses_its_seam_under_poisson_where_feather_keeps_it() {
    let base = linear();
    let mask = block_mask(&flat(&base), 14, 10, 34, 22);
    let fill = offset(&base, 40);
    let poisson = pasted(&base, &fill, &mask);
    let painted = whizbang_core::capability::Painted::from_rgb(
        W,
        H,
        fill.rgba()
            .chunks(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
    )
    .unwrap();
    let picture = Picture::new(base.clone()).unwrap();
    let feather = composite_in(&picture, &painted, &mask, Mode::Feather).unwrap();
    let (b, p, f) = (base.rgba(), poisson.rgba(), feather.pixels().rgba());
    for at in 0..(W * H) as usize {
        let px = at * 4..at * 4 + 4;
        if weight(&mask, at) <= 0.0 {
            assert_eq!(
                p[px.clone()],
                b[px.clone()],
                "{at}: weighs nothing, the base"
            );
            assert_eq!(f[px.clone()], b[px.clone()]);
        } else {
            for c in 0..3 {
                let (g, o) = (i32::from(p[at * 4 + c]), i32::from(b[at * 4 + c]));
                assert!((g - o).abs() <= 1, "{at}");
            }
        }
        if mask.core()[at] != 0 {
            assert_eq!(f[at * 4], b[at * 4] + 40, "{at}: feather keeps the offset");
        }
    }
}

#[test]
fn a_domain_with_nothing_to_anchor_to_is_refused_and_a_protected_hole_anchors() {
    let base = linear();
    let on = flat(&base);
    let fill = offset(&base, 40);
    // The whole image: nothing to anchor to.
    let whole = block_mask(&on, 0, 0, W, H);
    match solve(base.rgba(), fill.rgba(), &whole, &v1(), &|| false)
        .unwrap()
        .unwrap()
    {
        Solve::Unanchored { component_px } => assert_eq!(component_px, u64::from(W * H)),
        Solve::Solved { .. } => panic!("nothing anchors the whole image"),
    }
    // The top rows alone are anchored by the row beneath them.
    let top = Mask::from_core(
        on.binding(),
        UseCase::Remove,
        plane(W as usize, H as usize, |_, y| y < 6),
        Source::default(),
        made_by("m"),
    )
    .unwrap();
    match solve(base.rgba(), fill.rgba(), &top, &v1(), &|| false)
        .unwrap()
        .unwrap()
    {
        Solve::Solved { domain_px, .. } => assert_eq!(domain_px, u64::from(W) * 6),
        Solve::Unanchored { .. } => panic!("the row beneath anchors the top rows"),
    }
    // A protected pixel inside the core: the domain loses it, the composite is the base
    // exactly, and the protected pixel is untouched.
    let (fifty, two_hundred) = (constant(50), constant(200));
    let on = flat(&fifty);
    let mut mask = block_mask(&on, 14, 10, 34, 22);
    let editable = (0..(W * H) as usize)
        .filter(|at| weight(&mask, *at) > 0.0)
        .count() as u64;
    let mut hole = vec![0u8; (W * H) as usize];
    hole[(15 * W + 20) as usize] = 255;
    mask.protect_union(&hole).unwrap();
    match solve(fifty.rgba(), two_hundred.rgba(), &mask, &v1(), &|| false)
        .unwrap()
        .unwrap()
    {
        Solve::Solved { domain_px, .. } => assert_eq!(domain_px, editable - 1),
        Solve::Unanchored { .. } => panic!("anchored by the ring"),
    }
    assert_eq!(pasted(&fifty, &two_hundred, &mask), fifty);
    // The whole image as core with one protected pixel: that hole is the anchor.
    let mut whole = block_mask(&on, 0, 0, W, H);
    let mut hole = vec![0u8; (W * H) as usize];
    hole[(5 * W + 5) as usize] = 255;
    whole.protect_union(&hole).unwrap();
    assert_eq!(pasted(&fifty, &two_hundred, &whole), fifty);
}

#[test]
fn reaching_the_ceiling_of_iterations_is_an_error() {
    let base = linear();
    let mask = block_mask(&flat(&base), 14, 10, 34, 22);
    let fill = constant(0);
    let one = Settings {
        tol: 1e-4,
        max_iter: 1,
    };
    let why = solve(base.rgba(), fill.rgba(), &mask, &one, &|| false).unwrap_err();
    assert!(why.contains("1 iteration"), "{why}");
    // The same problem converges under poisson-v1, and a stop ends it with nothing.
    let (solution, _) = solved(&base, &fill, &mask);
    assert_eq!(solution.len(), (W * H * 3) as usize);
    assert!(
        solve(base.rgba(), fill.rgba(), &mask, &v1(), &|| true)
            .unwrap()
            .is_none()
    );
}

#[test]
fn poisson_through_the_action_refuses_a_wrapping_picture_and_keeps_the_alpha() {
    let world = World::new("poisson");
    let engine = world.engine(Capabilities::none(), Arc::new(NoBackends));
    let paste = |base: &Image, fill: &Image, mask: &Mask| {
        let call = Call::new("paste")
            .argument("mode", Value::text("poisson"))
            .input("image", world.put(base.clone().into()))
            .input("fill", world.put(fill.clone().into()))
            .input("mask", world.put(mask.clone().into()));
        engine.submit(&call).unwrap().outcome().clone()
    };
    // A panorama that wraps: refused, naming it.
    let (fifty, two_hundred) = (rgb(64, 32, |_, _| [50; 3]), rgb(64, 32, |_, _| [200; 3]));
    let pano = image(&fifty, Projection::Equirectangular, Topology::Wrap);
    let fill = image(&two_hundred, Projection::Equirectangular, Topology::Wrap);
    let outcome = paste(&pano, &fill, &block_mask(&pano, 20, 10, 40, 22));
    let whizbang_core::contract::Outcome::Invalid { why } = outcome else {
        panic!("{outcome:?}");
    };
    assert!(why.contains("wrap"), "{why}");
    // A base with alpha keeps it, and a fill of another colour is pulled to the base.
    let rgba = (0..W * H)
        .flat_map(|at| [50, 60, 70, ((at % W) + (at / W)) as u8])
        .collect();
    let base = CanonicalPixels::from_rgba(W, H, rgba).unwrap();
    let on = flat(&base);
    let fill = flat(&rgb(W, H, |_, _| [200; 3]));
    let outcome = paste(&on, &fill, &block_mask(&on, 14, 10, 34, 22));
    let [(_, made)] = outputs(&outcome).try_into().unwrap();
    let made = world.image(made);
    assert_eq!(made.pixels(&DecodeLimits::default()).unwrap(), base);
}

/// Not a gate: `just test-only -p whizbang-rascal poisson::bench` with `--ignored` is not
/// taken, so run it by hand in release to see the time of a solve over 1000 by 1000.
#[test]
#[ignore = "a benchmark, run by hand"]
fn bench_a_1000_by_1000_domain() {
    let side = 1024;
    let base = rgb(side, side, |x, y| {
        [(x / 4) as u8, (y / 4) as u8, ((x + y) / 8) as u8]
    });
    let fill = rgb(side, side, |x, y| {
        let texture = (((x * 7 + y * 3) % 23) as i32 - 11) as u8;
        [
            ((x / 4) as u8).wrapping_add(30).wrapping_add(texture),
            ((y / 4) as u8).wrapping_add(30),
            (((x + y) / 8) as u8).wrapping_add(30),
        ]
    });
    let mask = block_mask(&flat(&base), 12, 12, 1012, 1012);
    let started = std::time::Instant::now();
    let solved = solve(base.rgba(), fill.rgba(), &mask, &v1(), &|| false);
    println!(
        "poisson bench: {:?} after {:?}",
        solved.map(|s| s.is_some()),
        started.elapsed()
    );
}
