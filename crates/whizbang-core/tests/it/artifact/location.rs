//! The rules of rascal's `location.rs` that `location/1` now holds (plan m3, task O1,
//! item 2): a polygon whose edges cross is refused, by rascal's test of crossing
//! (`crates/rascal-core/src/location.rs`, `is_simple`, at the commit `docs/sources.toml`
//! pins), on the picture of 8 by 6 the other tests of the type use.

use whizbang_core::artifact::{ArtifactError, ArtifactType, Location, Shape};
use whizbang_core::identity::Point;

use super::support::{made_by, on};

fn polygon(points: &[(f64, f64)]) -> Result<Location, ArtifactError> {
    let points = points.iter().map(|(x, y)| Point::new(*x, *y)).collect();
    Location::new(on("photo"), Shape::Polygon { points }, made_by("find"))
}

#[test]
fn a_polygon_whose_edges_cross_is_refused() {
    // A bow tie: the first edge and the third cross in the middle.
    let refused = polygon(&[(1.0, 1.0), (7.0, 5.0), (7.0, 1.0), (1.0, 5.0)]);
    match refused {
        Err(ArtifactError::Invalid {
            artifact: ArtifactType::Location,
            why,
        }) => assert!(why.contains("edges cross"), "{why}"),
        other => panic!("{other:?}"),
    }
    // The same four points in order around: a simple quadrilateral.
    polygon(&[(1.0, 1.0), (7.0, 1.0), (7.0, 5.0), (1.0, 5.0)]).unwrap();
    // A concave polygon is simple.
    polygon(&[(1.0, 1.0), (7.0, 1.0), (4.0, 3.0), (7.0, 5.0), (1.0, 5.0)]).unwrap();
    // Edges that only touch at a point, or lie along one line, are not a crossing by
    // rascal's strict test: a vertex resting on another edge is kept.
    polygon(&[(1.0, 1.0), (7.0, 1.0), (4.0, 1.0), (4.0, 5.0)]).unwrap();
    // A pentagram drawn in one stroke crosses itself five times.
    let star = [(4.0, 0.5), (6.0, 5.5), (1.0, 2.0), (7.0, 2.0), (2.0, 5.5)];
    assert!(polygon(&star).is_err());
}
