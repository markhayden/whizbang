//! Squint's judgement of a submission, for the replay: its JSON Schema check, then its types,
//! then its rules in their order, each refusal in squint's words; and the verdict its
//! aggregation works out. Ported from squint at the commit `docs/sources.toml` pins:
//! `crates/squint-core/src/submission.rs` (`validate_schema`, `parse_and_validate`,
//! `validate_items`, `validate_findings`, `check_location`, `check_exemption_applies`,
//! `check_evidence`), `crates/squint-core/src/frames/mod.rs` (`Location`, `validate`,
//! `to_root` for flat frames, `map_through_bbox`), `crates/squint-core/src/frames/aligned.rs`
//! (`map_through_homography`), `crates/squint-core/src/run.rs` (`finish`: the rows) and
//! `crates/squint-core/src/aggregate.rs` (`strictest`, `verdict`).
//!
//! The schema is squint's (`use_case/submission-v1.json`), and the validator is the one squint
//! ran, at the version its lock file pins, so a schema refusal reads as squint's did. This is
//! the replay's judgement only: `check` keeps whizbang's own (`crate::use_case`).

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;
use serde_json::Value as Json;

use crate::use_case::{Checklist, Confidence, Disposition, Resolved, Row, row};

const SCHEMA: &str = include_str!("../use_case/submission-v1.json");
const SCHEMA_VERSION: u32 = 1;

/// A refusal, in squint's words, without the `submission: ` its error's display adds.
pub(super) type Refused = String;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Submission {
    schema_version: u32,
    items: Vec<ItemDisposition>,
    findings: Vec<Finding>,
    opinion: Opinion,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemDisposition {
    id: String,
    disposition: Disposition,
    confidence: Confidence,
    reason: String,
    allowed_by: Option<String>,
    location: Option<Location>,
    evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FindingDisposition {
    Violation,
    AllowedChange,
    CantTell,
    Observed,
}

impl From<FindingDisposition> for Disposition {
    fn from(d: FindingDisposition) -> Disposition {
        match d {
            FindingDisposition::Violation => Disposition::Violation,
            FindingDisposition::AllowedChange => Disposition::AllowedChange,
            FindingDisposition::CantTell => Disposition::CantTell,
            FindingDisposition::Observed => Disposition::Observed,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Finding {
    obligation_ids: Vec<String>,
    disposition: FindingDisposition,
    confidence: Confidence,
    reason: String,
    allowed_by: Option<String>,
    location: Location,
    evidence: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Opinion {
    verdict: String,
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Location {
    frame: String,
    kind: LocationKind,
    bbox: Option<[f64; 4]>,
    polygon: Option<Vec<[f64; 2]>>,
    yaw: Option<f64>,
    pitch: Option<f64>,
    fov: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LocationKind {
    Bbox,
    Polygon,
    View,
    WholeImage,
    Absent,
}

/// A frame of the run as a location is mapped through it: its parent and how it sits there
/// (`{"bbox": …}` for a crop, `{"homography": …}` for an aligned frame), none for a raw frame;
/// and the image at its root.
#[derive(Debug, Clone)]
pub(super) struct Link {
    pub parent: Option<String>,
    pub mapping: Option<Json>,
    pub root: String,
}

/// What a submission is judged against.
pub(super) struct Judged<'a> {
    pub checklist: &'a Checklist,
    pub resolved: &'a Resolved,
    pub frames: &'a BTreeMap<String, Link>,
    /// The ids (`call:<n>`) of the run's calls that completed.
    pub completed: &'a [String],
}

/// Squint's schema check: every error the validator finds, each with where it is.
fn schema(arguments: &Json) -> Result<(), Refused> {
    let schema: Json = serde_json::from_str(SCHEMA)
        .map_err(|e| format!("internal: canonical schema invalid: {e}"))?;
    let validator = jsonschema::validator_for(&schema)
        .map_err(|e| format!("internal: canonical schema invalid: {e}"))?;
    let errors: Vec<String> = validator
        .iter_errors(arguments)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "does not match the submission schema: {}",
            errors.join("; ")
        ))
    }
}

/// The verdict a submission states, or why squint refused it.
pub(super) fn judged(arguments: &Json, ctx: &Judged<'_>) -> Result<&'static str, Refused> {
    schema(arguments)?;
    let sub: Submission = serde_json::from_value(arguments.clone())
        .map_err(|e| format!("does not match the submission types: {e}"))?;
    if sub.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "schema_version must be {SCHEMA_VERSION}, got {}",
            sub.schema_version
        ));
    }
    items(&sub, ctx)?;
    findings(&sub, ctx)?;
    // The rows, as squint's `finish` made them: each item over the instances it depends on;
    // each finding that is not an observation over the instances of the obligations it cites
    // on the image it is located on.
    let mut rows = Vec::new();
    for item in &sub.items {
        let deps = ctx
            .checklist
            .get(&item.id)
            .map(|c| c.instances.clone())
            .unwrap_or_default();
        rows.push(strictest(
            ctx.resolved
                .instances
                .iter()
                .filter(|i| deps.contains(&i.id()))
                .map(|i| row(i, item.disposition, item.confidence)),
        ));
    }
    for f in &sub.findings {
        let d: Disposition = f.disposition.into();
        if d == Disposition::Observed {
            continue;
        }
        let root = to_root(&f.location, ctx)
            .ok()
            .and_then(|mapped| ctx.frames.get(&mapped.frame).map(|l| l.root.clone()));
        rows.push(strictest(
            ctx.resolved
                .instances
                .iter()
                .filter(|i| {
                    f.obligation_ids.contains(&i.obligation_id) && Some(&i.target) == root.as_ref()
                })
                .map(|i| row(i, d, f.confidence)),
        ));
    }
    Ok(match rows.iter().max() {
        Some(Row::Fail) => "fail",
        Some(Row::Unsure) => "unsure",
        _ => "pass",
    })
}

fn strictest(rows: impl IntoIterator<Item = Row>) -> Row {
    rows.into_iter().max().unwrap_or(Row::Recorded)
}

fn items(sub: &Submission, ctx: &Judged<'_>) -> Result<(), Refused> {
    let mut counts: HashMap<&str, usize> = ctx.checklist.ids().map(|id| (id, 0)).collect();
    for item in &sub.items {
        let Some(n) = counts.get_mut(item.id.as_str()) else {
            return Err(format!("unknown checklist item {:?}", item.id));
        };
        *n += 1;
        let entry = ctx
            .checklist
            .get(&item.id)
            .ok_or_else(|| format!("unknown checklist item {:?}", item.id))?;
        if item.reason.trim().is_empty() {
            return Err(format!("{}: reason must not be empty", item.id));
        }
        if let Some(loc) = &item.location {
            check_location(loc, ctx, &item.id)?;
        }
        match item.disposition {
            Disposition::Violation if item.location.is_none() => {
                return Err(format!("{}: a violation needs a location", item.id));
            }
            Disposition::Observed => {
                return Err(format!(
                    "{}: `observed` is a finding disposition, not an item one",
                    item.id
                ));
            }
            Disposition::AllowedChange => {
                let allowed_by = item
                    .allowed_by
                    .as_deref()
                    .ok_or_else(|| format!("{}: allowed_change needs allowed_by", item.id))?;
                exemption_applies(allowed_by, &entry.instances, ctx, &item.id)?;
            }
            _ => {
                if item.allowed_by.is_some() {
                    return Err(format!(
                        "{}: allowed_by is only valid with disposition allowed_change",
                        item.id
                    ));
                }
            }
        }
        evidence(&item.evidence, ctx, &item.id)?;
    }
    // Squint walked a hash map here; with every item answered once there is nothing to
    // report, and the order matters only for which of several items is named.
    for (id, n) in counts {
        if n != 1 {
            return Err(format!(
                "checklist item {id:?} must appear exactly once, appeared {n} times"
            ));
        }
    }
    Ok(())
}

fn findings(sub: &Submission, ctx: &Judged<'_>) -> Result<(), Refused> {
    for (n, f) in sub.findings.iter().enumerate() {
        let tag = format!("finding {n}");
        if f.reason.trim().is_empty() {
            return Err(format!("{tag}: reason must not be empty"));
        }
        if f.obligation_ids.is_empty() {
            return Err(format!("{tag}: cites no obligation"));
        }
        let mapped = check_location(&f.location, ctx, &tag)?;
        let root = ctx.frames.get(&mapped.frame).map(|l| l.root.clone());
        let mut mandatory: Vec<String> = Vec::new();
        let mut cited_observe = 0usize;
        for id in &f.obligation_ids {
            let instances: Vec<_> = ctx
                .resolved
                .instances
                .iter()
                .filter(|i| &i.obligation_id == id)
                .collect();
            if !instances.is_empty() {
                match instances.iter().find(|i| Some(&i.target) == root.as_ref()) {
                    Some(i) => mandatory.push(i.id()),
                    None => {
                        return Err(format!(
                            "{tag}: {id} does not judge image {root:?} where the finding is located"
                        ));
                    }
                }
            } else if let Some(ob) = ctx.resolved.observations.iter().find(|o| &o.id == id) {
                if !ob.targets.iter().any(|t| Some(t) == root.as_ref()) {
                    return Err(format!(
                        "{tag}: observation {id} is not on image {}",
                        root.as_deref().unwrap_or("?")
                    ));
                }
                cited_observe += 1;
            } else if let Some(ex) = ctx.resolved.exemptions.iter().find(|e| &e.id == id) {
                return Err(if ex.waives.is_empty() {
                    format!(
                        "{tag}: {id} waives nothing; cite the obligation the change bears on, or an observe rule"
                    )
                } else {
                    format!(
                        "{tag}: {id} is an exemption; cite the obligation it waives and name it in allowed_by"
                    )
                });
            } else {
                return Err(format!(
                    "{tag}: unknown obligation {id:?}; obligation_ids take rule ids as numbered under Obligations (for example SF4), not checklist item ids"
                ));
            }
        }
        match f.disposition {
            FindingDisposition::Observed => {
                if !mandatory.is_empty() || cited_observe == 0 {
                    return Err(format!(
                        "{tag}: `observed` findings cite observe rules only (such as SF4); a mandatory rule takes violation, allowed_change or cant_tell"
                    ));
                }
                if f.allowed_by.is_some() {
                    return Err(format!("{tag}: observed findings take no allowed_by"));
                }
            }
            FindingDisposition::Violation | FindingDisposition::CantTell => {
                if mandatory.is_empty() {
                    return Err(format!(
                        "{tag}: a {:?} finding must cite at least one mandatory obligation",
                        f.disposition
                    ));
                }
                if f.allowed_by.is_some() {
                    return Err(format!(
                        "{tag}: allowed_by is only valid with allowed_change"
                    ));
                }
            }
            FindingDisposition::AllowedChange => {
                if mandatory.is_empty() {
                    return Err(format!(
                        "{tag}: an allowed_change finding must cite the mandatory obligations waived"
                    ));
                }
                let allowed_by = f
                    .allowed_by
                    .as_deref()
                    .ok_or_else(|| format!("{tag}: allowed_change needs allowed_by"))?;
                exemption_applies(allowed_by, &mandatory, ctx, &tag)?;
            }
        }
        evidence(&f.evidence, ctx, &tag)?;
    }
    Ok(())
}

/// Squint's validity table, then the location mapped to its root's raw frame.
fn check_location(loc: &Location, ctx: &Judged<'_>, who: &str) -> Result<Location, Refused> {
    let mapped =
        to_root(loc, ctx).map_err(|e| format!("{who}: location invalid: submission: {e}"))?;
    if loc.kind == LocationKind::Absent && loc.bbox.is_some() {
        return Err(format!("{who}: absent carries no coordinates"));
    }
    Ok(mapped)
}

fn exemption_applies(
    allowed_by: &str,
    instance_ids: &[String],
    ctx: &Judged<'_>,
    who: &str,
) -> Result<(), Refused> {
    let ex = ctx
        .resolved
        .exemptions
        .iter()
        .find(|e| e.id == allowed_by)
        .ok_or_else(|| format!("{who}: allowed_by {allowed_by:?} is not an enabled exemption"))?;
    for inst_id in instance_ids {
        let inst = ctx
            .resolved
            .instance(inst_id)
            .ok_or_else(|| format!("{who}: internal: unknown instance {inst_id}"))?;
        if ex.waives.is_empty() {
            return Err(format!(
                "{who}: {allowed_by} waives nothing and is never an allowed_by; if {} still holds there, the item is `held`",
                inst.obligation_id
            ));
        }
        if !ex.applies_to(&inst.obligation_id, &inst.target) {
            return Err(format!(
                "{who}: exemption {allowed_by} does not waive {} on {}",
                inst.obligation_id, inst.target
            ));
        }
    }
    Ok(())
}

fn evidence(evidence: &[String], ctx: &Judged<'_>, who: &str) -> Result<(), Refused> {
    for call in evidence {
        if !ctx.completed.iter().any(|c| c == call) {
            return Err(format!(
                "{who}: evidence {call:?} is not a tool call that completed in this run"
            ));
        }
    }
    Ok(())
}

// --- frames: squint's `Registry::validate` and `to_root`, for flat frames ---------------------

fn validate(loc: &Location, ctx: &Judged<'_>) -> Result<(), String> {
    if !ctx.frames.contains_key(&loc.frame) {
        return Err(format!("unknown frame {:?}", loc.frame));
    }
    let null = |name: &str, present: bool| {
        if present {
            Err(format!("{:?} location must not set {name}", loc.kind))
        } else {
            Ok(())
        }
    };
    let no_view = || -> Result<(), String> {
        null("yaw", loc.yaw.is_some())?;
        null("pitch", loc.pitch.is_some())?;
        null("fov", loc.fov.is_some())
    };
    match loc.kind {
        LocationKind::Bbox => {
            let b = loc.bbox.ok_or("bbox location needs bbox")?;
            null("polygon", loc.polygon.is_some())?;
            no_view()?;
            if b.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) {
                return Err(format!("bbox {b:?} outside [0, 1]"));
            }
            if !(b[0] < b[2] && b[1] < b[3]) {
                return Err(format!("bbox {b:?} needs x0 < x1 and y0 < y1"));
            }
        }
        LocationKind::Polygon => {
            let p = loc
                .polygon
                .as_ref()
                .ok_or("polygon location needs polygon")?;
            null("bbox", loc.bbox.is_some())?;
            no_view()?;
            polygon(p)?;
        }
        LocationKind::View => {
            null("bbox", loc.bbox.is_some())?;
            null("polygon", loc.polygon.is_some())?;
            // Every frame of the replay is flat.
            return Err("view locations need an equirectangular frame".to_owned());
        }
        LocationKind::WholeImage | LocationKind::Absent => {
            null("bbox", loc.bbox.is_some())?;
            null("polygon", loc.polygon.is_some())?;
            no_view()?;
        }
    }
    Ok(())
}

fn polygon(p: &[[f64; 2]]) -> Result<(), String> {
    if p.len() < 3 || p.len() > 64 {
        return Err(format!("polygon needs 3 to 64 vertices, has {}", p.len()));
    }
    if p.iter()
        .flatten()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err("polygon vertex outside [0, 1]".to_owned());
    }
    let n = p.len();
    for i in 0..n {
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if crossing(p[i], p[(i + 1) % n], p[j], p[(j + 1) % n]) {
                return Err("polygon is self-intersecting".to_owned());
            }
        }
    }
    Ok(())
}

fn crossing(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    fn orient(p: [f64; 2], q: [f64; 2], r: [f64; 2]) -> f64 {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    }
    (orient(a, b, c) * orient(a, b, d) < 0.0) && (orient(c, d, a) * orient(c, d, b) < 0.0)
}

/// A location mapped to the raw frame of its root image. An error is squint's
/// `Error::Submission` text, without its prefix.
fn to_root(loc: &Location, ctx: &Judged<'_>) -> Result<Location, String> {
    validate(loc, ctx)?;
    let mut current = loc.clone();
    loop {
        let link = ctx
            .frames
            .get(&current.frame)
            .ok_or_else(|| format!("unknown frame {:?}", current.frame))?;
        let (Some(parent), Some(mapping)) = (&link.parent, &link.mapping) else {
            return Ok(current);
        };
        if let Some(h) = mapping.get("homography").and_then(Json::as_array) {
            let h: Vec<f64> = h.iter().filter_map(Json::as_f64).collect();
            current = through_homography(&current, &h, &format!("raw:{}", link.root))?;
        } else if let Some(m) = mapping.get("bbox").and_then(Json::as_array) {
            let m: Vec<f64> = m.iter().filter_map(Json::as_f64).collect();
            let m: [f64; 4] = m
                .try_into()
                .map_err(|_| "a crop's box is four numbers".to_owned())?;
            current = through_bbox(&current, &m, parent)?;
        } else {
            return Err(format!("frame {:?} carries no mapping", current.frame));
        }
    }
}

fn through_bbox(loc: &Location, m: &[f64; 4], parent: &str) -> Result<Location, String> {
    let (w, h) = (m[2] - m[0], m[3] - m[1]);
    let fwd = |x: f64, y: f64| [m[0] + x * w, m[1] + y * h];
    let mut out = loc.clone();
    out.frame = parent.to_owned();
    if loc.kind == LocationKind::WholeImage {
        out.kind = LocationKind::Bbox;
        out.bbox = Some([
            m[0].clamp(0.0, 1.0),
            m[1].clamp(0.0, 1.0),
            m[2].clamp(0.0, 1.0),
            m[3].clamp(0.0, 1.0),
        ]);
        return Ok(out);
    }
    if let Some(b) = loc.bbox {
        let [x0, y0] = fwd(b[0], b[1]);
        let [x1, y1] = fwd(b[2], b[3]);
        if x1 <= 0.0 || y1 <= 0.0 || x0 >= 1.0 || y0 >= 1.0 {
            return Err(format!(
                "location maps wholly outside its parent frame {parent:?}"
            ));
        }
        out.bbox = Some([
            x0.clamp(0.0, 1.0),
            y0.clamp(0.0, 1.0),
            x1.clamp(0.0, 1.0),
            y1.clamp(0.0, 1.0),
        ]);
    }
    if let Some(p) = &loc.polygon {
        let mapped: Vec<[f64; 2]> = p.iter().map(|v| fwd(v[0], v[1])).collect();
        let inside = mapped
            .iter()
            .any(|v| (0.0..=1.0).contains(&v[0]) && (0.0..=1.0).contains(&v[1]));
        if !inside {
            return Err(format!(
                "polygon maps wholly outside its parent frame {parent:?}"
            ));
        }
        out.polygon = Some(
            mapped
                .iter()
                .map(|v| [v[0].clamp(0.0, 1.0), v[1].clamp(0.0, 1.0)])
                .collect(),
        );
    }
    Ok(out)
}

fn through_homography(loc: &Location, h: &[f64], raw: &str) -> Result<Location, String> {
    let m: [f64; 9] = h
        .try_into()
        .map_err(|_| format!("a 3×3 transform needs 9 parameters, got {}", h.len()))?;
    let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6]);
    if det.abs() < 1e-300 || !det.is_finite() {
        return Err("the transform is singular".to_owned());
    }
    let inv = [
        (m[4] * m[8] - m[5] * m[7]) / det,
        (m[2] * m[7] - m[1] * m[8]) / det,
        (m[1] * m[5] - m[2] * m[4]) / det,
        (m[5] * m[6] - m[3] * m[8]) / det,
        (m[0] * m[8] - m[2] * m[6]) / det,
        (m[2] * m[3] - m[0] * m[5]) / det,
        (m[3] * m[7] - m[4] * m[6]) / det,
        (m[1] * m[6] - m[0] * m[7]) / det,
        (m[0] * m[4] - m[1] * m[3]) / det,
    ];
    let points = match loc.kind {
        LocationKind::Bbox => {
            let b = loc.bbox.ok_or("bbox location needs bbox")?;
            vec![[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]]
        }
        LocationKind::WholeImage => vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        LocationKind::Polygon => loc
            .polygon
            .clone()
            .ok_or("polygon location needs polygon")?,
        LocationKind::Absent => {
            let mut out = loc.clone();
            out.frame = raw.to_owned();
            return Ok(out);
        }
        LocationKind::View => {
            return Err("a view location cannot sit on an aligned frame".to_owned());
        }
    };
    let apply = |p: [f64; 2]| {
        let d = inv[6] * p[0] + inv[7] * p[1] + inv[8];
        if d.abs() < 1e-12 {
            return None;
        }
        let q = [
            (inv[0] * p[0] + inv[1] * p[1] + inv[2]) / d,
            (inv[3] * p[0] + inv[4] * p[1] + inv[5]) / d,
        ];
        q.iter().all(|v| v.is_finite()).then_some(q)
    };
    let mapped: Vec<[f64; 2]> = points.into_iter().filter_map(apply).collect();
    if mapped.len() < 3 {
        return Err(format!("location maps onto the horizon of {raw:?}"));
    }
    let inside = mapped
        .iter()
        .any(|v| (0.0..=1.0).contains(&v[0]) && (0.0..=1.0).contains(&v[1]));
    if !inside {
        return Err(format!("location maps wholly outside {raw:?}"));
    }
    Ok(Location {
        frame: raw.to_owned(),
        kind: LocationKind::Polygon,
        bbox: None,
        polygon: Some(
            mapped
                .iter()
                .map(|v| [v[0].clamp(0.0, 1.0), v[1].clamp(0.0, 1.0)])
                .collect(),
        ),
        yaw: None,
        pitch: None,
        fov: None,
    })
}
