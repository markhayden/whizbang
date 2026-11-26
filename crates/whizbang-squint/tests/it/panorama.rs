//! `view` on panoramas (plan mE, task P1, items 1 and 3): a crop of a panorama by box, the
//! square perspective view by yaw, pitch and field of view, bound onto its panorama by
//! ADR 0051's mapping, and what is refused. Squint's stored-view and analytic tests of its
//! perspective view (`crates/squint-tools/tests/it/builtin.rs`,
//! `view_perspective_matches_the_stored_view_and_the_analytic_rendering`) run here on
//! squint's own two files (`fixtures/view/`, from squint's `fixtures/images/view/` at the
//! commit `docs/sources.toml` pins), and make the third table of the record's
//! measurements.

use std::f64::consts::{FRAC_PI_2, PI};
use std::path::PathBuf;

use whizbang_core::artifact::Image;
use whizbang_core::identity::{
    Destination, Direction, Mapping, Perspective, Point, Projection, Space, Topology,
};
use whizbang_squint::view::{Crop, MAX_SIDE, MIN_VIEW, Region, Request, ViewError, side, view};

use super::record;
use super::scenes::Rng;
use super::support::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/view")
}

/// Squint's panorama of its stored-view test: 512 by 256, the scene of
/// [`SphereScene::random`] with seed 7 and 120 blobs, as squint rendered it.
pub fn squints_panorama() -> Image {
    let file = std::fs::read(fixtures().join("pano.png")).unwrap();
    image_in(file, Projection::Equirectangular, Topology::Wrap)
}

/// A whole panorama, painted.
pub fn panorama(width: u32, height: u32, colour: impl Fn(u32, u32) -> [u8; 4]) -> Image {
    let file = png(width, height, &paint(width, height, colour));
    image_in(file, Projection::Equirectangular, Topology::Wrap)
}

pub fn angles(yaw: f64, pitch: f64, fov: f64) -> Region {
    Region::Perspective(Perspective { yaw, pitch, fov })
}

/// The one view of a panorama by these angles.
pub fn seen(image: &Image, yaw: f64, pitch: f64, fov: f64) -> Crop {
    let request = Request {
        image,
        also: None,
        region: angles(yaw, pitch, fov),
    };
    let mut crops = view(&request, &limits()).unwrap();
    assert_eq!(crops.len(), 1);
    crops.remove(0)
}

fn refused(request: &Request<'_>) -> String {
    match view(request, &limits()) {
        Err(ViewError::Invalid(why)) => why,
        other => panic!("expected a refusal of the request, got {other:?}"),
    }
}

/// Squint's `SphereScene` (`crates/squint-testkit/src/images.rs`): blobs around unit
/// vectors, each a Gaussian of the angle from its centre. Its functions are `libm`'s, so
/// the measurements made against it are the same on every machine.
pub struct SphereScene {
    /// The centre, the angular sigma in radians, and the amplitude of each channel.
    blobs: Vec<([f64; 3], f64, [f64; 3])>,
    base: [f64; 3],
}

impl SphereScene {
    pub fn random(seed: u64, blobs: usize) -> SphereScene {
        let mut rng = Rng::new(seed);
        let base = [
            rng.range(0.35, 0.65),
            rng.range(0.35, 0.65),
            rng.range(0.35, 0.65),
        ];
        let blobs = (0..blobs)
            .map(|_| {
                // Uniform on the sphere.
                let z = rng.range(-1.0, 1.0);
                let lon = rng.range(-PI, PI);
                let r = libm::sqrt(1.0 - z * z);
                let centre = [r * libm::cos(lon), r * libm::sin(lon), z];
                let sigma = 0.01 * libm::pow(10.0, rng.range(0.0, 1.5));
                let amp = rng.range(0.25, 0.6) * if rng.range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
                let rgb = [
                    amp * rng.range(0.5, 1.0),
                    amp * rng.range(0.5, 1.0),
                    amp * rng.range(0.5, 1.0),
                ];
                (centre, sigma, rgb)
            })
            .collect();
        SphereScene { blobs, base }
    }

    /// The colour in direction `p`, a unit vector, as 8-bit levels. A blob adds nothing
    /// where its Gaussian is at most `1e-6`, as in squint; one more than 5.3 sigmas away
    /// is passed over before its angle is worked out, since it is under that there.
    pub fn at(&self, p: [f64; 3]) -> [u8; 3] {
        let mut c = self.base;
        for (centre, sigma, rgb) in &self.blobs {
            let dot = (p[0] * centre[0] + p[1] * centre[1] + p[2] * centre[2]).clamp(-1.0, 1.0);
            if 5.3 * sigma < PI && dot < libm::cos(5.3 * sigma) {
                continue;
            }
            let angle = libm::acos(dot);
            let g = libm::exp(-(angle * angle) / (2.0 * sigma * sigma));
            if g > 1e-6 {
                for (channel, amp) in c.iter_mut().zip(rgb) {
                    *channel += amp * g;
                }
            }
        }
        c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
    }
}

/// The direction of a point of a panorama `width` by `height`, as ADR 0001 defines it.
fn direction(point: Point, width: u32, height: u32) -> [f64; 3] {
    let lon = point.x / f64::from(width) * 2.0 * PI - PI;
    let lat = FRAC_PI_2 - point.y / f64::from(height) * PI;
    let cos_lat = libm::cos(lat);
    [
        cos_lat * libm::cos(lon),
        cos_lat * libm::sin(lon),
        libm::sin(lat),
    ]
}

/// The views the record measures, of squint's panorama: squint's own, one across the
/// seam, and one up to the north pole.
pub const LOOKS: [(f64, f64, f64); 3] =
    [(30.0, 10.0, 60.0), (-175.0, 0.0, 40.0), (0.0, 80.0, 90.0)];

/// What one view of squint's panorama measures.
pub struct Compared {
    pub look: (f64, f64, f64),
    pub side: u32,
    /// Against squint's stored view, which it has of the first look alone: the largest
    /// difference, in levels, and the share of samples that differ.
    pub stored: Option<(u8, f64)>,
    /// Against the scene seen along each pixel's direction: the largest difference, in
    /// levels, and the mean.
    pub scene: (u8, f64),
    /// How far a point of the view lands from itself, through its binding onto the
    /// panorama and back: the most of 81, in view pixels.
    pub round_trip: f64,
}

/// The largest and the mean difference of two pictures' red, green and blue, and the share
/// of those samples that differ.
fn differences(a: &[u8], b: &[[u8; 3]]) -> (u8, f64, f64) {
    let all: Vec<u8> = a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b)
        .flat_map(|(p, q)| (0..3).map(move |k| p[k].abs_diff(q[k])))
        .collect();
    let count = all.len() as f64;
    let largest = all.iter().copied().max().unwrap_or(0);
    let mean = all.iter().map(|d| f64::from(*d)).sum::<f64>() / count;
    let share = all.iter().filter(|d| **d > 0).count() as f64 / count;
    (largest, mean, share)
}

pub fn compared() -> Vec<Compared> {
    let panorama = squints_panorama();
    let scene = SphereScene::random(7, 120);
    let stored = image_in(
        std::fs::read(fixtures().join("view-yaw30-pitch10-fov60.png")).unwrap(),
        Projection::Flat,
        Topology::Bounded,
    );
    let stored: Vec<[u8; 3]> = rgba_of(&stored)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    LOOKS
        .iter()
        .map(|&(yaw, pitch, fov)| {
            let crop = seen(&panorama, yaw, pitch, fov);
            let side = crop.image.space().width();
            let shown = rgba_of(&crop.image);
            let mut analytic = Vec::with_capacity(side as usize * side as usize);
            for v in 0..side {
                for u in 0..side {
                    let centre = Point::new(f64::from(u) + 0.5, f64::from(v) + 0.5);
                    let there = crop.binding.to_destination(centre).unwrap();
                    analytic.push(scene.at(direction(there, 512, 256)));
                }
            }
            let (largest, mean, _) = differences(&shown, &analytic);
            let against = ((yaw, pitch, fov) == LOOKS[0]).then(|| {
                let (largest, _, share) = differences(&shown, &stored);
                (largest, share)
            });
            let mut round_trip: f64 = 0.0;
            for i in 0..=8 {
                for j in 0..=8 {
                    let at = |k: u32| f64::from(k) * f64::from(side) / 8.0;
                    let point = Point::new(at(i), at(j));
                    let there = crop.binding.to_destination(point).unwrap();
                    let back = crop.binding.to_source(there).unwrap();
                    round_trip = round_trip
                        .max((back.x - point.x).abs())
                        .max((back.y - point.y).abs());
                }
            }
            Compared {
                look: (yaw, pitch, fov),
                side,
                stored: against,
                scene: (largest, mean),
                round_trip,
            }
        })
        .collect()
}

/// The record's third table of measurements: one row for each view of [`LOOKS`].
pub fn perspective_table(compared: &[Compared]) -> String {
    let mut out = String::from(
        "| Panorama | Yaw, pitch and fov | Shown at | Against squint's stored view: largest \
         difference, in levels | Samples that differ from it | Against the scene: largest \
         difference, in levels | Mean difference | A point through the binding and back, at \
         most, in view pixels |\n|---|---|---|---|---|---|---|---|\n",
    );
    for c in compared {
        let (stored, share) = match c.stored {
            Some((largest, share)) => (largest.to_string(), format!("{:.1}%", 100.0 * share)),
            None => ("none stored".to_owned(), "none stored".to_owned()),
        };
        let round_trip = if c.round_trip == 0.0 {
            "0".to_owned()
        } else {
            format!("{:.0e}", c.round_trip)
        };
        out.push_str(&format!(
            "| squint's, 512x256 | {}, {}, {} | {}x{} | {stored} | {share} | {} | {:.3} | {round_trip} |\n",
            c.look.0, c.look.1, c.look.2, c.side, c.side, c.scene.0, c.scene.1,
        ));
    }
    out
}

#[test]
fn a_perspective_view_is_squints_stored_view_and_the_scene_it_shows() {
    let compared = compared();
    for c in &compared {
        // A 512-pixel panorama is too narrow for its views' own resolution: each is shown
        // at the least side.
        assert_eq!(c.side, MIN_VIEW);
        // Squint's bounds: sharp blob edges resampled from 8-bit pixels differ by more
        // at single pixels, and the mean says it is the right view.
        assert!(
            c.scene.0 <= 32 && c.scene.1 < 0.5,
            "{:?}: {:?}",
            c.look,
            c.scene
        );
        assert!(c.round_trip < 1e-6, "{:?}: {}", c.look, c.round_trip);
    }
    // Squint's own view, against the file it stored: squint's test allowed two levels,
    // for the platform's functions against `libm`'s; there is not one, and `libm` gives
    // the same on every machine, so none is allowed.
    let (largest, _) = compared[0].stored.unwrap();
    assert_eq!(largest, 0, "the stored view differs by {largest}");
    record::holds(2, &perspective_table(&compared));
}

#[test]
fn a_perspective_view_has_the_same_pixels_on_every_machine() {
    // Worked out with `libm` alone, so this digest holds on macOS and on Linux.
    let crop = seen(&squints_panorama(), 30.0, 10.0, 60.0);
    assert_eq!(
        crop.image.identity().pixels.to_string(),
        "sha256:5e37f8dc7caa6555720fe5b832c743915cea9dc34ba02790bb3d9b71b1b6c986"
    );
}

#[test]
fn a_perspective_view_is_bound_onto_its_panorama_and_says_what_made_it() {
    // Red counts columns six at a time and green rows three at a time, so a pixel tells
    // where it was sampled.
    let pano = panorama(1440, 720, |x, y| [(x / 6) as u8, (y / 3) as u8, 0, 255]);
    let crop = seen(&pano, -90.0, 20.0, 90.0);
    // A quarter of 1440 is 360 pixels across, shown pixel for pixel.
    assert_eq!(side(1440, 90.0), 360);
    assert_eq!(crop.image.space(), Space::flat(360, 360).unwrap());
    let binding = &crop.binding;
    assert_eq!(binding.source(), crop.image.identity().pixels);
    assert_eq!(
        binding.destination(),
        Destination::Image(pano.identity().pixels)
    );
    assert_eq!(binding.destination_space(), pano.space());
    assert_eq!(binding.direction(), Direction::SourceToDestination);
    assert_eq!(
        *binding.mapping(),
        Mapping::Perspective {
            yaw: -90.0,
            pitch: 20.0,
            fov: 90.0
        }
    );
    // The middle of the view is where it looks: 90 degrees left of the middle column,
    // 20 above the horizon.
    let middle = binding.to_destination(Point::new(180.0, 180.0)).unwrap();
    assert!((middle.x - 360.0).abs() < 1e-9 && (middle.y - 280.0).abs() < 1e-9);
    // Each pixel shows the panorama where its binding puts the pixel's centre, to
    // within the blend of two neighbours.
    let shown = rgba_of(&crop.image);
    for v in (0..360u32).step_by(7) {
        for u in (0..360u32).step_by(11) {
            let at = binding
                .to_destination(Point::new(f64::from(u) + 0.5, f64::from(v) + 0.5))
                .unwrap();
            let i = ((v * 360 + u) * 4) as usize;
            let (red, green) = (f64::from(shown[i]), f64::from(shown[i + 1]));
            assert!(
                (red - (at.x - 0.5) / 6.0).abs() <= 1.0
                    && (green - (at.y - 0.5) / 3.0).abs() <= 1.0,
                "pixel ({u}, {v}) shows ({red}, {green}), its binding says {at:?}"
            );
        }
    }
    let step = &crop.image.origin().steps()[0];
    assert_eq!((step.action.as_str(), step.version.as_str()), ("view", "1"));
    let arguments: Vec<(&str, &str)> = step
        .arguments
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    assert_eq!(arguments, [("yaw", "-90"), ("pitch", "20"), ("fov", "90")]);
}

#[test]
fn a_perspective_view_wraps_across_the_seam_and_is_held_to_its_least_and_greatest_side() {
    // The left half of this panorama is dark and the right half light; the seam is
    // between its last column and its first.
    let pano = panorama(2048, 1024, |x, _| {
        let level = if x < 1024 { 40 } else { 220 };
        [level, level, level, 255]
    });
    // At yaw -180 the view looks at the seam: its left half shows the panorama's right
    // edge, light, and its right half the left edge, dark, with no gap between.
    let crop = seen(&pano, -180.0, 0.0, 60.0);
    let across = crop.image.space().width() as usize;
    assert_eq!(across, 341);
    let shown = rgba_of(&crop.image);
    let row = 170 * across * 4;
    assert_eq!(shown[row], 220);
    assert_eq!(shown[row + (across - 1) * 4], 40);
    // Across the middle the level steps from one to the other through a blend of the
    // two, and nowhere else.
    let levels: Vec<u8> = (0..across).map(|u| shown[row + u * 4]).collect();
    let steps = levels.windows(2).filter(|w| w[0] != w[1]).count();
    assert!((1..=3).contains(&steps), "{levels:?}");
    // The side follows the field of view between its bounds.
    assert_eq!(side(2048, 10.0), MIN_VIEW);
    assert_eq!(side(4096, 120.0), MAX_SIDE);
    assert_eq!(side(2048, 120.0), 683);
}

#[test]
fn a_panorama_is_cut_by_box_and_its_whole_stays_a_panorama() {
    // A smooth pattern, cheap to write and read at this size, that still tells each
    // pixel from its neighbours.
    let colour = |x: u32, y: u32| [(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8, 255];
    let pano = panorama(480, 240, colour);
    // A box of a panorama is a flat crop, bound onto it by the box, as on a flat image.
    let crop = look(&pano, [0.0, 0.25, 0.25, 0.75]);
    assert_eq!(crop.image.space(), Space::flat(120, 120).unwrap());
    assert_eq!(cut_of(&crop.binding), [0.0, 60.0, 120.0, 180.0]);
    let expected = paint(120, 120, |x, y| colour(x, y + 60));
    assert_eq!(rgba_of(&crop.image), expected);
    // One that reaches the right edge holds the panorama's last column, and its right edge
    // is the seam.
    let right = look(&pano, [0.75, 0.0, 1.0, 0.5]);
    assert_eq!(cut_of(&right.binding), [360.0, 0.0, 480.0, 120.0]);
    let edge = right
        .binding
        .to_destination(Point::new(120.0, 10.0))
        .unwrap();
    assert_eq!(edge, Point::new(0.0, 10.0));
    // The whole of it is still a panorama, and so is the whole of part of one. (A whole
    // panorama over the bound is scaled down as a flat image is, and stays one.)
    let whole = look(&pano, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(whole.image.space(), pano.space());
    let bounded = image_in(
        png(64, 32, &paint(64, 32, noisy(22, false))),
        Projection::Equirectangular,
        Topology::Bounded,
    );
    assert_eq!(
        look(&bounded, [0.0, 0.0, 1.0, 1.0]).image.space(),
        Space::new(64, 32, Projection::Equirectangular, Topology::Bounded).unwrap()
    );
    // And a whole panorama can be looked into again by angles.
    let again = seen(&whole.image, 0.0, 0.0, 90.0);
    assert_eq!(again.image.space(), Space::flat(256, 256).unwrap());
    // Two panoramas show the same region, each bound onto its own.
    let smaller = panorama(360, 180, |x, y| colour(y, x));
    let both = view(
        &Request {
            image: &pano,
            also: Some(&smaller),
            region: angles(45.0, -10.0, 120.0),
        },
        &limits(),
    )
    .unwrap();
    let onto: Vec<Destination> = both.iter().map(|c| c.binding.destination()).collect();
    assert_eq!(
        onto,
        [
            Destination::Image(pano.identity().pixels),
            Destination::Image(smaller.identity().pixels)
        ]
    );
}

#[test]
fn a_second_image_of_another_projection_is_refused() {
    // Squint's `view_shows_the_same_region_in_two_frames_of_one_projection`, its refusal.
    let flat = picture(256, 192, noisy(23, false));
    let pano = panorama(128, 64, noisy(24, false));
    let why = refused(&Request {
        image: &flat,
        also: Some(&pano),
        region: Region::Box([0.5, 0.5, 1.0, 1.0]),
    });
    assert!(
        why.contains("also is a panorama and image is flat"),
        "{why}"
    );
    let why = refused(&Request {
        image: &pano,
        also: Some(&flat),
        region: angles(0.0, 0.0, 60.0),
    });
    assert!(
        why.contains("also is flat and image is a panorama"),
        "{why}"
    );
}

#[test]
fn angles_out_of_bounds_or_on_a_flat_image_are_the_callers_mistake() {
    // Squint's `view_refuses_bad_arguments_as_the_models_mistake`, its cases of angles.
    let flat = picture(256, 192, noisy(25, false));
    let pano = panorama(256, 128, noisy(26, false));
    let part = image_in(
        png(256, 128, &paint(256, 128, noisy(27, false))),
        Projection::Equirectangular,
        Topology::Bounded,
    );
    let on = |image, yaw, pitch, fov| Request {
        image,
        also: None,
        region: angles(yaw, pitch, fov),
    };
    let cases = [
        (on(&pano, 0.0, 0.0, 130.0), "fov 130"),
        (on(&pano, 0.0, 0.0, 9.5), "fov 9.5"),
        (on(&pano, 180.5, 0.0, 60.0), "yaw 180.5"),
        (on(&pano, f64::NAN, 0.0, 60.0), "yaw NaN"),
        (on(&pano, 0.0, 95.0, 60.0), "pitch 95"),
        // Angles on a flat image: the refusal says what to send instead, since a model
        // told only that the angles do not apply repeats the same call.
        (on(&flat, 0.0, 0.0, 60.0), "image is flat: send bbox"),
        (on(&part, 0.0, 0.0, 60.0), "part of a panorama"),
    ];
    for (request, needle) in cases {
        let why = refused(&request);
        assert!(why.contains(needle), "{why:?} should mention {needle:?}");
    }
    // A box past the right edge of a panorama is refused: the seam is crossed by two
    // boxes, never one.
    let why = refused(&Request {
        image: &pano,
        also: None,
        region: Region::Box([0.9, 0.25, 1.1, 0.75]),
    });
    assert!(why.contains("fractions"), "{why}");
    // The bounds themselves are in, and yaw 180 is the seam, stated as -180.
    for (yaw, pitch, fov) in [(-180.0, -90.0, 10.0), (179.5, 90.0, 120.0)] {
        seen(&pano, yaw, pitch, fov);
    }
    let at_seam = seen(&pano, 180.0, 0.0, 60.0);
    assert_eq!(at_seam, seen(&pano, -180.0, 0.0, 60.0));
    assert_eq!(
        *at_seam.binding.mapping(),
        Mapping::Perspective {
            yaw: -180.0,
            pitch: 0.0,
            fov: 60.0
        }
    );
}
