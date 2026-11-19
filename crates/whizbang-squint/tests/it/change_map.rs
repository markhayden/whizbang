//! `change_map` (plan `docs/plans/m2c-milestone-2.md`, task X1, item 9): squint's tool gives
//! here the map and the share it gave in squint, on the pictures of `fixtures/compare/`
//! (`edge_similarity.rs` says how they and the numbers were made). Each `map-*.png` there is
//! the map squint's tool returned for that pair, decoded from its envelope.

use whizbang_core::identity::Mapping;
use whizbang_squint::compare;

use super::edge_similarity::{close, fixture, fixtures, rgb};
use super::support::*;

/// Frame, against, and squint's changed fraction.
const VECTORS: [(&str, &str, f64); 4] = [
    ("before", "after", 0.0010579427083333333),
    ("scene", "toned", 0.0003255208333333333),
    ("scene", "scene-256", 0.0),
    ("scene", "other", 0.10636393229166667),
];

#[test]
fn the_ported_vectors_give_squints_map_and_measurements() {
    for (frame, against, fraction) in VECTORS {
        let what = format!("{frame} against {against}");
        let got = compare::changes(&rgb(frame), &rgb(against)).unwrap();
        close(got.fraction, fraction, &what);
        let file = fixtures().join(format!("map-{frame}-{against}.png"));
        let squints = image::open(file).unwrap().to_rgb8();
        assert_eq!(got.map.dimensions(), squints.dimensions(), "{what}");
        assert!(got.map == squints, "{what}: the map is not squint's");
        // Through the artifacts: the same map, as an image bound onto the whole frame.
        let (frame_image, against_image) = (fixture(frame), fixture(against));
        let (changes, map, binding) =
            compare::change_map(&frame_image, &against_image, &limits()).unwrap();
        assert_eq!(changes, got, "{what}");
        assert_eq!(map.space().width(), 128);
        assert_eq!(binding.destination_space(), frame_image.space());
        assert_eq!(
            *binding.mapping(),
            Mapping::Crop {
                left: 0.0,
                top: 0.0,
                right: 128.0,
                bottom: 96.0
            }
        );
        let said = compare::changes_summary(&changes, "raw:source", "aligned:candidate:t1", "m");
        assert!(
            said.contains(&format!("{:.1}% of pixels", fraction * 100.0)),
            "{said}"
        );
    }
    // The map lights where the blob was added, at scene (0.8, 0.2): pixel (102, 26).
    let map = compare::changes(&rgb("before"), &rgb("after")).unwrap().map;
    let peak = (96..110)
        .flat_map(|x| (20..34).map(move |y| (x, y)))
        .map(|(x, y)| map.get_pixel(x, y).0[0])
        .max()
        .unwrap();
    assert!(peak > 90, "{peak}");
}
