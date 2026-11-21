//! The Responses wire (plan m2c, task P1, item 1): a [`JudgeRequest`] written as the body
//! of a Responses request, in pieces, and the body of an answer read back to a [`Turn`].
//!
//! Ported from squint's OpenAI adapter (`crates/squint-providers/src/openai/mod.rs`, at
//! the commit `docs/sources.toml` pins), and reshaped as the Messages wire was: the body
//! is written as pieces for the gate (`crate::service::body`), and the controls squint
//! read from its catalogue (a forced tool choice, the image detail, strict schemas) are
//! not written, since a [`JudgeRequest`] carries none of them. A tool is declared with
//! `strict` false, so its schema is taken as it is written (squint wrote a strict
//! schema of its own; carry-over row on strict schemas).
//!
//! **The conversation** is a list of items: a user message's words and pictures are one
//! `message` item, after an item for each of its tools' results
//! (`function_call_output`, the words of an error led by `error: `, since the interface
//! has no flag of one); an assistant message's parts each an item, every item of a turn
//! echoed as it came (a reasoning item with its encrypted content included); a system
//! message a `developer` message. `store` is false on every request, and the reasoning
//! is asked back encrypted, so that nothing is kept by the provider between turns.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use whizbang_core::capability::{
    Fault, FaultKind, JudgeRequest, Message, Opaque, Part, Picture, Role, StopReason, ToolCall,
    ToolResult, Turn, Usage,
};
use whizbang_core::egress::DataClass;

use crate::service::body::{Body, NOT_SAID, OPAQUE, Written, list};

/// The name a turn's opaque parts are marked with: the backend's, as its model
/// identity names it.
pub const ADAPTER: &str = "openai";

/// What the request names of itself beyond its conversation: the model, the reasoning
/// effort, when one is set, the long edge of its pictures and the most output tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Settings {
    pub(crate) model: String,
    pub(crate) effort: Option<String>,
    pub(crate) long_edge: u32,
    pub(crate) max_output_tokens: u32,
}

/// An item of this adapter, read back, when it is one.
fn item_of(opaque: &Opaque) -> Option<Value> {
    (opaque.adapter() == ADAPTER)
        .then(|| serde_json::from_str::<Value>(opaque.body()).ok())
        .flatten()
        .filter(Value::is_object)
}

/// The body of the request, in pieces: `model`, `instructions` (when the request has a
/// system prompt), `input`, `tools`, `tool_choice` and `parallel_tool_calls` (when it
/// defines any tool), `max_output_tokens`, `store`, `include`, `truncation`, and
/// `reasoning` with its `effort` (when one is set).
pub(crate) fn request(request: &JudgeRequest, settings: &Settings) -> Result<Written, Fault> {
    let prompts = DataClass::Prompts;
    let mut body = Body::new();
    body.words(prompts, "{\"model\":");
    body.json(prompts, &json!(settings.model));
    if !request.system().is_empty() {
        body.words(prompts, ",\"instructions\":");
        body.json(prompts, &json!(request.system()));
    }
    body.words(prompts, ",\"input\":");
    list(&mut body, request.messages(), |body, message| {
        items(body, message, settings.long_edge)
    })?;
    if !request.tools().is_empty() {
        let tools: Vec<Value> = request
            .tools()
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.name(),
                    "description": tool.description(),
                    "parameters": tool.schema(),
                    "strict": false,
                })
            })
            .collect();
        body.words(prompts, ",\"tools\":");
        body.json(prompts, &Value::Array(tools));
        body.words(
            prompts,
            ",\"tool_choice\":\"auto\",\"parallel_tool_calls\":true",
        );
    }
    let most = request
        .controls()
        .max_output_tokens()
        .min(settings.max_output_tokens);
    body.words(prompts, ",\"max_output_tokens\":");
    body.json(prompts, &json!(most));
    body.words(
        prompts,
        ",\"store\":false,\"include\":[\"reasoning.encrypted_content\"],\"truncation\":\"disabled\"",
    );
    if let Some(effort) = &settings.effort {
        body.words(prompts, ",\"reasoning\":");
        body.json(prompts, &json!({ "effort": effort }));
    }
    body.words(prompts, "}");
    Ok(Written::of(
        body.into_pieces(),
        request.messages().len() as u64,
    ))
}

/// An `input_image` part whose data is the picture, prepared, as a piece of its own,
/// in a data address.
fn picture(
    body: &mut Body,
    class: DataClass,
    shown: &Picture,
    long_edge: u32,
) -> Result<(), Fault> {
    let before =
        |media: &str| format!("{{\"type\":\"input_image\",\"image_url\":\"data:{media};base64,");
    body.picture(class, shown, long_edge, before, "\"}")
}

/// The items of one message, between the commas of the input's list; false when it
/// wrote none.
fn items(body: &mut Body, message: &Message, long_edge: u32) -> Result<bool, Fault> {
    let prompts = DataClass::Prompts;
    let parts = message.parts();
    match message.role() {
        Role::User => {
            // The results first, each an item; then the words and pictures, one message.
            let results: Vec<&ToolResult> = parts
                .iter()
                .filter_map(|part| match part {
                    Part::ToolResult(result) => Some(result),
                    _ => None,
                })
                .collect();
            let said: Vec<&Part> = parts
                .iter()
                .filter(|part| !matches!(part, Part::ToolResult(_)))
                .collect();
            let mut first = true;
            for result in results {
                if !first {
                    body.glue(",");
                }
                first = false;
                function_call_output(body, result, long_edge)?;
            }
            if !said.is_empty() {
                if !first {
                    body.glue(",");
                }
                body.words(
                    prompts,
                    "{\"type\":\"message\",\"role\":\"user\",\"content\":",
                );
                list(body, &said, |body, part| {
                    match part {
                        Part::Text(text) => {
                            body.json(prompts, &json!({ "type": "input_text", "text": text }));
                        }
                        Part::Image(shown) => {
                            picture(body, DataClass::SourceImages, shown, long_edge)?;
                        }
                        _ => return Err(Fault::new(FaultKind::Rejected, NOT_SAID).not_sent()),
                    }
                    Ok(true)
                })?;
                body.glue("}");
            }
            Ok(true)
        }
        Role::Assistant => {
            // A call whose item came back as it was, opaque, is said by that item.
            let echoed: BTreeSet<String> = parts
                .iter()
                .filter_map(|part| match part {
                    Part::Opaque(opaque) => item_of(opaque),
                    _ => None,
                })
                .filter(|item| item.get("type") == Some(&json!("function_call")))
                .filter_map(|item| item.get("call_id")?.as_str().map(str::to_owned))
                .collect();
            let mut first = true;
            for part in parts {
                let item = match part {
                    Part::Opaque(opaque) => item_of(opaque)
                        .ok_or_else(|| Fault::new(FaultKind::Rejected, OPAQUE).not_sent())?,
                    Part::Text(text) => json!({
                        "type": "message", "role": "assistant",
                        "content": [{ "type": "output_text", "text": text, "annotations": [] }]
                    }),
                    Part::ToolCall(call) if echoed.contains(call.id()) => continue,
                    Part::ToolCall(call) => json!({
                        "type": "function_call", "call_id": call.id(), "name": call.name(),
                        "arguments": call.arguments().to_string()
                    }),
                    _ => return Err(Fault::new(FaultKind::Rejected, NOT_SAID).not_sent()),
                };
                if !first {
                    body.glue(",");
                }
                first = false;
                body.json(prompts, &item);
            }
            Ok(!first)
        }
        Role::System => {
            let words: Vec<&str> = parts
                .iter()
                .filter_map(|part| match part {
                    Part::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            body.json(
                prompts,
                &json!({
                    "type": "message", "role": "developer",
                    "content": [{ "type": "input_text", "text": words.join("\n\n") }]
                }),
            );
            Ok(true)
        }
    }
}

/// A tool's result: its call's id, and its text alone as the output, or, with pictures,
/// a list of its text and its pictures; the text of an error led by `error: `.
fn function_call_output(body: &mut Body, result: &ToolResult, long_edge: u32) -> Result<(), Fault> {
    let prompts = DataClass::Prompts;
    let evidence = DataClass::Evidence;
    body.words(prompts, "{\"type\":\"function_call_output\",\"call_id\":");
    body.json(prompts, &json!(result.call()));
    body.words(prompts, ",\"output\":");
    let text = if result.is_error() {
        format!("error: {}", result.text())
    } else {
        result.text().to_owned()
    };
    if result.images().is_empty() {
        body.json(evidence, &json!(text));
    } else {
        let mut parts: Vec<Option<&Picture>> = vec![None];
        parts.extend(result.images().iter().map(Some));
        list(body, &parts, |body, part| {
            match part {
                None => body.json(evidence, &json!({ "type": "input_text", "text": text })),
                Some(shown) => picture(body, evidence, shown, long_edge)?,
            }
            Ok(true)
        })?;
    }
    body.glue("}");
    Ok(())
}

/// What a fault says of an answer that is not a Responses response.
pub(crate) const NOT_A_RESPONSE: &str = "the answer is not a Responses response the adapter reads";
/// What it says of an answer whose status is `failed`.
pub(crate) const FAILED: &str = "the provider answered that the response failed";
/// What it says of a tool call with no id or no name, or one the core does not take.
pub(crate) const BAD_CALL: &str = "the answer holds a tool call the adapter cannot read";
/// What it says of a turn the core does not take.
pub(crate) const BAD_TURN: &str = "the answer is not a turn the core takes";

/// A count of usage, or nothing.
fn count(usage: &Value, name: &str) -> u64 {
    usage.get(name).and_then(Value::as_u64).unwrap_or(0)
}

/// The words of a message item of output text alone, each part's, when it is one.
fn words_of(item: &Value) -> Option<Vec<String>> {
    let content = item.get("content")?.as_array()?;
    let texts: Option<Vec<String>> = content
        .iter()
        .map(|part| {
            (part.get("type")? == "output_text")
                .then(|| part.get("text")?.as_str().map(str::to_owned))
                .flatten()
                .filter(|text| !text.is_empty())
        })
        .collect();
    texts.filter(|texts| !texts.is_empty())
}

/// The body of an answer of `200`, read back to a turn: every output item in order. A
/// message of output text alone as a text part for each of its texts; a `function_call`
/// as that item, opaque, so that it is echoed as it came, and beside it the call; every
/// other item (reasoning, a message that refuses, anything newer) opaque, as it came.
/// A refusal's words are the provider's and are not read. The usage: what was read from
/// the cache, apart from the rest of the input (a write to the cache is part of the rest,
/// at the input's price).
pub(crate) fn turn(body: &[u8]) -> Result<Turn, Fault> {
    let malformed = |why| Fault::new(FaultKind::Malformed, why).answered(None);
    let parsed: Value = serde_json::from_slice(body).map_err(|_| malformed(NOT_A_RESPONSE))?;
    let output = parsed
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| malformed(NOT_A_RESPONSE))?;
    let status = parsed.get("status").and_then(Value::as_str);
    if status == Some("failed") {
        // It may have been charged: it costs its bound.
        return Err(Fault::new(FaultKind::Unavailable, FAILED).answered(None));
    }
    let mut parts = Vec::new();
    let (mut calls, mut refused) = (0, false);
    for item in output {
        let opaque = || {
            let written = serde_json::to_string(item).unwrap_or_default();
            Opaque::new(ADAPTER, &written)
                .map(Part::Opaque)
                .map_err(|_| malformed(NOT_A_RESPONSE))
        };
        match item.get("type").and_then(Value::as_str) {
            Some("message") => match words_of(item) {
                Some(texts) => parts.extend(texts.into_iter().map(Part::Text)),
                None => {
                    let content = item.get("content").and_then(Value::as_array);
                    refused |= content
                        .into_iter()
                        .flatten()
                        .any(|part| part.get("type") == Some(&json!("refusal")));
                    parts.push(opaque()?);
                }
            },
            Some("function_call") => {
                let field = |name| item.get(name).and_then(Value::as_str).unwrap_or_default();
                let raw = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                // Arguments that are not JSON stay their text, which the action refuses.
                let arguments =
                    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_owned()));
                // A key written twice in the text is lost once it is parsed: where it was
                // is kept beside the call (plan e1, task F1).
                let call = ToolCall::new(field("call_id"), field("name"), arguments)
                    .map_err(|_| malformed(BAD_CALL))?
                    .with_repeated_keys(whizbang_core::capability::repeated_keys(raw));
                calls += 1;
                parts.push(opaque()?);
                parts.push(Part::ToolCall(call));
            }
            _ => parts.push(opaque()?),
        }
    }
    let reason = parsed
        .get("incomplete_details")
        .and_then(|details| details.get("reason"))
        .and_then(Value::as_str);
    let stop = match (status, reason) {
        (Some("incomplete"), Some("max_output_tokens")) => StopReason::MaxTokens,
        (Some("incomplete"), Some("content_filter")) => StopReason::Refusal,
        (Some("incomplete"), _) => StopReason::Other("incomplete".to_owned()),
        _ if refused => StopReason::Refusal,
        _ if calls > 0 => StopReason::ToolUse,
        _ => StopReason::EndTurn,
    };
    let usage = parsed
        .get("usage")
        .filter(|usage| usage.is_object())
        .map(|usage| {
            let cached = usage
                .get("input_tokens_details")
                .map_or(0, |details| count(details, "cached_tokens"));
            let input = count(usage, "input_tokens");
            Usage::new(input.saturating_sub(cached), count(usage, "output_tokens"))
                .cached(cached, 0)
        });
    let id = parsed.get("id").and_then(Value::as_str);
    let model = parsed.get("model").and_then(Value::as_str);
    Turn::new(parts, stop, usage, id)
        .map(|turn| turn.reported_by(model))
        .map_err(|_| malformed(BAD_TURN))
}
