//! The inputs of the worked examples of squint's tool records, `docs/reference/tools/<tool>.md`
//! (`SPEC.md` section 8), for `whizbang store record-inputs` (plan E, task O1). Every input of
//! these records is a picture, and the record's test holds the picture beside the record to the
//! file the test's constructor made, byte for byte (`tests/it/record.rs`, `align_record.rs`,
//! `compare_record.rs`, `resize_record.rs`, `tiles_record.rs`): so the file is taken from
//! there, and the image made of it is the one the test made, identity for identity
//! (`tests/it/record_inputs.rs` holds that). `details` has no record of its own: its record is
//! that of its `tiles`.

use whizbang_core::artifact::{Artifact, Image, ImageRole, Origin, Step};
use whizbang_core::identity::{DecodeLimits, Projection, Topology};

/// One input of a record's example: the tool, the example, the port of the example's call
/// that takes it, and the artifact.
pub struct Input {
    pub tool: &'static str,
    pub example: &'static str,
    pub port: &'static str,
    pub artifact: Artifact,
}

/// The records whose examples' inputs this module gives, in `describe`'s order; `tiles` is
/// the record of `details`.
pub const TOOLS: [&str; 6] = [
    "align",
    "change_map",
    "edge_similarity",
    "tiles",
    "resize",
    "view",
];

macro_rules! beside {
    ($record:literal, $file:literal) => {
        include_bytes!(concat!(
            "../../../docs/reference/tools/",
            $record,
            "/",
            $file
        ))
    };
}

/// A picture file beside a record, as the image its test made of it. Each is one the core
/// reads, which its record's test holds, so this is the one place the module may panic.
#[allow(clippy::expect_used)]
fn image(file: &[u8], projection: Projection, topology: Topology) -> Image {
    let origin = Origin::new(Step::new("fixture", "1")).expect("a step is an origin");
    Image::new(
        file.to_vec(),
        projection,
        topology,
        ImageRole::Picture,
        origin,
        &DecodeLimits::default(),
    )
    .expect("a picture beside a record is one the core reads")
}

fn flat(file: &[u8]) -> Image {
    image(file, Projection::Flat, Topology::Bounded)
}

/// Every input of every example of the records in [`TOOLS`], in that order.
pub fn inputs() -> Vec<Input> {
    let source = flat(beside!("align", "source.png"));
    let (before, after) = (
        flat(beside!("change_map", "before.png")),
        flat(beside!("change_map", "after.png")),
    );
    let edge_scene = flat(beside!("edge_similarity", "scene.png"));
    let scene = flat(beside!("view", "scene.png"));
    let panorama = image(
        beside!("view", "panorama.png"),
        Projection::Equirectangular,
        Topology::Wrap,
    );
    let given: Vec<(&'static str, &'static str, &'static str, Image)> = vec![
        (
            "align",
            "turned",
            "image",
            flat(beside!("align", "candidate.png")),
        ),
        ("align", "turned", "against", source.clone()),
        (
            "align",
            "moved",
            "image",
            flat(beside!("align", "reshaped.png")),
        ),
        ("align", "moved", "against", source),
        ("change_map", "added", "frame", before.clone()),
        ("change_map", "added", "against", after),
        ("change_map", "greyed", "frame", before),
        (
            "change_map",
            "greyed",
            "against",
            flat(beside!("change_map", "desaturated.png")),
        ),
        ("edge_similarity", "toned", "frame", edge_scene.clone()),
        (
            "edge_similarity",
            "toned",
            "against",
            flat(beside!("edge_similarity", "toned.png")),
        ),
        ("edge_similarity", "other", "frame", edge_scene),
        (
            "edge_similarity",
            "other",
            "against",
            flat(beside!("edge_similarity", "other.png")),
        ),
        (
            "edge_similarity",
            "moved",
            "frame",
            flat(beside!("edge_similarity", "before.png")),
        ),
        (
            "edge_similarity",
            "moved",
            "against",
            flat(beside!("edge_similarity", "after.png")),
        ),
        (
            "tiles",
            "scene",
            "images",
            flat(beside!("tiles", "scene.png")),
        ),
        (
            "tiles",
            "fine",
            "images",
            flat(beside!("tiles", "fine.png")),
        ),
        (
            "resize",
            "scene",
            "image",
            flat(beside!("resize", "scene.png")),
        ),
        (
            "resize",
            "fine",
            "image",
            flat(beside!("resize", "fine.png")),
        ),
        ("view", "detail", "image", scene.clone()),
        (
            "view",
            "detail",
            "also",
            flat(beside!("view", "scene-edited.png")),
        ),
        ("view", "whole", "image", scene),
        ("view", "across", "image", panorama.clone()),
        ("view", "up", "image", panorama.clone()),
        ("view", "strip", "image", panorama),
    ];
    given
        .into_iter()
        .map(|(tool, example, port, image)| Input {
            tool,
            example,
            port,
            artifact: image.into(),
        })
        .collect()
}
