//! The judge's answer, `submit_verdict`, held to the rules squint held it to
//! (`submission.rs`: every checklist item once, a reason, an exemption that applies, a
//! location for a violation, a finding that cites an obligation of the image it is on),
//! and the verdict worked out from it by the table ([`super::row`]).
//!
//! **A location outside the frame** (plan 2C, task A1, item 5). Squint refused a
//! location wholly outside its frame as an invalid submission. Here every coordinate of
//! a box or a polygon is clamped to the frame, from 0 to 1, and the location is kept,
//! marked as clamped: where the judge points is evidence, and a slip in its numbers
//! does not throw away what it saw.

use serde::Deserialize;
use serde_json::{Value as Json, json};
use whizbang_core::capability::ToolDefinition;
use whizbang_core::contract::Verdict;
use whizbang_core::contract::record::Decision;

use crate::job::Job;

use super::{Checklist, Confidence, Disposition, Resolved, Row, SUBMIT, row};

/// The submission's schema and description, as squint's adapter sent them.
const SUBMIT_TOOL: &str = include_str!("submit_verdict.json");

/// The name of the run's diagnostic that holds the reason's items whole, one to a line,
/// each a JSON string (plan 1D, task F1, item 7). Each is a turn's text, so the turn's
/// bound holds each and all of them together ([`crate::check::TURN_TEXT_LIMIT`]); the
/// diagnostic's writer takes the run's secrets out of the whole before anything cuts or
/// escapes it.
pub const REASONS: &str = "reasons.jsonl";

/// The most characters of a reason the verdict keeps of each item that decided it. The
/// verdict's reason is a summary; each item is kept whole beside it ([`REASONS`]).
const REASON_LIMIT: usize = 300;

/// The tool the judge answers with, as it is told of it.
pub fn submit_tool() -> Option<ToolDefinition> {
    let tool: Json = serde_json::from_str(SUBMIT_TOOL).ok()?;
    let description = tool.get("description")?.as_str()?;
    ToolDefinition::new(SUBMIT, description, tool.get("input_schema")?.clone()).ok()
}

/// The submission's schema as squint's harness declared the tool, before an adapter fitted
/// it to its provider's strict mode: squint's `schemas/submission-v1.json`, byte for byte.
const SUBMISSION_SCHEMA: &str = include_str!("submission-v1.json");

/// The tool the judge answers with as a request carries it, provider apart: its name, its
/// description and squint's schema of a submission. What a transcript records
/// (`docs/baseline/transcript.md`); each adapter sends its own form of it.
pub fn submit_definition() -> Option<Json> {
    let tool: Json = serde_json::from_str(SUBMIT_TOOL).ok()?;
    let schema: Json = serde_json::from_str(SUBMISSION_SCHEMA).ok()?;
    Some(json!({
        "name": SUBMIT,
        "description": tool.get("description")?,
        "input_schema": schema,
    }))
}

#[derive(Deserialize)]
struct Submission {
    #[serde(default)]
    items: Option<Vec<Json>>,
    #[serde(default)]
    findings: Option<Vec<Json>>,
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    obligation_ids: Option<Vec<String>>,
    disposition: Disposition,
    confidence: Confidence,
    reason: String,
    #[serde(default)]
    allowed_by: Option<String>,
    #[serde(default)]
    location: Option<Location>,
}

#[derive(Deserialize)]
struct Location {
    kind: String,
    frame: String,
    #[serde(default)]
    bbox: Option<Vec<f64>>,
    #[serde(default)]
    polygon: Option<Vec<Vec<f64>>>,
}

/// A location held to the frame of an image of the job: its image, the location as it
/// is kept, and whether a coordinate was clamped.
struct Placed {
    image: String,
    kept: Json,
    clamped: bool,
}

/// The location's image, and the location kept: a box or a polygon with each
/// coordinate clamped to the frame.
fn place(location: &Location, job: &Job) -> Result<Placed, String> {
    let image = location.frame.strip_prefix("raw:").unwrap_or("");
    if !job.images.iter().any(|i| i.id == image) {
        let frames: Vec<String> = job.images.iter().map(|i| format!("raw:{}", i.id)).collect();
        return Err(format!(
            "the location's frame is not one of {}: raw: and the id of an image of the job, \
             never the frame of a view or of an aligned picture",
            frames.join(", ")
        ));
    }
    let clamp = |v: f64, clamped: &mut bool| {
        let c = v.clamp(0.0, 1.0);
        *clamped |= c != v;
        c
    };
    let mut clamped = false;
    let kept = match location.kind.as_str() {
        "bbox" => {
            let b = location.bbox.as_deref().unwrap_or_default();
            let [l, t, r, b] = b else {
                return Err("a bbox is four numbers: left, top, right and bottom".to_owned());
            };
            if ![*l, *t, *r, *b].iter().all(|v| v.is_finite()) || l >= r || t >= b {
                return Err("a bbox has left under right and top under bottom".to_owned());
            }
            let b: Vec<f64> = [*l, *t, *r, *b]
                .iter()
                .map(|v| clamp(*v, &mut clamped))
                .collect();
            json!({"kind": "bbox", "frame": location.frame, "bbox": b})
        }
        "polygon" => {
            let p = location.polygon.as_deref().unwrap_or_default();
            let fine = p.len() >= 3
                && p.iter()
                    .all(|v| v.len() == 2 && v.iter().all(|x| x.is_finite()));
            if !fine {
                return Err("a polygon is three or more vertices of two numbers".to_owned());
            }
            let p: Vec<Vec<f64>> = p
                .iter()
                .map(|v| v.iter().map(|x| clamp(*x, &mut clamped)).collect())
                .collect();
            json!({"kind": "polygon", "frame": location.frame, "polygon": p})
        }
        "whole_image" | "absent" => {
            if location.bbox.is_some() || location.polygon.is_some() {
                return Err(format!("{} carries no coordinates", location.kind));
            }
            json!({"kind": location.kind, "frame": location.frame})
        }
        _ => return Err("the location's kind is bbox, polygon, whole_image or absent".to_owned()),
    };
    Ok(Placed {
        image: image.to_owned(),
        kept,
        clamped,
    })
}

/// What a submission decided: the verdict, whose reason is the summary; each item of
/// that reason whole; and what the run keeps of each answer (its id or obligations, its
/// disposition, its confidence, its row and its location).
#[derive(Debug, Clone, PartialEq)]
pub struct Decided {
    pub verdict: Verdict,
    pub items: Vec<String>,
    pub answers: Vec<Json>,
}

/// A reason as the summary keeps it: whole when it is at most [`REASON_LIMIT`]
/// characters; otherwise cut at the last white space before that, and three dots
/// follow.
fn summary(reason: &str) -> String {
    if reason.chars().count() <= REASON_LIMIT {
        return reason.to_owned();
    }
    let head: String = reason.chars().take(REASON_LIMIT + 1).collect();
    let cut = head
        .char_indices()
        .filter(|(_, c)| c.is_whitespace())
        .map(|(at, _)| at)
        .next_back()
        .unwrap_or(0);
    format!("{}...", head[..cut].trim_end())
}

fn word<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// One answer, read and checked.
struct Said {
    named: String,
    disposition: Disposition,
    confidence: Confidence,
    reason: String,
    row: Row,
    kept: Json,
}

/// The exemption named exists, waives each instance's obligation, and covers its
/// target.
fn applies(by: &str, instances: &[String], resolved: &Resolved, who: &str) -> Result<(), String> {
    let Some(ex) = resolved.exemptions.iter().find(|e| e.id == by) else {
        return Err(format!(
            "{who}: allowed_by {by:?} is not an enabled exemption"
        ));
    };
    for id in instances {
        let Some(inst) = resolved.instance(id) else {
            return Err(format!("{who}: unknown instance {id}"));
        };
        if ex.waives.is_empty() {
            return Err(format!(
                "{who}: {by} waives nothing and is never an allowed_by; if {} still holds \
                 there, the item is `held`",
                inst.obligation_id
            ));
        }
        if !ex.applies_to(&inst.obligation_id, &inst.target) {
            return Err(format!(
                "{who}: exemption {by} does not waive {} on {}",
                inst.obligation_id, inst.target
            ));
        }
    }
    Ok(())
}

/// Ids as a rejection names them: in the order given, each once, or `none is enabled`.
fn listed<'a>(ids: impl Iterator<Item = &'a str>) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for id in ids {
        if !seen.contains(&id) {
            seen.push(id);
        }
    }
    if seen.is_empty() {
        "none is enabled".to_owned()
    } else {
        seen.join(", ")
    }
}

/// What a finding may cite, as a rejection names it: the obligations of the job, then
/// its observe rules. Only ids the judge was given (plan 2C, task F1: E1's run ended 10
/// of 220 decisions at a second submission that cited nothing, or the wrong kind).
fn citable(resolved: &Resolved) -> String {
    let obligations = listed(resolved.instances.iter().map(|i| i.obligation_id.as_str()));
    let observations = listed(resolved.observations.iter().map(|o| o.id.as_str()));
    format!(
        "the obligations {obligations}, or for an observed finding the observe rules {observations}"
    )
}

/// Holds what a submission cites as evidence to the run (squint's rule): each id an item
/// or a finding lists under `evidence` names a tool call of this run that completed, by
/// the id the judge was shown for it (`shown`: the id a tool's result gives, `call:0`, or
/// the id of the run that measured a detail handed to it first) or by the provider's own
/// id of the call (`also`); or why not, naming the item or the finding and the ids it may
/// cite, quoting nothing it says. The provider's ids are accepted and never listed: no
/// judge is shown them as text, and some providers make them up for the adapter.
pub fn cited_calls_held(arguments: &Json, shown: &[String], also: &[String]) -> Result<(), String> {
    let lists = |key: &str| arguments.get(key).and_then(Json::as_array).cloned();
    let items = (lists("items").unwrap_or_default().into_iter()).map(|item| {
        let named = item.get("id").and_then(Json::as_str).unwrap_or("an item");
        (named.to_owned(), item)
    });
    let findings = (lists("findings").unwrap_or_default().into_iter())
        .enumerate()
        .map(|(at, finding)| (format!("finding {}", at + 1), finding));
    for (who, answer) in items.chain(findings) {
        let Some(cited) = answer.get("evidence") else {
            continue;
        };
        let Some(cited) = cited.as_array() else {
            return Err(format!("{who}: evidence is a list of tool call ids"));
        };
        for id in cited {
            let completed_here = id
                .as_str()
                .is_some_and(|id| shown.iter().chain(also).any(|c| c == id));
            if !completed_here {
                let may = if shown.is_empty() {
                    "no call of this run completed, so evidence is an empty list".to_owned()
                } else {
                    format!(
                        "cite only {}, or none",
                        listed(shown.iter().map(String::as_str))
                    )
                };
                return Err(format!(
                    "{who}: evidence cites a tool call that is not a completed call of this \
                     run; {may}"
                ));
            }
        }
    }
    Ok(())
}

/// An answer read, or why not, in words that quote nothing of it.
fn read(value: &Json, who: &str) -> Result<Answer, String> {
    serde_json::from_value(value.clone()).map_err(|_| {
        format!(
            "{who}: its disposition, confidence, reason, allowed_by or location is not of \
             the schema"
        )
    })
}

/// The verdict a submission states, or why it states none, in words that quote nothing
/// the judge wrote but the ids it was given.
pub fn decided_of(
    arguments: &Json,
    job: &Job,
    resolved: &Resolved,
    checklist: &Checklist,
) -> Result<Decided, String> {
    let submission: Submission = serde_json::from_value(arguments.clone()).map_err(|_| {
        let text = ["items", "findings"]
            .into_iter()
            .filter(|key| arguments.get(*key).is_some_and(Json::is_string))
            .collect::<Vec<_>>();
        match text.as_slice() {
            [] => "the submission is not an object of items and findings".to_owned(),
            keys => format!(
                "the submission is not an object of items and findings: {} is text; each \
                 is a list of objects, not a string that holds one",
                keys.join(" and ")
            ),
        }
    })?;
    let items = submission.items.ok_or("items is not a list")?;
    let mut said = Vec::new();
    let mut counted = Vec::new();
    for value in &items {
        let named = value.get("id").and_then(Json::as_str).unwrap_or_default();
        let Some(entry) = checklist.get(named) else {
            return Err("an item is not on the checklist".to_owned());
        };
        let id = entry.id.clone();
        let answer = read(value, &id)?;
        counted.push(id.clone());
        if answer.reason.trim().is_empty() {
            return Err(format!("{id}: the reason is empty"));
        }
        let placed = match &answer.location {
            Some(location) => Some(place(location, job).map_err(|e| format!("{id}: {e}"))?),
            None => None,
        };
        match answer.disposition {
            Disposition::Violation if placed.is_none() => {
                return Err(format!("{id}: a violation has a location"));
            }
            Disposition::Observed => {
                return Err(format!(
                    "{id}: the disposition is not one of violation, held, allowed_change, \
                     hidden, cant_tell"
                ));
            }
            Disposition::AllowedChange => {
                let by = answer.allowed_by.as_deref();
                let by =
                    by.ok_or_else(|| format!("{id}: an allowed change names its allowed_by"))?;
                applies(by, &entry.instances, resolved, &id)?;
            }
            _ if answer.allowed_by.is_some() => {
                return Err(format!(
                    "{id}: allowed_by is only valid with disposition allowed_change"
                ));
            }
            _ => {}
        }
        let rows = resolved
            .instances
            .iter()
            .filter(|i| entry.instances.contains(&i.id()))
            .map(|i| row(i, answer.disposition, answer.confidence));
        let row = rows.max().unwrap_or(Row::Recorded);
        said.push(Said {
            named: id.clone(),
            disposition: answer.disposition,
            confidence: answer.confidence,
            reason: answer.reason.trim().to_owned(),
            row,
            kept: json!({"id": id, "location": placed.as_ref().map(|p| &p.kept),
                         "clamped": placed.is_some_and(|p| p.clamped)}),
        });
    }
    for id in checklist.ids() {
        let n = counted.iter().filter(|c| *c == id).count();
        if n == 0 {
            return Err(format!("{id} has no disposition"));
        }
        if n > 1 {
            return Err(format!("{id} has more than one disposition"));
        }
    }
    let findings = submission.findings.unwrap_or_default();
    for (at, value) in findings.iter().enumerate() {
        let tag = format!("finding {at}");
        let answer = read(value, &tag)?;
        if answer.reason.trim().is_empty() {
            return Err(format!("{tag}: the reason is empty"));
        }
        let ids = answer.obligation_ids.clone().unwrap_or_default();
        if ids.is_empty() {
            return Err(format!(
                "{tag}: cites no obligation: obligation_ids lists the rules it bears on, of \
                 {}",
                citable(resolved)
            ));
        }
        let location = answer
            .location
            .as_ref()
            .ok_or(format!("{tag}: has no location"))?;
        let placed = place(location, job).map_err(|e| format!("{tag}: {e}"))?;
        let mut mandatory = Vec::new();
        let mut observed = 0;
        for id in &ids {
            let on = resolved.instances.iter().filter(|i| &i.obligation_id == id);
            let on: Vec<_> = on.collect();
            if !on.is_empty() {
                match on.iter().find(|i| i.target == placed.image) {
                    Some(inst) => mandatory.push(inst.id()),
                    None => {
                        return Err(format!(
                            "{tag}: {id} does not judge image {:?} where the finding is located",
                            placed.image
                        ));
                    }
                }
            } else if let Some(ob) = resolved.observations.iter().find(|o| &o.id == id) {
                if !ob.targets.contains(&placed.image) {
                    return Err(format!(
                        "{tag}: observation {id} is not on image {}",
                        placed.image
                    ));
                }
                observed += 1;
            } else if let Some(ex) = resolved.exemptions.iter().find(|e| &e.id == id) {
                let then = if ex.waives.is_empty() {
                    format!("; {id} waives nothing, so it is never an allowed_by")
                } else {
                    format!(
                        " ({id} waives {}), and name {id} as allowed_by",
                        ex.waives.join(", ")
                    )
                };
                return Err(format!(
                    "{tag}: {id} is an exemption; cite the obligation the change bears \
                     on{then}"
                ));
            } else {
                return Err(format!(
                    "{tag}: its obligation_ids are not rules of the list"
                ));
            }
        }
        match answer.disposition {
            Disposition::Observed if !mandatory.is_empty() || observed == 0 => {
                return Err(format!(
                    "{tag}: observed findings cite observe rules only ({}); a finding on an \
                     obligation is a violation, cant_tell or allowed_change",
                    listed(resolved.observations.iter().map(|o| o.id.as_str()))
                ));
            }
            Disposition::Violation | Disposition::CantTell if mandatory.is_empty() => {
                return Err(format!("{tag}: it must cite a mandatory obligation"));
            }
            Disposition::AllowedChange => {
                let by = answer.allowed_by.as_deref();
                let by =
                    by.ok_or_else(|| format!("{tag}: an allowed change names its allowed_by"))?;
                applies(by, &mandatory, resolved, &tag)?;
            }
            Disposition::Held | Disposition::Hidden => {
                return Err(format!(
                    "{tag}: the disposition is not one of violation, allowed_change, cant_tell, \
                     observed"
                ));
            }
            _ if answer.allowed_by.is_some() => {
                return Err(format!(
                    "{tag}: allowed_by is only valid with allowed_change"
                ));
            }
            _ => {}
        }
        let rows = resolved
            .instances
            .iter()
            .filter(|i| mandatory.contains(&i.id()))
            .map(|i| row(i, answer.disposition, answer.confidence));
        said.push(Said {
            named: format!("{tag} ({})", ids.join(", ")),
            disposition: answer.disposition,
            confidence: answer.confidence,
            reason: answer.reason.trim().to_owned(),
            row: rows.max().unwrap_or(Row::Recorded),
            kept: json!({"obligation_ids": ids, "location": placed.kept,
                         "clamped": placed.clamped}),
        });
    }
    let strictest = said.iter().map(|s| s.row).max().unwrap_or(Row::Recorded);
    let decision = match strictest {
        Row::Fail => Decision::Fail,
        Row::Unsure => Decision::Unsure,
        Row::Recorded => Decision::Pass,
    };
    // The reasons of what decided it: each answer at the strictest row, or, for a pass,
    // each item of the checklist.
    let deciding: Vec<&Said> = said
        .iter()
        .filter(|s| {
            s.row == strictest && (strictest != Row::Recorded || checklist.get(&s.named).is_some())
        })
        .collect();
    let line = |s: &Said, reason: &str| {
        let (d, c) = (word(s.disposition), word(s.confidence));
        format!("{}: {d} ({c}): {reason}", s.named)
    };
    let reasons: Vec<String> = deciding
        .iter()
        .map(|s| line(s, &summary(&s.reason)))
        .collect();
    let answers = said
        .iter()
        .map(|s| {
            let mut kept = s.kept.clone();
            kept["disposition"] = json!(s.disposition);
            kept["confidence"] = json!(s.confidence);
            kept["row"] = json!(s.row);
            kept
        })
        .collect();
    Ok(Decided {
        verdict: Verdict::new(decision, &reasons.join("; ")),
        items: deciding.iter().map(|s| line(s, &s.reason)).collect(),
        answers,
    })
}
