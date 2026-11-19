//! `edge_similarity` (plan `docs/plans/m2c-milestone-2.md`, task X1, item 9): squint's tool
//! gives here what it gave in squint, on pictures squint's own code painted.
//!
//! The pictures of `fixtures/compare/` were painted at 128 by 96 pixels (one at 256 by 192)
//! by squint's test kit (`crates/squint-testkit/src/images.rs`: `Scene::random`,
//! `low_texture`), as squint's own test of the tool made them
//! (`crates/squint-tools/tests/it/compare.rs`), and every number below was computed by
//! squint's `builtin/compare.rs` at the commit `docs/sources.toml` pins, apart from this
//! crate, through its tool's call.

use std::path::PathBuf;

use image::RgbImage;
use whizbang_core::artifact::Image;
use whizbang_core::identity::{Projection, Topology};
use whizbang_squint::compare::{self, CompareError};

use super::support::*;

pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/compare")
}

pub fn fixture(name: &str) -> Image {
    let file = std::fs::read(fixtures().join(format!("{name}.png"))).unwrap();
    image_in(file, Projection::Flat, Topology::Bounded)
}

pub fn rgb(name: &str) -> RgbImage {
    compare::rgb(&fixture(name), &limits()).unwrap()
}

/// Equal to within a few units in the last place.
pub fn close(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= 1e-12 * want.abs().max(1.0),
        "{what}: {got:?} against squint's {want:?}"
    );
}

/// Frame, against, and squint's edge similarity and edge energy.
const VECTORS: [(&str, &str, f64, f64); 6] = [
    ("scene", "scene", 1.0, 0.03202736902823933),
    ("scene", "blank", 0.20373068456420762, 0.0018151196583270728),
    ("scene", "toned", 0.9192660833039019, 0.02646882118789066),
    ("scene", "other", 0.004365808610761663, 0.03202736902823933),
    (
        "scene",
        "scene-256",
        0.9985284218592351,
        0.03143798610316114,
    ),
    ("before", "after", 0.9750041966929484, 0.02378151894719697),
];

#[test]
fn the_ported_vectors_give_squints_ratio() {
    for (frame, against, similarity, energy) in VECTORS {
        let what = format!("{frame} against {against}");
        let edges = compare::edges(&rgb(frame), &rgb(against)).unwrap();
        close(edges.similarity, similarity, &what);
        close(edges.energy, energy, &what);
        if frame == against {
            continue;
        }
        // Through the artifacts too, and squint's sentence.
        let got = compare::edge_similarity(&fixture(frame), &fixture(against), &limits()).unwrap();
        assert_eq!(got, edges, "{what}");
        let said = compare::edges_summary(&got, &format!("raw:{frame}"), &format!("raw:{against}"));
        assert!(
            said.starts_with(&format!(
                "Edge similarity of raw:{frame} and raw:{against}: {similarity:.3}"
            )),
            "{said}"
        );
    }
    // Squint's refusals: the same picture twice, and two grids.
    let scene = fixture("scene");
    let same = compare::edge_similarity(&scene, &scene, &limits());
    assert!(matches!(same, Err(CompareError::Invalid(_))), "{same:?}");
    let wide = picture(128, 64, |x, y| [x as u8, y as u8, 0, 255]);
    let two = compare::edge_similarity(&scene, &wide, &limits()).unwrap_err();
    assert!(two.to_string().contains("share one grid"), "{two}");
}
