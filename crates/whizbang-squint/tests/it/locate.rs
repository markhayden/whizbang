//! Mapping back: a point of a crop through its binding, and a box on a crop of a crop to
//! the root image (squint's `to_root`, `crates/squint-core/src/frames/mod.rs`).

use whizbang_core::artifact::{ArtifactError, Image};
use whizbang_core::identity::{Binding, Direction, IdentityError, Mapping, Point, Space};
use whizbang_squint::view::{Located, ViewError, box_to_root};

use super::support::*;

/// A known box on a known image, with one marked pixel inside the cut.
pub struct Known {
    pub size: (u32, u32),
    pub bbox: [f64; 4],
}

pub const KNOWN: &[Known] = &[
    Known {
        size: (320, 240),
        bbox: [0.1, 0.2, 0.6, 0.7],
    },
    Known {
        size: (320, 240),
        bbox: [0.1003, 0.2004, 0.6001, 0.7003],
    },
    Known {
        size: (111, 259),
        bbox: [0.0, 0.5001, 0.9999, 1.0],
    },
    Known {
        size: (2048, 96),
        bbox: [0.0, 0.0, 1.0, 1.0],
    },
    Known {
        size: (1500, 110),
        bbox: [0.013, 0.02, 0.991, 0.987],
    },
    Known {
        size: (3000, 40),
        bbox: [0.0, 0.0, 1.0, 1.0],
    },
    Known {
        size: (8000, 16),
        bbox: [0.0, 0.0, 1.0, 1.0],
    },
];

/// What was measured on one known box. Distances are in pixels of the source image
/// unless they say otherwise.
pub struct Measured {
    /// The cut: left, top, right, bottom.
    pub cut: [f64; 4],
    /// The crop's size.
    pub shown: (u32, u32),
    /// Source pixels to one crop pixel, along the more reduced side.
    pub ratio: f64,
    /// How far the cut's edges are outside the box asked for, at most.
    pub snapped: f64,
    /// How far the crop's four corners map from the cut's corners, at most.
    pub corners: f64,
    /// How far a grid of known source points, placed in the crop by the ratio, map from
    /// themselves, at most.
    pub points: f64,
    /// How far the centre of the crop pixel that shows the marked source pixel maps from
    /// the marked pixel's centre.
    pub mark: f64,
    /// The same, in crop pixels.
    pub mark_in_crop: f64,
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

pub fn measure(known: &Known) -> Measured {
    let (width, height) = known.size;
    // Where the cut will be, worked out here from the box: down for the low edges, up
    // for the high ones.
    let asked = [
        known.bbox[0] * f64::from(width),
        known.bbox[1] * f64::from(height),
        known.bbox[2] * f64::from(width),
        known.bbox[3] * f64::from(height),
    ];
    let cut = [
        asked[0].floor(),
        asked[1].floor(),
        asked[2].ceil(),
        asked[3].ceil(),
    ];
    // One white pixel on black, inside the cut and off its centre.
    let mark = (
        (cut[0] + ((cut[2] - cut[0]) * 0.37).floor()) as u32,
        (cut[1] + ((cut[3] - cut[1]) * 0.61).floor()) as u32,
    );
    let image = picture(width, height, |x, y| {
        if (x, y) == mark {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 255]
        }
    });
    let crop = look(&image, known.bbox);
    assert_eq!(cut_of(&crop.binding), cut, "the cut of {:?}", known.bbox);
    let space = crop.image.space();
    let (shown_width, shown_height) = (f64::from(space.width()), f64::from(space.height()));
    let to_source = |x: f64, y: f64| crop.binding.to_destination(Point::new(x, y)).unwrap();

    let snapped = (0..4)
        .map(|k| (asked[k] - cut[k]).abs())
        .fold(0.0, f64::max);
    let corners = [
        ((0.0, 0.0), (cut[0], cut[1])),
        ((shown_width, 0.0), (cut[2], cut[1])),
        ((0.0, shown_height), (cut[0], cut[3])),
        ((shown_width, shown_height), (cut[2], cut[3])),
    ]
    .iter()
    .map(|((x, y), (sx, sy))| distance(to_source(*x, *y), Point::new(*sx, *sy)))
    .fold(0.0, f64::max);

    // Known source points on a grid inside the cut, placed in the crop by the ratio.
    let mut points = 0.0f64;
    for i in 0..=8 {
        for j in 0..=8 {
            let source = Point::new(
                cut[0] + (cut[2] - cut[0]) * f64::from(i) / 8.0,
                cut[1] + (cut[3] - cut[1]) * f64::from(j) / 8.0,
            );
            let x = (source.x - cut[0]) * shown_width / (cut[2] - cut[0]);
            let y = (source.y - cut[1]) * shown_height / (cut[3] - cut[1]);
            points = points.max(distance(to_source(x, y), source));
            // And the other way: the source point's place in the crop.
            let back = crop.binding.to_source(source).unwrap();
            points = points.max(distance(to_source(back.x, back.y), source));
        }
    }

    // The brightest crop pixel shows the mark.
    let rgba = rgba_of(&crop.image);
    let brightest = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .max_by_key(|(at, pixel)| (pixel[0], std::cmp::Reverse(*at)))
        .map(|(at, _)| at as u32)
        .unwrap();
    assert!(rgba[brightest as usize * 4] > 0, "the mark is shown");
    let centre = to_source(
        f64::from(brightest % space.width()) + 0.5,
        f64::from(brightest / space.width()) + 0.5,
    );
    let marked = Point::new(f64::from(mark.0) + 0.5, f64::from(mark.1) + 0.5);
    let ratio_x = (cut[2] - cut[0]) / shown_width;
    let ratio_y = (cut[3] - cut[1]) / shown_height;
    Measured {
        cut,
        shown: (space.width(), space.height()),
        ratio: ratio_x.max(ratio_y),
        snapped,
        corners,
        points,
        mark: distance(centre, marked),
        mark_in_crop: ((centre.x - marked.x).abs() / ratio_x)
            .max((centre.y - marked.y).abs() / ratio_y),
    }
}

#[test]
fn a_location_in_a_crop_maps_back_to_its_source_within_a_pixel() {
    for known in KNOWN {
        let measured = measure(known);
        let name = format!("{:?} of {:?}", known.bbox, known.size);
        // The box is only ever widened, and by less than a pixel.
        assert!(measured.snapped < 1.0, "{name}: {}", measured.snapped);
        // The mapping itself is exact at the corners, and as good as exact inside.
        assert_eq!(measured.corners, 0.0, "{name}");
        assert!(measured.points < 1e-9, "{name}: {}", measured.points);
        // What a crop pixel shows is found again within half a crop pixel, which is
        // within one pixel of the source while the crop is at most twice reduced, and
        // exactly when it is not reduced.
        assert!(
            measured.mark_in_crop <= 0.5 + 1e-9,
            "{name}: {} crop pixels",
            measured.mark_in_crop
        );
        if measured.ratio <= 2.0 {
            assert!(measured.mark < 1.0, "{name}: {}", measured.mark);
        }
        if measured.ratio == 1.0 {
            assert_eq!(measured.mark, 0.0, "{name}");
        }
    }
}

/// A crop of `image` and the chain so far with the crop's binding put first.
fn crop_of(image: &Image, bbox: [f64; 4], chain: &[Binding]) -> (Image, Vec<Binding>) {
    let crop = look(image, bbox);
    let mut longer = vec![crop.binding];
    longer.extend_from_slice(chain);
    (crop.image, longer)
}

#[test]
fn a_box_on_a_crop_of_a_crop_maps_to_its_root() {
    // Squint's case: the top left quarter of a 100 by 100 image, then the bottom right
    // quarter of that. The whole of the inner crop is (25, 25) to (50, 50) of the root.
    let root = picture(100, 100, noisy(31, false));
    let (outer, chain) = crop_of(&root, [0.0, 0.0, 0.5, 0.5], &[]);
    let (inner, chain) = crop_of(&outer, [0.5, 0.5, 1.0, 1.0], &chain);
    assert_eq!(inner.space(), Space::flat(25, 25).unwrap());
    assert_eq!(
        box_to_root([0.0, 0.0, 25.0, 25.0], &chain).unwrap(),
        Located {
            bbox: [25.0, 25.0, 50.0, 50.0],
            partial: false
        }
    );
    // A box inside it.
    assert_eq!(
        box_to_root([5.0, 10.0, 6.5, 20.0], &chain).unwrap(),
        Located {
            bbox: [30.0, 35.0, 31.5, 45.0],
            partial: false
        }
    );
    // Through the outer crop alone, a box is in the outer crop's source.
    assert_eq!(
        box_to_root([25.0, 25.0, 50.0, 50.0], &chain[1..])
            .unwrap()
            .bbox,
        [25.0, 25.0, 50.0, 50.0]
    );
    // What the box shows is what the root shows there.
    let located = box_to_root([5.0, 10.0, 7.0, 20.0], &chain).unwrap().bbox;
    let root_rgba = rgba_of(&root);
    let inner_rgba = rgba_of(&inner);
    for (x, y) in [(5u32, 10u32), (6, 19)] {
        let (rx, ry) = (located[0] as u32 + (x - 5), located[1] as u32 + (y - 10));
        let in_root = (ry * 100 + rx) as usize * 4;
        let in_crop = (y * 25 + x) as usize * 4;
        assert_eq!(
            root_rgba[in_root..in_root + 4],
            inner_rgba[in_crop..in_crop + 4]
        );
    }
}

#[test]
fn a_box_maps_back_through_reduced_crops() {
    // 3000 wide: the whole is shown 1024 wide; then the right half of that, 512 wide.
    let root = picture(3000, 375, |x, y| [(x / 12) as u8, (y / 2) as u8, 0, 255]);
    let (outer, chain) = crop_of(&root, [0.0, 0.0, 1.0, 1.0], &[]);
    assert_eq!(outer.space(), Space::flat(1024, 128).unwrap());
    let (inner, chain) = crop_of(&outer, [0.5, 0.25, 1.0, 0.75], &chain);
    assert_eq!(inner.space(), Space::flat(512, 64).unwrap());
    // The inner crop's edges are the root's columns 1500 and 3000 and rows 93.75 and
    // 281.25, exactly, through both bindings.
    assert_eq!(
        box_to_root([0.0, 0.0, 512.0, 64.0], &chain).unwrap(),
        Located {
            bbox: [1500.0, 93.75, 3000.0, 281.25],
            partial: false
        }
    );
    let located = box_to_root([128.0, 16.0, 256.0, 32.0], &chain).unwrap();
    let expected = [1875.0, 140.625, 2250.0, 187.5];
    for (got, want) in located.bbox.iter().zip(expected) {
        assert!((got - want).abs() < 1e-9, "{:?}", located.bbox);
    }
}

#[test]
fn a_box_past_the_crop_is_clipped_and_flagged() {
    let root = picture(100, 100, noisy(32, false));
    let (_, chain) = crop_of(&root, [0.5, 0.5, 1.0, 1.0], &[]);
    assert_eq!(
        box_to_root([-10.0, 20.0, 30.0, 80.0], &chain).unwrap(),
        Located {
            bbox: [50.0, 70.0, 80.0, 100.0],
            partial: true
        }
    );
    // On the crop's edges is not past them.
    assert!(!box_to_root([0.0, 0.0, 50.0, 50.0], &chain).unwrap().partial);
    let refused = |bbox: [f64; 4]| match box_to_root(bbox, &chain) {
        Err(ViewError::Invalid(why)) => why,
        other => panic!("{bbox:?}: expected a refusal of the box, got {other:?}"),
    };
    assert!(refused([50.0, 10.0, 60.0, 20.0]).contains("wholly outside"));
    assert!(refused([10.0, -20.0, 20.0, 0.0]).contains("wholly outside"));
    assert!(refused([20.0, 10.0, 10.0, 20.0]).contains("left < right"));
    assert!(refused([10.0, 20.0, 20.0, 20.0]).contains("top < bottom"));
    assert!(refused([f64::NAN, 0.0, 10.0, 10.0]).contains("left < right"));
}

#[test]
fn a_chain_that_is_not_crops_end_to_end_is_refused() {
    let refused = |chain: &[Binding]| match box_to_root([0.0, 0.0, 10.0, 10.0], chain) {
        Err(ViewError::Artifact(ArtifactError::Identity(IdentityError::Binding(why)))) => why,
        other => panic!("expected a refusal of the chain, got {other:?}"),
    };
    let root = picture(100, 100, noisy(33, false));
    let other = picture(100, 100, noisy(34, false));
    let (outer, chain) = crop_of(&root, [0.0, 0.0, 0.5, 0.5], &[]);
    let (_, unrelated) = crop_of(&other, [0.0, 0.0, 0.5, 0.5], &[]);
    let (_, inner) = crop_of(&outer, [0.5, 0.5, 1.0, 1.0], &chain);

    assert!(refused(&[]).contains("at least one binding"));
    // The second binding is of another image than the first one's destination.
    let broken = [inner[0].clone(), unrelated[0].clone()];
    assert!(refused(&broken).contains("does not start where the first ends"));
    // An image in its own space is not a step of a chain.
    assert!(refused(&[root.binding()]).contains("is not one"));
    // Nor is a crop stated the other way round, or a homography.
    assert!(refused(&[chain[0].reversed().unwrap()]).contains("is not one"));
    let shift = Binding::new(
        outer.identity().pixels,
        outer.space(),
        root.identity().pixels,
        root.space(),
        Direction::SourceToDestination,
        Mapping::Homography {
            matrix: [1.0, 0.0, 5.0, 0.0, 1.0, 5.0, 0.0, 0.0, 1.0],
        },
    )
    .unwrap();
    assert!(refused(&[shift]).contains("is not one"));
}
