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
