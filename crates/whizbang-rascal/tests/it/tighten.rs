//! `tighten` (plan m3, task O4, item 1). The vectors are rascal's own
//! (`crates/rascal-ops/tests/it/tighten.rs` at the commit `docs/sources.toml` pins), held to
//! rascal's numbers: edges within a pixel on the smooth and the textured scratch; an
//! edgeless stroke untightened throughout and exactly the stroke `rasterize` draws; the
//! direction of a reversing and of a still path; ties by midpoint, width, then the lower
//! index; runs smoothed apart and a crossing reverted; a stroke partly outside the image
//! and an oversized radius bounded; and `tighten-v1` pinned. Rascal's pictures were 400 by
//! 300; these are the same pictures drawn at 256 by 192, the plan's largest side for a
//! test, and every threshold is rascal's.

use whizbang_core::artifact::{Artifact, Location, Origin, Shape, Step, UseCase};
use whizbang_core::contract::{Call, Value};
use whizbang_core::identity::{CanonicalPixels, Point};
use whizbang_rascal::constants::{self, TIGHTEN_V1};
use whizbang_rascal::rasterize::dilate_path;
use whizbang_rascal::tighten::{
    Extent, Settings, Tightened, Tunables, best_pair, directions, resample, smooth_extents,
    stroke_core, tighten,
};

use super::shaping::{Shop, count};

const W: u32 = 256;
const H: u32 = 192;

/// Rascal's test kit's generator of noise, a linear congruential step.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }
}

/// Rascal's scratch: 25 points along a sine, in fractions of the picture.
fn scratch_path() -> Vec<[f64; 2]> {
    (0..=24)
        .map(|i| {
            let t = f64::from(i) / 24.0;
            [
                0.15 + 0.7 * t,
                0.5 + 0.12 * (t * std::f64::consts::PI * 2.0).sin(),
            ]
        })
        .collect()
}

/// How wide the drawn scratch is: its two edges are this far apart.
const SCRATCH_WIDTH_PX: f64 = 3.0;

fn pixels_of(path: &[[f64; 2]]) -> Vec<Point> {
    path.iter()
        .map(|[x, y]| Point::new(x * f64::from(W), y * f64::from(H)))
        .collect()
}

/// Red, green and blue, three bytes a pixel.
type Rgb = Vec<[u8; 3]>;

/// Darkens the pixels within half the scratch's width of the path by `depth` levels.
fn draw_scratch(canvas: &mut Rgb, depth: u8) {
    let pts: Vec<(f64, f64)> = pixels_of(&scratch_path())
        .iter()
        .map(|p| (p.x, p.y))
        .collect();
    let half = SCRATCH_WIDTH_PX / 2.0;
    for y in 0..H {
        for x in 0..W {
            let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let near = pts.windows(2).any(|seg| {
                let ((ax, ay), (bx, by)) = (seg[0], seg[1]);
                let (dx, dy) = (bx - ax, by - ay);
                let len2 = dx * dx + dy * dy;
                let t = (((cx - ax) * dx + (cy - ay) * dy) / len2).clamp(0.0, 1.0);
                let (px, py) = (ax + t * dx, ay + t * dy);
                (cx - px).powi(2) + (cy - py).powi(2) <= half * half
            });
            if near {
                let p = &mut canvas[(y * W + x) as usize];
                *p = p.map(|v| v.saturating_sub(depth));
            }
        }
    }
}

/// Rascal's smooth scratch: a soft vertical gradient with the scratch 70 levels deep.
fn scratch_smooth() -> Rgb {
    let mut out: Rgb = (0..W * H)
        .map(|at| {
            let v = 150.0 + 40.0 * ((at / W) as f32 / H as f32);
            [v as u8, (v - 6.0) as u8, (v - 14.0) as u8]
        })
        .collect();
    draw_scratch(&mut out, 70);
    out
}

/// Rascal's textured floor: planks with a grain, darker joins every 40 columns, a light
/// falloff and a little noise.
fn floor() -> Rgb {
    let mut lcg = Lcg(0x5eed_f100_12ab_cd01);
    let mut out = Vec::with_capacity((W * H) as usize);
    for y in 0..H {
        let light = 1.0 - 0.25 * (y as f32 / H as f32);
        for x in 0..W {
            let join = x % 40 < 2;
            let grain = ((x as f32 * 0.37 + y as f32 * 0.11).sin() * 0.5 + 0.5) * 18.0;
            let noise = (lcg.next() % 11) as f32 - 5.0;
            let (r, g, b) = if join {
                (62.0, 44.0, 30.0)
            } else {
                (168.0 + grain, 124.0 + grain * 0.8, 84.0 + grain * 0.5)
            };
            let px = |v: f32| ((v + noise) * light).round().clamp(0.0, 255.0) as u8;
            out.push([px(r), px(g), px(b)]);
        }
    }
    out
}

/// The same scratch on the floor, 140 levels deep, so its edges outrank a join's.
fn scratch_textured() -> Rgb {
    let mut out = floor();
    draw_scratch(&mut out, 140);
    out
}

fn rgba(image: &Rgb) -> Vec<u8> {
    image
        .iter()
        .flat_map(|[r, g, b]| [*r, *g, *b, 255])
        .collect()
}

fn never() -> bool {
    false
}

fn run(image: &Rgb, path: &[[f64; 2]], radius: f64) -> Result<Option<Tightened>, String> {
    let tunables = Tunables::of(TIGHTEN_V1.1, |_| None).unwrap();
    tighten(
        &pixels_of(path),
        radius,
        &rgba(image),
        W,
        H,
        &tunables,
        &never,
    )
}

fn ran(image: &Rgb, path: &[[f64; 2]], radius: f64) -> Tightened {
    run(image, path, radius).unwrap().unwrap()
}

fn edges_within_a_pixel(image: &Rgb, label: &str) {
    let made = ran(image, &scratch_path(), 8.0);
    let tightened: Vec<&Extent> = made.extents.iter().filter(|e| e.tightened).collect();
    assert!(
        tightened.len() * 10 >= made.extents.len() * 8,
        "{label}: {} of {} samples tightened",
        tightened.len(),
        made.extents.len()
    );
    // The drawn scratch is 3 px wide about the path: its edges are at ±1.5 px. Each
    // tightened extent lies within a pixel of them, but for one in twenty.
    let half = SCRATCH_WIDTH_PX / 2.0;
    let off = tightened
        .iter()
        .filter(|e| (e.a + half).abs() > 1.0 || (e.b - half).abs() > 1.0)
        .count();
    assert!(
        off * 20 <= tightened.len(),
        "{label}: {off} of {} tightened extents are more than a pixel off the edges",
        tightened.len()
    );
    let median = |pick: fn(&Extent) -> f64| {
        let mut v: Vec<f64> = tightened.iter().map(|e| pick(e)).collect();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    assert!((median(|e| e.a) + half).abs() <= 1.0, "{label}: median a");
    assert!((median(|e| e.b) - half).abs() <= 1.0, "{label}: median b");
    // The core is far narrower than the stroke and covers the scratch's centre line.
    let stroke = dilate_path(&pixels_of(&scratch_path()), 8.0, W, H);
    assert!(
        count(&made.core) * 2 < count(&stroke),
        "{label}: core {} vs stroke {}",
        count(&made.core),
        count(&stroke)
    );
    for p in pixels_of(&scratch_path()).iter().skip(2).take(20) {
        let (x, y) = (p.x as u32, p.y as u32);
        assert_eq!(
            made.core[(y * W + x) as usize],
            255,
            "{label}: centre line at {x},{y}"
        );
    }
    // The worst tightened extent is within two pixels of the edges.
    let worst = tightened
        .iter()
        .map(|e| (e.a + half).abs().max((e.b - half).abs()))
        .fold(0.0_f64, f64::max);
    assert!(
        worst <= 2.0,
        "{label}: the worst is {worst} px off the edge"
    );
}

#[test]
fn the_ported_vectors_give_rascals_masks() {
    edges_within_a_pixel(&scratch_smooth(), "smooth");
    edges_within_a_pixel(&scratch_textured(), "textured");

    // An edgeless stroke across the smooth wall, away from the scratch: untightened
    // throughout, and exactly the stroke rasterize draws, caps and joins and all.
    let path = [[0.2, 0.15], [0.5, 0.22], [0.8, 0.12]];
    let made = ran(&scratch_smooth(), &path, 6.0);
    assert!(
        made.extents
            .iter()
            .all(|e| !e.tightened && e.a == -6.0 && e.b == 6.0)
    );
    assert_eq!(made.core, dilate_path(&pixels_of(&path), 6.0, W, H));

    // Half the stroke off the left edge: the reads clamp, and the core is what is inside.
    let made = ran(&scratch_smooth(), &[[-0.3, 0.5], [0.3, 0.5]], 5.0);
    let at = |core: &[u8], x: u32, y: u32| core[(y * W + x) as usize];
    // Rascal's columns 0, 119 and 130 of 400, at row 150 of 300.
    assert_ne!(at(&made.core, 0, 96), 0);
    assert_ne!(at(&made.core, 76, 96), 0);
    assert_eq!(at(&made.core, 83, 96), 0);
    // Wholly outside: nothing to paint.
    let outside = run(&scratch_smooth(), &[[-0.5, 0.5], [-0.2, 0.5]], 5.0).unwrap_err();
    assert!(outside.contains("covers no pixel"), "{outside}");
    // A radius far beyond the image is clamped to its diagonal.
    let made = ran(&scratch_smooth(), &[[0.3, 0.5], [0.7, 0.5]], 1e9);
    let diagonal = (f64::from(W).powi(2) + f64::from(H).powi(2)).sqrt();
    assert_eq!(made.radius, diagonal);
}

#[test]
fn a_reversing_path_keeps_its_direction_and_a_still_path_is_refused() {
    // Out and back along one line: the turn's central difference is zero and takes the
    // previous direction; the ends are one-sided.
    let samples = [
        [10.0, 10.0],
        [20.0, 10.0],
        [30.0, 10.0],
        [20.0, 10.0],
        [10.0, 10.0],
    ];
    let d = directions(&samples).unwrap();
    assert_eq!(d[0], [1.0, 0.0], "one-sided at the start");
    assert_eq!(d[2], d[1], "the turn takes the previous direction");
    assert_eq!(d[4], [-1.0, 0.0], "one-sided at the end");
    // The zero difference first: it takes the next nonzero one.
    let d = directions(&[[10.0, 10.0], [10.0, 10.0], [20.0, 10.0]]).unwrap();
    assert_eq!(d[0], [1.0, 0.0]);
    assert!(directions(&[[5.0, 5.0], [5.0, 5.0], [5.0, 5.0]]).is_none());
    let still = run(&scratch_smooth(), &[[0.3, 0.3], [0.3, 0.3]], 5.0).unwrap_err();
    assert!(still.contains("no nonzero direction"), "{still}");
    // Resampling: every multiple of the spacing along the length, and the end point.
    let r = resample(&[[0.0, 0.0], [10.0, 0.0]], 4.0);
    assert_eq!(r, vec![[0.0, 0.0], [4.0, 0.0], [8.0, 0.0], [10.0, 0.0]]);
}

#[test]
fn ties_break_by_midpoint_then_width_then_the_lower_index() {
    let settings = Settings {
        radius: 5.0,
        min_contrast: 10.0,
        min_defect: 1.0,
        max_defect: 10.0,
        path_sigma: 0.0,
        spacing: 1.0,
    };
    let offsets: Vec<f64> = (-5..=5).map(f64::from).collect();
    let with = |set: &[(usize, f64)]| {
        let mut g = vec![0.0; 11];
        for (at, v) in set {
            g[*at] = *v;
        }
        g
    };
    // Equal scores: the midpoint nearest the path wins.
    let g = with(&[(3, -20.0), (4, 20.0), (6, 20.0), (8, -20.0)]);
    assert_eq!(best_pair(&g, &offsets, &settings), Some((3, 6)));
    // Equal score and midpoint: the narrower pair.
    let g = with(&[(3, -20.0), (7, 20.0), (4, -20.0), (6, 20.0)]);
    assert_eq!(best_pair(&g, &offsets, &settings), Some((4, 6)));
    // Equal score, midpoint size and width: the lower first index.
    let g = with(&[(3, -20.0), (5, 20.0), (7, -20.0)]);
    assert_eq!(best_pair(&g, &offsets, &settings), Some((3, 5)));
    // Below the contrast, one sign, too narrow or too wide: no pair.
    let g = with(&[(3, -5.0), (6, 20.0)]);
    assert_eq!(best_pair(&g, &offsets, &settings), None);
    let g = with(&[(3, 20.0), (6, 20.0)]);
    assert_eq!(best_pair(&g, &offsets, &settings), None);
    let g = with(&[(3, -20.0), (6, 20.0)]);
    let narrow = Settings {
        min_defect: 4.0,
        ..settings
    };
    assert_eq!(best_pair(&g, &offsets, &narrow), None);
    let wide = Settings {
        max_defect: 2.0,
        ..settings
    };
    assert_eq!(best_pair(&g, &offsets, &wide), None);
}

#[test]
fn smoothing_runs_separately_and_the_crossing_guard_reverts() {
    let t = |a: f64, b: f64| Extent {
        a,
        b,
        tightened: true,
    };
    let mut extents = vec![
        t(-1.0, 1.0),
        t(-1.0, 1.0),
        t(2.5, 2.6),
        t(-1.0, 1.0),
        t(-1.0, 1.0),
        Extent::full(5.0),
        t(-2.0, 2.0),
    ];
    smooth_extents(&mut extents, 1.0, 5.0);
    assert!(extents[..5].iter().all(|e| e.tightened && e.a < e.b));
    assert!(
        extents[2].a < 2.5,
        "the outlier was pulled toward its neighbours"
    );
    assert!(!extents[5].tightened);
    assert_eq!(extents[6], t(-2.0, 2.0));
    let mut crossed = vec![t(-1.0, 1.0), t(2.0, 1.0), t(-1.0, 1.0)];
    smooth_extents(&mut crossed, 0.2, 5.0);
    assert_eq!(crossed[1], Extent::full(5.0));
    assert!(crossed[0].tightened && crossed[2].tightened);
    // A sigma under half a sample changes nothing.
    let mut same = vec![t(-1.0, 1.0), t(-3.0, 0.5)];
    smooth_extents(&mut same, 0.2, 5.0);
    assert_eq!(same, vec![t(-1.0, 1.0), t(-3.0, 0.5)]);
}

#[test]
fn the_tighten_v1_constants_are_pinned_and_a_call_may_override_each() {
    let v1 = Tunables::of(TIGHTEN_V1.1, |_| None).unwrap();
    assert_eq!(
        v1,
        Tunables {
            min_contrast: 12.0,
            min_defect_px: 1.0,
            max_defect_factor: 2.0,
            path_sigma: 4.0,
            sample_spacing_px: 1.0,
        }
    );
    assert_eq!(constants::number(TIGHTEN_V1.1, "path_sigma"), Some(4.0));
    let given = Tunables::of(TIGHTEN_V1.1, |key| (key == "path_sigma").then_some(0.0)).unwrap();
    assert_eq!(given.path_sigma, 0.0);
    assert_eq!(given.min_contrast, 12.0);
}

/// A picture of the smooth scratch, in the store, with a stroke on it.
fn stroked(path: &[[f64; 2]], radius: f64) -> (Artifact, Location) {
    let pixels = CanonicalPixels::from_rgba(W, H, rgba(&scratch_smooth())).unwrap();
    let image = super::paste::image_of(&pixels, "import");
    let shape = Shape::Stroke {
        path: pixels_of(path),
        radius,
    };
    let origin = Origin::new(Step::new("stroke", "1")).unwrap();
    let location = Location::new(image.binding(), shape, origin).unwrap();
    (Artifact::Image(image), location)
}

#[test]
fn tighten_as_an_action_makes_a_repair_mask_on_the_image() {
    let shop = Shop::new("tighten", None);
    let (image, location) = stroked(&scratch_path(), 8.0);
    let image_id = shop.put(image.clone());
    let stroke = shop.put(Artifact::Location(location.clone()));
    let call = Call::new("tighten")
        .input("stroke", stroke)
        .input("image", image_id);
    let mask = shop.mask(shop.one(&call));
    let Artifact::Image(image) = image else {
        panic!()
    };
    assert_eq!(mask.binding(), &image.binding());
    assert_eq!(mask.use_case(), UseCase::Repair);
    assert_eq!(mask.source().locations, vec![stroke]);
    assert!(mask.blend().iter().chain(mask.protect()).all(|s| *s == 0));
    // What the function makes, under tighten-v1.
    assert_eq!(
        mask.core(),
        ran(&scratch_smooth(), &scratch_path(), 8.0).core.as_slice()
    );
    let step = mask.origin().steps().last().unwrap().clone();
    assert_eq!(step.action, "tighten");
    assert_eq!(
        step,
        Step::new("tighten", "1.0.0")
            .argument("min_contrast", "12")
            .argument("min_defect_px", "1")
            .argument("max_defect_factor", "2")
            .argument("path_sigma", "4")
            .argument("sample_spacing_px", "1")
            .argument("constants", "tighten-v1")
            .input("stroke", stroke.0)
            .input("image", image_id.0)
    );
    // An argument given replaces the set's: with no smoothing the core differs.
    let unsmoothed = shop.mask(shop.one(&call.clone().argument("path_sigma", Value::Number(0.0))));
    assert_ne!(unsmoothed.core(), mask.core());
    // A location that is not a stroke is refused, naming its kind.
    let boxed = Location::new(
        image.binding(),
        Shape::Box {
            left: 10.0,
            top: 10.0,
            right: 20.0,
            bottom: 20.0,
        },
        Origin::new(Step::new("rect", "1")).unwrap(),
    )
    .unwrap();
    let boxed = shop.put(Artifact::Location(boxed));
    let why = shop.refused(
        &Call::new("tighten")
            .input("stroke", boxed)
            .input("image", image_id),
    );
    assert!(why.contains("not a box"), "{why}");
}

/// The core as `stroke_core` drew it before plan m3, task B1: every pixel of the stroke's
/// box asked of every cap and every segment. The reference the drawing by elements is held
/// to.
fn core_by_the_whole_box(samples: &[[f64; 2]], extents: &[Extent], w: u32, h: u32) -> Vec<u8> {
    let dirs = directions(samples).unwrap();
    let reach = extents
        .iter()
        .map(|e| e.a.abs().max(e.b.abs()))
        .fold(0.0, f64::max);
    let xs = samples.iter().map(|p| p[0]);
    let ys = samples.iter().map(|p| p[1]);
    let (min_x, max_x) = (
        xs.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
    );
    let (min_y, max_y) = (
        ys.clone().fold(f64::INFINITY, f64::min),
        ys.fold(f64::NEG_INFINITY, f64::max),
    );
    let x_lo = (min_x - reach - 1.0).floor().max(0.0) as u32;
    let x_hi = ((max_x + reach + 1.0).ceil().max(0.0) as u32).min(w);
    let y_lo = (min_y - reach - 1.0).floor().max(0.0) as u32;
    let y_hi = ((max_y + reach + 1.0).ceil().max(0.0) as u32).min(h);
    let caps: Vec<([f64; 2], f64)> = (samples.iter().zip(&dirs).zip(extents))
        .map(|((p, d), e)| {
            let m = (e.a + e.b) / 2.0;
            ([p[0] - m * d[1], p[1] + m * d[0]], (e.b - e.a) / 2.0)
        })
        .collect();
    let mut core = vec![0u8; (w * h) as usize];
    for y in y_lo..y_hi {
        let cy = f64::from(y) + 0.5;
        for x in x_lo..x_hi {
            let cx = f64::from(x) + 0.5;
            let in_cap = (caps.iter())
                .any(|(c, r)| (cx - c[0]).powi(2) + (cy - c[1]).powi(2) <= r * r + 1e-9);
            let in_tube = samples.windows(2).enumerate().any(|(i, seg)| {
                let (p, q) = (seg[0], seg[1]);
                let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                let len2 = dx * dx + dy * dy;
                if len2 == 0.0 {
                    return false;
                }
                let u = ((cx - p[0]) * dx + (cy - p[1]) * dy) / len2;
                if !(0.0..=1.0).contains(&u) {
                    return false;
                }
                let d = (-dy * (cx - p[0]) + dx * (cy - p[1])) / len2.sqrt();
                let (ea, eb) = (&extents[i], &extents[i + 1]);
                d >= ea.a + u * (eb.a - ea.a) - 1e-9 && d <= ea.b + u * (eb.b - ea.b) + 1e-9
            });
            if in_cap || in_tube {
                core[(y * w + x) as usize] = 255;
            }
        }
    }
    core
}

#[test]
fn the_core_drawn_by_elements_is_the_core_of_the_whole_box_and_grows_with_the_strokes_length() {
    // Strokes of every kind on a picture of 96 by 64: straight, bending back, partly
    // outside the picture, with extents tightened, lopsided, crossed and full, each core
    // the same pixel for pixel as the whole box gives.
    let (w, h) = (96u32, 64u32);
    let mut noise = Lcg(20_261_119);
    let mut unit = || f64::from(noise.next() % 10_000) / 10_000.0;
    for case in 0..40 {
        let points = 2 + case % 5;
        let path: Vec<[f64; 2]> = (0..points)
            .map(|_| [unit() * 120.0 - 12.0, unit() * 88.0 - 12.0])
            .collect();
        let samples = resample(&path, 0.5 + unit() * 2.0);
        if directions(&samples).is_none() {
            continue;
        }
        let extents: Vec<Extent> = samples
            .iter()
            .map(|_| match case % 4 {
                0 => Extent::full(1.0 + unit() * 8.0),
                1 => Extent {
                    a: -unit() * 6.0,
                    b: unit() * 6.0,
                    tightened: true,
                },
                2 => Extent {
                    a: unit() * 3.0,
                    b: 3.0 + unit() * 4.0,
                    tightened: true,
                },
                _ => Extent {
                    a: unit() * 4.0 - 2.0,
                    b: unit() * 4.0 - 2.0,
                    tightened: true,
                },
            })
            .collect();
        let dirs = directions(&samples).unwrap();
        let by_elements = stroke_core(&samples, &dirs, &extents, w, h, &never).unwrap();
        let reference = core_by_the_whole_box(&samples, &extents, w, h);
        assert!(by_elements == reference, "case {case}: the cores differ");
    }
    // A diagonal stroke across 8000 by 6000, one sample a pixel: about 10,000 samples,
    // which the whole box asked of each would take hours; drawn by elements, a moment.
    let (w, h) = (8000u32, 6000u32);
    let samples = resample(&[[10.0, 10.0], [7990.0, 5990.0]], 1.0);
    let dirs = directions(&samples).unwrap();
    let extents = vec![Extent::full(6.0); samples.len()];
    let core = stroke_core(&samples, &dirs, &extents, w, h, &never).unwrap();
    let set = count(&core);
    assert!(set > 9_000 * 12 && set < 10_100 * 13, "{set}");
}
