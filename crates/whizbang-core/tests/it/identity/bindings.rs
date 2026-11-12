//! Bindings: a source image placed onto a destination through a mapping with a stated
//! direction.

use whizbang_core::identity::{
    Binding, Destination, Digest, Direction, IdentityError, Mapping, PixelDigest, Point,
    Projection, Space, Topology,
};

/// A pixel digest for an image these tests never decode.
fn image(name: &str) -> PixelDigest {
    PixelDigest(Digest::of_bytes(name.as_bytes()))
}

fn close(a: Point, b: Point, tolerance: f64) -> bool {
    (a.x - b.x).abs() <= tolerance && (a.y - b.y).abs() <= tolerance
}

fn shift_right() -> Mapping {
    Mapping::Homography {
        matrix: [1.0, 0.0, 12.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    }
}

fn yaw(degrees: f64) -> [f64; 9] {
    let (s, c) = degrees.to_radians().sin_cos();
    [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0]
}

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

// --- what a binding is --------------------------------------------------------------------

#[test]
fn an_image_in_its_own_space_is_bound_to_the_root() {
    let space = Space::flat(320, 200).unwrap();
    let binding = Binding::root(image("a"), space);
    assert_eq!(binding.source(), image("a"));
    assert_eq!(binding.destination(), Destination::Root);
    assert_eq!(binding.source_space(), space);
    assert_eq!(binding.destination_space(), space);
    assert_eq!(*binding.mapping(), Mapping::Identity);
    let point = Point::new(3.0, 4.0);
    assert_eq!(binding.to_destination(point).unwrap(), point);
    assert_eq!(binding.to_source(point).unwrap(), point);
    // Two different images of the same size share a space and nothing else.
    let other = Binding::root(image("b"), space);
    assert_ne!(binding.id(), other.id());
    assert_ne!(binding, other);
    assert_eq!(binding.id(), Binding::root(image("a"), space).id());
    // The same image declared flat and declared part of a panorama are two bindings: the
    // projection alone tells them apart.
    let part = Space::new(320, 200, Projection::Equirectangular, Topology::Bounded).unwrap();
    assert_ne!(binding.id(), Binding::root(image("a"), part).id());
    // So are the same image declared flat and declared a panorama.
    assert_ne!(
        binding.id(),
        Binding::root(image("a"), Space::panorama(320, 200).unwrap()).id()
    );
}

#[test]
fn one_source_aligned_onto_two_references_of_the_same_size_gives_two_bindings() {
    let source_space = Space::flat(300, 200).unwrap();
    let reference_space = Space::flat(320, 200).unwrap();
    let onto = |reference: &str| {
        Binding::new(
            image("candidate"),
            source_space,
            image(reference),
            reference_space,
            Direction::SourceToDestination,
            shift_right(),
        )
        .unwrap()
    };
    let first = onto("reference one");
    let second = onto("reference two");
    assert_eq!(first.source(), second.source());
    assert_eq!(first.destination_space(), second.destination_space());
    assert_eq!(first.mapping(), second.mapping());
    assert_ne!(first, second);
    assert_ne!(first.id(), second.id());
    assert_eq!(first.id(), onto("reference one").id());
}

#[test]
fn every_part_of_a_binding_is_part_of_its_identity() {
    let small = Space::flat(300, 200).unwrap();
    let large = Space::flat(320, 200).unwrap();
    let make = |source: &str,
                source_space: Space,
                destination: &str,
                destination_space: Space,
                direction: Direction,
                mapping: Mapping| {
        Binding::new(
            image(source),
            source_space,
            image(destination),
            destination_space,
            direction,
            mapping,
        )
        .unwrap()
        .id()
    };
    let forward = Direction::SourceToDestination;
    let base = make("a", small, "b", large, forward, shift_right());
    let scaled = Mapping::Homography {
        matrix: [1.0, 0.0, 12.5, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    };
    for other in [
        make("c", small, "b", large, forward, shift_right()),
        make(
            "a",
            Space::flat(300, 199).unwrap(),
            "b",
            large,
            forward,
            shift_right(),
        ),
        make("a", small, "c", large, forward, shift_right()),
        make(
            "a",
            small,
            "b",
            Space::flat(321, 200).unwrap(),
            forward,
            shift_right(),
        ),
        make(
            "a",
            small,
            "b",
            large,
            Direction::DestinationToSource,
            shift_right(),
        ),
        make("a", small, "b", large, forward, scaled),
    ] {
        assert_ne!(base, other);
    }
}

#[test]
fn a_binding_identity_is_fixed() {
    // These two values pin the encoding of ADR 0001. Changing them is changing the
    // identity scheme: a new ADR.
    let root = Binding::root(image("a"), Space::flat(320, 200).unwrap());
    assert_eq!(
        root.id().to_string(),
        "sha256:28ea51e0a0b5954c468d07d898743a8047133c5fbd00dd04943884173f8fe5dd"
    );
    let aligned = Binding::new(
        image("a"),
        Space::flat(300, 200).unwrap(),
        image("b"),
        Space::flat(320, 200).unwrap(),
        Direction::SourceToDestination,
        shift_right(),
    )
    .unwrap();
    assert_eq!(
        aligned.id().to_string(),
        "sha256:34a9e8d011aa907997f133a0751ecf2f77dd1d7f278c357118608feb6689eafb"
    );
}

#[test]
fn a_mapping_that_does_not_fit_its_spaces_is_refused_when_the_binding_is_built() {
    let flat = Space::flat(100, 100).unwrap();
    let build =
        |direction, mapping| Binding::new(image("a"), flat, image("b"), flat, direction, mapping);
    // A singular homography, in either direction.
    let singular = Mapping::Homography {
        matrix: [1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 0.0, 0.0, 1.0],
    };
    for direction in [
        Direction::SourceToDestination,
        Direction::DestinationToSource,
    ] {
        assert!(matches!(
            build(direction, singular.clone()),
            Err(IdentityError::Binding(_))
        ));
    }
    // A rotation between flat images; a cut of a flat image.
    assert!(
        build(
            Direction::SourceToDestination,
            Mapping::Rotation { matrix: yaw(10.0) }
        )
        .is_err()
    );
    assert!(build(Direction::SourceToDestination, Mapping::Wrap { strip: 50 }).is_err());
}

// --- crops --------------------------------------------------------------------------------

#[test]
fn a_point_in_a_crop_of_a_crop_maps_to_its_root_and_back_within_one_pixel() {
    let root_space = Space::flat(1000, 800).unwrap();
    // A box of the root, rendered at a third of its size.
    let first_space = Space::flat(200, 100).unwrap();
    let first = Binding::new(
        image("first crop"),
        first_space,
        image("root"),
        root_space,
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 100.0,
            top: 200.0,
            right: 700.0,
            bottom: 500.0,
        },
    )
    .unwrap();
    // A box of that crop, at its own scale.
    let second = Binding::new(
        image("second crop"),
        Space::flat(50, 40).unwrap(),
        image("first crop"),
        first_space,
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 20.0,
            top: 30.0,
            right: 70.0,
            bottom: 70.0,
        },
    )
    .unwrap();
    for column in 0..50 {
        for row in 0..40 {
            // The centre of each pixel of the inner crop.
            let point = Point::new(f64::from(column) + 0.5, f64::from(row) + 0.5);
            let on_root = first
                .to_destination(second.to_destination(point).unwrap())
                .unwrap();
            let back = second.to_source(first.to_source(on_root).unwrap()).unwrap();
            assert!(
                close(back, point, 1e-9),
                "{point:?} -> {on_root:?} -> {back:?}"
            );
            // Even after the root point is rounded to a whole pixel, the way back lands
            // within one pixel of where it started.
            let snapped = Point::new(on_root.x.round(), on_root.y.round());
            let back = second.to_source(first.to_source(snapped).unwrap()).unwrap();
            assert!(
                close(back, point, 1.0),
                "{point:?} -> {snapped:?} -> {back:?}"
            );
        }
    }
    // The inner crop's first pixel centre: (20.5, 30.5) on the first crop, three times
    // that and the box's corner on the root.
    let on_root = first
        .to_destination(second.to_destination(Point::new(0.5, 0.5)).unwrap())
        .unwrap();
    assert!(
        close(on_root, Point::new(161.5, 291.5), 1e-9),
        "{on_root:?}"
    );

    // The two crops compose into one crop of the root, which maps the same way.
    let whole = second.then(&first).unwrap();
    assert_eq!(whole.source(), image("second crop"));
    assert_eq!(whole.destination(), Destination::Image(image("root")));
    let Mapping::Crop {
        left,
        top,
        right,
        bottom,
    } = *whole.mapping()
    else {
        panic!("{:?}", whole.mapping());
    };
    for (got, want) in [(left, 160.0), (top, 290.0), (right, 310.0), (bottom, 410.0)] {
        assert!((got - want).abs() < 1e-9, "{:?}", whole.mapping());
    }
    assert!(close(
        whole.to_destination(Point::new(0.5, 0.5)).unwrap(),
        on_root,
        1e-9
    ));
}

#[test]
fn a_point_outside_a_crop_is_refused() {
    let crop = Binding::new(
        image("crop"),
        Space::flat(64, 60).unwrap(),
        image("root"),
        Space::flat(320, 200).unwrap(),
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 64.0,
            top: 40.0,
            right: 128.0,
            bottom: 100.0,
        },
    )
    .unwrap();
    assert_eq!(
        crop.to_source(Point::new(64.0, 40.0)).unwrap(),
        Point::new(0.0, 0.0)
    );
    for (x, y) in [(63.0, 50.0), (129.0, 50.0), (100.0, 39.0), (100.0, 101.0)] {
        assert!(
            matches!(
                crop.to_source(Point::new(x, y)),
                Err(IdentityError::Outside(_))
            ),
            "({x}, {y})"
        );
    }
    // Outside the root itself, and outside the crop's own space.
    assert!(crop.to_source(Point::new(321.0, 50.0)).is_err());
    assert!(crop.to_destination(Point::new(65.0, 1.0)).is_err());
}

// --- homographies -------------------------------------------------------------------------

#[test]
fn a_homography_maps_in_its_stated_direction_and_the_reverse_binding_is_its_inverse() {
    let source_space = Space::flat(100, 100).unwrap();
    let destination_space = Space::flat(400, 300).unwrap();
    let matrix = [2.0, 0.1, 10.0, -0.05, 2.0, 20.0, 0.001, 0.0002, 1.0];
    let forward = Binding::new(
        image("candidate"),
        source_space,
        image("reference"),
        destination_space,
        Direction::SourceToDestination,
        Mapping::Homography { matrix },
    )
    .unwrap();
    // The parameters take the source's pixels to the destination's.
    assert_eq!(
        forward.to_destination(Point::new(0.0, 0.0)).unwrap(),
        Point::new(10.0, 20.0)
    );
    // The same numbers stated the other way take the destination's pixels to the source's.
    let stated_backward = Binding::new(
        image("reference"),
        destination_space,
        image("candidate"),
        source_space,
        Direction::DestinationToSource,
        Mapping::Homography { matrix },
    )
    .unwrap();
    assert_eq!(
        stated_backward.to_source(Point::new(0.0, 0.0)).unwrap(),
        Point::new(10.0, 20.0)
    );
    assert_ne!(forward.id(), stated_backward.id());

    // The reverse binding swaps the two ends and keeps the parameters and what they mean.
    let reverse = forward.reversed().unwrap();
    assert_eq!(reverse, stated_backward);
    assert_eq!(reverse.source(), image("reference"));
    assert_eq!(
        reverse.destination(),
        Destination::Image(image("candidate"))
    );
    assert_eq!(reverse.reversed().unwrap(), forward);
    for column in 0..=10 {
        for row in 0..=10 {
            let point = Point::new(f64::from(column) * 10.0, f64::from(row) * 10.0);
            let there = forward.to_destination(point).unwrap();
            let back = reverse.to_destination(there).unwrap();
            assert!(
                close(back, point, 1e-9),
                "{point:?} -> {there:?} -> {back:?}"
            );
            assert!(close(forward.to_source(there).unwrap(), point, 1e-9));
        }
    }
    // Going there and back as one binding is the identity, within tolerance.
    let round = forward.then(&reverse).unwrap();
    assert_eq!(round.source(), image("candidate"));
    assert_eq!(round.destination(), Destination::Image(image("candidate")));
    let moved = round.to_destination(Point::new(33.0, 71.0)).unwrap();
    assert!(close(moved, Point::new(33.0, 71.0), 1e-9), "{moved:?}");
}

#[test]
fn a_binding_to_the_root_has_no_reverse() {
    let root = Binding::root(image("a"), Space::flat(10, 10).unwrap());
    assert!(matches!(root.reversed(), Err(IdentityError::Binding(_))));
}

// --- rotations on the sphere --------------------------------------------------------------

#[test]
fn a_rotation_composes_with_its_inverse_to_the_identity_and_wraps_at_the_seam() {
    let space = Space::panorama(400, 200).unwrap();
    let rotation = Binding::new(
        image("candidate"),
        space,
        image("reference"),
        space,
        Direction::SourceToDestination,
        Mapping::Rotation {
            matrix: times(&yaw(100.0), &pitch(15.0)),
        },
    )
    .unwrap();
    let inverse = rotation.reversed().unwrap();
    let round = rotation.then(&inverse).unwrap();
    assert_eq!(round.source(), image("candidate"));
    assert_eq!(round.destination(), Destination::Image(image("candidate")));
    let Mapping::Rotation { matrix } = round.mapping() else {
        panic!("{:?}", round.mapping());
    };
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    for (got, want) in matrix.iter().zip(identity) {
        assert!((got - want).abs() < 1e-12, "{matrix:?}");
    }
    for column in 0..40 {
        for row in 1..20 {
            let point = Point::new(f64::from(column) * 10.0 + 0.5, f64::from(row) * 10.0);
            let same = round.to_destination(point).unwrap();
            assert!(close(same, point, 1e-9), "{point:?} -> {same:?}");
        }
    }

    // A quarter turn carries a point near the right edge across the seam: it comes
    // back in on the left, inside the panorama, and the inverse carries it back.
    let turn = Binding::new(
        image("candidate"),
        space,
        image("reference"),
        space,
        Direction::SourceToDestination,
        Mapping::Rotation { matrix: yaw(90.0) },
    )
    .unwrap();
    let before = Point::new(399.5, 100.0);
    let after = turn.to_destination(before).unwrap();
    assert!(close(after, Point::new(99.5, 100.0), 1e-9), "{after:?}");
    let back = turn.reversed().unwrap().to_destination(after).unwrap();
    assert!(close(back, before, 1e-9), "{back:?}");
}

// --- the wrapped cut and put --------------------------------------------------------------

#[test]
fn a_put_after_a_cut_returns_every_pixel_to_its_original_place() {
    let (width, height, strip) = (40u32, 6u32, 8u32);
    let panorama = Space::panorama(width, height).unwrap();
    let canvas = Space::flat(2 * strip, height).unwrap();
    // The cut takes the panorama's pixels onto the canvas; its parameters are stated from
    // the canvas to the panorama, which is the direction the put runs in.
    let cut = Binding::new(
        image("panorama"),
        panorama,
        image("canvas"),
        canvas,
        Direction::DestinationToSource,
        Mapping::Wrap { strip },
    )
    .unwrap();
    let put = cut.reversed().unwrap();
    assert_eq!(put.source(), image("canvas"));
    assert_eq!(put.destination(), Destination::Image(image("panorama")));

    let centre = |column: u32, row: u32| Point::new(f64::from(column) + 0.5, f64::from(row) + 0.5);
    let pixel = |point: Point| (point.x.floor() as u32, point.y.floor() as u32);
    let mut seen = vec![false; (2 * strip * height) as usize];
    for row in 0..height {
        for column in 0..width {
            let in_a_strip = column < strip || column >= width - strip;
            match cut.to_destination(centre(column, row)) {
                Ok(on_canvas) => {
                    assert!(in_a_strip, "column {column} is not in a strip");
                    // Exactly the pixel's centre again: the mapping is a whole-pixel move.
                    assert_eq!(put.to_destination(on_canvas).unwrap(), centre(column, row));
                    let (x, y) = pixel(on_canvas);
                    assert_eq!(y, row);
                    let index = (y * 2 * strip + x) as usize;
                    assert!(!seen[index], "two pixels landed on canvas pixel ({x}, {y})");
                    seen[index] = true;
                }
                Err(IdentityError::Outside(_)) => {
                    assert!(!in_a_strip, "column {column} is in a strip");
                }
                Err(other) => panic!("{other}"),
            }
        }
    }
    // Every pixel of the canvas was filled, and each goes back where it came from.
    assert!(seen.iter().all(|filled| *filled));
    for row in 0..height {
        for column in 0..2 * strip {
            let on_panorama = put.to_destination(centre(column, row)).unwrap();
            assert_eq!(
                cut.to_destination(on_panorama).unwrap(),
                centre(column, row)
            );
        }
    }
    // The seam is the middle of the canvas: the panorama's last column sits just left of
    // it and the first just right.
    assert_eq!(
        pixel(cut.to_destination(centre(width - 1, 0)).unwrap()).0,
        strip - 1
    );
    assert_eq!(pixel(cut.to_destination(centre(0, 0)).unwrap()).0, strip);
}

// --- composing ----------------------------------------------------------------------------

#[test]
fn bindings_compose_only_end_to_end() {
    let space = Space::flat(100, 100).unwrap();
    let step = |source: &str, destination: &str| {
        Binding::new(
            image(source),
            space,
            image(destination),
            space,
            Direction::SourceToDestination,
            Mapping::Homography {
                matrix: [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            },
        )
        .unwrap()
    };
    let a_to_b = step("a", "b");
    let b_to_c = step("b", "c");
    let both = a_to_b.then(&b_to_c).unwrap();
    assert_eq!(
        both.to_destination(Point::new(5.0, 5.0)).unwrap(),
        Point::new(7.0, 5.0)
    );
    // The second binding must start where the first ends: the same image, the same space.
    assert!(matches!(
        b_to_c.then(&a_to_b),
        Err(IdentityError::Binding(_))
    ));
    assert!(a_to_b.then(&step("x", "c")).is_err());
    let elsewhere = Binding::new(
        image("b"),
        Space::flat(100, 101).unwrap(),
        image("c"),
        space,
        Direction::SourceToDestination,
        Mapping::Homography {
            matrix: [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        },
    )
    .unwrap();
    assert!(a_to_b.then(&elsewhere).is_err());
    // Nothing follows a binding to the root.
    let root = Binding::root(image("a"), space);
    assert!(root.then(&a_to_b).is_err());
    // A cut and a crop do not make one mapping; their points are mapped one after the other.
    let panorama = Space::panorama(400, 100).unwrap();
    let put = Binding::new(
        image("canvas"),
        Space::flat(100, 100).unwrap(),
        image("panorama"),
        panorama,
        Direction::SourceToDestination,
        Mapping::Wrap { strip: 50 },
    )
    .unwrap();
    let crop = Binding::new(
        image("crop"),
        Space::flat(10, 10).unwrap(),
        image("canvas"),
        Space::flat(100, 100).unwrap(),
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 45.0,
            top: 0.0,
            right: 55.0,
            bottom: 10.0,
        },
    )
    .unwrap();
    assert!(matches!(crop.then(&put), Err(IdentityError::Binding(_))));
    let on_panorama = put
        .to_destination(crop.to_destination(Point::new(7.5, 2.0)).unwrap())
        .unwrap();
    assert_eq!(on_panorama, Point::new(2.5, 2.0));
}

// --- reading and writing ------------------------------------------------------------------

#[test]
fn a_binding_is_checked_again_when_it_is_read() {
    let binding = Binding::new(
        image("a"),
        Space::flat(100, 100).unwrap(),
        image("b"),
        Space::flat(100, 100).unwrap(),
        Direction::SourceToDestination,
        shift_right(),
    )
    .unwrap();
    let text = serde_json::to_string(&binding).unwrap();
    let read: Binding = serde_json::from_str(&text).unwrap();
    assert_eq!(read, binding);
    assert_eq!(read.id(), binding.id());
    // Nothing local is written: the two images by their pixel digests, the two spaces, the
    // direction and the mapping, and no more.
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "destination",
            "destination_space",
            "direction",
            "mapping",
            "source",
            "source_space"
        ]
    );
    // A record whose mapping is singular is refused on the way in.
    let singular = text.replace("[1.0,0.0,12.0,0.0,1.0,0.0,", "[1.0,0.0,12.0,2.0,0.0,24.0,");
    assert_ne!(singular, text);
    assert!(serde_json::from_str::<Binding>(&singular).is_err());
    // So is one that carries anything else.
    let extra = text.replacen('{', r#"{"id":"crop:1","#, 1);
    assert!(serde_json::from_str::<Binding>(&extra).is_err());

    let root = Binding::root(image("a"), Space::panorama(400, 200).unwrap());
    let text = serde_json::to_string(&root).unwrap();
    assert_eq!(serde_json::from_str::<Binding>(&text).unwrap(), root);
    // A binding to the root is the identity on one space, and nothing else reads as one.
    let moved = text.replace("\"identity\"", r#"{"wrap":{"strip":2}}"#);
    assert_ne!(moved, text);
    assert!(serde_json::from_str::<Binding>(&moved).is_err());
}

// --- the encoding, part by part -----------------------------------------------------------

#[test]
fn a_binding_read_from_json_has_every_number_it_was_written_with() {
    // A double is written in the shortest form that names it, up to seventeen digits.
    // Reading must give the same double back, or the binding that was read is another
    // binding, with another identity. These two values are ones a reader that takes the
    // fast path gets wrong in the last bit.
    for value in [1.6290118483412321_f64, 1.0715660391465826e-75] {
        let binding = Binding::new(
            image("a"),
            Space::flat(100, 100).unwrap(),
            image("b"),
            Space::flat(100, 100).unwrap(),
            Direction::SourceToDestination,
            Mapping::Homography {
                matrix: [1.0, 0.0, value, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            },
        )
        .unwrap();
        let text = serde_json::to_string(&binding).unwrap();
        let read: Binding = serde_json::from_str(&text).unwrap();
        assert_eq!(read, binding, "{value:?}");
        assert_eq!(read.id(), binding.id(), "{value:?}");
    }
}

#[test]
fn the_topology_alone_tells_two_bindings_apart() {
    let whole = Space::panorama(400, 200).unwrap();
    let part = Space::new(400, 200, Projection::Equirectangular, Topology::Bounded).unwrap();
    assert_ne!(
        Binding::root(image("a"), whole).id(),
        Binding::root(image("a"), part).id()
    );
}

#[test]
fn the_identity_of_each_kind_of_mapping_is_fixed() {
    // Computed apart from this code, from the encoding ADR 0001 states. Each pins the
    // kind's number and the order of its parameters.
    let onto = |source_space, destination_space, direction, mapping| {
        Binding::new(
            image("a"),
            source_space,
            image("b"),
            destination_space,
            direction,
            mapping,
        )
        .unwrap()
        .id()
        .to_string()
    };
    let crop = onto(
        Space::flat(64, 60).unwrap(),
        Space::flat(320, 200).unwrap(),
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 64.0,
            top: 40.5,
            right: 128.0,
            bottom: 100.25,
        },
    );
    assert_eq!(
        crop,
        "sha256:5d21789e426d597f46f0b66db3b9533e819faff1c950925083292e8e1f5ae9f4"
    );
    let rotation = onto(
        Space::panorama(400, 200).unwrap(),
        Space::panorama(800, 400).unwrap(),
        Direction::DestinationToSource,
        Mapping::Rotation {
            matrix: [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        },
    );
    assert_eq!(
        rotation,
        "sha256:f436f7decebc8ca88bba71e38b628d9fadcb6d690dc553b98ef939cd2e2a8c80"
    );
    let wrap = onto(
        Space::flat(16, 6).unwrap(),
        Space::panorama(40, 6).unwrap(),
        Direction::SourceToDestination,
        Mapping::Wrap { strip: 8 },
    );
    assert_eq!(
        wrap,
        "sha256:889eca0cbc19667a28da178b22ebbeac80e99eac3588dbc36d947184e6123a67"
    );
}

#[test]
fn negative_zero_is_zero_in_an_identity() {
    let space = Space::flat(100, 100).unwrap();
    let with = |zero: f64| {
        Binding::new(
            image("a"),
            space,
            image("b"),
            space,
            Direction::SourceToDestination,
            Mapping::Homography {
                matrix: [1.0, zero, 12.0, zero, 1.0, zero, zero, zero, 1.0],
            },
        )
        .unwrap()
    };
    assert_eq!(with(0.0).id(), with(-0.0).id());
    assert_eq!(with(0.0), with(-0.0));
}

#[test]
fn two_rotations_compose_in_the_order_they_are_taken() {
    let space = Space::panorama(400, 200).unwrap();
    let step = |source: &str, destination: &str, matrix| {
        Binding::new(
            image(source),
            space,
            image(destination),
            space,
            Direction::SourceToDestination,
            Mapping::Rotation { matrix },
        )
        .unwrap()
    };
    // A quarter turn about the vertical, then a quarter turn about a level axis: the two
    // do not commute.
    let first = step("a", "b", yaw(90.0));
    let second = step("b", "c", pitch(90.0));
    let both = first.then(&second).unwrap();
    let Mapping::Rotation { matrix } = both.mapping() else {
        panic!("{:?}", both.mapping());
    };
    for (got, want) in matrix.iter().zip(times(&pitch(90.0), &yaw(90.0))) {
        assert!((got - want).abs() < 1e-12, "{matrix:?}");
    }
    for (x, y) in [(10.0, 20.0), (130.5, 99.0), (399.0, 170.0), (250.0, 60.5)] {
        let point = Point::new(x, y);
        let one_by_one = second
            .to_destination(first.to_destination(point).unwrap())
            .unwrap();
        let at_once = both.to_destination(point).unwrap();
        assert!(
            close(at_once, one_by_one, 1e-9),
            "{point:?}: {at_once:?} {one_by_one:?}"
        );
    }
    // The same when one of the two is stated the other way round.
    let second_reversed = step("c", "b", pitch(-90.0)).reversed().unwrap();
    let both = first.then(&second_reversed).unwrap();
    let point = Point::new(130.5, 99.0);
    let one_by_one = second
        .to_destination(first.to_destination(point).unwrap())
        .unwrap();
    assert!(close(both.to_destination(point).unwrap(), one_by_one, 1e-9));
}

// --- crops that touch an edge -------------------------------------------------------------

/// A crop of `source` (width by height) onto `destination`.
fn crop_of(
    source: &str,
    size: (u32, u32),
    destination: &str,
    destination_space: Space,
    edges: (f64, f64, f64, f64),
) -> Result<Binding, IdentityError> {
    Binding::new(
        image(source),
        Space::flat(size.0, size.1).unwrap(),
        image(destination),
        destination_space,
        Direction::SourceToDestination,
        Mapping::Crop {
            left: edges.0,
            top: edges.1,
            right: edges.2,
            bottom: edges.3,
        },
    )
}

#[test]
fn a_crop_of_a_crop_that_touches_the_far_edge_composes() {
    // The outer crop is the right two thirds of a 3 by 3 root, rendered 5 by 5; the inner
    // one is the right two fifths of that. Its right edge is the root's right edge, and
    // must come out as exactly 3: a multiplication and a division do not give that.
    let root = Space::flat(3, 3).unwrap();
    let outer = crop_of("outer", (5, 5), "root", root, (1.0, 0.0, 3.0, 3.0)).unwrap();
    let inner = crop_of(
        "inner",
        (5, 5),
        "outer",
        Space::flat(5, 5).unwrap(),
        (3.0, 0.0, 5.0, 5.0),
    )
    .unwrap();
    let both = inner.then(&outer).unwrap();
    let Mapping::Crop {
        left,
        top,
        right,
        bottom,
    } = *both.mapping()
    else {
        panic!("{:?}", both.mapping());
    };
    assert_eq!((top, right, bottom), (0.0, 3.0, 3.0));
    assert!((left - 2.2).abs() < 1e-12, "{left}");
}

#[test]
fn crops_of_crops_compose_whatever_edges_they_touch() {
    // Random pairs from a fixed seed; every edge has one chance in three of touching its
    // parent's. Each pair must compose, into a box that lies inside the root, that keeps
    // exactly every edge the two crops share with their parents, and that maps as the two
    // crops do one after the other.
    struct Random(u64);
    impl Random {
        fn next(&mut self, below: u32) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 33) % u64::from(below)) as u32
        }
        /// Two edges inside `size`, on tenths of a pixel: most are not exact in binary.
        fn span(&mut self, size: u32) -> (f64, f64) {
            let tenths = size * 10;
            let low = if self.next(3) == 0 {
                0
            } else {
                self.next(tenths)
            };
            let high = if self.next(3) == 0 {
                tenths
            } else {
                low + 1 + self.next(tenths - low)
            };
            (f64::from(low) / 10.0, f64::from(high) / 10.0)
        }
        /// A box inside `width` by `height`.
        fn edges(&mut self, width: u32, height: u32) -> (f64, f64, f64, f64) {
            let (left, right) = self.span(width);
            let (top, bottom) = self.span(height);
            (left, top, right, bottom)
        }
        fn size(&mut self) -> (u32, u32) {
            (1 + self.next(64), 1 + self.next(64))
        }
    }
    let mut random = Random(0x2545_f491_4f6c_dd1d);
    let mut touching = 0;
    for pair in 0..4000 {
        let (root_size, outer_size, inner_size) = (random.size(), random.size(), random.size());
        let root = Space::flat(root_size.0, root_size.1).unwrap();
        let outer_edges = random.edges(root_size.0, root_size.1);
        let inner_edges = random.edges(outer_size.0, outer_size.1);
        let outer = crop_of("outer", outer_size, "root", root, outer_edges).unwrap();
        let outer_space = Space::flat(outer_size.0, outer_size.1).unwrap();
        let inner = crop_of("inner", inner_size, "outer", outer_space, inner_edges).unwrap();
        let both = inner.then(&outer).unwrap_or_else(|error| {
            panic!("pair {pair}: {outer_edges:?} of {root_size:?}, {inner_edges:?} of {outer_size:?}: {error}")
        });
        let Mapping::Crop {
            left,
            top,
            right,
            bottom,
        } = *both.mapping()
        else {
            panic!("pair {pair}: {:?}", both.mapping());
        };
        // An edge the inner crop shares with the outer one is the outer one's edge on the
        // root, exactly.
        let (width, height) = (f64::from(outer_size.0), f64::from(outer_size.1));
        for (shared, got, want) in [
            (inner_edges.0 == 0.0, left, outer_edges.0),
            (inner_edges.1 == 0.0, top, outer_edges.1),
            (inner_edges.2 == width, right, outer_edges.2),
            (inner_edges.3 == height, bottom, outer_edges.3),
        ] {
            if shared {
                assert_eq!(got, want, "pair {pair}");
                touching += 1;
            }
        }
        assert!(
            outer_edges.0 <= left && right <= outer_edges.2,
            "pair {pair}"
        );
        assert!(
            outer_edges.1 <= top && bottom <= outer_edges.3,
            "pair {pair}"
        );
        for (x, y) in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0), (0.25, 0.875)] {
            let point = Point::new(x * f64::from(inner_size.0), y * f64::from(inner_size.1));
            let one_by_one = outer
                .to_destination(inner.to_destination(point).unwrap())
                .unwrap();
            let at_once = both.to_destination(point).unwrap();
            assert!(
                close(at_once, one_by_one, 1e-9),
                "pair {pair}: {at_once:?} {one_by_one:?}"
            );
        }
    }
    assert!(touching > 3000, "{touching}");
}

#[test]
fn a_crop_that_touches_a_panoramas_right_edge_maps_there_and_back() {
    // The last 100 columns of a panorama. The crop's right edge is the panorama's right
    // edge, which is also its column 0: it maps back to the crop's right edge, not out of
    // the crop.
    let panorama = Space::panorama(400, 200).unwrap();
    let crop = crop_of(
        "crop",
        (100, 50),
        "panorama",
        panorama,
        (300.0, 50.0, 400.0, 100.0),
    )
    .unwrap();
    let edge = Point::new(100.0, 10.0);
    let on_panorama = crop.to_destination(edge).unwrap();
    assert_eq!(on_panorama, Point::new(0.0, 60.0));
    assert_eq!(crop.to_source(on_panorama).unwrap(), edge);
    assert_eq!(crop.to_source(Point::new(400.0, 60.0)).unwrap(), edge);
    // Every other column goes there and back as before, and the columns just right of the
    // seam are not in the crop.
    for column in 0..100 {
        let point = Point::new(f64::from(column) + 0.5, 10.0);
        let there = crop.to_destination(point).unwrap();
        assert_eq!(there, Point::new(300.5 + f64::from(column), 60.0));
        assert_eq!(crop.to_source(there).unwrap(), point);
    }
    assert!(crop.to_source(Point::new(0.5, 60.0)).is_err());
    assert!(crop.to_source(Point::new(299.5, 60.0)).is_err());
    // A crop that starts at the left edge keeps column 0 as its own left edge.
    let left = crop_of(
        "crop",
        (100, 50),
        "panorama",
        panorama,
        (0.0, 50.0, 100.0, 100.0),
    )
    .unwrap();
    assert_eq!(
        left.to_source(Point::new(0.0, 60.0)).unwrap(),
        Point::new(0.0, 10.0)
    );
    assert_eq!(
        left.to_source(Point::new(400.0, 60.0)).unwrap(),
        Point::new(0.0, 10.0)
    );
    // The whole panorama at half the size is a panorama: its column 0 is column 0.
    let half = Binding::new(
        image("half"),
        Space::panorama(200, 100).unwrap(),
        image("panorama"),
        panorama,
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 0.0,
            top: 0.0,
            right: 400.0,
            bottom: 200.0,
        },
    )
    .unwrap();
    assert_eq!(
        half.to_source(Point::new(0.0, 60.0)).unwrap(),
        Point::new(0.0, 30.0)
    );
    assert_eq!(
        half.to_destination(Point::new(200.0, 30.0)).unwrap(),
        Point::new(0.0, 60.0)
    );
}

#[test]
fn crops_that_touch_a_panoramas_right_edge_map_their_right_edge_there_and_back() {
    // Boxes whose left edge is on a tenth of a pixel and whose right edge is the
    // panorama's. The crop's right edge must land on column 0 exactly and come back
    // exactly, whatever the scale; the columns between go there and back within rounding.
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = |below: u32| -> u32 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 33) % u64::from(below)) as u32
    };
    for case in 0..3000 {
        let (width, height) = (2 + next(4000), 1 + next(200));
        let panorama = Space::panorama(width, height).unwrap();
        let left = f64::from(next(width * 10 - 1) + 1) / 10.0;
        let size = (1 + next(300), 1 + next(50));
        let crop = crop_of(
            "crop",
            size,
            "panorama",
            panorama,
            (left, 0.0, f64::from(width), f64::from(height)),
        )
        .unwrap();
        let edge = Point::new(f64::from(size.0), f64::from(size.1) / 2.0);
        let there = crop
            .to_destination(edge)
            .unwrap_or_else(|error| panic!("case {case}: {error}"));
        assert_eq!(
            there.x, 0.0,
            "case {case}: left {left} of {width}, {size:?}"
        );
        let back = crop.to_source(there).unwrap_or_else(|error| {
            panic!("case {case}: left {left} of {width}, {size:?}: {error}")
        });
        assert_eq!(back.x, edge.x, "case {case}");
        assert_eq!(crop.to_destination(Point::new(0.0, 0.0)).unwrap().x, left);
        assert_eq!(crop.to_source(Point::new(left, 0.0)).unwrap().x, 0.0);
        for step in 1..8 {
            let point = Point::new(f64::from(size.0) * f64::from(step) / 8.0, edge.y);
            let there = crop.to_destination(point).unwrap();
            let back = crop.to_source(there).unwrap();
            assert!(
                close(back, point, 1e-9),
                "case {case}: {point:?} {there:?} {back:?}"
            );
        }
    }
}

#[test]
fn a_crop_maps_its_top_and_bottom_edges_onto_its_box_exactly_and_back() {
    // Boxes whose four edges are on tenths of a pixel, most of which are not exact in
    // binary, at any scale. The crop's top edge must land on the box's top and its bottom
    // edge on the box's bottom, exactly, and each must come back exactly, as the left and
    // right edges do; the rows between go there and back within rounding.
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = |below: u32| -> u32 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 33) % u64::from(below)) as u32
    };
    for case in 0..3000 {
        let (width, height) = (2 + next(4000), 2 + next(3000));
        let whole = Space::flat(width, height).unwrap();
        // Two different tenths inside each dimension, in order.
        let mut pair = |size: u32| {
            let a = next(size * 10 + 1);
            let b = (a + 1 + next(size * 10)) % (size * 10 + 1);
            (f64::from(a.min(b)) / 10.0, f64::from(a.max(b)) / 10.0)
        };
        let (left, right) = pair(width);
        let (top, bottom) = pair(height);
        let size = (1 + next(300), 1 + next(300));
        let crop = crop_of("crop", size, "whole", whole, (left, top, right, bottom))
            .unwrap_or_else(|error| panic!("case {case}: {error}"));
        let (across, down) = (f64::from(size.0), f64::from(size.1));
        let what = format!("case {case}: ({left}, {top}) to ({right}, {bottom}), {size:?}");
        for x in [0.0, across / 2.0, across] {
            let on_top = crop.to_destination(Point::new(x, 0.0)).unwrap();
            let on_bottom = crop.to_destination(Point::new(x, down)).unwrap();
            assert_eq!(on_top.y, top, "{what}");
            assert_eq!(on_bottom.y, bottom, "{what}");
            assert_eq!(crop.to_source(on_top).unwrap().y, 0.0, "{what}");
            assert_eq!(crop.to_source(on_bottom).unwrap().y, down, "{what}");
        }
        // The left and right edges, on the same boxes.
        let on_left = crop.to_destination(Point::new(0.0, down / 2.0)).unwrap();
        let on_right = crop.to_destination(Point::new(across, down / 2.0)).unwrap();
        assert_eq!((on_left.x, on_right.x), (left, right), "{what}");
        assert_eq!(crop.to_source(on_left).unwrap().x, 0.0, "{what}");
        assert_eq!(crop.to_source(on_right).unwrap().x, across, "{what}");
        for step in 1..8 {
            let point = Point::new(across / 2.0, down * f64::from(step) / 8.0);
            let there = crop.to_destination(point).unwrap();
            let back = crop.to_source(there).unwrap();
            assert!(
                close(back, point, 1e-9),
                "{what}: {point:?} {there:?} {back:?}"
            );
        }
    }
}
