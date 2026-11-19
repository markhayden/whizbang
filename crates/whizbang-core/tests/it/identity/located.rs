//! A location made by carrying another through a rotation, a wrapped cut or a homography
//! has a fixed identity (plan m3, task O2, item 2): its coordinates are hashed (ADR 0002),
//! so the doubles must be the same on every machine. A rotation's sine, cosine and arc
//! functions are `libm`'s for that reason; a wrapped cut adds and subtracts alone, and a
//! homography multiplies, adds and divides. CI holds these
//! identities on both of its runners, macOS and Linux.

use whizbang_core::artifact::{Location, Origin, Shape, Step, Toward, carry};
use whizbang_core::identity::{Binding, Digest, Direction, Mapping, PixelDigest, Point, Space};

fn picture(name: &str) -> PixelDigest {
    PixelDigest(Digest::of_bytes(name.as_bytes()))
}

fn made() -> Origin {
    Origin::new(Step::new("fixture", "1")).unwrap()
}

#[test]
fn a_mapped_location_has_its_fixed_identity_for_a_rotation_and_a_wrap() {
    // A turn about the vertical axis by the angle of the 3-4-5 triangle, after a tilt by
    // that of the 5-12-13: every entry a quotient of whole numbers, so the binding is the
    // same doubles everywhere, and only the mapping's own functions are tested.
    let (c, s) = (5.0 / 13.0, 12.0 / 13.0);
    let matrix = [0.6, -0.8 * c, 0.8 * s, 0.8, 0.6 * c, -0.6 * s, 0.0, s, c];
    let space = Space::panorama(400, 200).unwrap();
    let turn = Binding::new(
        picture("turned"),
        space,
        picture("panorama"),
        space,
        Direction::SourceToDestination,
        Mapping::Rotation { matrix },
    )
    .unwrap();
    let polygon = Shape::Polygon {
        points: vec![
            Point::new(180.25, 60.5),
            Point::new(230.0, 70.125),
            Point::new(205.5, 120.75),
        ],
    };
    let start = Location::new(Binding::root(picture("turned"), space), polygon, made()).unwrap();
    let there = carry(&start, &turn, Toward::Destination).unwrap();
    let [part] = there.parts.as_slice() else {
        panic!("{there:?}");
    };
    let Shape::Polygon { points } = part.shape() else {
        panic!("{part:?}");
    };
    let bits: Vec<[u64; 2]> = points
        .iter()
        .map(|p| [p.x.to_bits(), p.y.to_bits()])
        .collect();
    assert_eq!(bits, ROTATED, "{points:?}");
    assert_eq!(part.id().to_string(), ROTATED_ID, "the rotated polygon");

    // A stroke across the middle of a cut's canvas: two strokes on the panorama.
    let cut = Binding::new(
        picture("canvas"),
        Space::flat(56, 200).unwrap(),
        picture("panorama"),
        space,
        Direction::SourceToDestination,
        Mapping::Wrap { strip: 28 },
    )
    .unwrap();
    let stroke = Shape::Stroke {
        path: vec![Point::new(10.5, 20.25), Point::new(41.75, 90.5)],
        radius: 3.5,
    };
    let start = Location::new(cut.clone(), stroke, made()).unwrap();
    let there = carry(&start, &cut, Toward::Destination).unwrap();
    let ids: Vec<String> = there.parts.iter().map(|p| p.id().to_string()).collect();
    assert_eq!(ids, WRAPPED_IDS, "{there:?}");

    // A box through a homography, the last kind of mapping owed a vector: the polygon of
    // its four corners, each a quotient worked by the mapping alone.
    let flat = Space::flat(200, 120).unwrap();
    let skew = Binding::new(
        picture("photo"),
        flat,
        picture("plate"),
        flat,
        Direction::SourceToDestination,
        Mapping::Homography {
            matrix: [0.9, 0.1, 5.0, -0.05, 1.1, 3.0, 0.0001, 0.0002, 1.0],
        },
    )
    .unwrap();
    let a_box = Shape::Box {
        left: 20.5,
        top: 10.25,
        right: 120.75,
        bottom: 90.0,
    };
    let start = Location::new(Binding::root(picture("photo"), flat), a_box, made()).unwrap();
    let there = carry(&start, &skew, Toward::Destination).unwrap();
    let [part] = there.parts.as_slice() else {
        panic!("{there:?}");
    };
    assert_eq!(part.id().to_string(), SKEWED_ID, "{part:?}");
}

/// The three vertices carried, as the bits of their doubles: 215.46146099044373,
/// 100.35980510413567; 238.77959047799897, 63.10041419565916; 280.2666790698342,
/// 103.03771903347716. The same points, to the last digit printed, came out of the
/// rotation worked again apart from this crate, in Python's double arithmetic.
const ROTATED: [[u64; 2]; 3] = [
    [4641784892778531517, 4636762610387963029],
    [4642605326522837226, 4634077411117577347],
    [4643638119751858035, 4636951051828186663],
];
const ROTATED_ID: &str = "sha256:3d274623d14ed61a5bdb5188bf637e152194e502b7240ad37853f0349dc8e18e";
/// The stroke cut where it crosses the canvas's column 28, at row 59.59: from 382.5, 20.25
/// to the panorama's right edge, and from its left edge to 13.75, 90.5.
const WRAPPED_IDS: [&str; 2] = [
    "sha256:d73785ff2776f6ec7686bebf85577fb63c9ffe33e2e3975b03be5be5863423d4",
    "sha256:04adb733e2071280e7a8e6206a522212668831d907126104e4868b637b8b9386",
];

/// The corners land at 24.375062244796332, 13.195896823025596; 113.10242820165168,
/// 8.122765931221497; 119.093269907531, 93.16069218260807; 31.81216607029068,
/// 98.99024557619727: the same, to the last digit printed, as the matrix worked again
/// apart from this crate in Python's double arithmetic.
const SKEWED_ID: &str = "sha256:04aee06d22caea27fc683169a1762648fa94d0bc201490b651c4344eea34ddda";
