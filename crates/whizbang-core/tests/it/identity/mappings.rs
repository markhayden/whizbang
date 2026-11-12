//! Mappings between two spaces, in the direction their parameters are stated: crops,
//! homographies, rotations on the sphere, and the wrapped cut.

use whizbang_core::identity::{IdentityError, Mapping, Point, Projection, Space, Topology};

fn close(a: Point, b: Point, tolerance: f64) -> bool {
    (a.x - b.x).abs() <= tolerance && (a.y - b.y).abs() <= tolerance
}

fn refused<T: std::fmt::Debug>(result: Result<T, IdentityError>) -> bool {
    matches!(result, Err(IdentityError::Binding(_)))
}

fn outside<T: std::fmt::Debug>(result: Result<T, IdentityError>) -> bool {
    matches!(result, Err(IdentityError::Outside(_)))
}

// --- identity -----------------------------------------------------------------------------

#[test]
fn the_identity_needs_two_equal_spaces() {
    let flat = Space::flat(8, 4).unwrap();
    Mapping::Identity.check(&flat, &flat).unwrap();
    assert_eq!(
        Mapping::Identity
            .forward(&flat, &flat, Point::new(3.0, 1.5))
            .unwrap(),
        Point::new(3.0, 1.5)
    );
    assert!(refused(
        Mapping::Identity.check(&flat, &Space::flat(8, 5).unwrap())
    ));
    assert!(refused(
        Mapping::Identity.check(&flat, &Space::panorama(8, 4).unwrap())
    ));
}

// --- crop ---------------------------------------------------------------------------------

#[test]
fn a_crop_maps_its_corners_onto_its_box_and_refuses_what_lies_outside() {
    let whole = Space::flat(320, 200).unwrap();
    let crop = Space::flat(64, 60).unwrap();
    let mapping = Mapping::Crop {
        left: 64.0,
        top: 40.0,
        right: 128.0,
        bottom: 100.0,
    };
    mapping.check(&crop, &whole).unwrap();
    for (from, to) in [
        ((0.0, 0.0), (64.0, 40.0)),
        ((64.0, 60.0), (128.0, 100.0)),
        ((10.5, 20.5), (74.5, 60.5)),
    ] {
        let (from, to) = (Point::new(from.0, from.1), Point::new(to.0, to.1));
        assert_eq!(mapping.forward(&crop, &whole, from).unwrap(), to);
        assert_eq!(mapping.backward(&crop, &whole, to).unwrap(), from);
    }
    // A point of the whole image that the crop does not cover has no place in the crop.
    for (x, y) in [(63.9, 50.0), (128.1, 50.0), (100.0, 39.9), (100.0, 100.1)] {
        assert!(
            outside(mapping.backward(&crop, &whole, Point::new(x, y))),
            "({x}, {y})"
        );
    }
    // A point outside the crop's own space is refused before it is mapped.
    assert!(outside(mapping.forward(
        &crop,
        &whole,
        Point::new(64.5, 1.0)
    )));
}

#[test]
fn a_downscaled_crop_maps_through_its_scale() {
    let whole = Space::flat(1000, 800).unwrap();
    // The box is 600 by 300; the crop was rendered at a third of that.
    let crop = Space::flat(200, 100).unwrap();
    let mapping = Mapping::Crop {
        left: 100.0,
        top: 200.0,
        right: 700.0,
        bottom: 500.0,
    };
    mapping.check(&crop, &whole).unwrap();
    let centre = mapping
        .forward(&crop, &whole, Point::new(100.0, 50.0))
        .unwrap();
    assert!(close(centre, Point::new(400.0, 350.0), 1e-9), "{centre:?}");
    let back = mapping.backward(&crop, &whole, centre).unwrap();
    assert!(close(back, Point::new(100.0, 50.0), 1e-9), "{back:?}");
}

#[test]
fn a_crop_that_is_empty_or_leaves_its_parent_is_refused() {
    let whole = Space::flat(320, 200).unwrap();
    let crop = Space::flat(64, 60).unwrap();
    for (left, top, right, bottom) in [
        (64.0, 40.0, 64.0, 100.0),
        (64.0, 100.0, 128.0, 40.0),
        (0.0, 0.0, 321.0, 10.0),
        (-1.0, 0.0, 10.0, 10.0),
        (0.0, 0.0, 10.0, 200.5),
        (f64::NAN, 0.0, 10.0, 10.0),
    ] {
        let mapping = Mapping::Crop {
            left,
            top,
            right,
            bottom,
        };
        assert!(
            refused(mapping.check(&crop, &whole)),
            "{left} {top} {right} {bottom}"
        );
    }
}

#[test]
fn a_box_of_a_panorama_is_flat_and_only_its_whole_frame_stays_a_panorama() {
    let panorama = Space::panorama(1024, 512).unwrap();
    let quarter = Mapping::Crop {
        left: 0.0,
        top: 128.0,
        right: 256.0,
        bottom: 384.0,
    };
    quarter
        .check(&Space::flat(256, 256).unwrap(), &panorama)
        .unwrap();
    // A quarter of a panorama is no sphere.
    let part = Space::new(256, 256, Projection::Equirectangular, Topology::Bounded).unwrap();
    assert!(refused(quarter.check(&part, &panorama)));
    assert!(refused(
        quarter.check(&Space::panorama(256, 256).unwrap(), &panorama)
    ));
    // The whole frame at half the size is still the panorama, and is not a flat picture.
    let whole = Mapping::Crop {
        left: 0.0,
        top: 0.0,
        right: 1024.0,
        bottom: 512.0,
    };
    whole
        .check(&Space::panorama(512, 256).unwrap(), &panorama)
        .unwrap();
    assert!(refused(
        whole.check(&Space::flat(512, 256).unwrap(), &panorama)
    ));
}

// --- homography ---------------------------------------------------------------------------

#[test]
fn a_homography_maps_in_the_direction_it_states() {
    let from = Space::flat(100, 100).unwrap();
    let to = Space::flat(400, 300).unwrap();
    // Twice the size, moved 10 right and 20 down, with a little perspective.
    let mapping = Mapping::Homography {
        matrix: [2.0, 0.0, 10.0, 0.0, 2.0, 20.0, 0.001, 0.0, 1.0],
    };
    mapping.check(&from, &to).unwrap();
    assert_eq!(
        mapping.forward(&from, &to, Point::new(0.0, 0.0)).unwrap(),
        Point::new(10.0, 20.0)
    );
    // At (50, 50): x = 110 / 1.05, y = 120 / 1.05.
    let mapped = mapping.forward(&from, &to, Point::new(50.0, 50.0)).unwrap();
    assert!(
        close(mapped, Point::new(110.0 / 1.05, 120.0 / 1.05), 1e-12),
        "{mapped:?}"
    );
    let back = mapping.backward(&from, &to, mapped).unwrap();
    assert!(close(back, Point::new(50.0, 50.0), 1e-9), "{back:?}");
}

#[test]
fn a_singular_homography_is_refused() {
    let space = Space::flat(100, 100).unwrap();
    for matrix in [
        // The second row is twice the first.
        [1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 0.0, 0.0, 1.0],
        [0.0; 9],
        // Everything lands on one line.
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        // Singular to within rounding, at any scale.
        [1e6, 2e6, 3e6, 2e6, 4e6 + 1e-9, 6e6, 0.0, 0.0, 1e6],
        [f64::NAN, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        [f64::INFINITY, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    ] {
        let mapping = Mapping::Homography { matrix };
        assert!(refused(mapping.check(&space, &space)), "{matrix:?}");
    }
    // The test is made between the two spaces' unit squares, not on the matrix as given: a
    // plain move of 15,000 pixels in a large image has entries of very different sizes,
    // and is no nearer to singular for that.
    let large = Space::flat(20_000, 20_000).unwrap();
    Mapping::Homography {
        matrix: [1.0, 0.0, 15_000.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    }
    .check(&large, &large)
    .unwrap();
    // A small matrix is not a singular one: the scale of a homography means nothing.
    Mapping::Homography {
        matrix: [1e-8, 0.0, 0.0, 0.0, 1e-8, 0.0, 0.0, 0.0, 1e-8],
    }
    .check(&space, &space)
    .unwrap();
}

#[test]
fn a_homography_refuses_a_panorama_a_point_on_its_horizon_and_a_point_it_sends_outside() {
    let flat = Space::flat(100, 100).unwrap();
    let shift = Mapping::Homography {
        matrix: [1.0, 0.0, 10.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    };
    assert!(refused(
        shift.check(&flat, &Space::panorama(100, 100).unwrap())
    ));
    assert!(refused(
        shift.check(&Space::panorama(100, 100).unwrap(), &flat)
    ));
    // The image sits 10 to the right: its last columns fall off the reference.
    assert!(outside(shift.forward(&flat, &flat, Point::new(95.0, 5.0))));
    assert!(outside(shift.backward(&flat, &flat, Point::new(5.0, 5.0))));
    // The denominator is zero along x = 50.
    let horizon = Mapping::Homography {
        matrix: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, -0.02, 0.0, 1.0],
    };
    horizon.check(&flat, &flat).unwrap();
    assert!(outside(horizon.forward(
        &flat,
        &flat,
        Point::new(50.0, 5.0)
    )));
}

// --- rotation on the sphere ---------------------------------------------------------------

/// A turn about the vertical axis: every longitude grows by `degrees`.
fn yaw(degrees: f64) -> [f64; 9] {
    let (s, c) = degrees.to_radians().sin_cos();
    [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0]
}

/// A turn about the axis through longitude 90 degrees on the equator.
fn pitch(degrees: f64) -> [f64; 9] {
    let (s, c) = degrees.to_radians().sin_cos();
    [c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c]
}

fn times(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut out = [0.0; 9];
    for row in 0..3 {
        for column in 0..3 {
            out[3 * row + column] = (0..3).map(|k| a[3 * row + k] * b[3 * k + column]).sum();
        }
    }
    out
}

#[test]
fn a_rotation_turns_longitudes_and_wraps_at_the_seam() {
    let space = Space::panorama(400, 200).unwrap();
    let mapping = Mapping::Rotation { matrix: yaw(90.0) };
    mapping.check(&space, &space).unwrap();
    // A quarter turn is a quarter of the width: 100 columns to the right.
    let mapped = mapping
        .forward(&space, &space, Point::new(150.0, 80.0))
        .unwrap();
    assert!(close(mapped, Point::new(250.0, 80.0), 1e-9), "{mapped:?}");
    // Past the right edge the point comes back in on the left.
    let wrapped = mapping
        .forward(&space, &space, Point::new(350.5, 80.0))
        .unwrap();
    assert!(close(wrapped, Point::new(50.5, 80.0), 1e-9), "{wrapped:?}");
    let back = mapping.backward(&space, &space, wrapped).unwrap();
    assert!(close(back, Point::new(350.5, 80.0), 1e-9), "{back:?}");
}

#[test]
fn a_rotation_and_its_inverse_return_every_point() {
    let from = Space::panorama(400, 200).unwrap();
    // The two panoramas need not be the same size.
    let to = Space::panorama(800, 400).unwrap();
    let mapping = Mapping::Rotation {
        matrix: times(&yaw(37.0), &pitch(21.0)),
    };
    mapping.check(&from, &to).unwrap();
    for column in 0..40 {
        for row in 1..20 {
            let point = Point::new(f64::from(column) * 10.0 + 0.5, f64::from(row) * 10.0);
            let there = mapping.forward(&from, &to, point).unwrap();
            assert!((0.0..800.0).contains(&there.x) && (0.0..=400.0).contains(&there.y));
            let back = mapping.backward(&from, &to, there).unwrap();
            assert!(
                close(back, point, 1e-9),
                "{point:?} -> {there:?} -> {back:?}"
            );
        }
    }
}

#[test]
fn a_rotation_needs_two_panoramas_and_a_proper_rotation() {
    let panorama = Space::panorama(400, 200).unwrap();
    let flat = Space::flat(400, 200).unwrap();
    let part = Space::new(400, 200, Projection::Equirectangular, Topology::Bounded).unwrap();
    let turn = Mapping::Rotation { matrix: yaw(10.0) };
    assert!(refused(turn.check(&flat, &panorama)));
    assert!(refused(turn.check(&panorama, &flat)));
    assert!(refused(turn.check(&part, &panorama)));
    // A rotation is one to within 1e-9, and no further off than that.
    let mut nearly = yaw(10.0);
    nearly[0] += 1e-11;
    Mapping::Rotation { matrix: nearly }
        .check(&panorama, &panorama)
        .unwrap();
    let mut off = yaw(10.0);
    off[0] += 1e-6;
    assert!(refused(
        Mapping::Rotation { matrix: off }.check(&panorama, &panorama)
    ));
    // A shear of one part in a million: its determinant is exactly 1, and its rows are
    // not at right angles.
    assert!(refused(
        Mapping::Rotation {
            matrix: [1.0, 1e-6, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
        }
        .check(&panorama, &panorama)
    ));
    let mut stretched = yaw(10.0);
    stretched[8] += 1e-6;
    assert!(refused(
        Mapping::Rotation { matrix: stretched }.check(&panorama, &panorama)
    ));
    for matrix in [
        // A mirror: orthogonal, but its determinant is -1.
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0],
        // A scaling.
        [2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0],
        // A shear.
        [1.0, 0.1, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        [0.0; 9],
        [f64::NAN, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    ] {
        let mapping = Mapping::Rotation { matrix };
        assert!(refused(mapping.check(&panorama, &panorama)), "{matrix:?}");
    }
}

#[test]
fn a_rotation_carries_a_pole_to_a_pole_whatever_rounding_does() {
    // A matrix inside the tolerance whose last row is a hair longer than 1: the pole's
    // direction comes out a hair past the sphere, and is brought back onto it.
    let space = Space::panorama(400, 200).unwrap();
    let mapping = Mapping::Rotation {
        matrix: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0 + 4e-10],
    };
    mapping.check(&space, &space).unwrap();
    let top = mapping
        .forward(&space, &space, Point::new(120.0, 0.0))
        .unwrap();
    assert_eq!(top.y, 0.0);
    let bottom = mapping
        .forward(&space, &space, Point::new(120.0, 200.0))
        .unwrap();
    assert_eq!(bottom.y, 200.0);
}

// --- the wrapped cut ----------------------------------------------------------------------

#[test]
fn a_wrapped_cut_puts_the_seam_in_the_middle_of_its_canvas() {
    let panorama = Space::panorama(4000, 2000).unwrap();
    let canvas = Space::flat(560, 2000).unwrap();
    let mapping = Mapping::Wrap { strip: 280 };
    mapping.check(&canvas, &panorama).unwrap();
    // The canvas's left half is the panorama's last 280 columns; its right half the first.
    for (canvas_x, panorama_x) in [(0.0, 3720.0), (279.5, 3999.5), (280.5, 0.5), (559.5, 279.5)] {
        let on_canvas = Point::new(canvas_x, 7.5);
        let on_panorama = Point::new(panorama_x, 7.5);
        assert_eq!(
            mapping.forward(&canvas, &panorama, on_canvas).unwrap(),
            on_panorama
        );
        assert_eq!(
            mapping.backward(&canvas, &panorama, on_panorama).unwrap(),
            on_canvas
        );
    }
    // The column where the right strip ends is the canvas's right edge, and the column
    // where the left strip begins is its left edge.
    assert_eq!(
        mapping
            .backward(&canvas, &panorama, Point::new(280.0, 7.5))
            .unwrap(),
        Point::new(560.0, 7.5)
    );
    assert_eq!(
        mapping
            .backward(&canvas, &panorama, Point::new(3720.0, 7.5))
            .unwrap(),
        Point::new(0.0, 7.5)
    );
    // The middle of the panorama is not on the canvas.
    assert!(outside(mapping.backward(
        &canvas,
        &panorama,
        Point::new(2000.0, 7.5)
    )));
    assert!(outside(mapping.backward(
        &canvas,
        &panorama,
        Point::new(280.5, 7.5)
    )));
}

#[test]
fn a_wrapped_cut_needs_a_wrapped_parent_and_a_strip_that_fits() {
    let panorama = Space::panorama(4000, 2000).unwrap();
    let canvas = Space::flat(560, 2000).unwrap();
    let cut = Mapping::Wrap { strip: 280 };
    // The parent must wrap, the canvas must be flat, twice the strip wide and as tall.
    assert!(refused(
        cut.check(&canvas, &Space::flat(4000, 2000).unwrap())
    ));
    assert!(refused(
        cut.check(&Space::flat(561, 2000).unwrap(), &panorama)
    ));
    assert!(refused(
        cut.check(&Space::flat(560, 1999).unwrap(), &panorama)
    ));
    assert!(refused(
        cut.check(&Space::panorama(560, 2000).unwrap(), &panorama)
    ));
    // The strip is even, at least 2, and at most half the width.
    for (strip, width) in [(0, 4000), (1, 4000), (3, 4000), (2002, 4000), (2, 3)] {
        let mapping = Mapping::Wrap { strip };
        let parent = Space::panorama(width, 10).unwrap();
        let canvas = Space::flat(2 * strip.max(1), 10).unwrap();
        assert!(
            refused(mapping.check(&canvas, &parent)),
            "strip {strip}, width {width}"
        );
    }
    // Half the width each side is the whole panorama, turned half way round.
    Mapping::Wrap { strip: 2 }
        .check(
            &Space::flat(4, 10).unwrap(),
            &Space::panorama(4, 10).unwrap(),
        )
        .unwrap();
}
