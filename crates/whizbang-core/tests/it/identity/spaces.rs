//! Coordinate spaces: dimensions, projection and topology, with no content.

use whizbang_core::identity::{IdentityError, Point, Projection, Space, Topology};

#[test]
fn a_flat_image_and_a_panorama_of_the_same_dimensions_are_different_spaces() {
    let flat = Space::flat(4000, 2000).unwrap();
    let panorama = Space::panorama(4000, 2000).unwrap();
    assert_ne!(flat, panorama);
    assert_eq!((flat.width(), flat.height()), (4000, 2000));
    assert_eq!((panorama.width(), panorama.height()), (4000, 2000));
    assert_eq!(flat.projection(), Projection::Flat);
    assert_eq!(flat.topology(), Topology::Bounded);
    assert_eq!(panorama.projection(), Projection::Equirectangular);
    assert_eq!(panorama.topology(), Topology::Wrap);
}

#[test]
fn spaces_are_equal_only_when_dimensions_projection_and_topology_all_match() {
    let space = Space::new(400, 200, Projection::Equirectangular, Topology::Wrap).unwrap();
    assert_eq!(space, Space::panorama(400, 200).unwrap());
    for other in [
        Space::new(401, 200, Projection::Equirectangular, Topology::Wrap).unwrap(),
        Space::new(400, 201, Projection::Equirectangular, Topology::Wrap).unwrap(),
        Space::new(400, 200, Projection::Flat, Topology::Bounded).unwrap(),
        // A part of a panorama: the same projection, but its edges are edges.
        Space::new(400, 200, Projection::Equirectangular, Topology::Bounded).unwrap(),
    ] {
        assert_ne!(space, other);
    }
}

#[test]
fn a_space_without_area_or_a_flat_space_that_wraps_is_refused() {
    for (width, height, projection, topology) in [
        (0, 10, Projection::Flat, Topology::Bounded),
        (10, 0, Projection::Flat, Topology::Bounded),
        (10, 10, Projection::Flat, Topology::Wrap),
    ] {
        assert!(matches!(
            Space::new(width, height, projection, topology),
            Err(IdentityError::Space(_))
        ));
    }
}

#[test]
fn a_bounded_space_holds_its_edges_and_nothing_past_them() {
    let space = Space::flat(10, 5).unwrap();
    for (x, y) in [(0.0, 0.0), (10.0, 5.0), (3.5, 2.25)] {
        assert_eq!(space.place(Point::new(x, y)).unwrap(), Point::new(x, y));
    }
    for (x, y) in [
        (-0.001, 1.0),
        (10.001, 1.0),
        (1.0, -0.001),
        (1.0, 5.001),
        (f64::NAN, 1.0),
        (1.0, f64::INFINITY),
    ] {
        assert!(
            matches!(
                space.place(Point::new(x, y)),
                Err(IdentityError::Outside(_))
            ),
            "({x}, {y})"
        );
    }
}

#[test]
fn a_wrapped_space_takes_any_column_round_to_its_place() {
    let space = Space::panorama(400, 200).unwrap();
    assert_eq!(
        space.place(Point::new(401.5, 10.0)).unwrap(),
        Point::new(1.5, 10.0)
    );
    assert_eq!(
        space.place(Point::new(-1.5, 10.0)).unwrap(),
        Point::new(398.5, 10.0)
    );
    // The right edge is the left edge.
    assert_eq!(
        space.place(Point::new(400.0, 10.0)).unwrap(),
        Point::new(0.0, 10.0)
    );
    // Rows do not wrap.
    assert!(space.place(Point::new(10.0, 200.5)).is_err());
    assert!(space.place(Point::new(f64::INFINITY, 1.0)).is_err());
}

#[test]
fn a_space_is_checked_again_when_it_is_read() {
    let space = Space::panorama(400, 200).unwrap();
    let text = serde_json::to_string(&space).unwrap();
    assert_eq!(
        text,
        r#"{"width":400,"height":200,"projection":"equirectangular","topology":"wrap"}"#
    );
    assert_eq!(serde_json::from_str::<Space>(&text).unwrap(), space);
    for wrong in [
        r#"{"width":0,"height":200,"projection":"flat","topology":"bounded"}"#,
        r#"{"width":4,"height":2,"projection":"flat","topology":"wrap"}"#,
        r#"{"width":4,"height":2,"projection":"flat","topology":"bounded","id":"x"}"#,
        r#"{"width":4,"height":2,"projection":"flat"}"#,
    ] {
        assert!(serde_json::from_str::<Space>(wrong).is_err(), "{wrong}");
    }
}

#[test]
fn a_point_on_the_edge_of_a_magnified_crop_of_a_crop_lands() {
    // Two crops, each a 1024-pixel picture of under a pixel of the one before: about
    // 2.8 million times magnified. A point on an edge of the inner one, mapped up to
    // the root and back down binding by binding, comes back within the root's rounding
    // times that magnification of its edge, far more than a fixed 1e-9 of a pixel; the
    // slack scaled to each crop's magnification takes it back onto the edge (plan mE,
    // task P1, item 5).
    use whizbang_core::identity::{Binding, Digest, Direction, Mapping, PixelDigest, Point};
    let image = |name: &str| PixelDigest(Digest::of_bytes(name.as_bytes()));
    let root = Space::flat(6000, 4000).unwrap();
    let tile = Space::flat(1024, 1024).unwrap();
    let crop = |source: &str, onto: &str, space: Space, left: f64, top: f64, side: f64| {
        Binding::new(
            image(source),
            tile,
            image(onto),
            space,
            Direction::SourceToDestination,
            Mapping::Crop {
                left,
                top,
                right: left + side,
                bottom: top + side,
            },
        )
        .unwrap()
    };
    let outer = crop("outer", "root", root, 1234.567, 987.654, 0.75);
    let inner = crop("inner", "outer", tile, 100.3, 200.7, 0.5);
    let mut landed = 0;
    for step in 0..=64 {
        let along = f64::from(step) * 16.0;
        for edge in [
            Point::new(0.0, along),
            Point::new(1024.0, along),
            Point::new(along, 0.0),
            Point::new(along, 1024.0),
        ] {
            let up = outer
                .to_destination(inner.to_destination(edge).unwrap())
                .unwrap();
            let back = inner.to_source(outer.to_source(up).unwrap()).unwrap();
            assert!(
                (back.x - edge.x).abs() < 1e-5 && (back.y - edge.y).abs() < 1e-5,
                "{edge:?} came back at {back:?}"
            );
            landed += 1;
        }
    }
    assert_eq!(landed, 260);
}

#[test]
fn the_slack_past_an_edge_is_never_more_than_a_thousandth_of_a_pixel() {
    // A crop a million million times magnified would be given a slack of a thousand
    // pixels by its magnification alone; it is held to a thousandth (plan mE, task P1).
    use whizbang_core::identity::{Mapping, Point};
    let crop = Space::flat(1000, 1000).unwrap();
    let root = Space::flat(100, 100).unwrap();
    let mapping = Mapping::Crop {
        left: 10.0,
        top: 10.0,
        right: 10.0 + 1e-9,
        bottom: 10.0 + 1e-9,
    };
    mapping.check(&crop, &root).unwrap();
    // A root point a few of its last bits past the box's right edge is pixels past the
    // crop's.
    let past = Point::new(10.0 + 1e-9 + 4e-15, 10.0);
    assert!(matches!(
        mapping.backward(&crop, &root, past),
        Err(IdentityError::Outside(_))
    ));
}
