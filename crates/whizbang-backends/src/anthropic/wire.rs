//! The Messages wire (plan 2A, task S1): a [`JudgeRequest`] written as the body of a
//! Messages request, in pieces, and the body of an answer read back to a [`Turn`].
//!
//! Ported from squint's Messages adapter (`crates/squint-providers/src/anthropic/mod.rs`
//! and `wire.rs`, at the commit `docs/sources.toml` pins), and reshaped: the body is
//! written as pieces for the gate, not as one JSON value, and the controls squint read
//! from its catalog (forced tool choice, removed tools, strict schemas) are not
//! written, since a [`JudgeRequest`] carries none of them.
//!
//! **What is checked is what is sent** (plan 2A, task S1, item 5). Each picture is a
//! piece of its own, prepared (`Piece::prepared`) and checked as an image; the
//! sender writes the base64 of exactly that piece's bytes between the words around it
//! (`net::http::post`). No picture is copied, decoded or encoded after it is made a
//! piece. The words are pieces too, split only where a picture stands or the class of
//! the words changes: a conversation's words are prompts, a tool's result is evidence,
//! a user's picture a source image, and a tool's picture evidence.

use serde_json::{Value, json};
use whizbang_core::capability::{
    Fault, FaultKind, JudgeRequest, Message, Opaque, Part, Picture, Role, StopReason, ToolCall,
    ToolResult, Turn, Usage,
};
use whizbang_core::egress::DataClass;

use crate::service::body::{Body, NOT_SAID, OPAQUE, Written, list, long_edge_of};

/// The name a turn's opaque parts are marked with: the backend's, as its model
/// identity names it.
pub const ADAPTER: &str = "anthropic";

/// What the request names of itself beyond its conversation: the model, the most
/// output tokens it may write, and the effort, when one is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Settings {
    pub(crate) model: String,
    pub(crate) effort: Option<String>,
    /// The long edge every picture is fitted to.
    pub(crate) long_edge: u32,
    /// The most output tokens a request asks for: one asking more is sent with this.
    pub(crate) max_output_tokens: u32,
}

/// The body of the request, in pieces: `model`, `max_tokens`, `system` (when the
/// request has one), `messages`, `tools` (when it defines any), `thinking`, and
/// `output_config` with its `effort` (when one is set).
pub(crate) fn request(request: &JudgeRequest, settings: &Settings) -> Result<Written, Fault> {
    let mut body = Body::new();
    let long_edge = long_edge_of(request, settings.long_edge);
    let most = request
        .controls()
        .max_output_tokens()
        .min(settings.max_output_tokens);
    let prompts = DataClass::Prompts;
    body.words(prompts, "{\"model\":");
    body.json(prompts, &json!(settings.model));
    body.words(prompts, ",\"max_tokens\":");
    body.json(prompts, &json!(most));
    if !request.system().is_empty() {
        body.words(prompts, ",\"system\":");
        body.json(
            prompts,
            &json!([{ "type": "text", "text": request.system() }]),
        );
    }
    body.words(prompts, ",\"messages\":");
    list(&mut body, request.messages(), |body, message| {
        self::message(body, message, long_edge)?;
        Ok(true)
    })?;
    if !request.tools().is_empty() {
        let tools: Vec<Value> = request
            .tools()
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name(),
                    "description": tool.description(),
                    "input_schema": tool.schema(),
                })
            })
            .collect();
        body.words(prompts, ",\"tools\":");
        body.json(prompts, &Value::Array(tools));
    }
    body.words(prompts, ",\"thinking\":");
    body.json(
        prompts,
        &json!({ "type": "adaptive", "block_binding": { "prefix_mismatch_behavior": "error" } }),
    );
    if let Some(effort) = &settings.effort {
        body.words(prompts, ",\"output_config\":");
        body.json(prompts, &json!({ "effort": effort }));
    }
    body.words(prompts, "}");
    Ok(Written::of(
        body.into_pieces(),
        request.messages().len() as u64,
    ))
}

/// An image block whose data is the picture, prepared, as a piece of its own.
fn picture(
    body: &mut Body,
    class: DataClass,
    picture: &Picture,
    long_edge: u32,
) -> Result<(), Fault> {
    let before = |media: &str| {
        format!(
            "{{\"type\":\"image\",\"source\":{{\"type\":\"base64\",\"media_type\":\"{media}\",\"data\":\""
        )
    };
    body.picture(class, picture, long_edge, before, "\"}}")
}

/// One message: its role, and its content as blocks.
fn message(body: &mut Body, message: &Message, long_edge: u32) -> Result<(), Fault> {
    let prompts = DataClass::Prompts;
    let role = match message.role() {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
    };
    body.words(prompts, &format!("{{\"role\":\"{role}\",\"content\":"));
    // A call whose block came back as it was, opaque, is said by that block.
    let echoed: Vec<String> = message
        .parts()
        .iter()
        .filter_map(|part| match part {
            Part::Opaque(opaque) => tool_use_id(opaque),
            _ => None,
        })
        .collect();
    list(body, message.parts(), |body, part| {
        match (message.role(), part) {
            (_, Part::Text(text)) => body.json(prompts, &json!({ "type": "text", "text": text })),
            (Role::User, Part::Image(picture)) => {
                self::picture(body, DataClass::SourceImages, picture, long_edge)?;
            }
            (Role::User, Part::ToolResult(result)) => tool_result(body, result, long_edge)?,
            (Role::Assistant, Part::ToolCall(call)) => {
                if echoed.iter().any(|id| id == call.id()) {
                    return Ok(false);
                }
                body.json(
                    prompts,
                    &json!({
                        "type": "tool_use",
                        "id": call.id(),
                        "name": call.name(),
                        "input": call.arguments(),
                    }),
                );
            }
            (Role::Assistant, Part::Opaque(opaque)) => {
                let block: Value = serde_json::from_str(opaque.body())
                    .ok()
                    .filter(|block: &Value| opaque.adapter() == ADAPTER && block.is_object())
                    .ok_or_else(|| Fault::new(FaultKind::Rejected, OPAQUE).not_sent())?;
                body.json(prompts, &block);
            }
            _ => return Err(Fault::new(FaultKind::Rejected, NOT_SAID).not_sent()),
        }
        Ok(true)
    })?;
    body.glue("}");
    Ok(())
}

/// The id of a `tool_use` block the adapter kept opaque, when it is one.
fn tool_use_id(opaque: &Opaque) -> Option<String> {
    if opaque.adapter() != ADAPTER {
        return None;
    }
    let block: Value = serde_json::from_str(opaque.body()).ok()?;
    (block.get("type")? == "tool_use")
        .then(|| block.get("id")?.as_str().map(str::to_owned))
        .flatten()
}

/// A tool's result: its text alone as a string, or, with pictures, a list of its text
/// (when it has any) and its pictures; and whether it is an error.
fn tool_result(body: &mut Body, result: &ToolResult, long_edge: u32) -> Result<(), Fault> {
    let evidence = DataClass::Evidence;
    body.words(
        DataClass::Prompts,
        "{\"type\":\"tool_result\",\"tool_use_id\":",
    );
    body.json(DataClass::Prompts, &json!(result.call()));
    body.words(DataClass::Prompts, ",\"content\":");
    if result.images().is_empty() {
        body.json(evidence, &json!(result.text()));
    } else {
        let mut parts: Vec<Option<&Picture>> = Vec::new();
        if !result.text().is_empty() {
            parts.push(None);
        }
        parts.extend(result.images().iter().map(Some));
        list(body, &parts, |body, part| {
            match part {
                None => body.json(evidence, &json!({ "type": "text", "text": result.text() })),
                Some(shown) => picture(body, evidence, shown, long_edge)?,
            }
            Ok(true)
        })?;
    }
    if result.is_error() {
        body.glue(",\"is_error\":true");
    }
    body.glue("}");
    Ok(())
}

/// What a fault says of an answer that is not a Messages response.
pub(crate) const NOT_A_RESPONSE: &str = "the answer is not a Messages response the adapter reads";
/// What it says of a tool call with no id or no name, or one the core does not take.
pub(crate) const BAD_CALL: &str = "the answer holds a tool call the adapter cannot read";
/// What it says of a turn the core does not take.
pub(crate) const BAD_TURN: &str = "the answer is not a turn the core takes";

/// A count of usage, or nothing.
fn count(usage: &Value, name: &str) -> u64 {
    usage.get(name).and_then(Value::as_u64).unwrap_or(0)
}

/// The body of an answer of `200`, read back to a turn. Every block is kept in order:
/// a text block of words as a text part; a `tool_use` block as that block, opaque, so
/// that it is echoed as it came, and beside it the call; every other block (thinking,
/// redacted thinking, an empty text, anything newer) opaque, as it came. The words of
/// a refusal's explanation are not read: they are the provider's.
pub(crate) fn turn(body: &[u8]) -> Result<Turn, Fault> {
    let malformed = |why| Fault::new(FaultKind::Malformed, why).answered(None);
    let parsed: Value = serde_json::from_slice(body).map_err(|_| malformed(NOT_A_RESPONSE))?;
    let content = parsed
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| malformed(NOT_A_RESPONSE))?;
    let mut parts = Vec::new();
    for block in content {
        let opaque = || {
            let written = serde_json::to_string(block).unwrap_or_default();
            Opaque::new(ADAPTER, &written)
                .map(Part::Opaque)
                .map_err(|_| malformed(NOT_A_RESPONSE))
        };
        match block.get("type").and_then(Value::as_str) {
            Some("text") => match block.get("text").and_then(Value::as_str) {
                Some(text)
                    if !text.is_empty() && block.as_object().is_some_and(|b| b.len() == 2) =>
                {
                    parts.push(Part::Text(text.to_owned()));
                }
                _ => parts.push(opaque()?),
            },
            Some("tool_use") => {
                let field = |name| block.get(name).and_then(Value::as_str).unwrap_or_default();
                let input = block.get("input").cloned().unwrap_or(Value::Null);
                let call = ToolCall::new(field("id"), field("name"), input)
                    .map_err(|_| malformed(BAD_CALL))?;
                parts.push(opaque()?);
                parts.push(Part::ToolCall(call));
            }
            _ => parts.push(opaque()?),
        }
    }
    let stop = match parsed.get("stop_reason").and_then(Value::as_str) {
        None | Some("end_turn" | "stop_sequence") => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens" | "model_context_window_exceeded") => StopReason::MaxTokens,
        Some("refusal") => StopReason::Refusal,
        Some(other) => StopReason::Other(other.to_owned()),
    };
    let usage = parsed
        .get("usage")
        .filter(|usage| usage.is_object())
        .map(|usage| {
            Usage::new(count(usage, "input_tokens"), count(usage, "output_tokens")).cached(
                count(usage, "cache_read_input_tokens"),
                count(usage, "cache_creation_input_tokens"),
            )
        });
    let id = parsed.get("id").and_then(Value::as_str);
    Turn::new(parts, stop, usage, id).map_err(|_| malformed(BAD_TURN))
}
