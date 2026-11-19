//! What `check` hands the judge beside the images (plan `docs/plans/m2c-milestone-2.md`,
//! task A2; ADR 0029): the tools `resize` and `align` as squint's judge knew them, what a call
//! of either is answered with under a projection, and the details measured first.
//!
//! **The details first.** The owner's goal: squint gives the judge every detail it can
//! measure, and the judge makes the final conclusion; it is not left to work the details
//! out. So under `details = first`, before the first turn, `check` calls `align` (and each
//! other tool the run names with `detail`) on each pair the use case relates, and puts what
//! each found in the opening message: the aligned picture, squint's sentence, and the
//! measurements whole, each labelled with its tool and the tool's mark. A measurement is
//! never presented as proof; the judge weighs it against what it sees. Under
//! `details = none` nothing is measured first: the judge is told of the tools and calls them,
//! as squint's arm C was.
//!
//! **Projections.** Squint's arm C offered `align` under the projection `visual_only`
//! (`crates/squint-core/src/tool/projection.rs`): the judge was handed the aligned picture
//! and a fixed sentence, and the measurements and the quality were withheld. A call of
//! `align` is answered here as squint's harness answered it
//! (`crates/squint-core/src/run.rs`, the presentation of an envelope): one JSON object of
//! the call's id, its status, its summary, its measurements, its regions and its images'
//! frames, with the pictures beside it. `full` hands everything.

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};
use whizbang_core::artifact::ArtifactId;
use whizbang_core::capability::ToolDefinition;
use whizbang_core::contract::{Call, Value};
use whizbang_core::identity::Digest;

use crate::{align, compare, resize, view};

/// What is measured before the first turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Details {
    /// Nothing: the judge is told of the tools and calls them (squint's arm C).
    None,
    /// The details of each pair the use case relates, in the opening message.
    First,
}

impl Details {
    pub const NAMES: [&'static str; 2] = ["none", "first"];

    pub fn named(name: &str) -> Option<Details> {
        match name {
            "none" => Some(Details::None),
            "first" => Some(Details::First),
            _ => None,
        }
    }
}

/// How a call of `align` by the judge is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    /// Everything the tool found.
    Full,
    /// The pictures and a fixed sentence; the measurements and the quality withheld.
    VisualOnly,
}

impl Projection {
    pub const NAMES: [&'static str; 2] = ["full", "visual_only"];

    pub fn named(name: &str) -> Option<Projection> {
        match name {
            "full" => Some(Projection::Full),
            "visual_only" => Some(Projection::VisualOnly),
            _ => None,
        }
    }
}

/// The summary a `visual_only` result carries when the tool computed something.
pub const VISUAL_ONLY_COMPUTED: &str = "Result computed; its measurements are withheld.";
/// The summary a `visual_only` result carries when the tool had nothing to give.
pub const VISUAL_ONLY_UNAVAILABLE: &str = "Result not available.";

/// The tools whose results can be measured first, by name: `align`, and the two that
/// compare a pair's content in one grid (plan m2c, task X1, item 9), each measured on the
/// source and the frame `align` returned, so a run that names either runs `align` first.
pub const DETAILED: [&str; 3] = [align::NAME, compare::CHANGE_MAP, compare::EDGE_SIMILARITY];

/// The tools measured on the frame `align` returns.
pub const AFTER_ALIGN: [&str; 2] = [compare::CHANGE_MAP, compare::EDGE_SIMILARITY];

/// A tool's mark (`SPEC.md` section 8, "Marks"), as its record states it
/// ([`crate::marks`]): experimental for a tool whose record states no other.
pub fn mark(tool: &str) -> &'static str {
    crate::marks::of(tool).map_or("experimental", crate::marks::Mark::word)
}

/// Squint's manifest of a tool, as its prompt's Tools section read it: the name, the cost
/// tier and seconds, the purpose, when to use it, when not, and what its output means.
struct Manifest {
    name: &'static str,
    tier: &'static str,
    seconds: &'static str,
    purpose: &'static str,
    when_to_use: &'static str,
    when_not_to_use: &'static str,
    output: &'static str,
}

/// Squint's manifests of the five tools (`crates/squint-tools/manifests/`).
const MANIFESTS: [Manifest; 5] = [
    Manifest {
        name: view::NAME,
        tier: "Cheap",
        seconds: "1",
        purpose: "Shows a region closer: a crop of a flat frame by box, or a square perspective view of a panorama by yaw, pitch and field of view; optionally the same region in a second frame.",
        when_to_use: "To inspect a detail, an edge or a seam at more resolution than the whole image gives, or to compare the same region in two frames side by side.",
        when_not_to_use: "To judge whole-image properties such as composition or projection; a crop shows less of the picture, not more.",
        output: "Each returned image is a new frame cut from the one you named. Point findings at the frame that shows them best; the harness maps them back.",
    },
    Manifest {
        name: align::NAME,
        tier: "Moderate",
        seconds: "3",
        purpose: "Registers one image onto another of the same scene and viewpoint: a homography for flat images, a rotation on the sphere for panoramas; returns the first image warped into the second's frame with how well the pair agrees.",
        when_to_use: "Before comparing a candidate with its source: to see them in one frame and to learn whether the candidate kept the source's geometry.",
        when_not_to_use: "On images of different scenes or viewpoints; it reports those as not registered rather than forcing a fit.",
        output: "A registered pair gives an aligned frame to compare against the second image, and measurements of agreement. Not registered is itself evidence that the geometry changed; low texture means the images could not be judged. Registered, even with high agreement, says only that the camera and frame match: it does not compare content, and a reshaped pool or wall inside the frame registers as well as an unchanged one.",
    },
    Manifest {
        name: resize::NAME,
        tier: "Cheap",
        seconds: "1",
        purpose: "Returns a frame scaled down to a chosen long edge, so the whole picture can be seen at a size the eye takes in at once.",
        when_to_use: "To step back from a large or detailed frame and judge its overall structure, or to compare two frames at one size.",
        when_not_to_use: "To see detail; use view for that. It never enlarges.",
        output: "The returned frame covers the same picture as the one you named at a smaller size; findings in it map back one to one.",
    },
    Manifest {
        name: compare::CHANGE_MAP,
        tier: "Cheap",
        seconds: "1",
        purpose: "Shows where two frames in one grid differ in structure: a grey map, bright where the locally normalised luminance disagrees, dark where it agrees.",
        when_to_use: "After align, over the aligned candidate and the source, to see at a glance where the picture changed before looking closer with view.",
        when_not_to_use: "Between frames that are not in one grid; everything would light up.",
        output: "The map is a pointer, not a verdict: bright areas are where to look, and a uniform tone or colour change leaves the map dark, and so does a local change of colour alone (a repainted wall or recoloured roof): the map is luminance structure only, so check colour with regions or view. The flat inside of a changed object can stay dark while its outline lights, so changed_fraction is a floor on the changed area, not its size. Inspect bright areas with view before deciding.",
    },
    Manifest {
        name: compare::EDGE_SIMILARITY,
        tier: "Cheap",
        seconds: "1",
        purpose: "Scores how much two frames share the same edges: the correlation of their edge-strength maps, 1 for the same structure, near 0 for unrelated or featureless frames.",
        when_to_use: "After align, between the aligned candidate and the source, to tell a restyle that kept the structure from one that moved or removed it.",
        when_not_to_use: "Between frames that are not in one grid, such as a raw candidate and a raw source that align has not registered; the score would measure the misalignment.",
        output: "A number, not a verdict: compare it with what you see. Shared edges survive colour and tone changes; a low score with a registered alignment points at changed structure.",
    },
];

fn manifest(name: &str) -> Option<&'static Manifest> {
    MANIFESTS.iter().find(|manifest| manifest.name == name)
}

/// Squint's Tools section of the system prompt (`crates/squint-core/src/render.rs`,
/// `tools_section`), for these tools in this order; empty for none.
pub fn tools_section(tools: &[&str]) -> String {
    let lines: Vec<String> = tools
        .iter()
        .filter_map(|name| manifest(name))
        .map(|m| {
            format!(
                "- {} ({}, about {}s): {}\n  When to use: {}\n  When not to use: {}\n  Output: {}",
                m.name, m.tier, m.seconds, m.purpose, m.when_to_use, m.when_not_to_use, m.output
            )
        })
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "\nTools. Use them to look closer before deciding; every call is logged. Refer to images \
         and frames by their ids.\n{}\n",
        lines.join("\n")
    )
}

/// `align` as squint's judge was told of it: its purpose and its model schema.
pub fn align_tool() -> Result<ToolDefinition, String> {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["image", "against"],
        "properties": {
            "image": {"type": "string", "description": "The image to register, usually the candidate: its image id (candidate) or its raw frame id (raw:candidate)."},
            "against": {"type": "string", "description": "The image to register onto, usually the source: its image id (source) or its raw frame id (raw:source); the aligned frame shares its grid."},
        },
    });
    let purpose = manifest(align::NAME).map_or("", |m| m.purpose);
    ToolDefinition::new(align::NAME, purpose, schema).map_err(|why| why.to_string())
}

/// `resize` as squint's judge was told of it.
pub fn resize_tool() -> Result<ToolDefinition, String> {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["frame", "long_edge"],
        "properties": {
            "frame": {"type": "string", "description": "The frame to scale down: a frame id (raw:candidate, or a crop a tool returned), or an image id (candidate) for its raw frame."},
            "long_edge": {"type": "integer", "minimum": 64, "maximum": 1024, "description": "The longer side of the result in pixels; at most the frame's own."},
        },
    });
    let purpose = manifest(resize::NAME).map_or("", |m| m.purpose);
    ToolDefinition::new(resize::NAME, purpose, schema).map_err(|why| why.to_string())
}

/// The names a judge may give an image: an image's identity, as `view` takes it; the name
/// of its role (`candidate`) and its raw frame (`raw:candidate`), as squint's tools took
/// them; and the frame a tool call of this run returned (`aligned:candidate:t1`,
/// `crop:call:0:0`).
#[derive(Debug, Default, Clone)]
pub struct Names {
    named: BTreeMap<String, ArtifactId>,
}

impl Names {
    /// Gives this image the name of a role, and of its raw frame.
    pub fn role(&mut self, role: &str, id: ArtifactId) {
        self.named.insert(role.to_owned(), id);
        self.named.insert(format!("raw:{role}"), id);
    }

    /// Gives this image the name of a frame a tool returned.
    pub fn frame(&mut self, frame: &str, id: ArtifactId) {
        self.named.insert(frame.to_owned(), id);
    }

    /// The image a name gives, and the name squint's tools would call it by: the role for
    /// `raw:<role>`, the name otherwise.
    pub fn resolve(&self, name: &str) -> Option<(ArtifactId, String)> {
        if let Some(id) = self.named.get(name) {
            let plain = name.strip_prefix("raw:").unwrap_or(name);
            return Some((*id, plain.to_owned()));
        }
        let digest = name.parse::<Digest>().ok()?;
        Some((ArtifactId(digest), name.to_owned()))
    }
}

/// The arguments of a judge's call, read as an object whose keys are these and no other.
fn object<'a>(
    arguments: &'a Json,
    known: &[&str],
) -> Result<&'a serde_json::Map<String, Json>, String> {
    let Some(arguments) = arguments.as_object() else {
        return Err("the arguments are not an object".to_owned());
    };
    if arguments.keys().any(|key| !known.contains(&key.as_str())) {
        return Err(format!(
            "the arguments are {}, and no other",
            known.join(" and ")
        ));
    }
    Ok(arguments)
}

/// An image a judge's argument names.
pub fn image_of(
    arguments: &serde_json::Map<String, Json>,
    key: &str,
    names: &Names,
) -> Result<(ArtifactId, String), String> {
    match arguments.get(key) {
        Some(Json::String(name)) => names
            .resolve(name)
            .ok_or_else(|| format!("{key}: unknown image id {name:?}")),
        Some(_) => Err(format!("{key} is not a text")),
        None => Err(format!("{key} is required")),
    }
}

/// A call of `align` a judge asked for: the call, and what squint would have called the
/// image and the image it is registered onto.
pub struct AlignAsked {
    pub call: Call,
    pub image: String,
    pub against: String,
    pub image_id: ArtifactId,
    pub against_id: ArtifactId,
}

/// The call of `align` that a judge's arguments ask for, or why they ask for none.
pub fn align_call(arguments: &Json, names: &Names) -> Result<AlignAsked, String> {
    let arguments = object(arguments, &["image", "against"])?;
    let (image, image_name) = image_of(arguments, "image", names)?;
    let (against, against_name) = image_of(arguments, "against", names)?;
    Ok(AlignAsked {
        call: Call::new(align::NAME)
            .input("image", image)
            .input("against", against),
        image: image_name,
        against: against_name,
        image_id: image,
        against_id: against,
    })
}

/// A call of `resize` a judge asked for, and the name of the frame it named.
pub fn resize_call(arguments: &Json, names: &Names) -> Result<(Call, String), String> {
    let arguments = object(arguments, &["frame", "long_edge"])?;
    let (image, name) = image_of(arguments, "frame", names)?;
    let Some(long_edge) = arguments.get("long_edge").and_then(Json::as_i64) else {
        return Err("long_edge must be a whole number of pixels".to_owned());
    };
    let call = Call::new(resize::NAME)
        .input("image", image)
        .argument("long_edge", Value::Integer(long_edge));
    Ok((call, name))
}

/// One result as squint's harness presented it to the judge: the envelope without its
/// pixels, keys in order, written as one line.
pub fn presentation(
    call: &str,
    status: &str,
    summary: &str,
    measurements: Json,
    regions: Json,
    images: Json,
) -> String {
    json!({
        "call_id": call,
        "status": status,
        "summary": summary,
        "measurements": measurements,
        "regions": regions,
        "images": images,
    })
    .to_string()
}

/// A call of `align`, answered: the text of the result, under the projection.
pub fn align_answer(
    call: &str,
    report: &Json,
    asked: &AlignAsked,
    projection: Projection,
) -> String {
    let registered = report["status"] == align::Status::Registered.word();
    let images = match &report["params"] {
        Json::Array(params) if registered => json!([{
            "frame": align::aligned_frame(&asked.image),
            "parent": format!("raw:{}", asked.against),
            "mapping": {"homography": params},
        }]),
        _ => json!([]),
    };
    if projection == Projection::VisualOnly {
        let (status, summary) = if registered {
            ("ok", VISUAL_ONLY_COMPUTED)
        } else {
            ("not_applicable", VISUAL_ONLY_UNAVAILABLE)
        };
        return presentation(call, status, summary, json!([]), json!([]), images);
    }
    let measurements: Vec<Json> = report["measurements"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|m| json!({"name": m["name"], "value": m["value"], "unit": m["unit"], "frame": null}))
        .collect();
    let regions = if report["status"] == align::Status::NotRegistered.word() {
        let ncc = report["measurements"]
            .as_array()
            .and_then(|all| all.iter().find(|m| m["name"] == "ncc"))
            .and_then(|m| m["value"].as_f64())
            .unwrap_or(0.0);
        json!([{
            "label": format!("not registered onto {}", asked.against),
            "score": (1.0 - ncc.max(0.0)).clamp(0.0, 1.0),
            "location": {"frame": format!("raw:{}", asked.image), "kind": "whole_image",
                         "bbox": null, "polygon": null, "yaw": null, "pitch": null, "fov": null},
        }])
    } else {
        json!([])
    };
    let status = if registered { "ok" } else { "not_applicable" };
    let said = align::summary(&summarised(report), &asked.image, &asked.against);
    presentation(call, status, &said, json!(measurements), regions, images)
}

/// The parts of a report [`align::summary`] reads.
fn summarised(report: &Json) -> align::Aligned {
    let status = match report["status"].as_str() {
        Some("registered") => align::Status::Registered,
        Some("low_texture") => align::Status::LowTexture,
        Some("not_converged") => align::Status::NotConverged,
        _ => align::Status::NotRegistered,
    };
    align::Aligned {
        status,
        quality: align::Quality {
            overlap: 0.0,
            ncc: 0.0,
            inlier_fraction: 0.0,
            texture: 0.0,
        },
        text: report["text"].as_str().unwrap_or_default().to_owned(),
        measurements: Vec::new(),
        transform: None,
        frame: None,
    }
}

/// A call of `resize`, answered as squint answered it: the frame scaled, named as the
/// crop of the call.
pub fn resize_answer(call: &str, frame: &str, width: u32, height: u32) -> String {
    let crop = format!("crop:{call}:0");
    let summary = format!("{frame} scaled to {width}×{height} ({crop}).");
    let images = json!([{
        "frame": crop,
        "parent": frame,
        "mapping": {"bbox": [0.0, 0.0, 1.0, 1.0]},
    }]);
    presentation(call, "ok", &summary, json!([]), json!([]), images)
}

/// One measurement as the opening states it: its name, its value to four places, its unit.
fn stated(measurement: &Json) -> String {
    let value = measurement["value"].as_f64().unwrap_or(f64::NAN);
    format!(
        "{} {value:.4} {}",
        measurement["name"].as_str().unwrap_or_default(),
        measurement["unit"].as_str().unwrap_or_default()
    )
}

/// The words that open the details.
pub const DETAILS_INTRO: &str = "Details measured before your first look. For each pair of \
    images this check relates, a tool was run on it and its result is below, labelled with \
    the tool and its mark. Each is a measurement, never proof: weigh it against what you see, \
    and look closer with the tools where they disagree.";

/// What the opening says of one call of `align` that `check` made first: the tool, its mark,
/// the pair, squint's sentence and every measurement.
pub fn align_detail(report: &Json, image: &str, against: &str) -> String {
    let said = align::summary(&summarised(report), image, against);
    let measured: Vec<String> = report["measurements"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(stated)
        .collect();
    format!(
        "{} ({}), {image} onto {against}: {} {said}\nMeasurements: {}.",
        align::NAME,
        mark(align::NAME),
        report["status"].as_str().unwrap_or_default(),
        measured.join("; ")
    )
}

/// `change_map` or `edge_similarity` as squint's judge was told of it: its purpose and its
/// model schema (squint's manifests, `[tool.model_schema]`).
pub fn compare_tool(name: &str) -> Result<ToolDefinition, String> {
    let frame = if name == compare::CHANGE_MAP {
        "The frame whose grid the map is drawn in, usually the source: a frame id (raw:source), or an image id (source) for its raw frame."
    } else {
        "One frame, usually the source: a frame id (raw:source), or an image id (source) for its raw frame."
    };
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["frame", "against"],
        "properties": {
            "frame": {"type": "string", "description": frame},
            "against": {"type": "string", "description": "The frame to compare it with, usually the aligned candidate (aligned:candidate:t1): a frame id, or an image id (candidate) for its raw frame (raw:candidate)."},
        },
    });
    let Some(manifest) = manifest(name).filter(|_| compare_named(name)) else {
        return Err(format!("{name} is no tool that compares a pair"));
    };
    ToolDefinition::new(manifest.name, manifest.purpose, schema).map_err(|why| why.to_string())
}

fn compare_named(name: &str) -> bool {
    AFTER_ALIGN.contains(&name)
}

/// A call of `change_map` or `edge_similarity` a judge asked for: the call, and what squint
/// would have called its two frames.
pub struct CompareAsked {
    pub call: Call,
    pub tool: &'static str,
    pub frame: String,
    pub against: String,
    pub frame_id: ArtifactId,
    pub against_id: ArtifactId,
}

/// The call of `change_map` or `edge_similarity` that the arguments ask for, or why they ask
/// for none.
pub fn compare_call(tool: &str, arguments: &Json, names: &Names) -> Result<CompareAsked, String> {
    let Some(tool) = AFTER_ALIGN.into_iter().find(|named| *named == tool) else {
        return Err(format!("{tool} is no tool that compares a pair"));
    };
    let arguments = object(arguments, &["frame", "against"])?;
    let (frame_id, frame) = image_of(arguments, "frame", names)?;
    let (against_id, against) = image_of(arguments, "against", names)?;
    Ok(CompareAsked {
        call: compare_of(tool, frame_id, against_id),
        tool,
        frame,
        against,
        frame_id,
        against_id,
    })
}

/// The call of a comparison of `against` in `frame`'s grid.
pub fn compare_of(tool: &str, frame: ArtifactId, against: ArtifactId) -> Call {
    Call::new(tool)
        .input("frame", frame)
        .input("against", against)
}

/// One measurement of a comparison: its name, its value and its unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Measure {
    pub name: String,
    pub value: f64,
    pub unit: String,
}

/// The measurements a comparison's run handed back (`{"measurements": [...]}`,
/// [`compare::report`]), as they are said; a value that is not a number is not one.
pub fn measures_of(measured: &Json) -> Vec<Measure> {
    let listed = measured["measurements"].as_array().map(Vec::as_slice);
    (listed.unwrap_or_default().iter())
        .map(|m| Measure {
            name: m["name"].as_str().unwrap_or_default().to_owned(),
            value: m["value"].as_f64().unwrap_or(f64::NAN),
            unit: m["unit"].as_str().unwrap_or_default().to_owned(),
        })
        .collect()
}

/// Squint's sentence of a comparison: the frames named as the judge knows them, the map, if
/// any, as `map`.
pub fn compare_said(
    tool: &str,
    measurements: &[Measure],
    frame: &str,
    against: &str,
    map: &str,
) -> String {
    let value = |name: &str| {
        measurements
            .iter()
            .find(|m| m.name == name)
            .map_or(f64::NAN, |m| m.value)
    };
    if tool == compare::CHANGE_MAP {
        let changes = compare::Changes {
            map: image::RgbImage::new(0, 0),
            fraction: value("changed_fraction"),
        };
        compare::changes_summary(&changes, frame, against, map)
    } else {
        let edges = compare::Edges {
            similarity: value("edge_similarity"),
            energy: value("edge_energy"),
        };
        compare::edges_summary(&edges, frame, against)
    }
}

fn measured(measurements: &[Measure], frame: &str) -> Json {
    measurements
        .iter()
        .map(|m| json!({"name": m.name, "value": m.value, "unit": m.unit, "frame": frame}))
        .collect()
}

/// A call of a comparison, answered as squint answered it: the map, if any, named as the
/// crop of the call in `frame`'s grid.
pub fn compare_answer(call: &str, asked: &CompareAsked, measurements: &[Measure]) -> String {
    let map = format!("crop:{call}:0");
    let said = compare_said(asked.tool, measurements, &asked.frame, &asked.against, &map);
    let images = if asked.tool == compare::CHANGE_MAP {
        json!([{"frame": map, "parent": asked.frame, "mapping": {"bbox": [0.0, 0.0, 1.0, 1.0]}}])
    } else {
        json!([])
    };
    let measurements = measured(measurements, &asked.frame);
    presentation(call, "ok", &said, measurements, json!([]), images)
}

/// What the opening says of one comparison `check` made first: the tool, its mark, the
/// pair, squint's sentence, every measurement, and what the measurement does not say
/// (squint's guidance on reading it, shortened).
pub fn compare_detail(
    tool: &str,
    measurements: &[Measure],
    frame: &str,
    against: &str,
    map: &str,
) -> String {
    let said = compare_said(tool, measurements, frame, against, map);
    let stated: Vec<String> = measurements
        .iter()
        .map(|m| format!("{} {:.4} {}", m.name, m.value, m.unit))
        .collect();
    let reading = if tool == compare::CHANGE_MAP {
        "The map is a pointer, not a verdict: it is luminance structure only, blind to a \
         change of colour or tone, and the flat inside of a changed object can stay dark, so \
         changed_fraction is a floor on the changed area. Look at bright areas with view."
    } else {
        "A number, not a verdict: shared edges survive colour and tone; a low score after a \
         registered alignment points at changed structure."
    };
    format!(
        "{tool} ({}), {against} against {frame}: {said}\nMeasurements: {}.\n{reading}",
        mark(tool),
        stated.join("; ")
    )
}

/// The detail that cuts the candidate into tiles (plan m3, task U1, item 3): for a use case
/// with one image, which has no pair for `align` to measure. Experimental: its record is
/// `docs/reference/tools/tiles.md`.
pub const TILES: &str = "tiles";

/// The four tiles, each named by its place, as a box of left, top, right and bottom in
/// fractions of the picture: each a little over half of each side, so neighbours overlap by
/// a tenth of the picture and nothing on a seam is cut in two.
pub const TILE_BOXES: [(&str, [f64; 4]); 4] = [
    ("top-left", [0.0, 0.0, 0.55, 0.55]),
    ("top-right", [0.45, 0.0, 1.0, 0.55]),
    ("bottom-left", [0.0, 0.45, 0.55, 1.0]),
    ("bottom-right", [0.45, 0.45, 1.0, 1.0]),
];

/// The words that open the tiles.
pub const TILES_INTRO: &str = "Details measured before your first look: the candidate cut \
    into four overlapping tiles by the tool view, at full resolution up to 1024 pixels a \
    side, each labelled with its place and its box in fractions of the candidate. A tile \
    shows what the whole picture is too small to show: hands, faces, text, edges and \
    contacts. It is part of the candidate, not another picture; a location you submit is on \
    raw:candidate, so map a tile's place back through its box.";

/// What the opening says of one tile: its name, its place and box, its size, and the name
/// the tool `view` takes for it.
pub fn tile_said(image: &str, place: &str, bbox: [f64; 4], size: (u32, u32), id: &str) -> String {
    let [l, t, r, b] = bbox;
    format!(
        "Tile `tile:{image}:{place}` ({} ({}), the {place} of {image}: left {l:.2}, top {t:.2}, \
         right {r:.2}, bottom {b:.2} of it, {}x{}); for the tool view it is named {id}.",
        TILES,
        mark(TILES),
        size.0,
        size.1
    )
}

/// Whether a detail can be measured first under a use case: `tiles` only where the job has
/// no pair (`reality-checks`, and `brief-compliance` without a reference); the details of a
/// pair only where the use case relates one. A job is held to it again when it is known.
pub fn takes(detail: &str, use_case: &str) -> bool {
    let paired = matches!(
        use_case,
        "source-fidelity" | "reference-consistency" | "product-fidelity"
    );
    match detail {
        TILES => matches!(use_case, "reality-checks" | "brief-compliance"),
        _ if DETAILED.contains(&detail) => paired || use_case == "brief-compliance",
        _ => false,
    }
}

/// The words of the refusal of `tiles` for a job that relates a pair.
pub fn tiles_refused(use_case: &str) -> String {
    format!(
        "the detail tiles is for a use case with one image, and this {use_case} job relates a \
         pair: measure its pair with align instead"
    )
}
