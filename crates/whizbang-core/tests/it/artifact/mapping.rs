//! A location carried through a binding (plan m3, task O2, item 2): through a wrapped
//! cut, split at the canvas's middle column and back; through a crop, clipped and said to
//! be partial; and what is refused. The cases are rascal's
//! (`crates/rascal-core/src/mapping.rs` at the commit `docs/sources.toml` pins), in pixels
//! where rascal divided by the frame's size.

use whizbang_core::artifact::{
    ArtifactError, Carried, Line, Location, Origin, Shape, Step, Toward, carry,
};
use whizbang_core::identity::{Binding, Digest, Direction, Mapping, PixelDigest, Point, Space};

fn picture(name: &str) -> PixelDigest {
    PixelDigest(Digest::of_bytes(name.as_bytes()))
}

fn made() -> Origin {
    Origin::new(Step::new("fixture", "1")).unwrap()
}

/// A panorama 40 by 10 and the canvas of its cut with strip 4: 8 by 10, its left half the
/// panorama's columns 36 to 40 and its right half 0 to 4.
fn wrap() -> Binding {
    Binding::new(
        picture("canvas"),
        Space::flat(8, 10).unwrap(),
        picture("panorama"),
        Space::panorama(40, 10).unwrap(),
        Direction::SourceToDestination,
        Mapping::Wrap { strip: 4 },
    )
    .unwrap()
}

fn on(binding: &Binding, shape: Shape) -> Location {
    Location::new(binding.clone(), shape, made()).unwrap()
}

fn shapes(carried: &Carried) -> Vec<Shape> {
    carried.parts.iter().map(|p| p.shape().clone()).collect()
}

fn boxed(left: f64, top: f64, right: f64, bottom: f64) -> Shape {
    Shape::Box {
        left,
        top,
        right,
        bottom,
    }
}

#[test]
fn a_location_through_a_wrap_and_back_lands_where_it_began() {
    let cut = wrap();
    let panorama = Binding::root(picture("panorama"), Space::panorama(40, 10).unwrap());
    // A box inside the canvas's right half lands on the panorama's first columns, and
    // comes back to where it began, bound by the cut.
    let start = on(&cut, boxed(5.0, 2.0, 7.5, 6.0));
    let there = carry(&start, &cut, Toward::Destination).unwrap();
    assert_eq!(shapes(&there), [boxed(1.0, 2.0, 3.5, 6.0)]);
    assert!(!there.partial);
    assert_eq!(*there.parts[0].binding(), panorama);
    let back = carry(&there.parts[0], &cut, Toward::Source).unwrap();
    assert_eq!(shapes(&back), [start.shape().clone()]);
    assert_eq!(*back.parts[0].binding(), cut);
    let steps: Vec<&str> = back.parts[0]
        .origin()
        .steps()
        .iter()
        .map(|s| s.action.as_str())
        .collect();
    assert_eq!(steps, ["fixture", "carry", "carry"]);

    // A stroke across the seam, the canvas's middle column, is two strokes on the
    // panorama: one ending at its right edge and one starting at its left. Both come back
    // to the halves of the stroke they were cut from, meeting at the middle.
    let path = vec![Point::new(2.0, 5.0), Point::new(6.0, 5.0)];
    let start = on(
        &cut,
        Shape::Stroke {
            path: path.clone(),
            radius: 1.5,
        },
    );
    let there = carry(&start, &cut, Toward::Destination).unwrap();
    assert!(!there.partial, "a split at the seam is no clipping");
    assert_eq!(
        shapes(&there),
        [
            Shape::Stroke {
                path: vec![Point::new(38.0, 5.0), Point::new(40.0, 5.0)],
                radius: 1.5
            },
            Shape::Stroke {
                path: vec![Point::new(0.0, 5.0), Point::new(2.0, 5.0)],
                radius: 1.5
            },
        ]
    );
    let halves: Vec<Shape> = there
        .parts
        .iter()
        .flat_map(|part| shapes(&carry(part, &cut, Toward::Source).unwrap()))
        .collect();
    assert_eq!(
        halves,
        [
            Shape::Stroke {
                path: vec![path[0], Point::new(4.0, 5.0)],
                radius: 1.5
            },
            Shape::Stroke {
                path: vec![Point::new(4.0, 5.0), path[1]],
                radius: 1.5
            },
        ]
    );

    // A column at the canvas's middle is the panorama's column 0, its right edge brought
    // round; the whole canvas is a box on each strip.
    let column = on(
        &cut,
        Shape::Line {
            line: Line::Column(4.0),
            width: 2.0,
        },
    );
    let there = carry(&column, &cut, Toward::Destination).unwrap();
    assert_eq!(
        shapes(&there),
        [Shape::Line {
            line: Line::Column(0.0),
            width: 2.0
        }]
    );
    let whole = carry(&on(&cut, Shape::WholeImage), &cut, Toward::Destination).unwrap();
    assert_eq!(
        shapes(&whole),
        [boxed(36.0, 0.0, 40.0, 10.0), boxed(0.0, 0.0, 4.0, 10.0)]
    );
}

#[test]
fn a_location_carried_into_a_crop_is_clipped_and_said_to_be_partial() {
    // The crop of a 40 by 20 picture from 10, 4 to 30, 14, as `window` places one.
    let crop = Binding::new(
        picture("crop"),
        Space::flat(20, 10).unwrap(),
        picture("photo"),
        Space::flat(40, 20).unwrap(),
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 10.0,
            top: 4.0,
            right: 30.0,
            bottom: 14.0,
        },
    )
    .unwrap();
    let photo = Binding::root(picture("photo"), Space::flat(40, 20).unwrap());
    // Inside the crop: carried in and out again exactly.
    let inside = on(&photo, boxed(12.0, 5.0, 20.0, 9.0));
    let into = carry(&inside, &crop, Toward::Source).unwrap();
    assert_eq!(shapes(&into), [boxed(2.0, 1.0, 10.0, 5.0)]);
    assert!(!into.partial);
    let out = carry(&into.parts[0], &crop, Toward::Destination).unwrap();
    assert_eq!(shapes(&out), [inside.shape().clone()]);
    // Reaching past the crop: clipped to it, and partial.
    let over = on(&photo, boxed(0.0, 0.0, 15.0, 10.0));
    let into = carry(&over, &crop, Toward::Source).unwrap();
    assert_eq!(shapes(&into), [boxed(0.0, 0.0, 5.0, 6.0)]);
    assert!(into.partial);
    // A triangle with one corner outside: the part inside, partial.
    let triangle = on(
        &photo,
        Shape::Polygon {
            points: vec![
                Point::new(20.0, 6.0),
                Point::new(36.0, 6.0),
                Point::new(20.0, 12.0),
            ],
        },
    );
    let into = carry(&triangle, &crop, Toward::Source).unwrap();
    assert!(into.partial);
    let clipped = shapes(&into);
    let [Shape::Polygon { points }] = clipped.as_slice() else {
        panic!("{into:?}");
    };
    assert!(points.iter().all(|p| p.x <= 20.0 && p.y <= 10.0));
    assert!(points.contains(&Point::new(20.0, 2.0)), "{points:?}");
    // A path that leaves the crop by its foot is cut there, as a polygon is.
    let path = on(
        &photo,
        Shape::Line {
            line: Line::Path(vec![Point::new(15.0, 8.0), Point::new(15.0, 18.0)]),
            width: 3.0,
        },
    );
    let into = carry(&path, &crop, Toward::Source).unwrap();
    assert!(into.partial);
    assert_eq!(
        shapes(&into),
        [Shape::Line {
            line: Line::Path(vec![Point::new(5.0, 4.0), Point::new(5.0, 10.0)]),
            width: 3.0
        }]
    );
    // Wholly outside: refused. An absent thing stays absent.
    let away = on(&photo, boxed(32.0, 15.0, 39.0, 19.0));
    assert!(matches!(
        carry(&away, &crop, Toward::Source),
        Err(ArtifactError::Invalid { .. })
    ));
    let absent = carry(&on(&photo, Shape::Absent), &crop, Toward::Source).unwrap();
    assert_eq!(shapes(&absent), [Shape::Absent]);
}

#[test]
fn what_is_not_carried_is_refused() {
    let cut = wrap();
    let refused =
        |location: &Location, binding: &Binding, toward| match carry(location, binding, toward) {
            Err(error) => error.to_string(),
            Ok(carried) => panic!("carried: {carried:?}"),
        };
    // A location on another image than the end it leaves.
    let elsewhere = Binding::root(picture("other"), Space::flat(8, 10).unwrap());
    let stray = on(&elsewhere, boxed(1.0, 1.0, 2.0, 2.0));
    assert!(refused(&stray, &cut, Toward::Destination).contains("not on the image"));
    // A view does not map, and nothing is carried to the root.
    let panorama = Binding::root(picture("panorama"), Space::panorama(40, 10).unwrap());
    let view = on(
        &panorama,
        Shape::View {
            yaw: 0.0,
            pitch: 0.0,
            fov: 60.0,
        },
    );
    assert!(refused(&view, &cut, Toward::Source).contains("view"));
    let box_on_panorama = on(&panorama, boxed(1.0, 1.0, 2.0, 2.0));
    assert!(refused(&box_on_panorama, &panorama, Toward::Destination).contains("root"));
    // Through a rotation a box has no one image, and a line no one width.
    let turn = Binding::new(
        picture("turned"),
        Space::panorama(40, 10).unwrap(),
        picture("panorama"),
        Space::panorama(40, 10).unwrap(),
        Direction::SourceToDestination,
        Mapping::Rotation {
            matrix: [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        },
    )
    .unwrap();
    let turned = Binding::root(picture("turned"), Space::panorama(40, 10).unwrap());
    let a_box = on(&turned, boxed(1.0, 1.0, 2.0, 2.0));
    assert!(refused(&a_box, &turn, Toward::Destination).contains("a box"));
    // A polygon that would cross the seam once rotated is refused, not split.
    let across = on(
        &turned,
        Shape::Polygon {
            points: vec![
                Point::new(28.0, 4.0),
                Point::new(32.0, 4.0),
                Point::new(30.0, 6.0),
            ],
        },
    );
    assert!(refused(&across, &turn, Toward::Destination).contains("seam"));
    // Nothing is carried through a perspective view yet (ADR 0051), whichever way.
    let look = Binding::new(
        picture("look"),
        Space::flat(10, 10).unwrap(),
        picture("panorama"),
        Space::panorama(40, 10).unwrap(),
        Direction::SourceToDestination,
        Mapping::Perspective {
            yaw: 0.0,
            pitch: 0.0,
            fov: 60.0,
        },
    )
    .unwrap();
    let in_view = on(
        &Binding::root(picture("look"), Space::flat(10, 10).unwrap()),
        boxed(1.0, 1.0, 2.0, 2.0),
    );
    assert!(refused(&in_view, &look, Toward::Destination).contains("perspective view"));
    let on_panorama = on(&panorama, boxed(18.0, 4.0, 22.0, 6.0));
    assert!(refused(&on_panorama, &look, Toward::Source).contains("perspective view"));
}
