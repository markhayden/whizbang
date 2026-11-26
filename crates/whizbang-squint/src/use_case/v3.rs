//! Submission v3 (plan mE, task F1; ADR 0042, "Amended: submission v3"): v1's list of items,
//! each with its checklist id, with a finding on an item written inside that item's answer,
//! so that the obligation it cites is the item's own. Beside v1 and v2, which stay exactly as
//! they are; v1 stays the default.
//!
//! Under v2, `claude-haiku-5-5`'s first rejections were mostly `finding_cites` (77 of 93 in
//! the format round, `docs/results/e-submission.md`): a finding's `obligation_ids` naming
//! none, one not of the list, an exemption, or no mandatory one. v3 takes the citation out of
//! the judge's hands.
//!
//! - **A finding on an item** is in the item's `findings`, and cites nothing: it bears on the
//!   item's instances. An item of one instance names it; an item of more than one (a
//!   reference's, an evidence item's) lets a finding list which of them it bears on, from an
//!   enum the schema writes for that item ([`submit_tool_v3`]), none listed meaning all.
//! - **An observed finding** is v1's, in the separate list `findings`, citing observe rules.
//! - **v2's one repair and its recoveries** are kept ([`super::v2`]), but `array_form`, which
//!   is v3's own form.
//!
//! What v3 reads is read back to v1's form ([`read_back`]): each finding inside an item
//! becomes a finding of v1 whose `obligation_ids` are its instances' obligations, after the
//! item's own; then the observed findings. That submission is decided by v1's rules,
//! unchanged (`decided_with`), so v3 gives no verdict v1 could not, and every check v1 makes
//! of a finding (its disposition, its location against what it cites, an exemption) is made
//! of it. What nesting makes impossible is a citation missing or not of the list; a finding
//! that lists an instance its item does not hold is refused, as `item_findings`.

use serde_json::{Value as Json, json};
use whizbang_core::capability::ToolDefinition;

use super::submission::{Class, Rejection, cited_calls_held, decided_with};
use super::v2::{Form, Given, Submitted, normalised, one};
use super::{Checklist, Resolved, SUBMIT};
use crate::job::Job;

/// v3's tool, before its checklist's ids are written into it.
const SUBMIT_TOOL_V3: &str = include_str!("submit_verdict_v3.json");

/// The items of a checklist by what a finding inside one may name: every item of one
/// instance together, first, then each item of more than one on its own, with its
/// instances; in the checklist's order.
fn groups(checklist: &Checklist) -> Vec<(Vec<String>, Option<Vec<String>>)> {
    let mut single = Vec::new();
    let mut several = Vec::new();
    for item in &checklist.items {
        if item.instances.len() > 1 {
            several.push((vec![item.id.clone()], Some(item.instances.clone())));
        } else {
            single.push(item.id.clone());
        }
    }
    let mut groups = Vec::new();
    if !single.is_empty() {
        groups.push((single, None));
    }
    groups.extend(several);
    groups
}

/// The tool the judge answers with under v3, for this checklist: `items` a list whose
/// entries are of one form for the items of one instance, their `id` one of those ids, and
/// of one form for each item of several, its `id` that item's and its findings' `instances`
/// an enum of its instances. One form alone is given as it is, more as `anyOf`.
pub fn submit_tool_v3(checklist: &Checklist) -> Option<ToolDefinition> {
    let mut tool: Json = serde_json::from_str(SUBMIT_TOOL_V3).ok()?;
    let item = tool.pointer("/input_schema/$defs/Item")?.clone();
    let finding = tool.pointer("/input_schema/$defs/ItemFinding")?.clone();
    let mut forms = Vec::new();
    for (ids, instances) in groups(checklist) {
        let mut form = item.clone();
        form["properties"]["id"] = json!({
            "description": "The checklist item id exactly as listed, for example `chk:ob:SF1@candidate`.",
            "enum": ids,
            "type": "string",
        });
        let mut finding = finding.clone();
        if let Some(instances) = instances {
            finding["properties"]["instances"] = json!({
                "description": "Which of this item's instances the finding bears on, by id; an empty list means all of them.",
                "items": {"enum": instances, "type": "string"},
                "type": "array",
            });
            let required = finding["required"].as_array_mut()?;
            required.push(json!("instances"));
            required.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        }
        form["properties"]["findings"]["items"] = finding;
        forms.push(form);
    }
    let items = tool.pointer_mut("/input_schema/properties/items")?;
    items["items"] = match forms.len() {
        1 => forms.remove(0),
        _ => json!({"anyOf": forms}),
    };
    let defs = tool
        .pointer_mut("/input_schema/$defs")
        .and_then(Json::as_object_mut)?;
    defs.remove("Item");
    defs.remove("ItemFinding");
    let description = tool.get("description")?.as_str()?.to_owned();
    ToolDefinition::new(SUBMIT, &description, tool.get("input_schema")?.clone()).ok()
}

/// What a submission of v3 decides, or every problem it has in one rejection, its class the
/// first problem's. `shown` and `also` are what a submission may cite, as
/// [`cited_calls_held`] takes them.
pub fn decided_of_v3(
    call: &Given<'_>,
    job: &Job,
    resolved: &Resolved,
    checklist: &Checklist,
    shown: &[String],
    also: &[String],
) -> Result<Submitted, Rejection> {
    let mut recoveries = Vec::new();
    let (read, twice) = normalised(
        Form::V3,
        call.arguments,
        call.repeated,
        checklist,
        shown,
        also,
        &mut recoveries,
    );
    let back = read_back(&read, checklist, resolved);
    let mut found = Vec::new();
    if let Err(rejection) = cited_calls_held(&back.arguments, shown, also, checklist) {
        found.push(rejection);
    }
    found.extend(back.problems);
    found.extend(twice.into_iter().map(|id| {
        let said = format!("{id} has more than one disposition");
        Rejection::new(Class::DuplicateItem, said)
    }));
    let decided = decided_with(&back.arguments, job, resolved, checklist, &back.named, true);
    match decided {
        Ok(decided) if found.is_empty() => Ok(Submitted {
            decided,
            arguments: back.arguments,
            recoveries,
        }),
        Ok(_) => Err(one(found)),
        Err(more) => {
            found.extend(more);
            Err(one(found))
        }
    }
}

/// A submission of v3 read back to v1's form: the submission, each finding's name in the
/// order of its `findings`, as a rejection names it, and what reading it back refused.
pub struct ReadBack {
    pub arguments: Json,
    pub named: Vec<String>,
    pub problems: Vec<Rejection>,
}

/// The exemption an item's id names when it is no item of the checklist: the id itself, or
/// the obligation of `chk:ob:<obligation>@<target>`.
fn exemption_named<'a>(id: &str, resolved: &'a Resolved) -> Option<&'a str> {
    let plain = id.strip_prefix("chk:ob:").unwrap_or(id);
    let plain = plain
        .split_once('@')
        .map_or(plain, |(obligation, _)| obligation);
    (resolved.exemptions.iter())
        .find(|e| e.id == plain)
        .map(|e| e.id.as_str())
}

/// Reads a submission of v3 back to v1's form ([`ReadBack`]). Each item's `findings` are
/// taken out of it; each finding in them becomes one of v1's whose `obligation_ids` are the
/// obligations of the instances it bears on, each once, in the order of the item's
/// instances, and whose other fields are its own; the observed findings follow, as given.
/// A finding of an item that is no item of the checklist cites the exemption the item's id
/// names, so v1 refuses it as citing one; of any other such item, it is left out, since the
/// item is refused. What is no object is left for v1 to refuse.
pub fn read_back(read: &Json, checklist: &Checklist, resolved: &Resolved) -> ReadBack {
    let mut back = ReadBack {
        arguments: read.clone(),
        named: Vec::new(),
        problems: Vec::new(),
    };
    let Some(Json::Array(items)) = read.get("items") else {
        return back;
    };
    let mut kept_items = Vec::new();
    let mut findings: Vec<Json> = Vec::new();
    for item in items {
        let Json::Object(fields) = item else {
            kept_items.push(item.clone());
            continue;
        };
        let mut fields = fields.clone();
        let inside = fields.remove("findings");
        kept_items.push(Json::Object(fields.clone()));
        let id = fields.get("id").and_then(Json::as_str).unwrap_or_default();
        let entry = checklist.get(id);
        let who = entry
            .map_or("an item", |entry| entry.id.as_str())
            .to_owned();
        let inside = match inside {
            None | Some(Json::Null) => continue,
            Some(Json::Array(inside)) => inside,
            Some(_) => {
                back.problems.push(Rejection::new(
                    Class::ItemFindings,
                    format!("{who}: findings is a list of the findings on this item"),
                ));
                continue;
            }
        };
        for (at, finding) in inside.into_iter().enumerate() {
            let tag = format!("{who} finding {at}");
            let Json::Object(mut finding) = finding else {
                findings.push(finding);
                back.named.push(tag);
                continue;
            };
            let listed = finding.remove("instances");
            let cited: Vec<String> = match entry {
                Some(entry) => {
                    match instances_of(listed.as_ref(), &entry.instances, resolved, &tag) {
                        Ok(cited) => cited,
                        Err(rejection) => {
                            back.problems.push(rejection);
                            continue;
                        }
                    }
                }
                None => match exemption_named(id, resolved) {
                    Some(exemption) => vec![exemption.to_owned()],
                    None => continue,
                },
            };
            let mut obligations: Vec<String> = Vec::new();
            for id in cited {
                if !obligations.contains(&id) {
                    obligations.push(id);
                }
            }
            finding.insert("obligation_ids".to_owned(), json!(obligations));
            findings.push(Json::Object(finding));
            back.named.push(tag);
        }
    }
    if let Some(Json::Array(observed)) = read.get("findings") {
        for (at, finding) in observed.iter().enumerate() {
            findings.push(finding.clone());
            back.named.push(format!("finding {at}"));
        }
    } else if let Some(other) = read.get("findings") {
        // Not a list: left for v1 to refuse, with nothing read back beside it.
        back.arguments["items"] = Json::Array(kept_items);
        back.arguments["findings"] = other.clone();
        return back;
    }
    back.arguments["items"] = Json::Array(kept_items);
    back.arguments["findings"] = Json::Array(findings);
    back
}

/// The obligations a finding inside an item bears on: those of the instances it lists, each
/// of the item's (`instances`), or of all the item's when it lists none; or why not, naming
/// only the ids the judge was given. An instance of an exemption is cited as the exemption,
/// so v1 refuses it in its words.
fn instances_of(
    listed: Option<&Json>,
    instances: &[String],
    resolved: &Resolved,
    tag: &str,
) -> Result<Vec<String>, Rejection> {
    let obligation = |id: &str| resolved.instance(id).map(|i| i.obligation_id.clone());
    let all = || instances.iter().filter_map(|id| obligation(id)).collect();
    let named: Vec<&Json> = match listed {
        None | Some(Json::Null) => return Ok(all()),
        Some(Json::Array(named)) if named.is_empty() => return Ok(all()),
        Some(Json::Array(named)) => named.iter().collect(),
        Some(_) => {
            return Err(Rejection::new(
                Class::ItemFindings,
                format!("{tag}: instances is a list of this item's instances"),
            ));
        }
    };
    let mut cited = Vec::new();
    for one in named {
        let one = one.as_str().unwrap_or_default();
        if instances.iter().any(|i| i == one) {
            cited.extend(obligation(one));
        } else if let Some(exemption) = exemption_named(one, resolved) {
            cited.push(exemption.to_owned());
        } else {
            return Err(Rejection::new(
                Class::ItemFindings,
                format!(
                    "{tag}: instances names one that is not this item's; list some of {}, or none for all of them",
                    instances.join(", ")
                ),
            ));
        }
    }
    Ok(cited)
}

/// The keys of an answer two copies of one item must share to be read as one, under v3:
/// v2's, and its findings.
pub(super) const SAME_V3: [&str; 6] = [
    "id",
    "disposition",
    "confidence",
    "location",
    "allowed_by",
    "findings",
];
