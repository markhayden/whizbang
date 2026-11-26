//! The inputs of the worked examples of squint's tool records, `docs/reference/tools/<tool>.md`
//! (`SPEC.md` section 8), for `whizbang store record-inputs` (plan E, task O1). Every input of
//! these records is a picture, and the record's test holds the picture beside the record to the
//! file the test's constructor made, byte for byte (`tests/it/record.rs`, `align_record.rs`,
//! `compare_record.rs`, `resize_record.rs`, `tiles_record.rs`): so the file is read from
//! there (the binary has its own copy of the records, `whizbang_serve::play::records`), and
//! the image made of it is the one the test made, identity for identity
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

/// A picture file beside a record, as the image its test made of it.
fn image(file: &[u8], projection: Projection, topology: Topology) -> Result<Image, String> {
    let origin = Origin::new(Step::new("fixture", "1")).map_err(|error| error.to_string())?;
    let limits = DecodeLimits::default();
    Image::new(
        file.to_vec(),
        projection,
        topology,
        ImageRole::Picture,
        origin,
        &limits,
    )
    .map_err(|error| error.to_string())
}

fn flat(file: &[u8]) -> Result<Image, String> {
    image(file, Projection::Flat, Topology::Bounded)
}

/// Every input of every example of the records in [`TOOLS`], in that order. `find` gives
/// the file at a path under `docs/reference/tools/`, as `<record>/<file>`: the binary's own
/// copy of the records, or a test's read of them.
pub fn inputs(find: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Vec<Input>, String> {
    let read = |path: &str| {
        let file = find(path).ok_or_else(|| format!("no record's file is at {path}"))?;
        Ok::<_, String>(file)
    };
    let source = flat(&read("align/source.png")?)?;
    let (before, after) = (
        flat(&read("change_map/before.png")?)?,
        flat(&read("change_map/after.png")?)?,
    );
    let edge_scene = flat(&read("edge_similarity/scene.png")?)?;
    let scene = flat(&read("view/scene.png")?)?;
    let panorama = image(
        &read("view/panorama.png")?,
        Projection::Equirectangular,
        Topology::Wrap,
    )?;
    let given: Vec<(&'static str, &'static str, &'static str, Image)> = vec![
        (
            "align",
            "turned",
            "image",
            flat(&read("align/candidate.png")?)?,
        ),
        ("align", "turned", "against", source.clone()),
        (
            "align",
            "moved",
            "image",
            flat(&read("align/reshaped.png")?)?,
        ),
        ("align", "moved", "against", source),
        ("change_map", "added", "frame", before.clone()),
        ("change_map", "added", "against", after),
        ("change_map", "greyed", "frame", before),
        (
            "change_map",
            "greyed",
            "against",
            flat(&read("change_map/desaturated.png")?)?,
        ),
        ("edge_similarity", "toned", "frame", edge_scene.clone()),
        (
            "edge_similarity",
            "toned",
            "against",
            flat(&read("edge_similarity/toned.png")?)?,
        ),
        ("edge_similarity", "other", "frame", edge_scene),
        (
            "edge_similarity",
            "other",
            "against",
            flat(&read("edge_similarity/other.png")?)?,
        ),
        (
            "edge_similarity",
            "moved",
            "frame",
            flat(&read("edge_similarity/before.png")?)?,
        ),
        (
            "edge_similarity",
            "moved",
            "against",
            flat(&read("edge_similarity/after.png")?)?,
        ),
        ("tiles", "scene", "images", flat(&read("tiles/scene.png")?)?),
        ("tiles", "fine", "images", flat(&read("tiles/fine.png")?)?),
        (
            "resize",
            "scene",
            "image",
            flat(&read("resize/scene.png")?)?,
        ),
        ("resize", "fine", "image", flat(&read("resize/fine.png")?)?),
        ("view", "detail", "image", scene.clone()),
        (
            "view",
            "detail",
            "also",
            flat(&read("view/scene-edited.png")?)?,
        ),
        ("view", "whole", "image", scene),
        ("view", "across", "image", panorama.clone()),
        ("view", "up", "image", panorama.clone()),
        ("view", "strip", "image", panorama),
    ];
    let inputs = given
        .into_iter()
        .map(|(tool, example, port, image)| Input {
            tool,
            example,
            port,
            artifact: image.into(),
        })
        .collect();
    Ok(inputs)
}
