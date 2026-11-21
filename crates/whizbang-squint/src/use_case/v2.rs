//! Submission v2 (plan e1, task F1; ADR 0042): the form of `submit_verdict` made easier
//! for a small model to follow, beside v1, which stays exactly as it is and the default.
//!
//! - **A tool keyed by checklist id.** The judge is given `items` as an object whose
//!   required keys are the job's checklist ids, each value an item's answer without `id`
//!   ([`submit_tool_v2`]), so a missing, duplicate or unknown item cannot be written
//!   within the schema. `findings` is v1's.
//! - **Near-misses read, each recorded** ([`Recovery`]), and only where one reading is
//!   possible: a list given as a string that holds it; an enum word differing in case,
//!   surrounding space, or a space or hyphen for an underscore (and `can't tell` or
//!   `cannot_tell` for `cant_tell`); a checklist id differing only in case or surrounding
//!   space when exactly one id matches; two answers of one item identical in disposition,
//!   confidence, location and `allowed_by`, read as the first; evidence that cites a call
//!   that did not complete, or is null, dropped (evidence does not enter the verdict); and
//!   the submission wrapped once in an object whose only key is `submission` or `verdict`.
//!   The answers of v1's array form are read too, and recorded as such. A recovered answer
//!   is accepted under v2; nothing claims it is the verdict a v1 repair would have reached.
//! - **Every problem at once.** What is left is held to v1's rules unchanged, and every
//!   problem is named in the one repair, at most [`PROBLEMS_SHOWN`] of them, then a count.
//!
//! What v2 reads is turned into v1's array form ([`Submitted::arguments`]), which the
//! second look, `merged` and the strict rule read as they always have.

use serde_json::{Map, Value as Json};
use whizbang_core::capability::ToolDefinition;

use super::submission::{Class, Decided, Rejection, cited_calls_held, decided_with};
use super::{Checklist, Resolved, SUBMIT};
use crate::job::Job;

/// The values of the setting `submission`, and its default.
pub const FORMATS: [&str; 2] = ["v1", "v2"];
pub const DEFAULT_FORMAT: &str = "v1";

/// The most problems one repair names; the rest are counted.
pub const PROBLEMS_SHOWN: usize = 12;

/// v2's tool, before its checklist's ids are written into it.
const SUBMIT_TOOL_V2: &str = include_str!("submit_verdict_v2.json");

/// The most characters of a key of v2's `items`, and the characters it may hold: what
/// every provider takes as a property's name (Anthropic holds them to
/// `^[a-zA-Z0-9_.-]{1,64}$`).
pub const KEY_LIMIT: usize = 64;

/// Whether a character may be in a key of v2's `items`.
fn safe(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'
}

/// The key of each checklist item in v2's `items`, in the checklist's order, with its id:
/// the id with every character a provider's property name cannot hold made `_` and cut
/// to [`KEY_LIMIT`]; one that would repeat an earlier key, or comes out empty, is `item_`
/// and its place from 1 instead, made longer until it is unique. One to one, so a key
/// names one item and an item has one key.
pub fn item_keys(checklist: &Checklist) -> Vec<(String, String)> {
    let mut keys: Vec<(String, String)> = Vec::new();
    for (at, id) in checklist.ids().enumerate() {
        let mut key: String = id.chars().map(|c| if safe(c) { c } else { '_' }).collect();
        key.truncate(KEY_LIMIT);
        let mut n = at + 1;
        while key.is_empty() || keys.iter().any(|(k, _)| *k == key) {
            key = format!("item_{n}");
            n += checklist.ids().count();
        }
        keys.push((key, id.to_owned()));
    }
    keys
}

/// The tool the judge answers with under v2, for this checklist: `items` keyed by each
/// item's key ([`item_keys`]), in its order, every one required, no other allowed; each
/// value an item's answer without its id, whose description names the item's id.
pub fn submit_tool_v2(checklist: &Checklist) -> Option<ToolDefinition> {
    let mut tool: Json = serde_json::from_str(SUBMIT_TOOL_V2).ok()?;
    let answer = tool.pointer("/input_schema/$defs/ItemAnswer")?.clone();
    let items = tool.pointer_mut("/input_schema/properties/items")?;
    let mut properties = Map::new();
    let mut required = Vec::new();
    for (key, id) in item_keys(checklist) {
        let mut answer = answer.clone();
        answer["description"] = Json::String(format!(
            "The answer for the checklist item {id}, as the checklist lists it."
        ));
        properties.insert(key.clone(), answer);
        required.push(Json::String(key));
    }
    items["properties"] = Json::Object(properties);
    items["required"] = Json::Array(required);
    if let Some(defs) = tool
        .pointer_mut("/input_schema/$defs")
        .and_then(Json::as_object_mut)
    {
        defs.remove("ItemAnswer");
    }
    let description = tool.get("description")?.as_str()?.to_owned();
    ToolDefinition::new(SUBMIT, &description, tool.get("input_schema")?.clone()).ok()
}

/// A near-miss v2 read, by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// `items` or `findings` given as a string that holds the list (or, for `items`, the
    /// keyed object).
    ListAsText,
    /// `items` given as v1's list of answers, each with its id, not keyed.
    ArrayForm,
    /// An enum word read by case, surrounding space, or a space or hyphen.
    EnumWord,
    /// A checklist id read by case or surrounding space, exactly one id matching.
    IdCase,
    /// A second answer of one item identical to the first, read as one.
    IdenticalDuplicate,
    /// Evidence citing a call that did not complete, or null, dropped.
    EvidenceDropped,
    /// The submission unwrapped from an object whose only key is `submission` or
    /// `verdict`.
    Wrapper,
}

impl Recovery {
    /// Every kind, in this order.
    pub const ALL: [Recovery; 7] = [
        Recovery::ListAsText,
        Recovery::ArrayForm,
        Recovery::EnumWord,
        Recovery::IdCase,
        Recovery::IdenticalDuplicate,
        Recovery::EvidenceDropped,
        Recovery::Wrapper,
    ];

    /// The kind as it is recorded.
    pub fn name(self) -> &'static str {
        match self {
            Recovery::ListAsText => "list_as_text",
            Recovery::ArrayForm => "array_form",
            Recovery::EnumWord => "enum_word",
            Recovery::IdCase => "id_case",
            Recovery::IdenticalDuplicate => "identical_duplicate",
            Recovery::EvidenceDropped => "evidence_dropped",
            Recovery::Wrapper => "wrapper",
        }
    }
}

/// What a submission of v2 decided: the verdict, the submission as v1's array form, and
/// each near-miss read on the way, in order.
#[derive(Debug, Clone)]
pub struct Submitted {
    pub decided: Decided,
    pub arguments: Json,
    pub recoveries: Vec<Recovery>,
}

/// What a call of `submit_verdict` under v2 gave: its arguments, and the JSON pointers
/// of every key its adapter found written more than once in them as sent
/// (`ToolCall::repeated_keys`), which the parsed arguments no longer show.
pub struct Given<'a> {
    pub arguments: &'a Json,
    pub repeated: &'a [String],
}

/// What a submission of v2 decides, or every problem it has in one rejection, its class
/// the first problem's. `shown` and `also` are what a submission may cite, as
/// [`cited_calls_held`] takes them.
pub fn decided_of_v2(
    call: &Given<'_>,
    job: &Job,
    resolved: &Resolved,
    checklist: &Checklist,
    shown: &[String],
    also: &[String],
) -> Result<Submitted, Rejection> {
    let mut recoveries = Vec::new();
    let (read, twice) = normalised(
        call.arguments,
        call.repeated,
        checklist,
        shown,
        also,
        &mut recoveries,
    );
    let mut found = Vec::new();
    if let Err(rejection) = cited_calls_held(&read, shown, also, checklist) {
        found.push(rejection);
    }
    // An item whose key was written twice: v1's words for an item given twice.
    let twice = twice.into_iter().map(|id| {
        let said = format!("{id} has more than one disposition");
        Rejection::new(Class::DuplicateItem, said)
    });
    let decided = decided_with(&read, job, resolved, checklist, true);
    found.extend(twice);
    match decided {
        Ok(decided) if found.is_empty() => Ok(Submitted {
            decided,
            arguments: read,
            recoveries,
        }),
        Ok(_) => Err(one(found)),
        Err(more) => {
            found.extend(more);
            Err(one(found))
        }
    }
}

/// Every problem as one rejection: the first's words alone, or how many and each, at most
/// [`PROBLEMS_SHOWN`], then how many more.
fn one(found: Vec<Rejection>) -> Rejection {
    let mut found = found.into_iter();
    let Some(first) = found.next() else {
        return Rejection::new(Class::NotAList, "the submission is not a verdict");
    };
    let rest: Vec<Rejection> = found.collect();
    if rest.is_empty() {
        return first;
    }
    let count = rest.len() + 1;
    let shown: Vec<&str> = std::iter::once(&first)
        .chain(&rest)
        .take(PROBLEMS_SHOWN)
        .map(|r| r.message.as_str())
        .collect();
    let mut message = format!("{count} problems: {}", shown.join("; "));
    if count > PROBLEMS_SHOWN {
        message.push_str(&format!("; and {} more", count - PROBLEMS_SHOWN));
    }
    let mut rejection = Rejection::new(first.class, message);
    rejection.also = rest.iter().map(|r| r.class).collect();
    rejection
}

/// The enum words an answer's fields take.
const DISPOSITIONS: [&str; 6] = [
    "violation",
    "held",
    "allowed_change",
    "hidden",
    "cant_tell",
    "observed",
];
const CONFIDENCES: [&str; 2] = ["confident", "tentative"];
const KINDS: [&str; 4] = ["bbox", "polygon", "whole_image", "absent"];

/// A word as v2 reads it: trimmed, in lower case, a space or hyphen as an underscore, and
/// the ways of writing `cant_tell`; when that is one of `words` and the word was not.
fn word_read(said: &str, words: &[&str]) -> Option<String> {
    if words.contains(&said) {
        return None;
    }
    let plain: String = (said.trim().to_lowercase().chars())
        .map(|c| if c == ' ' || c == '-' { '_' } else { c })
        .collect();
    let plain = match plain.as_str() {
        "can't_tell" | "cannot_tell" | "can_not_tell" => "cant_tell".to_owned(),
        _ => plain,
    };
    words.contains(&plain.as_str()).then_some(plain)
}

/// Reads one enum field of an object by [`word_read`].
fn enum_field(value: &mut Json, key: &str, words: &[&str], recoveries: &mut Vec<Recovery>) {
    let Some(said) = value.get(key).and_then(Json::as_str) else {
        return;
    };
    if let Some(read) = word_read(said, words) {
        value[key] = Json::String(read);
        recoveries.push(Recovery::EnumWord);
    }
}

/// An answer's enum words: its disposition, its confidence and its location's kind.
fn enum_words(answer: &mut Json, recoveries: &mut Vec<Recovery>) {
    if !answer.is_object() {
        return;
    }
    enum_field(answer, "disposition", &DISPOSITIONS, recoveries);
    enum_field(answer, "confidence", &CONFIDENCES, recoveries);
    if let Some(location) = answer.get_mut("location")
        && location.is_object()
    {
        enum_field(location, "kind", &KINDS, recoveries);
    }
}

/// An answer's evidence with every id that is not of a completed call dropped, and null
/// read as none.
fn evidence(answer: &mut Json, shown: &[String], also: &[String], recoveries: &mut Vec<Recovery>) {
    let Some(cited) = answer.get_mut("evidence") else {
        return;
    };
    match cited {
        Json::Null => {
            *cited = Json::Array(Vec::new());
            recoveries.push(Recovery::EvidenceDropped);
        }
        Json::Array(ids) => {
            let before = ids.len();
            ids.retain(|id| {
                id.as_str()
                    .is_some_and(|id| shown.iter().chain(also).any(|c| c == id))
            });
            if ids.len() != before {
                recoveries.push(Recovery::EvidenceDropped);
            }
        }
        _ => {}
    }
}

/// The checklist's id this one names: itself when it is one, or the one id it matches by
/// case and surrounding space when exactly one does.
fn id_read(
    said: &str,
    keys: &[(String, String)],
    checklist: &Checklist,
    recoveries: &mut Vec<Recovery>,
) -> String {
    if let Some((_, id)) = keys.iter().find(|(key, _)| key == said) {
        return id.clone();
    }
    if checklist.get(said).is_some() {
        return said.to_owned();
    }
    // By case and surrounding space, against the keys and the ids alike: one item
    // matching, by either, is one reading.
    let plain = said.trim().to_lowercase();
    let mut matching: Vec<&str> = Vec::new();
    for (key, id) in keys {
        if (key.to_lowercase() == plain || id.to_lowercase() == plain)
            && !matching.contains(&id.as_str())
        {
            matching.push(id);
        }
    }
    match matching.as_slice() {
        [one] => {
            recoveries.push(Recovery::IdCase);
            (*one).to_owned()
        }
        _ => said.to_owned(),
    }
}

/// The key a JSON pointer names under `items`, unescaped: `/items/<key>`, or `/<key>`
/// when `at_items` is the pointer's root; none for a pointer to anything else.
fn item_key(pointer: &str, at_items: bool) -> Option<String> {
    let rest = if at_items {
        pointer.strip_prefix('/')?
    } else {
        pointer.strip_prefix("/items/")?
    };
    (!rest.contains('/')).then(|| rest.replace("~1", "/").replace("~0", "~"))
}

/// A list given as a string that holds one, read as it; or, where `object` is allowed, a
/// string that holds an object.
fn text_read(value: &mut Json, object: bool, recoveries: &mut Vec<Recovery>) {
    let Some(text) = value.as_str() else {
        return;
    };
    match serde_json::from_str::<Json>(text) {
        Ok(read @ Json::Array(_)) => *value = read,
        Ok(read @ Json::Object(_)) if object => *value = read,
        _ => return,
    }
    recoveries.push(Recovery::ListAsText);
}

/// The submission as v1's array form, every near-miss read and recorded.
/// The submission as v1's array form, every near-miss read and recorded; and the ids of
/// the checklist whose key the submission wrote more than once in `items`, which a JSON
/// object cannot hold and its reader collapses: from the pointers the judge's adapter
/// found repeated in the arguments as sent (`repeated`), and from `items` given as text.
fn normalised(
    arguments: &Json,
    repeated: &[String],
    checklist: &Checklist,
    shown: &[String],
    also: &[String],
    recoveries: &mut Vec<Recovery>,
) -> (Json, Vec<String>) {
    let keys = item_keys(checklist);
    let mut read = arguments.clone();
    let mut prefix = String::new();
    if let Some(object) = read.as_object()
        && object.len() == 1
        && let Some((key, inner)) = object.iter().next()
        && (key == "submission" || key == "verdict")
        && inner.is_object()
    {
        prefix = format!("/{key}");
        let inner = inner.clone();
        read = inner;
        recoveries.push(Recovery::Wrapper);
    }
    let mut twice: Vec<String> = (repeated.iter())
        .filter_map(|pointer| item_key(pointer.strip_prefix(prefix.as_str())?, false))
        .collect();
    if !read.is_object() {
        return (read, Vec::new());
    }
    if let Some(items) = read.get_mut("items") {
        if let Some(text) = items.as_str() {
            let found = whizbang_core::capability::repeated_keys(text);
            twice.extend(found.iter().filter_map(|pointer| item_key(pointer, true)));
        }
        text_read(items, true, recoveries);
    }
    let mut ids_twice: Vec<String> = Vec::new();
    for key in &twice {
        let id = id_read(key, &keys, checklist, &mut Vec::new());
        if checklist.get(&id).is_some() && !ids_twice.contains(&id) {
            ids_twice.push(id);
        }
    }
    let items = match read.get("items") {
        Some(Json::Object(keyed)) => {
            // In the order of the checklist, then any other key in the order given.
            let mut answers: Vec<(usize, Json)> = Vec::new();
            for (key, answer) in keyed {
                let id = id_read(key, &keys, checklist, recoveries);
                let at = checklist.ids().position(|c| c == id).unwrap_or(usize::MAX);
                // An answer that is not an object is still the item's answer: refused
                // for what it holds, in v1's words, and not as missing or unknown.
                let answer = match answer {
                    Json::Object(fields) => {
                        let mut fields = fields.clone();
                        fields.insert("id".to_owned(), Json::String(id));
                        Json::Object(fields)
                    }
                    _ => serde_json::json!({"id": id}),
                };
                answers.push((at, answer));
            }
            answers.sort_by_key(|(at, _)| *at);
            Some(answers.into_iter().map(|(_, a)| a).collect::<Vec<_>>())
        }
        Some(Json::Array(list)) => {
            recoveries.push(Recovery::ArrayForm);
            let mut answers = list.clone();
            for answer in &mut answers {
                if let Some(said) = answer.get("id").and_then(Json::as_str) {
                    let id = id_read(said, &keys, checklist, recoveries);
                    answer["id"] = Json::String(id);
                }
            }
            Some(answers)
        }
        _ => None,
    };
    if let Some(findings) = read.get_mut("findings") {
        text_read(findings, false, recoveries);
    }
    if let Some(mut answers) = items {
        for answer in &mut answers {
            enum_words(answer, recoveries);
        }
        // Two answers of one item that say the same, read as the first.
        let mut kept: Vec<Json> = Vec::new();
        for answer in answers {
            let same = |a: &Json, b: &Json| {
                ["id", "disposition", "confidence", "location", "allowed_by"]
                    .iter()
                    .all(|key| a.get(*key) == b.get(*key))
            };
            if answer.get("id").is_some() && kept.iter().any(|k| same(k, &answer)) {
                recoveries.push(Recovery::IdenticalDuplicate);
                continue;
            }
            kept.push(answer);
        }
        for answer in &mut kept {
            evidence(answer, shown, also, recoveries);
        }
        read["items"] = Json::Array(kept);
    }
    if let Some(Json::Array(findings)) = read.get_mut("findings") {
        for finding in findings.iter_mut() {
            enum_words(finding, recoveries);
            evidence(finding, shown, also, recoveries);
        }
    }
    (read, ids_twice)
}
