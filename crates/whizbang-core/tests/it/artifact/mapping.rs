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
    // Through a rotation a line has no one width.
    let turn = quarter_turn();
    let turned = Binding::root(picture("turned"), Space::panorama(40, 10).unwrap());
    let a_line = on(
        &turned,
        Shape::Line {
            line: Line::Column(3.0),
            width: 1.0,
        },
    );
    assert!(refused(&a_line, &turn, Toward::Destination).contains("a line"));
    // Nor through a perspective view, and a view is not carried through one.
    let look = look();
    let in_view = Binding::root(picture("look"), Space::flat(256, 256).unwrap());
    let stroke = on(
        &in_view,
        Shape::Stroke {
            path: vec![Point::new(1.0, 1.0), Point::new(9.0, 9.0)],
            radius: 2.0,
        },
    );
    assert!(refused(&stroke, &look, Toward::Destination).contains("a stroke"));
    let a_view = on(
        &Binding::root(picture("panorama"), Space::panorama(3600, 1800).unwrap()),
        Shape::View {
            yaw: 0.0,
            pitch: 0.0,
            fov: 60.0,
        },
    );
    assert!(refused(&a_view, &look, Toward::Source).contains("view"));
}

/// A quarter turn about the vertical of a panorama 40 by 10.
fn quarter_turn() -> Binding {
    Binding::new(
        picture("turned"),
        Space::panorama(40, 10).unwrap(),
        picture("panorama"),
        Space::panorama(40, 10).unwrap(),
        Direction::SourceToDestination,
        Mapping::Rotation {
            matrix: [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        },
    )
    .unwrap()
}

/// A view 256 pixels square at yaw 45, pitch 0 and a field of view of 60 degrees, into a
/// panorama 3600 by 1800: ten pixels a degree.
fn look() -> Binding {
    looking(45.0, 0.0, 60.0)
}

fn looking(yaw: f64, pitch: f64, fov: f64) -> Binding {
    Binding::new(
        picture("look"),
        Space::flat(256, 256).unwrap(),
        picture("panorama"),
        Space::panorama(3600, 1800).unwrap(),
        Direction::SourceToDestination,
        Mapping::Perspective { yaw, pitch, fov },
    )
    .unwrap()
}

fn polygon_of(shape: &Shape) -> &[Point] {
    match shape {
        Shape::Polygon { points } => points,
        other => panic!("{other:?} is not a polygon"),
    }
}

fn spread(points: &[Point]) -> Spread {
    let xs = points.iter().map(|p| p.x);
    let ys = points.iter().map(|p| p.y);
    (
        xs.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
        ys.clone().fold(f64::INFINITY, f64::min),
        ys.fold(f64::NEG_INFINITY, f64::max),
    )
}

/// The spreads of a carry's two parts, the one on the left of the panorama first.
/// Left, right, top and bottom of a part.
type Spread = (f64, f64, f64, f64);

fn sides(carried: &Carried) -> (Spread, Spread) {
    let [a, b] = &carried.parts[..] else {
        panic!("{:?}", carried.parts);
    };
    let (a, b) = (spread(polygon_of(a.shape())), spread(polygon_of(b.shape())));
    if a.0 <= b.0 { (a, b) } else { (b, a) }
}

#[test]
fn a_box_in_a_view_lands_on_its_panorama_as_the_polygon_around_where_it_looks() {
    // Squint's `a_box_in_a_view_becomes_a_polygon_around_its_direction`, in pixels.
    let look = look();
    let in_view = Binding::root(picture("look"), Space::flat(256, 256).unwrap());
    let middle = on(&in_view, boxed(102.4, 102.4, 153.6, 153.6));
    let there = carry(&middle, &look, Toward::Destination).unwrap();
    assert!(!there.partial);
    assert_eq!(there.parts.len(), 1);
    assert_eq!(
        *there.parts[0].binding(),
        Binding::root(picture("panorama"), Space::panorama(3600, 1800).unwrap())
    );
    let points = polygon_of(there.parts[0].shape());
    assert_eq!(points.len(), 64);
    // Yaw 45 is column 2250. A fifth of the view across, at its middle, spans twice
    // atan(tan(30) / 5), 13.17 degrees: 131.7 pixels at ten a degree.
    let (x0, x1, y0, y1) = spread(points);
    assert!(x0 < 2250.0 && 2250.0 < x1 && y0 < 900.0 && 900.0 < y1);
    assert!((x1 - x0 - 131.7).abs() < 0.1, "{}", x1 - x0);
    // And back into the view: the box it came from, to within the sampling of its edges.
    let back = carry(&there.parts[0], &look, Toward::Source).unwrap();
    let Shape::Polygon { points } = back.parts[0].shape() else {
        panic!("{:?}", back.parts[0].shape());
    };
    let (x0, x1, y0, y1) = spread(points);
    for (got, want) in [(x0, 102.4), (x1, 153.6), (y0, 102.4), (y1, 153.6)] {
        assert!((got - want).abs() < 1e-6, "{got} for {want}");
    }
    assert_eq!(*back.parts[0].binding(), look);
}

#[test]
fn a_footprint_over_the_seam_is_two_parts_and_one_that_holds_a_pole_is_a_band() {
    // Squint's `a_footprint_over_the_seam_is_cut_and_flagged_and_a_pole_spans_the_width`:
    // here the two sides of the seam are both parts of the one carry.
    let in_view = Binding::root(picture("look"), Space::flat(256, 256).unwrap());
    let whole = on(&in_view, Shape::WholeImage);
    let seam = carry(&whole, &looking(-178.0, 0.0, 60.0), Toward::Destination).unwrap();
    assert_eq!(seam.parts.len(), 2);
    let (b, a) = sides(&seam);
    // One part ends at the panorama's right edge and the other starts at its left.
    assert!(a.1 == 3600.0 && a.0 > 3000.0, "{a:?}");
    assert!(b.0 == 0.0 && b.1 < 600.0, "{b:?}");
    // A view up near the north pole holds it: a band from the top, the full width.
    let pole = carry(&whole, &looking(0.0, 80.0, 60.0), Toward::Destination).unwrap();
    let [part] = &pole.parts[..] else {
        panic!("{:?}", pole.parts);
    };
    let Shape::Box {
        left,
        top,
        right,
        bottom,
    } = part.shape()
    else {
        panic!("{:?}", part.shape());
    };
    assert_eq!((*left, *top, *right), (0.0, 0.0, 3600.0));
    assert!(*bottom > 180.0 && *bottom < 540.0, "{bottom}");
    // A small box in that view below the pole holds no pole: a polygon.
    let below = on(&in_view, boxed(102.4, 153.6, 153.6, 204.8));
    let below = carry(&below, &looking(0.0, 80.0, 60.0), Toward::Destination).unwrap();
    assert_eq!(below.parts.len(), 1);
    polygon_of(below.parts[0].shape());
    // A region at the top of a view looking high up crosses the seam behind it.
    let top = on(&in_view, boxed(76.8, 0.0, 179.2, 25.6));
    let high = carry(&top, &looking(0.0, 60.0, 120.0), Toward::Destination).unwrap();
    assert_eq!(high.parts.len(), 2, "{:?}", high.parts);
}

#[test]
fn a_box_on_a_panorama_lands_in_the_view_and_one_behind_it_is_refused() {
    // Squint's `a_raw_box_maps_into_the_view_and_a_box_behind_it_is_refused`.
    let look = looking(0.0, 0.0, 90.0);
    let panorama = Binding::root(picture("panorama"), Space::panorama(3600, 1800).unwrap());
    let ahead = on(&panorama, boxed(1728.0, 810.0, 1872.0, 990.0));
    let seen = carry(&ahead, &look, Toward::Source).unwrap();
    assert!(!seen.partial);
    let Shape::Box {
        left,
        top,
        right,
        bottom,
    } = seen.parts[0].shape()
    else {
        panic!("{:?}", seen.parts[0].shape());
    };
    assert!(*left < 128.0 && *right > 128.0 && *top < 128.0 && *bottom > 128.0);
    // Wider than the view: clipped to it, and said to be partial.
    let wide = on(&panorama, boxed(1000.0, 400.0, 2600.0, 1400.0));
    let clipped = carry(&wide, &look, Toward::Source).unwrap();
    assert!(clipped.partial);
    assert_eq!(*clipped.parts[0].shape(), boxed(0.0, 0.0, 256.0, 256.0));
    // Behind the view: column 0 is longitude -180.
    let behind = on(&panorama, boxed(0.0, 810.0, 144.0, 990.0));
    assert!(carry(&behind, &look, Toward::Source).is_err());
    // The whole panorama fills the view.
    let all = carry(&on(&panorama, Shape::WholeImage), &look, Toward::Source).unwrap();
    assert_eq!(*all.parts[0].shape(), Shape::WholeImage);
}

#[test]
fn a_polygon_or_a_box_rotated_over_the_seam_is_cut_there() {
    let turn = quarter_turn();
    let turned = Binding::root(picture("turned"), Space::panorama(40, 10).unwrap());
    // A quarter turn takes columns 28 to 32 round the seam.
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
    let cut = carry(&across, &turn, Toward::Destination).unwrap();
    assert_eq!(cut.parts.len(), 2);
    let (b, a) = sides(&cut);
    assert!(a.1 == 40.0 && b.0 == 0.0, "{a:?} {b:?}");
    // A box that stays on one side is the polygon of its boundary; one that holds
    // nothing of the seam is one part.
    let a_box = on(&turned, boxed(1.0, 3.0, 4.0, 6.0));
    let moved = carry(&a_box, &turn, Toward::Destination).unwrap();
    assert_eq!(moved.parts.len(), 1);
    assert_eq!(polygon_of(moved.parts[0].shape()).len(), 64);
    // The same polygon, not crossing, is carried vertex by vertex as before.
    let inside = on(
        &turned,
        Shape::Polygon {
            points: vec![
                Point::new(2.0, 4.0),
                Point::new(6.0, 4.0),
                Point::new(4.0, 6.0),
            ],
        },
    );
    let one = carry(&inside, &turn, Toward::Destination).unwrap();
    assert_eq!(polygon_of(one.parts[0].shape()).len(), 3);
}

/// The area of a polygon, by the shoelace.
fn area(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

fn turned_by(matrix: [f64; 9]) -> Binding {
    Binding::new(
        picture("turned"),
        Space::panorama(40, 10).unwrap(),
        picture("panorama"),
        Space::panorama(40, 10).unwrap(),
        Direction::SourceToDestination,
        Mapping::Rotation { matrix },
    )
    .unwrap()
}

#[test]
fn a_polygon_wider_than_half_the_panorama_is_rotated_onto_the_columns_it_covers() {
    // Columns 2 to 25 of a panorama 40 wide: wider than half of it. Taken round by its
    // vertices alone it would be the 17 columns outside it; its edges, sampled, say
    // which way round it goes.
    let turned = Binding::root(picture("turned"), Space::panorama(40, 10).unwrap());
    let wide = |right: f64| {
        on(
            &turned,
            Shape::Polygon {
                points: vec![
                    Point::new(2.0, 4.0),
                    Point::new(right, 4.0),
                    Point::new(right, 6.0),
                    Point::new(2.0, 6.0),
                ],
            },
        )
    };
    let identity = turned_by([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    let same = carry(&wide(25.0), &identity, Toward::Destination).unwrap();
    let [part] = &same.parts[..] else {
        panic!("{:?}", same.parts);
    };
    let points = polygon_of(part.shape());
    assert_eq!(points.len(), 4);
    let (x0, x1, _, _) = spread(points);
    assert_eq!((x0, x1), (2.0, 25.0));
    // A quarter turn moves it ten columns, to 12 to 35, still 46 square pixels.
    let moved = carry(&wide(25.0), &quarter_turn(), Toward::Destination).unwrap();
    let [part] = &moved.parts[..] else {
        panic!("{:?}", moved.parts);
    };
    let (x0, x1, _, _) = spread(polygon_of(part.shape()));
    assert!(
        (x0 - 12.0).abs() < 1e-9 && (x1 - 35.0).abs() < 1e-9,
        "{x0} {x1}"
    );
    assert!((area(polygon_of(part.shape())) - 46.0).abs() < 1e-9);
    // Columns 2 to 34, turned to 12 to 44, cross the seam: 28 columns and 4, 64 in all.
    let across = carry(&wide(34.0), &quarter_turn(), Toward::Destination).unwrap();
    let total: f64 = across
        .parts
        .iter()
        .map(|p| area(polygon_of(p.shape())))
        .sum();
    assert_eq!(across.parts.len(), 2);
    assert!((total - 64.0).abs() < 1e-9, "{total}");
}

#[test]
fn a_band_round_the_panorama_rotated_and_tilted_keeps_all_it_covers() {
    // A band nearly the full width of a panorama 400 by 100, columns 10 to 390 and rows 40
    // to 60, under a tilt and eight turns about the vertical. Its footprint can reach past both edges of the
    // panorama once taken round, and every whole turn it reaches into is kept: the parts
    // cover what the band covers, to within its edges' sampling and thinning to 64
    // vertices, which moves an edge by a pixel or so. A part left out would be a share of
    // the band, far more.
    let (c, s) = (5.0 / 13.0, 12.0 / 13.0);
    let tilt = [1.0, 0.0, 0.0, 0.0, c, -s, 0.0, s, c];
    let space = Space::panorama(400, 100).unwrap();
    let start = Binding::root(picture("turned"), space);
    let band = on(&start, boxed(10.0, 40.0, 390.0, 60.0));
    for step in 0..8 {
        let angle = f64::from(step) * std::f64::consts::PI / 4.0;
        let (sn, cs) = angle.sin_cos();
        let yaw = [cs, -sn, 0.0, sn, cs, 0.0, 0.0, 0.0, 1.0];
        let mut matrix = [0.0; 9];
        for row in 0..3 {
            for column in 0..3 {
                matrix[3 * row + column] = (0..3)
                    .map(|k| yaw[3 * row + k] * tilt[3 * k + column])
                    .sum();
            }
        }
        let turn = Binding::new(
            picture("turned"),
            space,
            picture("panorama"),
            space,
            Direction::SourceToDestination,
            Mapping::Rotation { matrix },
        )
        .unwrap();
        let carried = carry(&band, &turn, Toward::Destination).unwrap();
        let inside = |p: Point, points: &[Point]| {
            let mut odd = false;
            for (i, a) in points.iter().enumerate() {
                let b = points[(i + 1) % points.len()];
                if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x)
                {
                    odd = !odd;
                }
            }
            odd
        };
        let (mut truth, mut wrong) = (0u32, 0u32);
        for i in 0..200 {
            for j in 0..50 {
                let p = Point::new(f64::from(i) * 2.0 + 1.0, f64::from(j) * 2.0 + 1.0);
                let from = turn.to_source(p).unwrap();
                let is = (40.0..=60.0).contains(&from.y) && (10.0..=390.0).contains(&from.x);
                let shown = carried
                    .parts
                    .iter()
                    .any(|part| inside(p, polygon_of(part.shape())));
                truth += u32::from(is);
                wrong += u32::from(is != shown);
            }
        }
        assert!(
            f64::from(wrong) < 0.15 * f64::from(truth),
            "turn {step}: {wrong} of {truth} points wrong, {} parts",
            carried.parts.len()
        );
    }
}

#[test]
fn a_box_that_holds_both_poles_is_the_whole_panorama_once_rotated() {
    let whole = on(
        &Binding::root(picture("turned"), Space::panorama(40, 10).unwrap()),
        boxed(0.0, 0.0, 40.0, 10.0),
    );
    let carried = carry(&whole, &quarter_turn(), Toward::Destination).unwrap();
    assert_eq!(
        carried
            .parts
            .iter()
            .map(|p| p.shape().clone())
            .collect::<Vec<_>>(),
        [boxed(0.0, 0.0, 40.0, 10.0)]
    );
}

#[test]
fn a_polygon_on_the_panorama_is_clipped_to_the_view_and_behind_it() {
    // A triangle on the panorama with one corner far right of a 90 degree view and one
    // behind it: what the view sees is clipped to its edges, not held to them, and said
    // to be partial.
    let look = looking(0.0, 0.0, 90.0);
    let panorama = Binding::root(picture("panorama"), Space::panorama(3600, 1800).unwrap());
    let triangle = on(
        &panorama,
        Shape::Polygon {
            points: vec![
                Point::new(1700.0, 850.0),
                Point::new(2500.0, 900.0),
                Point::new(1750.0, 1000.0),
            ],
        },
    );
    let seen = carry(&triangle, &look, Toward::Source).unwrap();
    assert!(seen.partial);
    let points = polygon_of(seen.parts[0].shape());
    assert!(
        points
            .iter()
            .all(|p| (0.0..=256.0).contains(&p.x) && (0.0..=256.0).contains(&p.y))
    );
    // The far corner is cut away by the view's right edge: two vertices lie on it.
    assert_eq!(
        points.iter().filter(|p| p.x == 256.0).count(),
        2,
        "{points:?}"
    );
    // A triangle with a corner behind the view keeps the part in front.
    let behind = on(
        &panorama,
        Shape::Polygon {
            points: vec![
                Point::new(1750.0, 850.0),
                Point::new(100.0, 900.0),
                Point::new(1850.0, 950.0),
            ],
        },
    );
    let front = carry(&behind, &look, Toward::Source).unwrap();
    assert!(front.partial);
    assert!(polygon_of(front.parts[0].shape()).len() >= 3);
    // Wholly behind: refused as behind, not as outside.
    let back = on(
        &panorama,
        Shape::Polygon {
            points: vec![
                Point::new(10.0, 850.0),
                Point::new(60.0, 900.0),
                Point::new(20.0, 950.0),
            ],
        },
    );
    let ArtifactError::Invalid { why, .. } = carry(&back, &look, Toward::Source).unwrap_err()
    else {
        panic!();
    };
    assert!(why.contains("behind"), "{why}");
}
