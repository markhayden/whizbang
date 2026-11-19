//! A stored run of `check` written as a transcript (plan 2C, task E2, item 5; plan m3, task
//! B1, item 3): the form `docs/baseline/transcript.md` gives a judged decision, read from
//! what the run recorded, its record's judge group, each exchange it kept as a diagnostic
//! and what it decided (`judged.json`), so that `whizbang_eval::baseline::compare` reads
//! two stored runs, or a stored run beside a recorded decision, as it reads the replay's.
//!
//! What the store does not keep is written as the page's section "A stored run of `check`"
//! says: an image is named by its canonical pixel digest, and its prepared bytes and its
//! source file are given by that digest, since a run keeps pixels, not the bytes a backend
//! sent; a tool image's place on its parent is not in an exchange, and is null; and an
//! opaque part is its adapter, the digest of its body and its length, since a body that
//! cannot be redacted is never stored. The subject and the preparation are the caller's:
//! a run of `check` names neither a set's item nor a matrix's arm.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde_json::{Value as Json, json};
use whizbang_core::contract::record::{self, Status};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;

/// What `origin` says of a transcript written from a stored run.
pub const ORIGIN: &str = "stored";

/// The diagnostic of a run of `check` that holds what it decided.
const JUDGED: &str = "diagnostics/judged.json";

/// The transcript of the run `run` of the store at `root`, as the text of its file: keys
/// sorted, two spaces an indent, one line feed at the end.
pub fn of_run(
    root: &Path,
    run: &str,
    subject: &Json,
    preparation: &Json,
) -> Result<String, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let entries = generation
        .run(run)
        .ok_or_else(|| format!("the store holds no run {}", run.escape_debug()))?
        .entries();
    let read = |name: &str| entries.get(name).and_then(|entry| reader.read(entry).ok());
    let record = record::stored(&generation, run, |e| reader.read(e))
        .ok_or("the run has no record")?
        .map_err(|e| e.to_string())?;
    let written = written(&record, &read, subject, preparation)?;
    let text = serde_json::to_string_pretty(&written).map_err(|e| e.to_string())?;
    Ok(text + "\n")
}

/// The transcript of a run of `check` from its record and its names, each read by the
/// name the run's entries give it.
pub fn written(
    record: &record::RunRecord,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
    subject: &Json,
    preparation: &Json,
) -> Result<Json, String> {
    if record.asked().action.as_deref() != Some(whizbang_squint::check::NAME) {
        return Err("the run is not a run of check".to_owned());
    }
    let judged: Option<Json> = read(JUDGED).and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let mut missing: Vec<String> = Vec::new();
    // Each turn, with what it was sent and what came back, by its exchange.
    let mut exchanges = Vec::new();
    for entry in serde_json::to_value(record.judge())
        .map_err(|e| e.to_string())?
        .as_array()
        .into_iter()
        .flatten()
    {
        if entry["judge"] != "turn" {
            continue;
        }
        let kept = (entry["exchange"].as_str())
            .and_then(|name| read(name))
            .and_then(|bytes| serde_json::from_slice::<Json>(&bytes).ok());
        if kept.is_none() {
            missing.push(format!("the exchange of turn {}", exchanges.len() + 1));
        }
        exchanges.push((entry.clone(), kept));
    }
    if exchanges.is_empty() {
        return Err("the run sent its judge nothing".to_owned());
    }
    let submission = "submit_verdict";
    let first_tools = (exchanges[0].1.as_ref())
        .map(|e| e["tools"].clone())
        .unwrap_or(Json::Null);
    let names: Vec<&str> = (first_tools.as_array().into_iter().flatten())
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    let offered = names.contains(&submission);
    let mut images = Images::new(judged.as_ref());
    let mut turns = Vec::new();
    let mut repairs = Vec::new();
    let last = exchanges.len() - 1;
    let terminal = terminal_of(record);
    for (at, (entry, kept)) in exchanges.iter().enumerate() {
        let excluded = entry["excluded"]
            .as_str()
            .map(|reason| json!({"reason": reason}));
        let Some(exchange) = kept else {
            turns.push(json!({"turn": at + 1, "request": null, "response": null,
                              "excluded": excluded, "calls": [], "meta": {}}));
            continue;
        };
        let request = json!({
            "system": exchange["system"],
            "history": (exchange["messages"].as_array().into_iter().flatten())
                .map(|message| json!({"role": message["role"],
                                      "content": blocks(&message["parts"], &mut images, None)}))
                .collect::<Vec<_>>(),
            "tools": (exchange["tools"].as_array().into_iter().flatten())
                .map(|t| json!({"name": t["name"], "description": t["description"],
                                "input_schema": t["schema"]}))
                .collect::<Vec<_>>(),
            "max_output_tokens": exchange["max_output_tokens"],
        });
        let turn = &exchange["answer"]["turn"];
        let response = if turn.is_object() {
            let parts = &turn["parts"];
            let content = blocks(parts, &mut images, None);
            let text: Vec<&Json> = (parts.as_array().into_iter().flatten())
                .filter_map(|p| p.get("text"))
                .collect();
            let calls: Vec<Json> = (parts.as_array().into_iter().flatten())
                .filter_map(|p| p.get("tool_call"))
                .map(|c| json!({"id": c["id"], "name": c["name"], "arguments": c["arguments"]}))
                .collect();
            let stop = match &turn["stop"] {
                Json::String(word) => json!(word),
                Json::Object(other) => {
                    json!({"other": other.get("other").cloned().unwrap_or(Json::Null)})
                }
                _ => Json::Null,
            };
            json!({"content": content, "text": text, "tool_calls": calls, "stop_reason": stop})
        } else {
            if exchange["answer"].get("turn_withheld").is_some() {
                missing.push(format!("the answer of turn {}", at + 1));
            }
            Json::Null
        };
        // Each call: what the next request answered it with, or the end of the run.
        let next = (exchanges.get(at + 1)).and_then(|(_, kept)| kept.as_ref());
        let answered = next.map(|n| results_in(&n["messages"])).unwrap_or_default();
        let mut calls = Vec::new();
        for call in (response["tool_calls"].as_array().into_iter().flatten()).cloned() {
            let id = call["id"].as_str().unwrap_or_default().to_owned();
            let (disposition, result) = if excluded.is_some() {
                (json!("excluded"), Json::Null)
            } else if at == last && call["name"] == submission && terminal["kind"] != "cancelled" {
                (json!("final"), Json::Null)
            } else if let Some(result) = answered.get(&id) {
                let error = result["error"].as_bool().unwrap_or(false);
                let shown = blocks(&json!([{"tool_result": result}]), &mut images, Some(&id));
                let shown = &shown[0];
                let result = json!({"content": result["text"], "is_error": error,
                                    "images": shown["images"]});
                (json!(if error { "rejected" } else { "executed" }), result)
            } else {
                missing.push(format!("the result of the call {id}"));
                (Json::Null, Json::Null)
            };
            calls.push(
                json!({"id": call["id"], "name": call["name"], "arguments": call["arguments"],
                              "disposition": disposition, "result": result, "meta": {}}),
            );
        }
        // A repair: a turn left out and asked for again, a submission answered with why it
        // is not one, or a turn that ended with no call and was asked again.
        if at < last {
            let rejected = (answered.values()).any(|r| {
                r["error"] == true
                    && r["text"]
                        .as_str()
                        .is_some_and(|t| t.starts_with("Submission rejected"))
            });
            let repair = match (&excluded, rejected) {
                (Some(why), _) => Some(("reissue", why["reason"].clone())),
                (None, true) => Some(("rejected", json!("invalid_submission"))),
                (None, false) if calls.is_empty() => Some(("reissue", json!("no_call"))),
                _ => None,
            };
            if let Some((form, reason)) = repair {
                repairs.push(json!({"after_turn": at + 1, "form": form, "reason": reason}));
            }
        }
        turns.push(
            json!({"turn": at + 1, "request": request, "response": response,
                          "excluded": excluded, "calls": calls,
                          "meta": {"request": entry["request"], "usage": entry["usage"],
                                   "provider_id": entry["provider_id"]}}),
        );
    }
    missing.extend(images.missing.iter().cloned());
    let files: Vec<Json> = (record.diagnostics().iter())
        .map(|d| {
            json!({"role": "diagnostic", "path": d.name(), "sha256": d.digest().to_string(),
                        "bytes": d.bytes(), "truncated": false})
        })
        .collect();
    Ok(json!({
        "transcript_version": 1,
        "origin": ORIGIN,
        "complete": missing.is_empty(),
        "incomplete": missing,
        "subject": subject,
        "preparation": preparation,
        "evidence_tools": names.iter().filter(|n| **n != submission).collect::<Vec<_>>(),
        "submission_tool": if offered { json!(submission) } else { Json::Null },
        "images": images.list,
        "turns": turns,
        "repairs": repairs,
        "terminal": terminal,
        "volatile": ["/origin", "/meta", "/files", "/turns/*/meta", "/turns/*/calls/*/meta"],
        "files": files,
        "meta": {"run": record.identity().run, "writer": env!("CARGO_PKG_VERSION")},
    }))
}

/// How the run ended, as a transcript's `terminal`.
fn terminal_of(record: &record::RunRecord) -> Json {
    let outcome = record.outcome();
    match (outcome.status, record.verdict(), &outcome.error) {
        (Status::Completed, Some((decision, _)), _) => {
            let word = serde_json::to_value(decision).unwrap_or(Json::Null);
            json!({"kind": "verdict", "verdict": word})
        }
        (Status::Cancelled, _, _) => {
            json!({"kind": "cancelled", "class": "cancelled", "message": "the run was cancelled"})
        }
        (_, _, Some(error)) => {
            json!({"kind": "error", "class": error.kind, "message": error.message})
        }
        (_, _, None) => {
            let why = outcome.invalid.clone().unwrap_or_default();
            json!({"kind": "error", "class": "invalid", "message": why})
        }
    }
}

/// The tool results of a request's last message, by the call each answers.
fn results_in(messages: &Json) -> BTreeMap<String, Json> {
    let parts = (messages.as_array())
        .and_then(|list| list.last())
        .map(|last| last["parts"].clone())
        .unwrap_or(Json::Null);
    (parts.as_array().into_iter().flatten())
        .filter_map(|p| p.get("tool_result"))
        .filter_map(|r| Some((r["call"].as_str()?.to_owned(), r.clone())))
        .collect()
}

/// The images a transcript names, in the order they were first sent: the job's, then each
/// tool's, by the call whose result first showed it.
struct Images {
    list: Vec<Json>,
    seen: Vec<String>,
    /// The job's images as the run kept them, in the job's order: each one's name and role.
    roles: Vec<(String, String)>,
    missing: Vec<String>,
}

impl Images {
    fn new(judged: Option<&Json>) -> Images {
        let roles = (judged
            .and_then(|j| j["images"].as_array())
            .into_iter()
            .flatten())
        .filter_map(|image| {
            let name = image["id"].as_str()?.to_owned();
            Some((name, image["role"].as_str()?.to_owned()))
        })
        .collect();
        Images {
            list: Vec::new(),
            seen: Vec::new(),
            roles,
            missing: Vec::new(),
        }
    }

    /// The name of the image an exchange gives as its size and pixel digest, entered the
    /// first time it is met: as a job's image before any tool showed one, else as the image
    /// of the call `by`.
    fn named(&mut self, image: &Json, by: Option<&str>) -> String {
        let id = image["pixels"].as_str().unwrap_or_default().to_owned();
        if self.seen.contains(&id) {
            return id;
        }
        let at = self.seen.len();
        self.seen.push(id.clone());
        let common = json!({"id": id, "width": image["width"], "height": image["height"],
                            "prepared": {"pixels": id}});
        let mut entry = common;
        match by {
            None => {
                let role = self.roles.get(at).cloned();
                if role.is_none() {
                    self.missing.push(format!("the role of image {}", at + 1));
                }
                let (name, role) = role.unzip();
                entry["frame"] = json!(format!("raw:{}", name.as_deref().unwrap_or("image")));
                entry["origin"] = json!("job");
                entry["role"] = json!(role);
                entry["source"] = json!({"pixels": id});
            }
            Some(call) => {
                let n = (self.list.iter()).filter(|i| i["origin"] == call).count();
                entry["frame"] = json!(format!("crop:{call}:{n}"));
                entry["origin"] = json!(call);
                entry["parent"] = Json::Null;
                entry["root"] = Json::Null;
                entry["mapping"] = Json::Null;
            }
        }
        self.list.push(entry);
        id
    }
}

/// The content blocks of a list of parts as an exchange wrote them.
fn blocks(parts: &Json, images: &mut Images, by: Option<&str>) -> Vec<Json> {
    let mut out = Vec::new();
    for part in parts.as_array().into_iter().flatten() {
        let Some((kind, body)) = part.as_object().and_then(|o| o.iter().next()) else {
            continue;
        };
        let block = match kind.as_str() {
            "text" => json!({"type": "text", "text": body}),
            "image" => json!({"type": "image", "image_id": images.named(body, by)}),
            "tool_call" => json!({"type": "tool_use", "id": body["id"], "name": body["name"],
                                  "arguments": body["arguments"]}),
            "tool_result" => {
                let call = body["call"].as_str().unwrap_or_default().to_owned();
                let shown: Vec<String> = (body["images"].as_array().into_iter().flatten())
                    .map(|image| images.named(image, Some(&call)))
                    .collect();
                json!({"type": "tool_result", "tool_use_id": call, "content": body["text"],
                       "is_error": body["error"], "images": shown})
            }
            "opaque" => {
                let text = body["body"].as_str().unwrap_or_default();
                let block = serde_json::from_str(text).unwrap_or_else(|_| json!(text));
                json!({"type": "opaque", "adapter": body["adapter"], "block": block})
            }
            "opaque_withheld" => {
                let held = json!({"digest": body["digest"], "bytes": body["bytes"]});
                json!({"type": "opaque", "adapter": body["adapter"], "block": held})
            }
            other => json!({"type": other}),
        };
        out.push(block);
    }
    out
}
