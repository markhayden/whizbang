//! `conformance::…`: squint's recorded exchanges of its OpenAI adapter
//! (`fixtures/adapters/openai-responses/`, at the commit `docs/sources.toml` pins),
//! served by a recorded fake on loopback.
//!
//! Ported as `tests/fixtures/openai-responses/`, reshaped as the Messages exchanges
//! were: each picture's data address keeps its media type and has `<picture WxH>`, its
//! size as the recording's JPEG states it, in place of its bytes; each keeps its
//! request's body and headers (the key's written as `<redacted>`) and its answer's body
//! and status; nothing else was kept or changed.

use std::sync::Arc;

use serde_json::{Value, json};
use whizbang_core::capability::{
    Controls, JudgeRequest, Message, Opaque, Part, Picture, Role, StopReason, ToolCall,
    ToolDefinition, ToolResult,
};
use whizbang_core::egress::fitted;

use super::super::{ADAPTER, LONG_EDGE, spend};
use super::responses;
use crate::net::testing::Pki;
use crate::service::testing::{
    answer, ask, exchanges, named_size, picture_named, scale_of, serving, unbase64, words,
};

const SET: &str = "openai-responses";

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

fn opaque(item: &Value) -> Part {
    Part::Opaque(Opaque::new(ADAPTER, &serde_json::to_string(item).unwrap()).unwrap())
}

/// An output item as the reader keeps it: a message of output text alone as a text part
/// for each text; a call as its item, kept, and the call beside it; every other item
/// kept as it came.
fn output_parts(item: &Value) -> Vec<Part> {
    match item["type"].as_str().unwrap() {
        "message" => {
            let content = item["content"].as_array().unwrap();
            if content.iter().all(|part| part["type"] == "output_text") {
                content
                    .iter()
                    .map(|part| Part::Text(part["text"].as_str().unwrap().to_owned()))
                    .collect()
            } else {
                vec![opaque(item)]
            }
        }
        "function_call" => {
            let arguments = serde_json::from_str(item["arguments"].as_str().unwrap()).unwrap();
            let call = ToolCall::new(
                item["call_id"].as_str().unwrap(),
                item["name"].as_str().unwrap(),
                arguments,
            );
            vec![opaque(item), Part::ToolCall(call.unwrap())]
        }
        _ => vec![opaque(item)],
    }
}

#[test]
fn each_recorded_exchange_of_squint_is_read_back_to_its_turn() {
    let exchanges = exchanges(SET);
    assert_eq!(exchanges.len(), 24);
    let pki = Arc::new(Pki::loopback());
    let answers = exchanges
        .iter()
        .map(|(_, exchange)| answer("200 OK", "", &bytes(&exchange["response"]["body"])))
        .collect();
    let (listener, _) = serving(&pki, answers);
    for (name, exchange) in &exchanges {
        assert_eq!(exchange["response"]["status"], 200, "{name}");
        let recorded = &exchange["response"]["body"];
        let turn = ask(
            &pki,
            responses(listener.port(), "gpt-6.1-terra", None),
            &words(),
        )
        .unwrap_or_else(|fault| panic!("{name}: {fault:?}"));
        let expected: Vec<Part> = recorded["output"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(output_parts)
            .collect();
        assert_eq!(turn.parts(), expected.as_slice(), "{name}");
        let calls = turn.tool_calls().count();
        let reason = recorded["incomplete_details"]["reason"].as_str();
        let stop = match (recorded["status"].as_str().unwrap(), reason, calls) {
            ("completed", None, 0) => StopReason::EndTurn,
            ("completed", None, _) => StopReason::ToolUse,
            ("incomplete", Some("max_output_tokens"), _) => StopReason::MaxTokens,
            (other, _, _) => panic!("{name}: a status the test does not know: {other}"),
        };
        assert_eq!(turn.stop(), &stop, "{name}");
        let usage = &recorded["usage"];
        let cached = usage["input_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap();
        let read = turn.usage().unwrap();
        assert_eq!(
            (
                read.input_tokens(),
                read.output_tokens(),
                read.cache_read_tokens(),
                read.cache_write_tokens()
            ),
            (
                usage["input_tokens"].as_u64().unwrap() - cached,
                usage["output_tokens"].as_u64().unwrap(),
                cached,
                0
            ),
            "{name}"
        );
        assert_eq!(turn.request_id(), recorded["id"].as_str(), "{name}");
    }
    assert_eq!(listener.finish(), exchanges.len());
}

/// The picture of an `input_image` part.
fn shown(part: &Value) -> Picture {
    picture_named(part["image_url"].as_str().unwrap())
}

/// The items of a recorded input, by the message each was written from: a user's
/// message is the results of its tools and then one message of its words and pictures,
/// as the writer writes one, so an item after that message starts another; an
/// assistant's items run together; a developer's message is a system message.
fn grouped(body: &Value) -> Vec<(Role, Vec<&Value>)> {
    let mut grouped: Vec<(Role, Vec<&Value>)> = Vec::new();
    for item in body["input"].as_array().unwrap() {
        let role = match (item["type"].as_str().unwrap(), item["role"].as_str()) {
            ("message", Some("user")) | ("function_call_output", _) => Role::User,
            ("message", Some("developer")) => Role::System,
            _ => Role::Assistant,
        };
        let joins = match grouped.last() {
            Some((Role::User, items)) if role == Role::User => items
                .iter()
                .all(|item| item["type"] == "function_call_output"),
            Some((Role::Assistant, _)) => role == Role::Assistant,
            _ => false,
        };
        if joins {
            grouped.last_mut().unwrap().1.push(item);
        } else {
            grouped.push((role, vec![item]));
        }
    }
    grouped
}

/// The request a recorded body was written from, as whizbang holds one, with the effort
/// it names; none for a body that forces the call of a tool, which a [`JudgeRequest`]
/// does not carry. A user's message is the results of its tools and then its words and
/// pictures, as the writer writes one; a result after words starts another.
fn request_of(body: &Value) -> Option<(JudgeRequest, Option<String>)> {
    if body["tool_choice"] != "auto" {
        return None;
    }
    let mut messages = Vec::new();
    for (role, items) in grouped(body) {
        let mut parts = Vec::new();
        for item in items {
            match (role, item["type"].as_str().unwrap()) {
                (Role::User, "function_call_output") => {
                    let (text, images) = match &item["output"] {
                        Value::String(text) => (text.clone(), Vec::new()),
                        Value::Array(said) => (
                            said[0]["text"].as_str().unwrap().to_owned(),
                            said[1..].iter().map(shown).collect(),
                        ),
                        other => panic!("an output of {other}"),
                    };
                    let (text, error) = match text.strip_prefix("error: ") {
                        Some(rest) => (rest.to_owned(), true),
                        None => (text, false),
                    };
                    let id = item["call_id"].as_str().unwrap();
                    parts.push(Part::ToolResult(
                        ToolResult::new(id, &text, error, images).unwrap(),
                    ));
                }
                (Role::User | Role::System, _) => {
                    for part in item["content"].as_array().unwrap() {
                        parts.push(match part["type"].as_str().unwrap() {
                            "input_text" => Part::Text(part["text"].as_str().unwrap().to_owned()),
                            "input_image" => Part::Image(shown(part)),
                            other => panic!("a part of {other}"),
                        });
                    }
                }
                (Role::Assistant, _) => parts.extend(output_parts(item)),
            }
        }
        messages.push(Message::new(role, parts).unwrap());
    }
    let tools = body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| {
            ToolDefinition::new(
                tool["name"].as_str().unwrap(),
                tool["description"].as_str().unwrap(),
                tool["parameters"].clone(),
            )
            .unwrap()
        })
        .collect();
    let most = u32::try_from(body["max_output_tokens"].as_u64().unwrap()).unwrap();
    let system = body["instructions"].as_str().unwrap();
    let request = JudgeRequest::new(system, messages, tools, Controls::new(most).unwrap()).unwrap();
    let effort = body["reasoning"]["effort"].as_str().map(str::to_owned);
    Some((request, effort))
}

/// The recorded body as whizbang writes one: `strict` false on every tool, since a
/// [`JudgeRequest`] carries no strict schema, and the adapter writes a tool's schema as
/// it is given.
fn not_strict(body: &Value) -> Value {
    let mut body = body.clone();
    for tool in body["tools"].as_array_mut().unwrap() {
        tool["strict"] = json!(false);
    }
    body
}

/// Every recorded picture's address, in order.
fn addresses(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(address)) = map.get("image_url") {
                found.push(address.clone());
            }
            map.values().for_each(|value| addresses(value, found));
        }
        Value::Array(values) => values.iter().for_each(|value| addresses(value, found)),
        _ => {}
    }
}

/// A sent body with each picture's address written as the recorded body names it, once
/// the JPEG it holds states the size the test made of that recorded size and its media
/// type is the recorded one.
fn pictures_named(sent: &Value, recorded: &Value) -> Value {
    fn walk(value: &mut Value, names: &[String], at: &mut usize) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(address)) = map.get_mut("image_url") {
                    let (head, data) = address.split_once(',').unwrap();
                    let (name_head, _) = names[*at].split_once(',').unwrap();
                    assert_eq!(head, name_head);
                    let read = image::load_from_memory(&unbase64(data)).unwrap();
                    let (w, h) = named_size(&names[*at]).unwrap();
                    assert_eq!(
                        (read.width(), read.height()),
                        (w / scale_of(w), h / scale_of(w))
                    );
                    *address = names[*at].clone();
                    *at += 1;
                }
                map.values_mut().for_each(|value| walk(value, names, at));
            }
            Value::Array(values) => values.iter_mut().for_each(|value| walk(value, names, at)),
            _ => {}
        }
    }
    let mut names = Vec::new();
    addresses(recorded, &mut names);
    let mut body = sent.clone();
    let mut at = 0;
    walk(&mut body, &names, &mut at);
    assert_eq!(at, names.len());
    body
}

#[test]
fn a_request_is_written_as_the_recorded_request_was() {
    let exchanges = exchanges(SET);
    let written: Vec<(String, JudgeRequest, Option<String>, Value)> = exchanges
        .iter()
        .filter_map(|(name, exchange)| {
            let (request, effort) = request_of(&exchange["request"]["body"])?;
            Some((name.clone(), request, effort, exchange.clone()))
        })
        .collect();
    // One forces the call of a tool, which a request of the core does not carry.
    assert_eq!(written.len(), exchanges.len() - 1);
    let pki = Arc::new(Pki::loopback());
    let answers = written
        .iter()
        .map(|(_, _, _, exchange)| answer("200 OK", "", &bytes(&exchange["response"]["body"])))
        .collect();
    let (listener, log) = serving(&pki, answers);
    for (name, request, effort, exchange) in &written {
        let recorded = &exchange["request"];
        let model = recorded["body"]["model"].as_str().unwrap();
        let backend = responses(listener.port(), model, effort.as_deref());
        ask(&pki, backend, request).unwrap_or_else(|fault| panic!("{name}: {fault:?}"));
        let seen = log.lock().unwrap().last().unwrap().clone();
        let sent: Value = serde_json::from_slice(&seen.body).unwrap();
        assert_eq!(
            pictures_named(&sent, &recorded["body"]),
            not_strict(&recorded["body"]),
            "{name}"
        );
        // The recorded fields, but the key's, are among those sent, with their values;
        // the key's is the executor's to attach, and no field of the request names it.
        let head = seen.head.to_ascii_lowercase();
        for (field, value) in recorded["headers"].as_object().unwrap() {
            if field == "authorization" {
                assert!(!head.contains("authorization"), "{name}");
                continue;
            }
            let line = format!("{field}: {}\r\n", value.as_str().unwrap());
            assert!(head.contains(&line), "{name}: {field}");
        }
        assert!(head.starts_with("post /v1/responses "), "{name}");
    }
    assert_eq!(listener.finish(), written.len());
}

#[test]
fn the_input_count_is_at_least_the_usage_of_every_recorded_exchange() {
    // The count as the adapter makes it, of each recorded request: its words as they
    // were written, less each picture's data, and each picture by its recorded size,
    // fitted as the adapter fits it; against the whole input the provider reported for
    // it, what was read from its cache and written to it included.
    fn emptied(value: &mut Value, pictures: &mut Vec<(u32, u32)>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(address)) = map.get_mut("image_url") {
                    let (w, h) = named_size(address).unwrap();
                    pictures.push(fitted(w, h, LONG_EDGE));
                    let head = address.split_once(',').unwrap().0.to_owned();
                    *address = format!("{head},");
                }
                map.values_mut().for_each(|value| emptied(value, pictures));
            }
            Value::Array(values) => values.iter_mut().for_each(|value| emptied(value, pictures)),
            _ => {}
        }
    }
    for (name, exchange) in exchanges(SET) {
        let mut body = exchange["request"]["body"].clone();
        let mut pictures = Vec::new();
        emptied(&mut body, &mut pictures);
        let words = serde_json::to_vec(&body).unwrap().len() as u64;
        let messages = grouped(&exchange["request"]["body"]).len() as u64;
        let counted = spend::input_count(words, messages, &pictures);
        let used = exchange["response"]["body"]["usage"]["input_tokens"]
            .as_u64()
            .unwrap();
        assert!(counted >= used, "{name}: counted {counted}, used {used}");
    }
}
