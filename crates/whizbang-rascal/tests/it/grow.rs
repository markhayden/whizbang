//! `grow` on planes and on masks built directly (`whizbang_rascal::grow`, "The rule, in
//! whole numbers"): the core grows into every pixel whose centre is within the radius of
//! a core pixel's centre, a fractional radius included, and into no other; it extends
//! downward and only downward; protect wins; and the refinement is cleared.
//!
//! What is expected here was worked out apart from the code. The counts of a grown
//! point are the numbers of whole-number pairs `(a, b)` with `a * a + b * b` at most `K`,
//! counted by hand. A grown rectangle is every pixel whose distance to the rectangle,
//! measured along each axis to its nearest column and row, is within the radius. The
//! fixed vectors are the digests a separate statement of the rule's four steps gave, in
//! Python's whole numbers and exact fractions, over the three masks stated below.

use std::cell::Cell;
use std::collections::BTreeSet;

use whizbang_core::artifact::{
    BandProfile, Line, Location, Mask, Origin, Refinement, Series, Shape, Source, Step, UseCase,
    Windows,
};
use whizbang_core::identity::{Binding, Digest, PixelDigest, Space};
use whizbang_rascal::grow::{NAME, VERSION, grow, grown, radius_squared, rows_down};

/// A plane `width` by `height` with 255 wherever `set` says, and 0 elsewhere.
pub fn plane(width: usize, height: usize, set: impl Fn(usize, usize) -> bool) -> Vec<u8> {
    (0..width * height)
        .map(|at| if set(at % width, at / width) { 255 } else { 0 })
        .collect()
}

fn never() -> bool {
    false
}

fn grown_of(core: &[u8], width: usize, height: usize, px: f64, down_px: f64) -> Vec<u8> {
    let out = grown(core, width, height, px, down_px, &never).unwrap();
    assert!(out.iter().all(|sample| *sample == 0 || *sample == 255));
    out
}

fn places(plane: &[u8], width: usize) -> BTreeSet<(usize, usize)> {
    let set = plane.iter().enumerate().filter(|(_, sample)| **sample != 0);
    set.map(|(at, _)| (at % width, at / width)).collect()
}

pub fn made_by(action: &str) -> Origin {
    Origin::new(Step::new(action, "1")).unwrap()
}

pub fn on(width: u32, height: u32) -> Binding {
    Binding::root(
        PixelDigest(Digest::of_bytes(b"photo")),
        Space::flat(width, height).unwrap(),
    )
}

/// A mask of an image `width` by `height` with this core and these pixels protected.
pub fn mask_of(width: usize, height: usize, core: Vec<u8>, protect: &[u8]) -> Mask {
    let mut mask = Mask::from_core(
        on(width as u32, height as u32),
        UseCase::Remove,
        core,
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    mask.protect_union(protect).unwrap();
    mask
}

// --- growth by a radius -----------------------------------------------------------------------

#[test]
fn a_point_grows_into_every_pixel_within_the_radius_and_into_no_other() {
    // One core pixel, far enough from every edge. For each radius: K, the whole part
    // of its square, and how many whole-number pairs have a sum of squares within K.
    let (width, height, at) = (41, 31, (20_i64, 15_i64));
    let point = plane(width, height, |x, y| (x as i64, y as i64) == at);
    for (px, k, count) in [
        (0.0, 0, 1),
        (0.99, 0, 1),
        (1.0, 1, 5),
        (1.41, 1, 5),
        (1.5, 2, 9),
        (2.0, 4, 13),
        (2.5, 6, 21),
        (2.9, 8, 25),
        (3.0, 9, 29),
        (3.5, 12, 37),
        (4.0, 16, 49),
        (10.0, 100, 317),
    ] {
        let out = grown_of(&point, width, height, px, 0.0);
        let expected = plane(width, height, |x, y| {
            let (dx, dy) = (x as i64 - at.0, y as i64 - at.1);
            dx * dx + dy * dy <= k
        });
        assert_eq!(places(&out, width), places(&expected, width), "px {px}");
        assert_eq!(places(&out, width).len(), count, "px {px}");
    }
}

#[test]
fn the_radius_is_squared_exactly_and_not_as_doubles_multiply() {
    for (px, k) in [
        (0.0, 0),
        (0.5, 0),
        (0.999_999_999_999_999_9, 0),
        (1.0, 1),
        (1.5, 2),
        (2.5, 6),
        (7.3, 53),
        (10_000.0, 100_000_000),
        // The double nearest the square root of 2 is over it, and the one before is under.
        (std::f64::consts::SQRT_2, 2),
        (f64::from_bits(std::f64::consts::SQRT_2.to_bits() - 1), 1),
        // The double nearest the square root of 13 is under it: its square is under 13.
        (3.605_551_275_463_989, 12),
        (3.605_551_275_463_989_6, 13),
        // Multiplied as doubles, this one's square rounds up to 11. It is under 11.
        (3.316_624_790_355_4, 10),
        (4_294_967_295.0, 18_446_744_065_119_617_025),
        // What is not a radius at all still has an answer, and never a panic.
        (-1.0, 0),
        (-0.0, 0),
        (f64::NAN, 0),
        (5e-324, 0),
        (4_294_967_296.0, u64::MAX),
        (1e300, u64::MAX),
        (f64::INFINITY, u64::MAX),
        (f64::NEG_INFINITY, 0),
    ] {
        assert_eq!(radius_squared(px), k, "px {px}");
    }
    assert_eq!(3.316_624_790_355_4_f64 * 3.316_624_790_355_4_f64, 11.0);
}

#[test]
fn a_rectangle_grows_by_the_distance_to_it() {
    // Columns 10 to 19 and rows 8 to 11 of a 40 by 30 image.
    let (width, height) = (40, 30);
    let block = plane(width, height, |x, y| {
        (10..20).contains(&x) && (8..12).contains(&y)
    });
    // By 3.5 pixels: K is 12. The block has 40 pixels; each side grows by 3 whole
    // rows or columns, 2 * 3 * (10 + 4) pixels; and each corner by the 6 pairs of
    // whole numbers from 1 up whose sum of squares is at most 12.
    for (px, k, count) in [(3.5, 12_i64, 40 + 84 + 24), (2.25, 5, 40 + 56 + 12)] {
        let out = grown_of(&block, width, height, px, 0.0);
        let expected = plane(width, height, |x, y| {
            let beside = |at: usize, first: usize, last: usize| {
                (first.saturating_sub(at) + at.saturating_sub(last)) as i64
            };
            let (dx, dy) = (beside(x, 10, 19), beside(y, 8, 11));
            dx * dx + dy * dy <= k
        });
        assert_eq!(places(&out, width), places(&expected, width), "px {px}");
        assert_eq!(places(&out, width).len(), count, "px {px}");
    }
}

#[test]
fn growth_stops_at_the_edges_of_the_image() {
    let corner = |x: usize, y: usize| plane(5, 4, |px, py| (px, py) == (x, y));
    let out = grown_of(&corner(0, 0), 5, 4, 2.0, 0.0);
    assert_eq!(
        places(&out, 5),
        BTreeSet::from([(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (0, 2)])
    );
    let out = grown_of(&corner(4, 3), 5, 4, 2.0, 0.0);
    assert_eq!(
        places(&out, 5),
        BTreeSet::from([(4, 3), (3, 3), (2, 3), (4, 2), (3, 2), (4, 1)])
    );
    // The largest radius a call may give, and far larger ones: the whole image, at
    // once, with nothing the size of the radius made on the way.
    for px in [10_000.0, 1e300, f64::INFINITY] {
        assert_eq!(grown_of(&corner(2, 1), 5, 4, px, 0.0), vec![255; 20]);
    }
    // Nor does it wrap: on a panorama, whose left and right edges meet, a core at the
    // left edge does not grow across the seam.
    let panorama = Binding::root(
        PixelDigest(Digest::of_bytes(b"panorama")),
        Space::panorama(8, 4).unwrap(),
    );
    let at_the_seam = plane(8, 4, |x, y| (x, y) == (0, 1));
    let mask = Mask::from_core(
        panorama,
        UseCase::Remove,
        at_the_seam,
        Source::default(),
        made_by("fixture"),
    );
    let made = grow(mask.unwrap(), 1.0, None, &never).unwrap().unwrap();
    assert_eq!(
        places(made.mask.core(), 8),
        BTreeSet::from([(0, 0), (0, 1), (1, 1), (0, 2)])
    );
    // Nothing grows from nothing, and what is full stays full.
    assert_eq!(grown_of(&[0; 20], 5, 4, 3.0, 2.0), vec![0; 20]);
    assert_eq!(grown_of(&[255; 20], 5, 4, 3.0, 2.0), vec![255; 20]);
    // One pixel wide, and one pixel high.
    assert_eq!(
        grown_of(&[0, 0, 7, 0, 0, 0], 1, 6, 1.5, 0.0),
        [0, 255, 255, 255, 0, 0]
    );
    assert_eq!(
        grown_of(&[0, 0, 7, 0, 0, 0], 6, 1, 1.5, 9.0),
        [0, 255, 255, 255, 0, 0]
    );
}

/// The rule, as the module states it, pixel by pixel: every pixel against every core
/// pixel, then every pixel against the rows above it.
fn by_the_rule(core: &[u8], width: usize, height: usize, px: f64, down_px: f64) -> Vec<u8> {
    let k = i128::from(radius_squared(px));
    let seeds = places(core, width);
    let grown = plane(width, height, |x, y| {
        seeds.iter().any(|(u, v)| {
            let (dx, dy) = (x as i128 - *u as i128, y as i128 - *v as i128);
            dx * dx + dy * dy <= k
        })
    });
    let rows = rows_down(down_px) as usize;
    plane(width, height, |x, y| {
        (0..=rows.min(y)).any(|up| grown[(y - up) * width + x] != 0)
    })
}

#[test]
fn any_mask_grows_as_the_rule_says_pixel_by_pixel() {
    // Scattered masks of several sizes and densities, each at several radii. The
    // numbers come from one fixed sequence, so the masks are the same on every run.
    let mut state = 12_345_u32;
    let mut next = move || {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345) & 0x7fff_ffff;
        state >> 16
    };
    let mut compared = 0;
    for (width, height, one_in) in [(23, 17, 9), (40, 9, 31), (7, 33, 5), (31, 31, 200)] {
        let core: Vec<u8> = (0..width * height)
            .map(|_| if next() % one_in == 0 { 255 } else { 0 })
            .collect();
        for (px, down_px) in [
            (0.0, 0.0),
            (1.0, 0.0),
            (1.5, 2.0),
            (2.24, 0.0),
            (3.605_551_275_463_989, 0.0),
            (5.1, 7.9),
            (11.0, 1.0),
            (40.0, 40.0),
        ] {
            assert_eq!(
                grown_of(&core, width, height, px, down_px),
                by_the_rule(&core, width, height, px, down_px),
                "{width}x{height}, px {px}, down_px {down_px}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 32);
}

// --- the fixed vectors ------------------------------------------------------------------------

type Scene = (Vec<u8>, Vec<u8>);

/// The first mask of the vectors: a lattice of single pixels, and a sparser lattice protected.
pub fn dots(width: usize, height: usize) -> Scene {
    (
        plane(width, height, |x, y| (x * 7 + y * 13) % 101 == 0),
        plane(width, height, |x, y| (x * 5 + y * 3) % 53 == 0),
    )
}

/// The second: scattered pixels from a stated sequence, and a grid of
/// protected pixels.
pub fn noise(width: usize, height: usize) -> Scene {
    let mut state = 1_u64;
    let core = (0..width * height)
        .map(|_| {
            state = (state * 1_103_515_245 + 12_345) % (1 << 31);
            if (state >> 16).is_multiple_of(23) {
                255
            } else {
                0
            }
        })
        .collect();
    (core, plane(width, height, |x, y| x % 11 == 0 && y % 7 == 0))
}

/// The third: two blocks against the edges, and nothing protected.
pub fn edge(width: usize, height: usize) -> Scene {
    (
        plane(width, height, |x, y| {
            (x < 3 && y >= 25) || (x >= 30 && y < 2)
        }),
        vec![0; width * height],
    )
}

/// One fixed vector: the mask, its size, `px`, `down_px`, and what a separate statement
/// of the rule gave for the new core: how many pixels it has, and the SHA-256 of its
/// plane, one byte a pixel, row by row, 0 or 255.
pub type Vector = (&'static str, usize, usize, f64, f64, usize, &'static str);

// One radius is the double nearest the square root of 2, written out in full.
#[rustfmt::skip]
#[allow(clippy::approx_constant)]
pub const VECTORS: [Vector; 16] = [
    ("dots", 97, 61, 0.0, 0.0, 58, "sha256:44c5b4f8204bf9bad96e4cb7f9f5b66cc59ea49ee2472942edd162599de261e9"),
    ("dots", 97, 61, 1.0, 0.0, 284, "sha256:65936a01c371a95dec28f5c78882ac199be4c70540fbfd2a321853d4071d9517"),
    ("dots", 97, 61, 1.5, 0.0, 508, "sha256:4c760e18fe0599c22fa38f155f0e22ea55735f98bfa61fec3fc42265b10a28ac"),
    ("dots", 97, 61, 2.5, 3.0, 1957, "sha256:f1b89901ecf05b73019344fb12af3d9473a3666e8faa1118f55d0ce38e850b40"),
    ("dots", 97, 61, 3.605551275463989, 0.0, 2036, "sha256:b96d177527815c87ef4e8829b9fede844df53519f6ff669726ec67666db0edd0"),
    ("dots", 97, 61, 3.6055512754639896, 0.0, 2461, "sha256:aea944f32f1b008874a986d08e1944e6a1131e0a50c0e202c58605c0b671d738"),
    ("dots", 97, 61, 7.3, 9.9, 5737, "sha256:c127d8237acf18e64ac0cce55622f3dbfdd7329b3fc8767593b9d5d47b5f3bf9"),
    ("noise", 64, 48, 0.99, 0.0, 129, "sha256:8b67012f157401794f5ea582adaa051cfc60da57181ed56682b421be5570f86c"),
    ("noise", 64, 48, 1.4142135623730951, 0.0, 965, "sha256:cf80f03154b32467d87dcdb6eeec6d879b148cf4ad6ea855b0fa1eaa65bb3482"),
    ("noise", 64, 48, 2.0, 0.5, 1281, "sha256:b5fb0ff6e5d42d1fda1e6e3ee89131087f88dbef73e59af62d89131eb5cc0f4b"),
    ("noise", 64, 48, 0.0, 5.0, 653, "sha256:b4f5a474bdf83d07ea1626f0f1b283c06c291334daa2227b70586baa9f96b2e7"),
    ("noise", 64, 48, 4.75, 2.0, 2896, "sha256:f05ec9a0462efdbe14f33b327a2a8f626fd2c3ca24f45115522d2c602115c6af"),
    ("edge", 40, 30, 2.9, 0.0, 83, "sha256:a729ad98743c1dc2f9f2c04c7323bfad7c66f3223daafc3e26dd4f4b8ac7923b"),
    ("edge", 40, 30, 3.0, 4.0, 155, "sha256:15d32e29d2a69c7665983049fe7126078f386b5103d6b1c1801c69788eedbeb0"),
    ("edge", 40, 30, 12.0, 0.0, 471, "sha256:341f2554273ee0f9e13cf9646cb164ed1e490a945b29d61475f2bb9bb90b11bd"),
    ("edge", 40, 30, 100.0, 100.0, 1200, "sha256:710baa8a6ca5b64e17e58b95140486de08b5dcd1369ef96114c39a72a18647ee"),
];

/// The mask of a vector, before it is grown.
pub fn vector_mask(vector: &Vector) -> Mask {
    let (name, width, height, ..) = *vector;
    let (core, protect) = match name {
        "dots" => dots(width, height),
        "noise" => noise(width, height),
        "edge" => edge(width, height),
        other => panic!("no mask {other}"),
    };
    mask_of(width, height, core, &protect)
}

/// The new core of a vector, as `grow` gives it: its count and its digest.
pub fn vector_result(vector: &Vector) -> (usize, String) {
    let (.., px, down_px, _, _) = *vector;
    let made = grow(vector_mask(vector), px, Some(down_px), &never)
        .unwrap()
        .unwrap();
    assert_eq!(made.core_after as usize, places(made.mask.core(), 1).len());
    (
        made.core_after as usize,
        Digest::of_bytes(made.mask.core()).to_string(),
    )
}

#[test]
fn the_fixed_vectors_have_the_cores_a_separate_statement_of_the_rule_gave() {
    for vector in &VECTORS {
        let (name, _, _, px, down_px, count, digest) = *vector;
        assert_eq!(
            vector_result(vector),
            (count, digest.to_owned()),
            "{name}, px {px}, down_px {down_px}"
        );
    }
}

// --- downward ---------------------------------------------------------------------------------

#[test]
fn down_px_extends_the_grown_core_downward_and_only_downward() {
    // Columns 10 to 19 and rows 8 to 11 again, grown by 2: columns 8 to 21 and rows 6
    // to 13, less the corners. Then 5 rows down.
    let (width, height) = (40, 30);
    let block = plane(width, height, |x, y| {
        (10..20).contains(&x) && (8..12).contains(&y)
    });
    let out = grown_of(&block, width, height, 2.0, 5.0);
    let column =
        |x: usize| -> Vec<usize> { (0..height).filter(|y| out[y * width + x] != 0).collect() };
    // Through the block: rows 6 to 13 grown, and 14 to 18 below them.
    assert_eq!(column(15), (6..=18).collect::<Vec<_>>());
    // At the grown edge, column 8 is grown in rows 8 to 11 only: it reaches row 16.
    assert_eq!(column(8), (8..=16).collect::<Vec<_>>());
    assert_eq!(column(21), (8..=16).collect::<Vec<_>>());
    // Nothing above, and nothing beside.
    assert!(column(7).is_empty() && column(22).is_empty());
    assert_eq!(out[..6 * width], vec![0; 6 * width]);
    // It is the grown core and every pixel under it, 5 rows at most, and no other.
    let alone = grown_of(&block, width, height, 2.0, 0.0);
    let expected = plane(width, height, |x, y| {
        (0..=5.min(y)).any(|up| alone[(y - up) * width + x] != 0)
    });
    assert_eq!(out, expected);

    // It stops at the last row, and a core of one pixel with no radius is a column.
    let point = plane(5, 6, |x, y| (x, y) == (2, 3));
    assert_eq!(
        places(&grown_of(&point, 5, 6, 0.0, 10_000.0), 5),
        BTreeSet::from([(2, 3), (2, 4), (2, 5)])
    );
}

#[test]
fn a_part_of_a_row_extends_nothing() {
    assert_eq!(
        [0.0, 0.5, 0.999, 1.0, 1.5, 4.0, 9.99, 10_000.0].map(rows_down),
        [0, 0, 0, 1, 1, 4, 9, 10_000]
    );
    assert_eq!(
        [-3.0, f64::NAN, f64::NEG_INFINITY].map(rows_down),
        [0, 0, 0]
    );
    let point = plane(3, 5, |x, y| (x, y) == (1, 1));
    let rows = |down_px: f64| places(&grown_of(&point, 3, 5, 0.0, down_px), 3).len();
    assert_eq!([0.5, 0.99, 1.0, 1.99, 2.0].map(rows), [1, 1, 2, 2, 3]);
}

// --- the mask ---------------------------------------------------------------------------------

const WIDTH: usize = 16;
const HEIGHT: usize = 12;

/// A mask that holds everything a refinement must clear or keep: a core in columns 7
/// and 8, a blend beside it, a profile, a tone ramp, and a protected block that the
/// core does not touch and its growth does.
fn band_mask() -> Mask {
    let binding = on(WIDTH as u32, HEIGHT as u32);
    let line = Location::new(
        binding.clone(),
        Shape::Line {
            line: Line::Column(8.0),
            width: 3.0,
        },
        made_by("line"),
    )
    .unwrap();
    let mut mask = Mask::from_core(
        binding.clone(),
        UseCase::Seam,
        plane(WIDTH, HEIGHT, |x, _| x == 7 || x == 8),
        Source {
            locations: vec![line.id()],
            instances: vec![Digest::of_bytes(b"an instance")],
        },
        made_by("profile"),
    )
    .unwrap();
    mask.set_blend(
        (0..WIDTH * HEIGHT)
            .map(|at| {
                if at % WIDTH == 6 || at % WIDTH == 9 {
                    128
                } else {
                    0
                }
            })
            .collect(),
    )
    .unwrap();
    mask.protect_union(&plane(WIDTH, HEIGHT, |x, y| {
        (10..13).contains(&x) && (2..5).contains(&y)
    }))
    .unwrap();
    let series = |name: &str, unit: &str, channels: u32, action: &str| {
        Series::new(
            binding.clone(),
            name,
            unit,
            channels,
            vec![1.0; channels as usize * HEIGHT],
            made_by(action),
        )
        .unwrap()
    };
    let band = BandProfile::new(
        binding.clone(),
        line.id(),
        8,
        1.0,
        vec![1.5; HEIGHT],
        Windows {
            gap: 1,
            step_columns: 3,
            texture_columns: 8,
        },
        "fan-v1",
        made_by("fan"),
    )
    .unwrap();
    mask.set_profile(band, series("sigma_px", "px", 1, "feather"))
        .unwrap();
    mask.push_tone_ramp(series("tone_ramp.step", "levels", 3, "ramp"), 0.5)
        .unwrap();
    mask
}

#[test]
fn protect_takes_precedence_over_the_grown_core() {
    let before = band_mask();
    let made = grow(before.clone(), 3.0, Some(2.0), &never)
        .unwrap()
        .unwrap();
    let after = &made.mask;
    // Columns 4 to 11 are within 3 of the core, in every row. Columns 10 and 11 of
    // rows 2 to 4 are protected: the core is not set there, and is set all around.
    for (at, sample) in after.core().iter().enumerate() {
        let (x, y) = (at % WIDTH, at / WIDTH);
        let protected = (10..13).contains(&x) && (2..5).contains(&y);
        let expected = (4..=11).contains(&x) && !protected;
        assert_eq!(*sample != 0, expected, "({x}, {y})");
        assert_eq!(after.protect()[at] != 0, protected, "({x}, {y})");
        assert_eq!(
            after.weight(x as u32, y as u32),
            Some(if expected { 1.0 } else { 0.0 })
        );
    }
    assert_eq!(after.protect(), before.protect());
    assert_eq!((made.core_before, made.core_after), (24, 8 * 12 - 6));
}

#[test]
fn the_refinement_is_cleared_and_everything_else_is_kept() {
    let before = band_mask();
    assert!(before.profile().is_some() && before.prescriptions().len() == 1);
    assert!(before.blend().iter().any(|sample| *sample != 0));
    let made = grow(before.clone(), 1.0, None, &never).unwrap().unwrap();
    let after = &made.mask;
    assert_eq!(
        made.cleared,
        Refinement {
            blend_cleared: true,
            profile_cleared: true
        }
    );
    assert_eq!(after.blend(), vec![0; WIDTH * HEIGHT]);
    assert!(after.profile().is_none());
    assert!(after.prescriptions().is_empty());
    assert_eq!(after.protect(), before.protect());
    assert_eq!(after.binding(), before.binding());
    assert_eq!(after.use_case(), before.use_case());
    assert_eq!(after.source(), before.source());
    assert_ne!(after.id(), before.id());
    // What comes back is a mask that reads back from its own file as itself.
    let limits = whizbang_core::identity::DecodeLimits::default();
    assert_eq!(
        &Mask::from_bytes(&after.to_bytes(), &limits).unwrap(),
        after
    );

    // By nothing at all, the core is the same and the refinement is cleared all the
    // same: a mask with no refinement comes back with the identity it had.
    let made = grow(before.clone(), 0.0, None, &never).unwrap().unwrap();
    assert_eq!(made.mask.core(), before.core());
    assert!(made.mask.profile().is_none() && made.mask.blend().iter().all(|s| *s == 0));
    let plain = mask_of(5, 4, plane(5, 4, |x, _| x == 2), &[0; 20]);
    let made = grow(plain.clone(), 0.0, None, &never).unwrap().unwrap();
    assert_eq!(made.mask.id(), plain.id());
    assert_eq!(
        made.cleared,
        Refinement {
            blend_cleared: true,
            profile_cleared: false
        }
    );
}

#[test]
fn the_origin_gains_a_step_that_names_the_action_its_arguments_and_its_input() {
    let before = band_mask();
    let step = |arguments: &[(&str, &str)]| {
        let step = Step::new(NAME, VERSION);
        let step = arguments
            .iter()
            .fold(step, |step, (name, value)| step.argument(name, value));
        step.input("mask", before.id().0)
    };
    assert_eq!((NAME, VERSION), ("grow", "1.0.0"));
    for (px, down_px, arguments) in [
        (8.0, None, vec![("px", "8")]),
        (2.5, Some(4.0), vec![("px", "2.5"), ("down_px", "4")]),
        (0.1, Some(0.0), vec![("px", "0.1"), ("down_px", "0")]),
        (
            3.605_551_275_463_989,
            Some(1e-7),
            vec![("px", "3.605551275463989"), ("down_px", "0.0000001")],
        ),
    ] {
        let made = grow(before.clone(), px, down_px, &never).unwrap().unwrap();
        let steps = made.mask.origin().steps();
        assert_eq!(steps[..steps.len() - 1], *before.origin().steps());
        assert_eq!(steps[steps.len() - 1], step(&arguments));
    }
}

#[test]
fn a_grow_that_is_told_to_stop_gives_nothing() {
    // Told at the first look, and at each later one in turn: nothing comes back, and
    // once it is never told, the whole core does.
    let core = plane(9, 200, |x, y| (x + y) % 5 == 0);
    let mut looks = 0;
    loop {
        let asked = Cell::new(0);
        let stopped = || {
            asked.set(asked.get() + 1);
            asked.get() > looks
        };
        match grown(&core, 9, 200, 2.0, 3.0, &stopped) {
            None => looks += 1,
            Some(out) => {
                assert_eq!(out, by_the_rule(&core, 9, 200, 2.0, 3.0));
                break;
            }
        }
        assert!(looks < 64, "it never finished");
    }
    // 200 rows are looked at in three passes, each every 64 rows.
    assert_eq!(looks, 12);
    assert!(grow(band_mask(), 2.0, None, &|| true).unwrap().is_none());
}

#[test]
fn zero_of_either_sign_is_written_as_one_zero() {
    // A recipe may call the function with a zero that carries a sign. It is the same
    // radius, and is written the one way: an origin has no second spelling of it.
    let before = mask_of(5, 4, plane(5, 4, |x, _| x == 2), &[0; 20]);
    let plain = grow(before.clone(), 0.0, Some(0.0), &never)
        .unwrap()
        .unwrap();
    let signed = grow(before, -0.0, Some(-0.0), &never).unwrap().unwrap();
    let last = |mask: &Mask| mask.origin().steps().last().unwrap().arguments.clone();
    let zero = [
        ("px".to_owned(), "0".to_owned()),
        ("down_px".to_owned(), "0".to_owned()),
    ];
    assert_eq!(last(&signed.mask), zero);
    assert_eq!(last(&plain.mask), zero);
    assert_eq!(signed.mask.to_bytes(), plain.mask.to_bytes());
    assert_eq!(whizbang_rascal::grow::text_of(-0.0), "0");
}
