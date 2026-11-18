//! `align` (plan `docs/plans/m2c-milestone-2.md`, task A2, item 2): squint's estimator gives
//! here what it gave in squint, on squint's own vectors; a known homography is recovered on
//! synthetic pairs within the error its record states; and the aligned frame is bound onto
//! the image it was warped from by the homography it was warped with.

use std::path::PathBuf;

use image::RgbImage;
use whizbang_core::artifact::Image;
use whizbang_core::identity::{Mapping, Point};
use whizbang_squint::align::{self, Options, Status, align_flat};

use super::scenes::{self, IDENTITY, Scene, perspective, similarity};
use super::support::*;

fn planes() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/align")
}

fn plane(name: &str) -> Image {
    let file = std::fs::read(planes().join(format!("{name}.png"))).unwrap();
    image_in(
        file,
        whizbang_core::identity::Projection::Flat,
        whizbang_core::identity::Topology::Bounded,
    )
}

fn rgb(image: &Image) -> RgbImage {
    let pixels = image.pixels(&limits()).unwrap();
    let rgb = pixels
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    RgbImage::from_raw(pixels.width(), pixels.height(), rgb).unwrap()
}

/// One of squint's vectors: a pair, and what squint's own code gave for it.
struct Vector {
    name: &'static str,
    against: &'static str,
    image: &'static str,
    status: Status,
    homography: [f64; 9],
    normalised: [f64; 9],
    /// Overlap, agreement (ncc), inliers, texture.
    quality: [f64; 4],
    /// Shift, rotation, scale, perspective.
    motion: [f64; 4],
    iterations: usize,
}

const UNIT: [f64; 9] = IDENTITY;

/// Squint's failure fixtures (`fixtures/images/align/` and `cases.toml` in squint), made
/// again at 128 by 96 pixels by squint's own scene, and its keystone case
/// (`crates/squint-tools/tests/it/align.rs`): the planes of `fixtures/align/`. Every number
/// was computed by squint's code at the commit `docs/sources.toml` pins (its estimator,
/// `normalise_homography` and `Motion` of `builtin/align.rs`, unchanged), apart from this
/// crate. `align` registers `image` onto `against`. The gradient is set against itself:
/// it has no texture, and squint's fixture did the same.
const VECTORS: [Vector; 7] = [
    Vector {
        name: "registered",
        against: "scene",
        image: "registered",
        status: Status::Registered,
        homography: [
            0.9805934100533797,
            0.03439308756458301,
            -1.8059054345276013,
            -0.034245365132587885,
            0.9802528384596025,
            4.204331925925098,
            6.419678768368666e-6,
            3.7628032930907564e-6,
            1.0,
        ],
        normalised: [
            0.9805934100533797,
            0.025794815673437256,
            -0.014108636207246885,
            -0.04566048684345051,
            0.9802528384596025,
            0.04379512422838644,
            0.0008217188823511892,
            0.0003612291161367126,
            1.0,
        ],
        quality: [
            0.9884031198686372,
            0.9956538200418558,
            1.0,
            0.023843737712701877,
        ],
        motion: [
            1.7689953293964782,
            -2.0077078871507235,
            0.9801523333398741,
            0.0005911243645522202,
        ],
        iterations: 17,
    },
    Vector {
        name: "keystone",
        against: "scene",
        image: "keystone",
        status: Status::Registered,
        homography: [
            0.9525337470268602,
            -0.050227182557289245,
            1.664512267484359,
            0.023472037364061334,
            0.992433054110963,
            -3.971081626150811,
            -0.0005957530192300381,
            2.87162387785755e-5,
            1.0,
        ],
        normalised: [
            0.9525337470268602,
            -0.03767038691796693,
            0.013004002089721555,
            0.03129604981874844,
            0.992433054110963,
            -0.04136543360573761,
            -0.07625638646144488,
            0.002756758922743248,
            1.0,
        ],
        quality: [
            0.9305213464696224,
            0.9990499464600525,
            1.0,
            0.026254052753362216,
        ],
        motion: [
            1.854822866750595,
            2.9889606703262617,
            1.0296185642757938,
            0.04101382305119472,
        ],
        iterations: 19,
    },
    Vector {
        name: "unrelated",
        against: "scene",
        image: "unrelated",
        status: Status::NotRegistered,
        homography: UNIT,
        normalised: UNIT,
        quality: [
            0.5489532019704434,
            0.17383029099736577,
            0.41007194244604317,
            0.026254052753362216,
        ],
        motion: [0.0, 0.0, 1.0, 0.0],
        iterations: 120,
    },
    Vector {
        name: "low-texture",
        against: "gradient-a",
        image: "gradient-a",
        status: Status::LowTexture,
        homography: UNIT,
        normalised: UNIT,
        quality: [0.0, 0.0, 0.0, 0.00018329203851994092],
        motion: [0.0, 0.0, 1.0, 0.0],
        iterations: 0,
    },
    Vector {
        name: "mirror",
        against: "square",
        image: "mirror",
        status: Status::NotRegistered,
        homography: UNIT,
        normalised: UNIT,
        quality: [
            0.6792800453514739,
            0.32236112138220774,
            0.3782435129740519,
            0.03276689441255534,
        ],
        motion: [0.0, 0.0, 1.0, 0.0],
        iterations: 120,
    },
    Vector {
        name: "quarter-turn",
        against: "square",
        image: "quarter-turn",
        status: Status::NotRegistered,
        homography: UNIT,
        normalised: UNIT,
        quality: [
            0.7677154195011338,
            0.2188632847326813,
            0.33585858585858586,
            0.03276689441255534,
        ],
        motion: [0.0, 0.0, 1.0, 0.0],
        iterations: 120,
    },
    Vector {
        name: "far-shift",
        against: "scene",
        image: "far-shift",
        status: Status::NotRegistered,
        homography: UNIT,
        normalised: UNIT,
        quality: [
            1.0,
            0.000665578687065482,
            0.009501187648456057,
            0.012844112601408416,
        ],
        motion: [0.0, 0.0, 1.0, 0.0],
        iterations: 60,
    },
];

/// Equal to within a few units in the last place: squint's mathematics library and this
/// machine's may round a sine or an arc tangent their own ways.
fn close(got: &[f64], want: &[f64], what: &str) {
    assert_eq!(got.len(), want.len(), "{what}");
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g - w).abs() <= 1e-9 * w.abs().max(1.0),
            "{what}: {got:?} against {want:?}"
        );
    }
}

#[test]
fn the_ported_vectors_of_squint_give_squints_homography_and_residual() {
    for v in &VECTORS {
        let (image, against) = (plane(v.image), plane(v.against));
        let a = align_flat(&rgb(&image), &rgb(&against), &Options::default());
        assert_eq!(a.status, v.status, "{}", v.name);
        assert_eq!(a.iterations, v.iterations, "{}", v.name);
        close(&a.homography, &v.homography, v.name);
        let q = a.quality;
        close(
            &[q.overlap, q.ncc, q.inlier_fraction, q.texture],
            &v.quality,
            v.name,
        );
        if v.status != Status::Registered || image.id() == against.id() {
            continue;
        }
        // Through the tool: the same transform, squint's normalised one, and its motion.
        let aligned = align::align(&image, &against, &Options::default(), &limits()).unwrap();
        let transform = aligned.transform.unwrap();
        close(&transform.homography, &v.homography, v.name);
        close(&transform.params, &v.normalised, v.name);
        let motion: Vec<f64> = aligned.measurements[..4].iter().map(|m| m.value).collect();
        close(&motion, &v.motion, v.name);
        let names: Vec<&str> = aligned.measurements.iter().map(|m| m.name).collect();
        assert_eq!(
            names,
            [
                "translation_px",
                "rotation_deg",
                "scale",
                "perspective",
                "ncc",
                "inlier_fraction",
                "overlap",
                "texture"
            ]
        );
    }
}

/// The source of the synthetic pairs, and the size of what is rendered of it.
const SOURCE: (u32, u32) = (128, 96);

/// One kind of known transform, made for the `i`th pair of its kind: the transform (source
/// pixels to candidate pixels), the candidate's size, and its tone.
/// A transform, the candidate's size, and its tone.
type Made = ([f64; 9], (u32, u32), fn(f64) -> f64);

struct Kind {
    name: &'static str,
    made: fn(usize) -> Made,
}

fn turn(degrees_times_100: i32) -> f64 {
    // The tangent of half the angle, to a good approximation for small angles: rational.
    let half = f64::from(degrees_times_100) / 100.0 * std::f64::consts::PI / 360.0;
    half + half * half * half / 3.0
}

const KINDS: [Kind; 6] = [
    Kind {
        name: "translation",
        made: |i| {
            let t = similarity(
                SOURCE,
                1.0,
                0.0,
                (3.3 + 1.7 * i as f64, -2.4 + 1.1 * i as f64),
            );
            (t, SOURCE, scenes::same)
        },
    },
    Kind {
        name: "rotation and scale",
        made: |i| {
            let t = similarity(
                SOURCE,
                0.94 + 0.03 * i as f64,
                turn(-500 + 300 * i as i32),
                (1.2, -0.7),
            );
            (t, SOURCE, scenes::same)
        },
    },
    Kind {
        name: "perspective",
        made: |i| {
            let base = similarity(SOURCE, 1.0, turn(150), (0.8, 0.4));
            let g = 0.0004 + 0.0002 * i as f64;
            (
                perspective(SOURCE, &base, g, -g / 2.0),
                SOURCE,
                scenes::same,
            )
        },
    },
    Kind {
        name: "restyled",
        made: |i| {
            let t = similarity(SOURCE, 1.02, turn(200 - 100 * i as i32), (2.5, -1.5));
            (t, SOURCE, scenes::restyled)
        },
    },
    Kind {
        name: "smaller candidate",
        made: |i| {
            let t = similarity(SOURCE, 1.0, turn(100 * i as i32 - 150), (1.0, 2.0));
            let shrink = [0.75, 0.0, 0.0, 0.0, 0.75, 0.0, 0.0, 0.0, 1.0];
            (scenes::matmul(&shrink, &t), (96, 72), scenes::same)
        },
    },
    Kind {
        name: "all together",
        made: |i| {
            let base = similarity(
                SOURCE,
                1.05 - 0.02 * i as f64,
                turn(350 - 200 * i as i32),
                (-2.0, 3.0),
            );
            (
                perspective(SOURCE, &base, -0.0005, 0.0003),
                SOURCE,
                scenes::restyled,
            )
        },
    },
];

/// Four scenes, each under every kind: twenty-four pairs.
const SEEDS: [u64; 4] = [3, 5, 8, 13];

/// What one pair came to: the status, and how far the estimate maps the candidate's centre
/// and its worst corner from where the truth maps them, in source pixels.
pub struct Pair {
    pub kind: &'static str,
    pub status: Status,
    pub centre: f64,
    pub corner: f64,
}

/// The errors the record states at most: the bar every pair is held to.
pub const CENTRE_BAR: f64 = 0.1;
pub const CORNER_BAR: f64 = 0.5;

/// The twenty-four pairs, a thread for each scene: four threads, each joined.
pub fn pairs() -> Vec<Pair> {
    whizbang_testkit::threads::scope(|scope| {
        let each: Vec<_> = SEEDS
            .iter()
            .enumerate()
            .map(|(s, seed)| scope.spawn(move || pairs_of(s, *seed)))
            .collect();
        each.into_iter()
            .flat_map(|done| done.join().unwrap())
            .collect()
    })
}

fn pairs_of(s: usize, seed: u64) -> Vec<Pair> {
    let mut out = Vec::new();
    {
        let scene = Scene::random(seed, 160, f64::from(SOURCE.1) / f64::from(SOURCE.0));
        let source = scene.render(SOURCE.0, SOURCE, &IDENTITY, scenes::same);
        for kind in &KINDS {
            let (truth, size, tone) = (kind.made)(s);
            let candidate = scene.render(SOURCE.0, size, &truth, tone);
            let a = align::align(&candidate, &source, &Options::default(), &limits()).unwrap();
            // The estimate takes the candidate's pixels to the source's: the truth's inverse.
            let want = scenes::invert(&truth);
            let got = a.transform.map_or(IDENTITY, |t| t.homography);
            let off = |x: f64, y: f64| {
                let (gx, gy) = scenes::apply(&got, x, y);
                let (wx, wy) = scenes::apply(&want, x, y);
                (gx - wx).hypot(gy - wy)
            };
            let (w, h) = (f64::from(size.0), f64::from(size.1));
            let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)];
            out.push(Pair {
                kind: kind.name,
                status: a.status,
                centre: off(w / 2.0, h / 2.0),
                corner: corners.iter().map(|&(x, y)| off(x, y)).fold(0.0, f64::max),
            });
        }
    }
    out
}

/// The table of the record's rung 2: for each kind, its pairs, how many registered, and the
/// largest errors.
pub fn table(pairs: &[Pair]) -> String {
    let mut text = String::from(
        "| Kind of transform | Pairs | Registered | Centre, at most (px) | Worst corner, at most (px) |\n\
         |---|---|---|---|---|\n",
    );
    for kind in &KINDS {
        let of: Vec<&Pair> = pairs.iter().filter(|p| p.kind == kind.name).collect();
        let registered = of.iter().filter(|p| p.status == Status::Registered).count();
        let centre = of.iter().map(|p| p.centre).fold(0.0, f64::max);
        let corner = of.iter().map(|p| p.corner).fold(0.0, f64::max);
        text.push_str(&format!(
            "| {} | {} | {registered} | {centre:.2} | {corner:.2} |\n",
            kind.name,
            of.len()
        ));
    }
    text
}

#[test]
fn a_known_homography_is_recovered_within_the_records_error_on_the_synthetic_pairs() {
    let pairs = pairs();
    assert_eq!(pairs.len(), 24);
    for pair in &pairs {
        assert_eq!(pair.status, Status::Registered, "{}", pair.kind);
        assert!(
            pair.centre <= CENTRE_BAR,
            "{}: centre {}",
            pair.kind,
            pair.centre
        );
        assert!(
            pair.corner <= CORNER_BAR,
            "{}: corner {}",
            pair.kind,
            pair.corner
        );
    }
    super::align_record::holds(&table(&pairs));
}

#[test]
fn the_aligned_frame_is_bound_onto_the_source_by_its_homography() {
    let (image, against) = (plane("registered"), plane("scene"));
    let aligned = align::align(&image, &against, &Options::default(), &limits()).unwrap();
    let frame = aligned.frame.unwrap();
    let h = aligned.transform.unwrap().homography;
    // The frame is in the other's grid, at its size; its origin names the tool and both
    // images; it is bound onto the image it was warped from, stated from the frame.
    assert_eq!(frame.image.space().width(), against.space().width());
    assert_eq!(frame.image.space().height(), against.space().height());
    assert_eq!(frame.binding.source(), frame.image.identity().pixels);
    assert_eq!(frame.binding.source_space(), frame.image.space());
    let Mapping::Homography { matrix } = frame.binding.mapping() else {
        panic!("{:?}", frame.binding.mapping());
    };
    // The binding is the inverse of the registered homography: a point of the image goes to
    // the frame by the homography, and back to itself through the binding.
    let inverse = scenes::invert(matrix);
    close(
        &scenes::matmul(&inverse, &scenes::invert(&h)),
        &IDENTITY,
        "the binding",
    );
    for (x, y) in [(10.5, 20.25), (64.0, 48.0), (120.0, 90.0)] {
        let (fx, fy) = scenes::apply(&h, x, y);
        let back = frame.binding.to_destination(Point::new(fx, fy)).unwrap();
        assert!(
            (back.x - x).abs() < 1e-9 && (back.y - y).abs() < 1e-9,
            "{back:?}"
        );
    }
    // Where the image has no pixel to give, the frame is black; inside, it shows the image:
    // its centre is the image's pixel that the homography sends there, to within a level.
    let pixels = frame.image.pixels(&limits()).unwrap();
    let source = rgb(&image);
    let (cx, cy) = (64u32, 48u32);
    let (sx, sy) = scenes::apply(
        &scenes::invert(&h),
        f64::from(cx) + 0.5,
        f64::from(cy) + 0.5,
    );
    let near = source.get_pixel(sx as u32, sy as u32).0;
    let at = (cy * pixels.width() + cx) as usize * 4;
    for (k, want) in near.iter().enumerate() {
        assert!(pixels.rgba()[at + k].abs_diff(*want) <= 24, "{k}");
    }
    // A pair that does not register has no frame and no transform.
    let none = align::align(
        &plane("unrelated"),
        &against,
        &Options::default(),
        &limits(),
    )
    .unwrap();
    assert_eq!(none.status, Status::NotRegistered);
    assert!(none.frame.is_none() && none.transform.is_none());
    // One image against itself, and a panorama, are refused as the caller's mistakes.
    let same = align::align(&against, &against, &Options::default(), &limits());
    assert!(matches!(same, Err(align::AlignError::Invalid(_))));
}
