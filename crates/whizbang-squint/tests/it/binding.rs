//! A location through a homography (plan `docs/plans/m2c-milestone-2.md`, task A2, item 2;
//! squint's `crates/squint-core/src/frames/aligned.rs`): a box on an aligned frame maps to the
//! image the frame was warped from, and back.

use whizbang_core::identity::Point;
use whizbang_squint::align::{self, Options};
use whizbang_squint::view::{ViewError, box_through_homography, points_into_homography};

use super::scenes;
use super::support::*;

fn plane(name: &str) -> whizbang_core::artifact::Image {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/align");
    let file = std::fs::read(dir.join(format!("{name}.png"))).unwrap();
    image_in(
        file,
        whizbang_core::identity::Projection::Flat,
        whizbang_core::identity::Topology::Bounded,
    )
}

#[test]
fn a_location_maps_through_a_homography_and_back() {
    let (image, against) = (plane("keystone"), plane("scene"));
    let aligned = align::align(&image, &against, &Options::default(), &limits()).unwrap();
    let h = aligned.transform.unwrap().homography;
    let frame = aligned.frame.unwrap();
    // A box on the frame becomes the polygon of its corners on the image: each where the
    // inverse of the registered homography sends it.
    let bbox = [30.0, 20.0, 90.0, 70.0];
    let traced = box_through_homography(bbox, &frame.binding).unwrap();
    assert!(!traced.partial);
    let corners = [(30.0, 20.0), (90.0, 20.0), (90.0, 70.0), (30.0, 70.0)];
    let inverse = scenes::invert(&h);
    for (point, (x, y)) in traced.points.iter().zip(corners) {
        let (wx, wy) = scenes::apply(&inverse, x, y);
        assert!(
            (point.x - wx).abs() < 1e-9 && (point.y - wy).abs() < 1e-9,
            "{point:?}"
        );
    }
    // A keystone does not keep a box a box: the polygon's sides are not level.
    assert!((traced.points[0].y - traced.points[1].y).abs() > 0.1);
    // And back: the polygon on the image goes into the frame at the box's corners.
    let back = points_into_homography(&traced.points, &frame.binding).unwrap();
    for (point, (x, y)) in back.iter().zip(corners) {
        assert!(
            (point.x - x).abs() < 1e-9 && (point.y - y).abs() < 1e-9,
            "{point:?}"
        );
    }
    // A box that reaches past the image is clamped into it and flagged; one that lies wholly
    // outside it is refused; and a crop's binding is not an aligned frame's.
    let past = box_through_homography([-40.0, -30.0, 20.0, 20.0], &frame.binding).unwrap();
    assert!(past.partial);
    assert!(past.points.iter().all(|p| p.x >= 0.0 && p.y >= 0.0));
    // One that reaches past the frame alone is clipped to the frame and flagged, though each
    // corner it keeps lands inside the image.
    let edge = box_through_homography([100.0, 40.0, 140.0, 60.0], &frame.binding).unwrap();
    assert!(edge.partial);
    assert!((edge.points[1].x - scenes::apply(&inverse, 128.0, 40.0).0).abs() < 1e-9);
    let outside = box_through_homography([-400.0, -300.0, -200.0, -100.0], &frame.binding);
    assert!(matches!(outside, Err(ViewError::Invalid(_))), "{outside:?}");
    let crop = look(&image, [0.25, 0.25, 0.75, 0.75]);
    assert!(box_through_homography(bbox, &crop.binding).is_err());
    assert!(points_into_homography(&[Point::new(1.0, 1.0)], &crop.binding).is_err());
}
