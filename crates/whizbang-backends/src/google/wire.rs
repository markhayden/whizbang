//! The `generateContent` wire (plan m2c, task P1, item 1): a [`JudgeRequest`] written as
//! the body of a Gemini `generateContent` request, in pieces, and the body of an answer
//! read back to a [`Turn`].
//!
//! Ported from squint's Gemini adapter (`crates/squint-providers/src/gemini/mod.rs`, and
//! of `wire.rs` the inlining of a schema's definitions, at the commit `docs/sources.toml`
//! pins), and reshaped as the Messages wire was: the body is written as pieces for the
//! gate (`crate::service::body`), and the controls squint read from its catalogue (a
//! forced function mode, the media resolution) are not written, since a
//! [`JudgeRequest`] carries none of them.
//!
//! **The conversation.** A user message is a `user` content: a text as a text part, a
//! picture as an `inlineData` part, a tool's result as a `functionResponse` part, which
//! names the function the call it answers named. An assistant message is a `model`
//! content: every part of a turn is echoed as it came (a thought's signature rides on
//! the part). The interface has no system role inside a conversation: a system message's
//! words close the user content before it, or open one. The system prompt is the
//! `systemInstruction`; the tools are `functionDeclarations`, each schema with its
//! definitions written in place (squint's `inline_refs` for this adapter: Google's
//! documentation does not say `$defs` is taken).
//!
//! **A call with no id.** Gemini may answer a `functionCall` without an id; the call is
//! given one made from the part's bytes ([`NO_ID`] and a digest), and its result is sent
//! back with none. Two calls of one answer without an id cannot be told apart, and the
//! answer is refused.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};
use whizbang_core::capability::{
    Fault, FaultKind, JudgeRequest, Opaque, Part, Role, StopReason, ToolCall, ToolResult, Turn,
    Usage,
};
use whizbang_core::egress::DataClass;

use crate::service::body::{Body, NOT_SAID, OPAQUE, Written, list};

/// The name a turn's opaque parts are marked with: the backend's, as its model
/// identity names it.
pub const ADAPTER: &str = "google";

/// What starts the id of a call the answer gave none.
pub const NO_ID: &str = "gemini-noid:";

/// What the request names of itself beyond its conversation: the thinking level, when
/// one is set, the long edge of its pictures and the most output tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Settings {
    pub(crate) effort: Option<String>,
    pub(crate) long_edge: u32,
    pub(crate) max_output_tokens: u32,
}

/// What a fault says of a tool's result that answers no call of the conversation.
pub(crate) const UNANSWERED: &str = "a tool's result answers no call of the conversation";
/// What it says of a tool whose schema's definitions cannot be written in place.
pub(crate) const SCHEMA: &str = "a tool's schema refers to itself, which the adapter cannot write";

/// The id of a call the answer gave none: [`NO_ID`] and the first eight bytes of the
/// digest of its part, in hex.
fn made_id(part: &Value) -> String {
    let digest = Sha256::digest(part.to_string().as_bytes());
    let hex: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    format!("{NO_ID}{hex}")
}

/// The id and name of the call a part of a turn holds, when it holds one.
fn call_of(part: &Value) -> Option<(String, String)> {
    let call = part.get("functionCall")?;
    let name = call.get("name")?.as_str()?.to_owned();
    let id = call
        .get("id")
        .and_then(Value::as_str)
        .map_or_else(|| made_id(part), str::to_owned);
    Some((id, name))
}

/// A part of this adapter, read back, when it is one.
fn part_of(opaque: &Opaque) -> Option<Value> {
    (opaque.adapter() == ADAPTER)
        .then(|| serde_json::from_str::<Value>(opaque.body()).ok())
        .flatten()
        .filter(Value::is_object)
}

/// Every reference to a definition written in place, and the definitions dropped; a
/// schema that refers to itself is refused (squint's `transform` for this adapter).
fn inlined(schema: &Value) -> Result<Value, Fault> {
    fn walk(value: &mut Value, defs: &Map<String, Value>, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        match value {
            Value::Object(map) => {
                let named = map
                    .get("$ref")
                    .and_then(Value::as_str)
                    .and_then(|r| {
                        r.strip_prefix("#/$defs/")
                            .or_else(|| r.strip_prefix("#/definitions/"))
                    })
                    .and_then(|name| defs.get(name));
                if let Some(definition) = named {
                    let description = map.get("description").cloned();
                    let mut replacement = definition.clone();
                    if let (Some(said), Some(object)) = (description, replacement.as_object_mut()) {
                        object.entry("description").or_insert(said);
                    }
                    *value = replacement;
                    return walk(value, defs, depth + 1);
                }
                map.iter_mut()
                    .filter(|(key, _)| *key != "$defs" && *key != "definitions")
                    .all(|(_, value)| walk(value, defs, depth + 1))
            }
            Value::Array(values) => values.iter_mut().all(|value| walk(value, defs, depth + 1)),
            _ => true,
        }
    }
    let mut defs = Map::new();
    for key in ["$defs", "definitions"] {
        if let Some(found) = schema.get(key).and_then(Value::as_object) {
            defs.extend(found.clone());
        }
    }
    let mut written = schema.clone();
    if !walk(&mut written, &defs, 0) {
        return Err(Fault::new(FaultKind::Rejected, SCHEMA).not_sent());
    }
    if let Some(object) = written.as_object_mut() {
        object.remove("$defs");
        object.remove("definitions");
    }
    Ok(written)
}

/// One item of a content as it is written: a part of a message, or a system message's
/// words.
enum Item<'a> {
    Part(&'a Part),
    Words(String),
}

/// The contents of the conversation: each message a content of its role, a system
/// message's words joined to the user content before it, or one of their own.
fn contents(request: &JudgeRequest) -> Vec<(&'static str, Vec<Item<'_>>)> {
    let mut contents: Vec<(&'static str, Vec<Item<'_>>)> = Vec::new();
    for message in request.messages() {
        match message.role() {
            Role::User => contents.push(("user", message.parts().iter().map(Item::Part).collect())),
            Role::Assistant => {
                contents.push(("model", message.parts().iter().map(Item::Part).collect()));
            }
            Role::System => {
                let words: Vec<&str> = message
                    .parts()
                    .iter()
                    .filter_map(|part| match part {
                        Part::Text(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                let words = Item::Words(words.join("\n\n"));
                match contents.last_mut() {
                    Some(("user", items)) => items.push(words),
                    _ => contents.push(("user", vec![words])),
                }
            }
        }
    }
    contents
}

/// The body of the request, in pieces: `systemInstruction` (when the request has a
/// system prompt), `contents`, `tools` and `toolConfig` (when it defines any), and
/// `generationConfig` with the most output tokens and the thinking level, when one is
/// set.
pub(crate) fn request(request: &JudgeRequest, settings: &Settings) -> Result<Written, Fault> {
    let prompts = DataClass::Prompts;
    let mut body = Body::new();
    body.glue("{");
    if !request.system().is_empty() {
        body.words(prompts, "\"systemInstruction\":");
        body.json(prompts, &json!({ "parts": [{ "text": request.system() }] }));
        body.glue(",");
    }
    // The name of each call of the conversation, by its id: a result names its call's.
    let mut names = BTreeMap::new();
    for message in request.messages() {
        for part in message.parts() {
            match part {
                Part::ToolCall(call) => {
                    names.insert(call.id().to_owned(), call.name().to_owned());
                }
                Part::Opaque(opaque) => {
                    if let Some((id, name)) = part_of(opaque).as_ref().and_then(call_of) {
                        names.insert(id, name);
                    }
                }
                _ => {}
            }
        }
    }
    body.words(prompts, "\"contents\":");
    let contents = contents(request);
    list(&mut body, &contents, |body, (role, items)| {
        content(body, role, items, &names, settings.long_edge)?;
        Ok(true)
    })?;
    if !request.tools().is_empty() {
        let mut declarations = Vec::new();
        for tool in request.tools() {
            declarations.push(json!({
                "name": tool.name(),
                "description": tool.description(),
                "parametersJsonSchema": inlined(tool.schema())?,
            }));
        }
        body.words(prompts, ",\"tools\":");
        body.json(prompts, &json!([{ "functionDeclarations": declarations }]));
        body.words(prompts, ",\"toolConfig\":");
        body.json(
            prompts,
            &json!({ "functionCallingConfig": { "mode": "AUTO" } }),
        );
    }
    let most = request
        .controls()
        .max_output_tokens()
        .min(settings.max_output_tokens);
    let mut generation = json!({ "maxOutputTokens": most });
    if let Some(level) = &settings.effort {
        generation["thinkingConfig"] = json!({ "thinkingLevel": level });
    }
    body.words(prompts, ",\"generationConfig\":");
    body.json(prompts, &generation);
    body.words(prompts, "}");
    Ok(Written::of(
        body.into_pieces(),
        request.messages().len() as u64,
    ))
}

/// An `inlineData` part whose data is the picture, prepared, as a piece of its own.
fn picture(
    body: &mut Body,
    class: DataClass,
    picture: &whizbang_core::capability::Picture,
    long_edge: u32,
) -> Result<(), Fault> {
    let before = |media: &str| format!("{{\"inlineData\":{{\"mimeType\":\"{media}\",\"data\":\"");
    body.picture(class, picture, long_edge, before, "\"}}")
}

/// One content: its role, and its parts.
fn content(
    body: &mut Body,
    role: &str,
    items: &[Item<'_>],
    names: &BTreeMap<String, String>,
    long_edge: u32,
) -> Result<(), Fault> {
    let prompts = DataClass::Prompts;
    body.words(prompts, &format!("{{\"role\":\"{role}\",\"parts\":"));
    // A call whose part came back as it was, opaque, is said by that part.
    let echoed: BTreeSet<String> = items
        .iter()
        .filter_map(|item| match item {
            Item::Part(Part::Opaque(opaque)) => part_of(opaque).and_then(|part| call_of(&part)),
            _ => None,
        })
        .map(|(id, _)| id)
        .collect();
    list(body, items, |body, item| {
        let part = match item {
            Item::Words(words) => {
                body.json(prompts, &json!({ "text": words }));
                return Ok(true);
            }
            Item::Part(part) => part,
        };
        match (role, part) {
            (_, Part::Text(text)) => body.json(prompts, &json!({ "text": text })),
            ("user", Part::Image(shown)) => {
                picture(body, DataClass::SourceImages, shown, long_edge)?;
            }
            ("user", Part::ToolResult(result)) => {
                function_response(body, result, names, long_edge)?;
            }
            ("model", Part::ToolCall(call)) => {
                if echoed.contains(call.id()) {
                    return Ok(false);
                }
                let mut said = json!({ "name": call.name(), "args": call.arguments() });
                if !call.id().starts_with(NO_ID) {
                    said["id"] = json!(call.id());
                }
                body.json(prompts, &json!({ "functionCall": said }));
            }
            ("model", Part::Opaque(opaque)) => {
                let part = part_of(opaque)
                    .ok_or_else(|| Fault::new(FaultKind::Rejected, OPAQUE).not_sent())?;
                body.json(prompts, &part);
            }
            _ => return Err(Fault::new(FaultKind::Rejected, NOT_SAID).not_sent()),
        }
        Ok(true)
    })?;
    body.glue("}");
    Ok(())
}

/// A tool's result: the call's id (when the call had one of its own) and name, its
/// text as the `output`, or as the `error` when it is one, and its pictures as parts.
fn function_response(
    body: &mut Body,
    result: &ToolResult,
    names: &BTreeMap<String, String>,
    long_edge: u32,
) -> Result<(), Fault> {
    let prompts = DataClass::Prompts;
    let name = names
        .get(result.call())
        .ok_or_else(|| Fault::new(FaultKind::Rejected, UNANSWERED).not_sent())?;
    body.words(prompts, "{\"functionResponse\":{");
    if !result.call().starts_with(NO_ID) {
        body.words(prompts, "\"id\":");
        body.json(prompts, &json!(result.call()));
        body.glue(",");
    }
    body.words(prompts, "\"name\":");
    body.json(prompts, &json!(name));
    let key = if result.is_error() { "error" } else { "output" };
    body.words(prompts, &format!(",\"response\":{{\"{key}\":"));
    body.json(DataClass::Evidence, &json!(result.text()));
    body.glue("}");
    if !result.images().is_empty() {
        body.glue(",\"parts\":");
        list(body, result.images(), |body, shown| {
            picture(body, DataClass::Evidence, shown, long_edge)?;
            Ok(true)
        })?;
    }
    body.glue("}}");
    Ok(())
}

/// What a fault says of an answer whose model could not form it: the finish reasons
/// [`UNFORMED`]. The same request may not fail so again, and is sent again.
pub const MALFORMED: &str = "the model could not form its answer: MALFORMED_FUNCTION_CALL or \
                             MALFORMED_RESPONSE";

/// The finish reasons of an answer the model could not form, which a turn keeps as
/// [`StopReason::Other`] and the backend makes a fault [`MALFORMED`].
pub const UNFORMED: [&str; 2] = ["MALFORMED_FUNCTION_CALL", "MALFORMED_RESPONSE"];

/// The codes of Google's errors (`google.rpc.Code`), by the word an error's `status`
/// gives. A fault names one of these and no other word of an error's body.
pub(crate) const CODES: [&str; 16] = [
    "CANCELLED",
    "UNKNOWN",
    "INVALID_ARGUMENT",
    "DEADLINE_EXCEEDED",
    "NOT_FOUND",
    "ALREADY_EXISTS",
    "PERMISSION_DENIED",
    "RESOURCE_EXHAUSTED",
    "FAILED_PRECONDITION",
    "ABORTED",
    "OUT_OF_RANGE",
    "UNIMPLEMENTED",
    "INTERNAL",
    "UNAVAILABLE",
    "DATA_LOSS",
    "UNAUTHENTICATED",
];

/// The codes Google's documentation of the Gemini interface says a caller may send the
/// same request again after: the quota or rate, an overload, a deadline and an internal
/// error.
pub(crate) const TRANSIENT: [&str; 4] = [
    "RESOURCE_EXHAUSTED",
    "UNAVAILABLE",
    "DEADLINE_EXCEEDED",
    "INTERNAL",
];

/// What the body of an error states of itself: the code its `status` gives, when that is
/// one of [`CODES`], and the wait a `RetryInfo` among its `details` gives
/// (`retryDelay`, as seconds with an `s`), in milliseconds. Nothing else of the body is
/// read, and a body that is not an error states nothing.
pub(crate) fn stated(body: &[u8]) -> (Option<&'static str>, Option<u64>) {
    let Ok(read) = serde_json::from_slice::<Value>(body) else {
        return (None, None);
    };
    let error = &read["error"];
    let code = (error["status"].as_str()).and_then(|word| CODES.into_iter().find(|c| *c == word));
    let details = error["details"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let wait = details
        .iter()
        .filter(|detail| detail["@type"] == "type.googleapis.com/google.rpc.RetryInfo")
        .find_map(|detail| detail["retryDelay"].as_str())
        .and_then(|delay| delay.strip_suffix('s'))
        .and_then(|seconds| seconds.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| (seconds * 1000.0).round().min(u64::MAX as f64) as u64);
    (code, wait)
}

/// What a fault says of an answer that is not a `generateContent` response.
pub(crate) const NOT_A_RESPONSE: &str =
    "the answer is not a generateContent response the adapter reads";
/// What it says of a tool call with no name, or one the core does not take, or two
/// calls with no id.
pub(crate) const BAD_CALL: &str = "the answer holds a tool call the adapter cannot read";
/// What it says of a turn the core does not take.
pub(crate) const BAD_TURN: &str = "the answer is not a turn the core takes";

/// A count of usage, or nothing.
fn count(usage: &Value, name: &str) -> u64 {
    usage.get(name).and_then(Value::as_u64).unwrap_or(0)
}

/// The body of an answer of `200`, read back to a turn: the first candidate's parts in
/// order, each kept. A part of words alone as a text part; a `functionCall` as that
/// part, opaque, so that it is echoed as it came, and beside it the call; every other
/// part (a thought, a part with a signature, anything newer) opaque, as it came. An
/// answer with no candidate is a refusal of the prompt; its words are the provider's and
/// are not read. The usage: what was read from the cache, apart from the rest of the
/// prompt, and the thoughts with the output.
pub(crate) fn turn(body: &[u8]) -> Result<Turn, Fault> {
    let malformed = |why| Fault::new(FaultKind::Malformed, why).answered(None);
    let parsed: Value = serde_json::from_slice(body).map_err(|_| malformed(NOT_A_RESPONSE))?;
    // A response holds its candidates, or says why the prompt was blocked.
    let read = parsed.get("candidates").is_some_and(Value::is_array)
        || parsed.get("promptFeedback").is_some_and(Value::is_object);
    if !read {
        return Err(malformed(NOT_A_RESPONSE));
    }
    let usage = parsed
        .get("usageMetadata")
        .filter(|usage| usage.is_object())
        .map(|usage| {
            let cached = count(usage, "cachedContentTokenCount");
            let prompt = count(usage, "promptTokenCount");
            let output = count(usage, "candidatesTokenCount")
                .saturating_add(count(usage, "thoughtsTokenCount"));
            Usage::new(prompt.saturating_sub(cached), output).cached(cached, 0)
        });
    let id = parsed.get("responseId").and_then(Value::as_str);
    let candidate = parsed
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|candidates| candidates.first());
    let Some(candidate) = candidate else {
        let model = parsed.get("modelVersion").and_then(Value::as_str);
        return Turn::new(Vec::new(), StopReason::Refusal, usage, id)
            .map(|turn| turn.reported_by(model))
            .map_err(|_| malformed(BAD_TURN));
    };
    let parts_of = candidate
        .get("content")
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array);
    let mut parts = Vec::new();
    let (mut calls, mut without_id) = (0, 0);
    for part in parts_of.into_iter().flatten() {
        let opaque = || {
            let written = serde_json::to_string(part).unwrap_or_default();
            Opaque::new(ADAPTER, &written)
                .map(Part::Opaque)
                .map_err(|_| malformed(NOT_A_RESPONSE))
        };
        if let Some(said) = part.get("functionCall") {
            let (id, name) = call_of(part).ok_or_else(|| malformed(BAD_CALL))?;
            if said.get("id").is_none() {
                without_id += 1;
            }
            calls += 1;
            let arguments = said.get("args").cloned().unwrap_or_else(|| json!({}));
            let call = ToolCall::new(&id, &name, arguments).map_err(|_| malformed(BAD_CALL))?;
            parts.push(opaque()?);
            parts.push(Part::ToolCall(call));
            continue;
        }
        let words = part
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty() && part.as_object().is_some_and(|p| p.len() == 1));
        match words {
            Some(text) => parts.push(Part::Text(text.to_owned())),
            None => parts.push(opaque()?),
        }
    }
    if without_id > 0 && calls > 1 {
        return Err(malformed(BAD_CALL));
    }
    let stop = match candidate.get("finishReason").and_then(Value::as_str) {
        Some("STOP") | None if calls > 0 => StopReason::ToolUse,
        Some("STOP") | None => StopReason::EndTurn,
        Some("MAX_TOKENS") => StopReason::MaxTokens,
        Some(
            "SAFETY"
            | "RECITATION"
            | "BLOCKLIST"
            | "PROHIBITED_CONTENT"
            | "SPII"
            | "IMAGE_SAFETY"
            | "IMAGE_PROHIBITED_CONTENT"
            | "IMAGE_RECITATION",
        ) => StopReason::Refusal,
        Some(other) => StopReason::Other(other.to_owned()),
    };
    let model = parsed.get("modelVersion").and_then(Value::as_str);
    Turn::new(parts, stop, usage, id)
        .map(|turn| turn.reported_by(model))
        .map_err(|_| malformed(BAD_TURN))
}
