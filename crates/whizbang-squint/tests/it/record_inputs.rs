//! The inputs `whizbang store record-inputs` writes for squint's records
//! (`whizbang_squint::examples`) are the images the records' tests construct, identity for
//! identity (plan E, task O1, "The inputs, made before O1.1").

use whizbang_core::artifact::{ArtifactId, Image};
use whizbang_squint::examples::{TOOLS, inputs};

use super::edge_similarity::fixture;

/// What each record's test constructs for its examples: tool, example, port, image.
fn constructed() -> Vec<(&'static str, &'static str, &'static str, Image)> {
    let [source, candidate, reshaped] = super::align_record::pictures();
    let [scene, edited, panorama] = super::record::pictures();
    let [(_, tiled_scene), (_, tiled_fine)] = super::tiles_record::pictures();
    vec![
        ("align", "turned", "image", candidate),
        ("align", "turned", "against", source.clone()),
        ("align", "moved", "image", reshaped),
        ("align", "moved", "against", source),
        ("change_map", "added", "frame", fixture("before")),
        ("change_map", "added", "against", fixture("after")),
        ("change_map", "greyed", "frame", fixture("before")),
        (
            "change_map",
            "greyed",
            "against",
            super::compare_record::desaturated(),
        ),
        ("edge_similarity", "toned", "frame", fixture("scene")),
        ("edge_similarity", "toned", "against", fixture("toned")),
        ("edge_similarity", "other", "frame", fixture("scene")),
        ("edge_similarity", "other", "against", fixture("other")),
        ("edge_similarity", "moved", "frame", fixture("before")),
        ("edge_similarity", "moved", "against", fixture("after")),
        ("tiles", "scene", "images", tiled_scene),
        ("tiles", "fine", "images", tiled_fine),
        (
            "resize",
            "scene",
            "image",
            super::resize_record::scene(256, 192),
        ),
        ("resize", "fine", "image", super::resize_record::fine()),
        ("view", "detail", "image", scene.clone()),
        ("view", "detail", "also", edited),
        ("view", "whole", "image", scene),
        ("view", "across", "image", panorama.clone()),
        ("view", "up", "image", panorama.clone()),
        ("view", "strip", "image", panorama),
    ]
}

#[test]
fn the_inputs_written_are_the_images_the_records_tests_construct() {
    type Row = (&'static str, &'static str, &'static str, ArtifactId);
    let written: Vec<Row> = inputs()
        .into_iter()
        .map(|input| (input.tool, input.example, input.port, input.artifact.id()))
        .collect();
    let made: Vec<Row> = constructed()
        .into_iter()
        .map(|(tool, example, port, image)| (tool, example, port, image.id()))
        .collect();
    assert_eq!(written, made);
    let tools: Vec<&str> = written.iter().map(|row| row.0).collect();
    for tool in TOOLS {
        assert!(tools.contains(&tool), "{tool} has no inputs");
    }
}
