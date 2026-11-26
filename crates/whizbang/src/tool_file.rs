//! Tool files (plan 2C, task X1, items 1, 2, 4 and 5; ADR 0032): a perception tool written
//! as one Python file, listed under `[tools] paths`, tried, scored, edited and run again
//! with nothing rebuilt or installed (`SPEC.md` section 4, "Experimental tools").
//!
//! **Its declaration** is a block of TOML at the top of the file, each line a comment, in
//! the form of inline script metadata, between `# /// whizbang-tool` and `# ///`
//! ([`Declaration`]): its format, `tool-file/1`; its name, version and purpose, which make
//! its action's; the images it reads (`bindings`, each an input port); the kinds of
//! evidence it returns (`measurements`, and `regions` beside them, plan mE, task T1,
//! item 0); each measurement with its unit and its bounds; the most readings it returns;
//! the most regions and the longest label of one, when it returns regions; its primary
//! measurement, which way of it is changed and how several readings reduce to one (the
//! worst); its time; and its arguments. A declaration that lacks a bound, or breaks a
//! rule, is refused when the config is read, and nothing of the file runs
//! ([`Files::read`]).
//!
//! **What runs is the file as it is now, by its digest.** Each run reads the file's bytes
//! once, through a name that is no link, from a file its group and others cannot write;
//! works out their SHA-256; notes it in the run's record ([`DIGEST_SETTING`]); and sends
//! those bytes and that digest to the tool worker, which hashes them again and compiles
//! only bytes that match. Nothing is fetched and nothing is copied: an edit is the next
//! run's. An edit that changes the tool's name, images or arguments, which make its
//! action, is refused until whizbang is started again.
//!
//! **Its output is held to its declaration before anything uses it** ([`checked`]): an
//! object of `readings`, each a subject and a value for declared measurements within
//! their bounds, at most the declared count, and `not_applicable`, subjects it could not
//! read; and, from a tool that declares them, `regions`, each a box or a polygon in pixels
//! of an image the run handed it, a label and a score, at most the declared count. A run
//! that completed states them in its diagnostic [`REPORT`], with the primary measurement
//! reduced by the declared rule.
//!
//! **Its capabilities are nested calls** (ADR 0008): a tool's `segment` is a call of the
//! action `segment` through the executor of the run's context, under its allowance and
//! policy, cancelled with it. A mask in the store holds no score, so the answer gives
//! none. A tool's `labels` (plan mE, task T2; ADR 0053) is a call of the action `labels`
//! the same way, and its answer is one plane of each pixel's class, made from the masks
//! it committed, with each class's confidence, which the run handed back.
//!
//! **A model by name.** An ask may name a model of the catalogue (`eomt-ade20k`): it is
//! routed to the segment backend of the config whose `model` is that name, by the call's
//! `backend` ([`Files::backend_for`]): the default, when it is one of them, and otherwise
//! the first of them by name. A model no backend of the config is refuses the ask, and
//! so does one whose backend is not ready (a model not installed) or does not declare the
//! feature the call needs (`labels`), each as the nested call is refused. An ask that names
//! none is the default's, as before.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Map, Value as Json, json};
use whizbang_backends::pack::tool::{
    ASKS_LIMIT, Answered, Asked, CODE_LIMIT, Labelled, Mask as ToolMask, Sought, ToolImage,
    ToolLimits, ToolRun,
};
use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Call, Capability, Context, Failure, InputPort, Kind,
    Outcome, PreparedCall, Produced, Requirement, Secrets, Value, ValueType,
};
use whizbang_core::identity::{DecodeLimits, Digest};
use whizbang_eval::tool::Changed;
use whizbang_squint::files::{FileArgument, FileRan, FileTool, FileTools};

use crate::pack::OneWorker;

/// The format a declaration names.
pub const FORMAT: &str = "tool-file/1";
/// The line that opens a declaration, and the one that closes it.
pub const OPENS: &str = "# /// whizbang-tool";
pub const CLOSES: &str = "# ///";
/// The diagnostic a run of a tool file states its readings in.
pub const REPORT: &str = "tool_file.json";
/// The setting a run of a tool file notes the digest of the file it ran under.
pub const DIGEST_SETTING: &str = "tool_file_digest";
/// The most readings a declaration may allow.
pub const READINGS_MOST: usize = 64;
/// The most measurements, arguments and images a declaration names.
pub const MEASUREMENTS_MOST: usize = 16;
pub const ARGUMENTS_MOST: usize = 8;
pub const BINDINGS_MOST: usize = 4;
/// The most characters of a subject, and of a unit.
pub const SUBJECT_LIMIT: usize = 200;
pub const UNIT_LIMIT: usize = 32;
/// The longest a declaration may give a run, in seconds.
pub const TIME_MOST_S: u64 = 600;
/// The most regions a declaration may allow, and the longest label of one, in bytes.
pub const REGIONS_MOST: usize = 64;
pub const REGION_LABEL_MOST: usize = 200;
/// The longest name of the kind a tool's regions are.
pub const REGION_KIND_MOST: usize = 32;
/// The most vertices of a region's polygon (as a location's, ADR 0002).
pub const POLYGON_MOST: usize = 64;
/// The kinds of evidence a tool file may return: `measurements` always, `regions` beside.
pub const MEASUREMENTS: &str = "measurements";
pub const REGIONS: &str = "regions";

/// A tool file's declaration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub format: String,
    pub name: String,
    pub version: String,
    pub purpose: String,
    pub evidence: Evidence,
    pub bindings: Vec<String>,
    pub primary: String,
    pub changed: String,
    pub reduce: String,
    pub readings_most: usize,
    pub time_limit_s: u64,
    /// The most segments one run asks for: what its admission reserves for. A paid ask
    /// past what is reserved is refused by the run's allowance. None when not given.
    #[serde(default)]
    pub segments_most: u64,
    /// The most regions one output holds, and the longest label of one, in bytes: given
    /// when, and only when, `evidence` names `regions`.
    #[serde(default)]
    pub max_regions: Option<usize>,
    #[serde(default)]
    pub max_region_label_bytes: Option<usize>,
    /// The kind of evidence its regions are, as the rules' `evidence_kinds` name kinds
    /// (`change_region`): which obligations a region of it bears on (plan mE, task T1,
    /// item 4). Given when, and only when, `evidence` names `regions`.
    #[serde(default)]
    pub region_kind: Option<String>,
    pub measurements: Vec<Measure>,
    #[serde(default)]
    pub arguments: Vec<Arg>,
}

/// The kinds of evidence a declaration names: one, as `"measurements"`, or a list, as
/// `["measurements", "regions"]`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Evidence {
    One(String),
    Many(Vec<String>),
}

impl Evidence {
    /// Each kind named, in the order given.
    pub fn kinds(&self) -> Vec<&str> {
        match self {
            Evidence::One(kind) => vec![kind.as_str()],
            Evidence::Many(kinds) => kinds.iter().map(String::as_str).collect(),
        }
    }
}

/// One measurement a tool reports, with its unit and its bounds.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measure {
    pub name: String,
    pub unit: String,
    pub min: f64,
    pub max: f64,
}

/// One argument a tool takes: a `text` or a `number`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Arg {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub purpose: String,
    #[serde(default)]
    pub required: bool,
}

impl Declaration {
    /// The declaration at the top of a tool file's bytes, held to its rules.
    pub fn parse(code: &[u8]) -> Result<Declaration, String> {
        let text = std::str::from_utf8(code).map_err(|_| "the file is not UTF-8".to_owned())?;
        let mut lines = text.lines();
        if !lines.by_ref().any(|line| line.trim_end() == OPENS) {
            return Err(format!("the file has no declaration: no line {OPENS}"));
        }
        let mut block = String::new();
        let mut closed = false;
        for line in lines {
            let line = line.trim_end();
            if line == CLOSES {
                closed = true;
                break;
            }
            let Some(rest) = line.strip_prefix('#') else {
                return Err("a line of the declaration is not a comment".to_owned());
            };
            block.push_str(rest.strip_prefix(' ').unwrap_or(rest));
            block.push('\n');
        }
        if !closed {
            return Err(format!("the declaration is not closed by a line {CLOSES}"));
        }
        let declaration: Declaration =
            toml::from_str(&block).map_err(|e| format!("the declaration: {}", e.message()))?;
        declaration.check()?;
        Ok(declaration)
    }

    fn check(&self) -> Result<(), String> {
        let rule = |ok: bool, why: &str| if ok { Ok(()) } else { Err(why.to_owned()) };
        rule(
            self.format == FORMAT,
            "the declaration's format is not tool-file/1",
        )?;
        let kinds = self.evidence.kinds();
        rule(
            kinds.first() == Some(&MEASUREMENTS)
                && kinds[1..].iter().all(|kind| *kind == REGIONS)
                && kinds.len() <= 2,
            "evidence is measurements, or measurements and regions",
        )?;
        match (
            self.regions(),
            self.max_regions,
            self.max_region_label_bytes,
        ) {
            (true, Some(most), Some(label)) => {
                let kind = self.region_kind.as_deref().unwrap_or_default();
                rule(
                    (1..=REGION_KIND_MOST).contains(&kind.len())
                        && kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                    "a tool that returns regions declares region_kind, 1 to 32 lower-case letters or _",
                )?;
                rule(
                    (1..=REGIONS_MOST).contains(&most),
                    "max_regions is from 1 to 64",
                )?;
                rule(
                    (1..=REGION_LABEL_MOST).contains(&label),
                    "max_region_label_bytes is from 1 to 200",
                )?;
            }
            (true, _, _) => {
                return Err(
                    "a tool that returns regions declares max_regions and max_region_label_bytes"
                        .to_owned(),
                );
            }
            (false, None, None) if self.region_kind.is_none() => {}
            (false, _, _) => {
                return Err(
                    "max_regions, max_region_label_bytes and region_kind are for a tool whose evidence names regions"
                        .to_owned(),
                );
            }
        }
        rule(
            (1..=BINDINGS_MOST).contains(&self.bindings.len()),
            "bindings names 1 to 4 images",
        )?;
        rule(
            (1..=MEASUREMENTS_MOST).contains(&self.measurements.len()),
            "measurements names 1 to 16",
        )?;
        rule(
            self.arguments.len() <= ARGUMENTS_MOST,
            "arguments names at most 8",
        )?;
        rule(
            (1..=READINGS_MOST).contains(&self.readings_most),
            "readings_most is from 1 to 64",
        )?;
        rule(
            (1..=TIME_MOST_S).contains(&self.time_limit_s),
            "time_limit_s is from 1 to 600",
        )?;
        rule(
            self.segments_most <= ASKS_LIMIT,
            "segments_most is at most 64",
        )?;
        rule(
            matches!(self.changed.as_str(), "larger" | "smaller"),
            "changed is larger or smaller",
        )?;
        rule(
            self.reduce == "worst",
            "reduce is worst, the one rule at tool-file/1",
        )?;
        let mut names: Vec<&str> = Vec::new();
        for measure in &self.measurements {
            rule(
                measure.min.is_finite() && measure.max.is_finite() && measure.min < measure.max,
                "a measurement's bounds are finite, and min is under max",
            )?;
            rule(
                !measure.unit.is_empty()
                    && measure.unit.len() <= UNIT_LIMIT
                    && measure.unit.chars().all(|c| (' '..='~').contains(&c)),
                "a measurement's unit is 1 to 32 printable characters",
            )?;
            names.push(&measure.name);
        }
        rule(
            names.contains(&self.primary.as_str()),
            "primary is one of the measurements",
        )?;
        for argument in &self.arguments {
            rule(
                matches!(argument.kind.as_str(), "text" | "number"),
                "an argument's type is text or number",
            )?;
        }
        let mut all: Vec<&str> = self.bindings.iter().map(String::as_str).collect();
        all.extend(self.arguments.iter().map(|a| a.name.as_str()));
        let mut seen = std::collections::BTreeSet::new();
        rule(
            all.iter().all(|name| seen.insert(*name)) && {
                let mut measured = std::collections::BTreeSet::new();
                names.iter().all(|name| measured.insert(*name))
            },
            "a name is given twice",
        )?;
        self.descriptor().map(|_| ())
    }

    /// The action a tool file is: its name, version and purpose; an image input for each
    /// binding; an argument for each it declares; and, when it asks for segments, the
    /// need of the default segmenter for as many requests as it declares, which its
    /// admission reserves for. It runs one at a time.
    pub fn descriptor(&self) -> Result<ActionDescriptor, String> {
        let mut draft =
            ActionDescriptor::draft(&self.name, &self.version, Kind::Primitive, &self.purpose)
                .concurrent(false);
        for binding in &self.bindings {
            draft = draft.input(InputPort::required(
                binding,
                ArtifactType::Image,
                "An image the tool file reads.",
            ));
        }
        for argument in &self.arguments {
            let kind = if argument.kind == "number" {
                ValueType::number(None, None)
            } else {
                ValueType::Text
            };
            draft = draft.argument(if argument.required {
                Argument::required(&argument.name, &argument.purpose, kind)
            } else {
                Argument::optional(&argument.name, &argument.purpose, kind)
            });
        }
        if self.segments_most > 0 {
            draft = draft.needs(Requirement::new(Capability::Segment).requests(self.segments_most));
        }
        draft
            .build()
            .map_err(|why| format!("the declaration: {why}"))
    }

    /// Whether it returns regions beside its readings.
    pub fn regions(&self) -> bool {
        self.evidence.kinds().contains(&REGIONS)
    }

    /// Which way of its primary measurement is changed.
    pub fn changed(&self) -> Changed {
        if self.changed == "smaller" {
            Changed::Smaller
        } else {
            Changed::Larger
        }
    }

    /// What a judge is told of it.
    pub fn file_tool(&self) -> FileTool {
        FileTool {
            name: self.name.clone(),
            purpose: self.purpose.clone(),
            bindings: self.bindings.clone(),
            arguments: (self.arguments.iter())
                .map(|a| FileArgument {
                    name: a.name.clone(),
                    number: a.kind == "number",
                    purpose: a.purpose.clone(),
                    required: a.required,
                })
                .collect(),
        }
    }

    /// The unit of a measurement.
    pub fn unit(&self, name: &str) -> &str {
        (self.measurements.iter())
            .find(|m| m.name == name)
            .map_or("", |m| m.unit.as_str())
    }
}

/// A tool file as it is now: where it is, its bytes, their digest, its declaration.
pub struct ToolFile {
    pub path: PathBuf,
    pub code: Vec<u8>,
    pub digest: Digest,
    pub declaration: Declaration,
}

/// Reads a tool file once: a regular file under its own name, not a link, that its
/// group and others cannot write, of at most 1 MiB; and its declaration. The sentence of
/// a refusal names the file by its name alone.
pub fn read(path: &Path) -> Result<ToolFile, String> {
    let shown = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let refused = |why: &str| format!("the tool file {shown}: {why}");
    let meta = std::fs::symlink_metadata(path).map_err(|_| refused("cannot be read"))?;
    if !meta.file_type().is_file() {
        return Err(refused("is a link or not a file, and is not read"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return Err(refused(
                "can be written by its group or others, and is not run",
            ));
        }
    }
    let file = std::fs::File::open(path).map_err(|_| refused("cannot be read"))?;
    let opened = file.metadata().map_err(|_| refused("cannot be read"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (opened.dev(), opened.ino()) != (meta.dev(), meta.ino()) {
            return Err(refused("was replaced while it was opened"));
        }
    }
    if !opened.file_type().is_file() {
        return Err(refused("is a link or not a file, and is not read"));
    }
    let mut code = Vec::new();
    file.take(CODE_LIMIT as u64 + 1)
        .read_to_end(&mut code)
        .map_err(|_| refused("cannot be read"))?;
    if code.is_empty() || code.len() > CODE_LIMIT {
        return Err(refused("is empty or over 1 MiB"));
    }
    let declaration = Declaration::parse(&code).map_err(|why| refused(&why))?;
    Ok(ToolFile {
        path: path.to_path_buf(),
        digest: Digest::of_bytes(&code),
        code,
        declaration,
    })
}

/// One reading of a tool: what it read, and its values.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub subject: String,
    pub values: BTreeMap<String, f64>,
}

/// Where a region is, in pixels of its image: a box, or a polygon of 3 to 64 vertices.
#[derive(Debug, Clone, PartialEq)]
pub enum RegionShape {
    Box([f64; 4]),
    Polygon(Vec<[f64; 2]>),
}

/// One region a tool located: the image it is on, by the binding the run handed it under,
/// its shape, a label and a score from 0 to 1.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub frame: String,
    pub shape: RegionShape,
    pub label: String,
    pub score: f64,
}

/// An image a run handed a tool, by its binding: what a region may be on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub name: String,
    pub width: u32,
    pub height: u32,
}

/// A tool's output, held to its declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Readings {
    pub readings: Vec<Reading>,
    pub not_applicable: Vec<String>,
    /// The regions it located: empty from a tool that declares none.
    pub regions: Vec<Region>,
}

fn subject(value: &Json) -> Result<String, String> {
    match value.as_str() {
        Some(text)
            if text.chars().count() <= SUBJECT_LIMIT
                && text.chars().all(|c| (' '..='~').contains(&c)) =>
        {
            Ok(text.to_owned())
        }
        _ => Err("a subject is a text of at most 200 printable characters".to_owned()),
    }
}

/// A tool's output held to its declaration: an object of `readings` and, if it likes,
/// `not_applicable` and, from a tool that declares them, `regions`, and nothing else; at
/// most the declared count of readings, each a subject and values of declared
/// measurements, each finite and within its bounds; at most the declared count of
/// regions, each on one of `frames`, the images the run handed it ([`region`]).
pub fn checked(
    declaration: &Declaration,
    output: &Json,
    frames: &[Frame],
) -> Result<Readings, String> {
    let Some(fields) = output.as_object() else {
        return Err("the output is not an object".to_owned());
    };
    let declared = |key: &str| {
        matches!(key, "readings" | "not_applicable") || (key == "regions" && declaration.regions())
    };
    if let Some(key) = fields.keys().find(|k| !declared(k)) {
        let shown: String = key.chars().take(32).collect();
        return Err(format!(
            "the output holds {shown:?}, which it does not declare"
        ));
    }
    let listed = fields
        .get("readings")
        .and_then(Json::as_array)
        .ok_or("the output has no list of readings")?;
    if listed.len() > declaration.readings_most {
        return Err(format!(
            "the output has {} readings, more than the {} it declares",
            listed.len(),
            declaration.readings_most
        ));
    }
    let mut readings = Vec::new();
    for item in listed {
        let Some(item) = item.as_object() else {
            return Err("a reading is not an object".to_owned());
        };
        if item.len() != 2 || !item.contains_key("subject") {
            return Err("a reading is a subject and its values, and nothing else".to_owned());
        }
        let subject = subject(&item["subject"])?;
        let Some(values) = item.get("values").and_then(Json::as_object) else {
            return Err("a reading's values are not an object".to_owned());
        };
        let mut read = BTreeMap::new();
        for (name, value) in values {
            let Some(measure) = declaration.measurements.iter().find(|m| &m.name == name) else {
                let shown: String = name.chars().take(32).collect();
                return Err(format!(
                    "a reading gives {shown:?}, which the tool does not declare"
                ));
            };
            let number = value.as_f64().filter(|n| n.is_finite());
            let Some(number) = number.filter(|n| (measure.min..=measure.max).contains(n)) else {
                return Err(format!(
                    "{} is outside its declared bounds, {} to {}",
                    measure.name, measure.min, measure.max
                ));
            };
            read.insert(name.clone(), number);
        }
        readings.push(Reading {
            subject,
            values: read,
        });
    }
    let mut not_applicable = Vec::new();
    if let Some(value) = fields.get("not_applicable") {
        let listed = value.as_array().ok_or("not_applicable is not a list")?;
        if listed.len() > READINGS_MOST {
            return Err("not_applicable lists more than 64".to_owned());
        }
        for item in listed {
            not_applicable.push(subject(item)?);
        }
    }
    let mut regions = Vec::new();
    if let Some(value) = fields.get("regions") {
        let listed = value.as_array().ok_or("regions is not a list")?;
        let most = declaration.max_regions.unwrap_or(0);
        if listed.len() > most {
            return Err(format!(
                "the output has {} regions, more than the {most} it declares",
                listed.len()
            ));
        }
        let label_most = declaration.max_region_label_bytes.unwrap_or(0);
        for item in listed {
            regions.push(region(item, frames, label_most)?);
        }
    }
    Ok(Readings {
        readings,
        not_applicable,
        regions,
    })
}

/// One region held to its rules: an object of `frame`, the binding of an image the run
/// handed the tool; exactly one of `box`, `[left, top, right, bottom]` with area inside
/// that image, and `polygon`, 3 to 64 points `[x, y]` inside it, with area; `label`,
/// printable ASCII of 1 to `label_most` bytes; and `score`, from 0 to 1. Nothing else.
pub fn region(item: &Json, frames: &[Frame], label_most: usize) -> Result<Region, String> {
    let Some(item) = item.as_object() else {
        return Err("a region is not an object".to_owned());
    };
    if let Some(key) = item
        .keys()
        .find(|k| !matches!(k.as_str(), "frame" | "box" | "polygon" | "label" | "score"))
    {
        let shown: String = key.chars().take(32).collect();
        return Err(format!("a region holds {shown:?}, which a region does not"));
    }
    let named = item.get("frame").and_then(Json::as_str).unwrap_or_default();
    let Some(frame) = frames.iter().find(|f| f.name == named) else {
        let shown: String = named.chars().take(32).collect();
        return Err(format!(
            "a region is on {shown:?}, which is no image the run handed the tool"
        ));
    };
    let number = |value: &Json| value.as_f64().filter(|n| n.is_finite());
    let (width, height) = (f64::from(frame.width), f64::from(frame.height));
    let inside = |x: f64, y: f64| (0.0..=width).contains(&x) && (0.0..=height).contains(&y);
    let shape = match (item.get("box"), item.get("polygon")) {
        (Some(given), None) => {
            let values: Vec<f64> = (given.as_array().into_iter().flatten())
                .filter_map(number)
                .collect();
            let [left, top, right, bottom] = values[..] else {
                return Err("a region's box is four numbers".to_owned());
            };
            if !(inside(left, top) && inside(right, bottom) && left < right && top < bottom) {
                return Err(format!(
                    "a region's box is not inside the {}x{} image {}, with area",
                    frame.width, frame.height, frame.name
                ));
            }
            RegionShape::Box([left, top, right, bottom])
        }
        (None, Some(given)) => {
            let listed = given.as_array().ok_or("a region's polygon is not a list")?;
            if !(3..=POLYGON_MOST).contains(&listed.len()) {
                return Err("a region's polygon has 3 to 64 points".to_owned());
            }
            let mut points = Vec::new();
            for point in listed {
                let values: Vec<f64> = (point.as_array().into_iter().flatten())
                    .filter_map(number)
                    .collect();
                let [x, y] = values[..] else {
                    return Err("a point of a region's polygon is two numbers".to_owned());
                };
                if !inside(x, y) {
                    return Err(format!(
                        "a point of a region's polygon is not inside the {}x{} image {}",
                        frame.width, frame.height, frame.name
                    ));
                }
                points.push([x, y]);
            }
            // Twice its signed area, by the shoelace: a polygon with none is no region.
            let twice: f64 = (0..points.len())
                .map(|i| {
                    let ([x0, y0], [x1, y1]) = (points[i], points[(i + 1) % points.len()]);
                    x0 * y1 - x1 * y0
                })
                .sum();
            if twice == 0.0 {
                return Err("a region's polygon has no area".to_owned());
            }
            RegionShape::Polygon(points)
        }
        _ => return Err("a region has a box or a polygon, one of the two".to_owned()),
    };
    let label = match item.get("label").and_then(Json::as_str) {
        Some(text)
            if !text.is_empty()
                && text.len() <= label_most
                && text.chars().all(|c| (' '..='~').contains(&c)) =>
        {
            text.to_owned()
        }
        _ => {
            return Err(format!(
                "a region's label is printable text of 1 to the {label_most} bytes declared"
            ));
        }
    };
    let Some(score) = item
        .get("score")
        .and_then(number)
        .filter(|n| (0.0..=1.0).contains(n))
    else {
        return Err("a region's score is a number from 0 to 1".to_owned());
    };
    Ok(Region {
        frame: frame.name.clone(),
        shape,
        label,
        score,
    })
}

impl Readings {
    /// The primary measurement over every reading that gives it, reduced by the worst
    /// rule: the largest when larger is changed, the smallest otherwise. None when no
    /// reading gives it.
    pub fn primary(&self, declaration: &Declaration) -> Option<f64> {
        let values =
            (self.readings.iter()).filter_map(|r| r.values.get(&declaration.primary).copied());
        match declaration.changed() {
            Changed::Larger => values.reduce(f64::max),
            Changed::Smaller => values.reduce(f64::min),
        }
    }

    /// The readings as JSON: what a judge is shown and a run's diagnostic holds. The
    /// regions are there when the tool declares them.
    pub fn to_json(&self, declaration: &Declaration) -> Json {
        let mut shown = json!({
            "readings": self.readings.iter().map(|r| json!({"subject": r.subject, "values": r.values})).collect::<Vec<_>>(),
            "not_applicable": self.not_applicable,
        });
        if declaration.regions()
            && let Json::Object(fields) = &mut shown
        {
            let regions = (self.regions.iter())
                .map(|r| {
                    let mut region = json!({"frame": r.frame, "label": r.label, "score": r.score});
                    match &r.shape {
                        RegionShape::Box(edges) => region["box"] = json!(edges),
                        RegionShape::Polygon(points) => region["polygon"] = json!(points),
                    }
                    region
                })
                .collect();
            fields.insert("regions".to_owned(), Json::Array(regions));
        }
        shown
    }
}

/// The tool files of a build: each listed file with the declaration its action was made
/// from, and the one worker of the process, whose tool worker runs them.
pub struct Files {
    tools: Vec<(PathBuf, Declaration)>,
    worker: Arc<OneWorker>,
    /// The config's segment backends that name a model, each its name and its model's;
    /// and the name of the default: what an ask that names a model is routed by.
    segmenters: Vec<(String, String)>,
    default: Option<String>,
}

impl Files {
    /// Reads each listed tool file and its declaration: what the config lists, when it
    /// is read. A file that cannot be read, or whose declaration breaks a rule, is
    /// refused, and so is a name given twice.
    pub fn read<'a>(
        paths: impl IntoIterator<Item = &'a Path>,
        worker: Arc<OneWorker>,
    ) -> Result<Files, String> {
        let mut tools: Vec<(PathBuf, Declaration)> = Vec::new();
        for path in paths {
            let file = read(path)?;
            if tools.iter().any(|(_, d)| d.name == file.declaration.name) {
                return Err(format!(
                    "two tool files are named {}",
                    file.declaration.name
                ));
            }
            tools.push((file.path, file.declaration));
        }
        Ok(Files {
            tools,
            worker,
            segmenters: Vec::new(),
            default: None,
        })
    }

    /// The same, routing an ask that names a model by these segment backends of the
    /// config, each its name and the model it names, and the name of the default.
    #[must_use]
    pub fn routing<'a>(
        mut self,
        segmenters: impl IntoIterator<Item = (&'a str, &'a str)>,
        default: Option<&str>,
    ) -> Files {
        self.segmenters = (segmenters.into_iter())
            .map(|(name, model)| (name.to_owned(), model.to_owned()))
            .collect();
        self.segmenters.sort();
        self.default = default.map(str::to_owned);
        self
    }

    /// The segment backend an ask that names this model is routed to: the default when
    /// it is that model's, and otherwise the first by name whose model it is. None is
    /// a refusal that says how to add one.
    pub fn backend_for(&self, model: &str) -> Result<String, String> {
        let serving: Vec<&str> = (self.segmenters.iter())
            .filter(|(_, named)| named == model)
            .map(|(name, _)| name.as_str())
            .collect();
        if let Some(default) = (self.default.as_deref()).filter(|d| serving.contains(d)) {
            return Ok(default.to_owned());
        }
        serving.first().map(|name| (*name).to_owned()).ok_or_else(|| {
            format!(
                "the tool asked for the model {model}, and no segment backend of the config is that model; add one whose model is {model}"
            )
        })
    }

    /// The declaration of the tool file of this name, as its action was made.
    pub fn declared(&self, name: &str) -> Option<&Declaration> {
        self.tools.iter().map(|(_, d)| d).find(|d| d.name == name)
    }

    /// Each tool file as an action.
    pub fn actions(self: &Arc<Files>) -> Result<Vec<Arc<dyn Action>>, String> {
        let mut actions: Vec<Arc<dyn Action>> = Vec::new();
        for (_, declaration) in &self.tools {
            actions.push(Arc::new(ToolFileAction {
                descriptor: declaration.descriptor()?,
                files: self.clone(),
                name: declaration.name.clone(),
            }));
        }
        Ok(actions)
    }

    /// Runs the tool file of this name, as it is now, under this context.
    fn ran(
        &self,
        name: &str,
        images: &[(String, ArtifactId)],
        arguments: &Map<String, Json>,
        context: &Context,
    ) -> Result<(ToolFile, Readings), String> {
        let Some((path, made)) = self.tools.iter().find(|(_, d)| d.name == name) else {
            return Err(format!("no tool file is named {name}"));
        };
        let file = read(path)?;
        let shape = |d: &Declaration| (d.name.clone(), d.bindings.clone(), d.arguments.clone());
        if shape(&file.declaration) != shape(made) {
            return Err("the tool file was edited in its name, its images or its arguments; start whizbang again to take it".to_owned());
        }
        let declaration = &file.declaration;
        let limits = DecodeLimits::default();
        let mut handed = Vec::new();
        let mut frames = Vec::new();
        for (binding, id) in images {
            let image = match artifact::get(context.store(), ArtifactType::Image, *id, &limits) {
                Ok(Artifact::Image(image)) => image,
                Ok(_) => return Err(format!("{binding} is not an image")),
                Err(error) => return Err(format!("{binding}: {error}")),
            };
            let pixels = image.pixels(&limits).map_err(|e| e.to_string())?;
            frames.push(Frame {
                name: binding.clone(),
                width: pixels.width(),
                height: pixels.height(),
            });
            handed.push(ToolImage {
                name: binding.clone(),
                width: pixels.width(),
                height: pixels.height(),
                rgba: pixels.rgba().to_vec(),
            });
        }
        let run = ToolRun {
            tool: declaration.name.clone(),
            digest: file.digest.hex(),
            code: file.code.clone(),
            arguments: arguments.clone(),
            images: handed,
        };
        let limits = ToolLimits {
            cancellation: context.cancellation(),
            deadline: context.deadline(),
            limit: Duration::from_secs(declaration.time_limit_s),
        };
        let mut answer = |asked: &Asked| self.answer(context, images, &frames, asked);
        let output = self
            .worker
            .run_tool(&Secrets::none(), run, &limits, &mut answer)
            .map_err(|fault| fault.to_string())?;
        let readings = checked(declaration, &output, &frames)?;
        Ok((file, readings))
    }
}

impl Files {
    /// What answers a tool's ask: a nested call of `segment` or of `labels` under the
    /// run's context, of the backend its model is routed to when it names one.
    fn answer(
        &self,
        context: &Context,
        images: &[(String, ArtifactId)],
        frames: &[Frame],
        asked: &Asked,
    ) -> Result<Answered, String> {
        let Some((_, id)) = images.iter().find(|(name, _)| *name == asked.image) else {
            return Err("the tool asked about an image the run did not hand it".to_owned());
        };
        let Some(frame) = frames.iter().find(|frame| frame.name == asked.image) else {
            return Err("the tool asked about an image the run did not hand it".to_owned());
        };
        let backend = asked
            .model
            .as_deref()
            .map(|model| self.backend_for(model))
            .transpose()?;
        let query = match &asked.sought {
            Sought::Phrase(phrase) => Some(("phrase", phrase.clone())),
            Sought::Point([x, y]) => Some(("point", format!("{x} {y}"))),
            Sought::Box([l, t, r, b]) => Some(("box", format!("{l} {t} {r} {b}"))),
            Sought::Labels(_) => None,
        };
        let (action, call) = match (&asked.sought, query) {
            (_, Some((kind, query))) => (
                whizbang_rascal::segment::NAME,
                Call::new(whizbang_rascal::segment::NAME)
                    .input("image", *id)
                    .argument("kind", Value::text(kind))
                    .argument("query", Value::text(&query))
                    .argument("max_instances", Value::Integer(i64::from(asked.most))),
            ),
            (Sought::Labels(classes), None) => (
                whizbang_rascal::labels::NAME,
                Call::new(whizbang_rascal::labels::NAME)
                    .input("image", *id)
                    .argument("classes", Value::text(&classes.join("\n"))),
            ),
            (_, None) => return Err("the tool asked for nothing".to_owned()),
        };
        let call = match &backend {
            Some(name) => call.argument("backend", Value::text(name)),
            None => call,
        };
        let finished = context
            .executor()
            .submit(&call)
            .map_err(|e| format!("{action} was refused: {e}"))?;
        let outputs = match finished.outcome() {
            Outcome::Completed { outputs } => outputs,
            Outcome::Invalid { why } => return Err(format!("{action} was refused: {why}")),
            Outcome::Error { kind, .. } => return Err(format!("{action} failed: {kind}")),
            Outcome::Cancelled => return Err(format!("{action} was cancelled")),
        };
        let masks = masks_of(context, outputs)?;
        if !matches!(asked.sought, Sought::Labels(_)) {
            return Ok(Answered::Masks(masks));
        }
        labelled(masks, finished.measurements(), frame)
    }
}

/// A picture's labels, from the masks `labels` committed, in the order its measurements
/// list the classes: each pixel the place of its class in that list, from 1, or 0.
fn labelled(
    masks: Vec<ToolMask>,
    measured: Option<&Json>,
    frame: &Frame,
) -> Result<Answered, String> {
    let unread = || "what labels handed back cannot be read".to_owned();
    let listed = (measured
        .and_then(|m| m.get("classes"))
        .and_then(Json::as_array))
    .ok_or_else(unread)?;
    if listed.len() != masks.len() || listed.len() > usize::from(u8::MAX) {
        return Err(unread());
    }
    let mut classes = Vec::new();
    for class in listed {
        let name = class
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(unread)?;
        let score = class
            .get("score")
            .and_then(Json::as_f64)
            .ok_or_else(unread)?;
        classes.push((name.to_owned(), score.clamp(0.0, 1.0)));
    }
    let (width, height) = (frame.width, frame.height);
    let mut plane = vec![0u8; width as usize * height as usize];
    for (at, mask) in masks.iter().enumerate() {
        if (mask.width, mask.height) != (width, height) || mask.plane.len() != plane.len() {
            return Err(unread());
        }
        let k = u8::try_from(at + 1).map_err(|_| unread())?;
        for (place, sample) in mask.plane.iter().enumerate() {
            if *sample != 0 {
                plane[place] = k;
            }
        }
    }
    Ok(Answered::Labels(Labelled {
        width,
        height,
        classes,
        plane,
    }))
}

/// The masks a nested call committed, each its plane, 255 on and 0 off, and its bounds.
fn masks_of(
    context: &Context,
    outputs: &[whizbang_core::contract::Committed],
) -> Result<Vec<ToolMask>, String> {
    let limits = DecodeLimits::default();
    let mut masks = Vec::new();
    for output in outputs {
        let mask = match artifact::get(context.store(), ArtifactType::Mask, output.id(), &limits) {
            Ok(Artifact::Mask(mask)) => mask,
            _ => return Err("a mask the nested call committed cannot be read".to_owned()),
        };
        let (width, height) = (mask.space().width(), mask.space().height());
        let plane: Vec<u8> = mask
            .core()
            .iter()
            .map(|v| if *v == 0 { 0 } else { 255 })
            .collect();
        let mut bounds = [f64::MAX, f64::MAX, 0.0f64, 0.0f64];
        for (at, _) in plane.iter().enumerate().filter(|(_, v)| **v != 0) {
            let (x, y) = ((at as u32 % width) as f64, (at as u32 / width) as f64);
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x + 1.0),
                bounds[3].max(y + 1.0),
            ];
        }
        if bounds[0] == f64::MAX {
            bounds = [0.0; 4];
        }
        masks.push(ToolMask {
            score: None,
            bounds,
            width,
            height,
            plane,
        });
    }
    Ok(masks)
}

impl FileTools for Files {
    fn listed(&self) -> Vec<FileTool> {
        self.tools.iter().map(|(_, d)| d.file_tool()).collect()
    }

    fn revision(&self, name: &str) -> Option<String> {
        let (path, _) = self.tools.iter().find(|(_, d)| d.name == name)?;
        read(path).ok().map(|file| file.digest.to_string())
    }

    fn run(
        &self,
        name: &str,
        images: &[(String, ArtifactId)],
        arguments: &Map<String, Json>,
        context: &Context,
    ) -> Result<FileRan, String> {
        let (file, readings) = self.ran(name, images, arguments, context)?;
        Ok(FileRan {
            digest: file.digest.to_string(),
            output: readings.to_json(&file.declaration),
        })
    }
}

/// A tool file as an action: `try` and `eval tool` run it so, each read a run.
struct ToolFileAction {
    descriptor: ActionDescriptor,
    files: Arc<Files>,
    name: String,
}

impl Action for ToolFileAction {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        // What a tool file asks for is a call of `segment`, and of nothing else: frozen
        // here, so a tool file run as a step of a recipe, whose list binds only its own
        // calls (ADR 0035), is held as it is under a judge. Where the run froze its calls
        // already (a judge's, which lists `segment`), this changes nothing.
        let _ = context.freeze_calls([
            whizbang_rascal::segment::NAME,
            whizbang_rascal::labels::NAME,
        ]);
        let images: Vec<(String, ArtifactId)> = (call.inputs().iter())
            .map(|input| (input.port().to_owned(), input.id()))
            .collect();
        let arguments: Map<String, Json> = (call.arguments().iter())
            .map(|(name, value)| {
                let value = match value {
                    Value::Text(text) => json!(text),
                    Value::Number(number) => json!(number),
                    Value::Integer(number) => json!(number),
                    Value::Boolean(boolean) => json!(boolean),
                };
                (name.clone(), value)
            })
            .collect();
        let (file, readings) = self
            .files
            .ran(&self.name, &images, &arguments, context)
            .map_err(|why| Failure::error("tool_file", &why))?;
        let digest = file.digest.to_string();
        context
            .facts()
            .setting(DIGEST_SETTING, &Value::text(&digest));
        let declaration = &file.declaration;
        let measurements: Vec<Json> = (readings.primary(declaration).into_iter())
            .map(|value| {
                json!({"name": declaration.primary, "value": value, "unit": declaration.unit(&declaration.primary)})
            })
            .collect();
        let mut report = readings.to_json(declaration);
        if let Json::Object(fields) = &mut report {
            fields.insert("tool".to_owned(), json!(declaration.name));
            fields.insert("version".to_owned(), json!(declaration.version));
            fields.insert("digest".to_owned(), json!(digest));
            fields.insert("measurements".to_owned(), Json::Array(measurements));
        }
        let kept = context
            .diagnostics()
            .write(REPORT, report.to_string().as_bytes());
        kept.map_err(|why| Failure::error("tool_file", &why.to_string()))?;
        Ok(Produced::nothing())
    }
}
