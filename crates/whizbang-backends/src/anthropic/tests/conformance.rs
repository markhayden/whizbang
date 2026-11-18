//! `conformance::…`: squint's recorded exchanges of its Messages adapter
//! (`fixtures/adapters/anthropic-messages/`, at the commit `docs/sources.toml` pins),
//! served by a recorded fake on loopback.
//!
//! Ported as `tests/fixtures/anthropic-messages/`, reshaped: each request's pictures
//! are written as `<picture WxH>`, their sizes as the recording's JPEG states them, in
//! place of their bytes; nothing else was changed. A picture cannot be compared byte
//! for byte: whizbang holds pixels, and writes its own JPEG of them.

use std::sync::Arc;

use serde_json::{Map, Value, json};
use whizbang_core::capability::{
    Controls, JudgeRequest, Message, Opaque, Part, Role, StopReason, ToolCall, ToolDefinition,
    ToolResult,
};

use super::super::{ADAPTER, FIELDS};
use super::{images_of, messages, picture, serving, unbase64, words};
use crate::net::testing::Pki;
use crate::service::testing::scale_of;

/// Every recorded exchange, by its set and its number, in order.
fn exchanges() -> Vec<(String, Value)> {
    crate::service::testing::exchanges("anthropic-messages")
}

/// A recorded body as bytes.
fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

/// A text block of words alone, read as a text part; any other block is kept as it
/// came, as the reader keeps it.
fn is_words(block: &Value) -> bool {
    block.get("type") == Some(&json!("text"))
        && block.as_object().is_some_and(|map| map.len() == 2)
        && block
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty())
}

fn opaque(block: &Value) -> Part {
    Part::Opaque(Opaque::new(ADAPTER, &serde_json::to_string(block).unwrap()).unwrap())
}

#[test]
fn each_recorded_exchange_of_squint_is_read_back_to_its_turn() {
    let exchanges = exchanges();
    assert_eq!(exchanges.len(), 29);
    let pki = Arc::new(Pki::loopback());
    let answers = exchanges
        .iter()
        .map(|(_, exchange)| {
            let body = bytes(&exchange["response"]["body"]);
            super::answer("200 OK", "request-id: req_recorded\r\n", &body)
        })
        .collect();
    let (listener, _) = serving(&pki, answers);
    for (name, exchange) in &exchanges {
        assert_eq!(exchange["response"]["status"], 200, "{name}");
        let recorded = &exchange["response"]["body"];
        let turn = super::ask(
            &pki,
            messages(listener.port(), "claude-haiku-5-5", None),
            &words(),
        )
        .unwrap_or_else(|fault| panic!("{name}: {fault:?}"));
        // Every block in order: words as a text part; a call as its block, kept, and
        // the call beside it; every other block kept as it came.
        let mut expected = Vec::new();
        for block in recorded["content"].as_array().unwrap() {
            if is_words(block) {
                expected.push(Part::Text(block["text"].as_str().unwrap().to_owned()));
            } else if block["type"] == "tool_use" {
                expected.push(opaque(block));
                let call = ToolCall::new(
                    block["id"].as_str().unwrap(),
                    block["name"].as_str().unwrap(),
                    block["input"].clone(),
                );
                expected.push(Part::ToolCall(call.unwrap()));
            } else {
                expected.push(opaque(block));
            }
        }
        assert_eq!(turn.parts(), expected.as_slice(), "{name}");
        // A kept block is the recorded block, as JSON.
        for part in turn.parts() {
            if let Part::Opaque(kept) = part {
                let block: Value = serde_json::from_str(kept.body()).unwrap();
                assert!(
                    recorded["content"].as_array().unwrap().contains(&block),
                    "{name}"
                );
            }
        }
        let stop = match recorded["stop_reason"].as_str().unwrap() {
            "end_turn" => StopReason::EndTurn,
            "tool_use" => StopReason::ToolUse,
            "max_tokens" => StopReason::MaxTokens,
            "refusal" => StopReason::Refusal,
            other => panic!("{name}: a stop reason the test does not know: {other}"),
        };
        assert_eq!(turn.stop(), &stop, "{name}");
        let usage = &recorded["usage"];
        let count = |field: &str| usage[field].as_u64().unwrap();
        let read = turn.usage().unwrap();
        assert_eq!(
            (
                read.input_tokens(),
                read.output_tokens(),
                read.cache_read_tokens(),
                read.cache_write_tokens()
            ),
            (
                count("input_tokens"),
                count("output_tokens"),
                count("cache_read_input_tokens"),
                count("cache_creation_input_tokens")
            ),
            "{name}"
        );
        assert_eq!(turn.request_id(), recorded["id"].as_str(), "{name}");
        // A refusal's explanation is the provider's words, and is not read.
        let words: Vec<&str> = turn.texts().collect();
        assert!(words.iter().all(|text| !text.contains("legal")), "{name}");
    }
    assert_eq!(listener.finish(), exchanges.len());
}

/// The request a recorded body was written from, as whizbang holds one, with the
/// effort it names; none for a body that names a control a [`JudgeRequest`] does
/// not carry: a forced tool choice, or a removal of tools.
fn request_of(body: &Value) -> Option<(JudgeRequest, Option<String>)> {
    if body.get("tool_choice").is_some() || body.to_string().contains("\"tool_removal\"") {
        return None;
    }
    let system: Vec<&str> = body["system"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["text"].as_str().unwrap())
        .collect();
    assert_eq!(system.len(), 1);
    let pictured = |block: &Value| {
        let data = block["source"]["data"].as_str().unwrap();
        let size = data
            .strip_prefix("<picture ")
            .and_then(|rest| rest.strip_suffix('>'))
            .unwrap();
        let (w, h) = size.split_once('x').unwrap();
        let (w, h): (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
        picture(w / scale_of(w), h / scale_of(w), false)
    };
    let mut messages = Vec::new();
    for message in body["messages"].as_array().unwrap() {
        let blocks = message["content"].as_array().unwrap();
        let (role, parts) = match message["role"].as_str().unwrap() {
            "system" => {
                let parts = blocks
                    .iter()
                    .map(|block| Part::Text(block["text"].as_str().unwrap().to_owned()))
                    .collect();
                (Role::System, parts)
            }
            "user" => {
                let mut parts = Vec::new();
                for block in blocks {
                    match block["type"].as_str().unwrap() {
                        "text" => {
                            parts.push(Part::Text(block["text"].as_str().unwrap().to_owned()))
                        }
                        "image" => parts.push(Part::Image(pictured(block))),
                        "tool_result" => {
                            let (text, images) = match &block["content"] {
                                Value::String(text) => (text.clone(), Vec::new()),
                                Value::Array(items) => {
                                    let text = items
                                        .iter()
                                        .filter(|item| item["type"] == "text")
                                        .map(|item| item["text"].as_str().unwrap())
                                        .collect::<String>();
                                    let images = items
                                        .iter()
                                        .filter(|item| item["type"] == "image")
                                        .map(pictured)
                                        .collect();
                                    (text, images)
                                }
                                other => panic!("a tool result of {other}"),
                            };
                            let error = block.get("is_error") == Some(&json!(true));
                            let id = block["tool_use_id"].as_str().unwrap();
                            parts.push(Part::ToolResult(
                                ToolResult::new(id, &text, error, images).unwrap(),
                            ));
                        }
                        other => panic!("a user block of {other}"),
                    }
                }
                (Role::User, parts)
            }
            "assistant" => {
                let mut parts = Vec::new();
                for block in blocks {
                    if is_words(block) {
                        parts.push(Part::Text(block["text"].as_str().unwrap().to_owned()));
                    } else if block["type"] == "tool_use" {
                        parts.push(opaque(block));
                        let call = ToolCall::new(
                            block["id"].as_str().unwrap(),
                            block["name"].as_str().unwrap(),
                            block["input"].clone(),
                        );
                        parts.push(Part::ToolCall(call.unwrap()));
                    } else {
                        parts.push(opaque(block));
                    }
                }
                (Role::Assistant, parts)
            }
            other => panic!("a role of {other}"),
        };
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
                tool["input_schema"].clone(),
            )
            .unwrap()
        })
        .collect();
    let most = u32::try_from(body["max_tokens"].as_u64().unwrap()).unwrap();
    let request =
        JudgeRequest::new(system[0], messages, tools, Controls::new(most).unwrap()).unwrap();
    let effort = body["output_config"]["effort"].as_str().map(str::to_owned);
    Some((request, effort))
}

/// The recorded body as whizbang writes one: no `strict` on a tool, which a
/// [`JudgeRequest`] does not carry.
fn without_strict(body: &Value) -> Value {
    let mut body = body.clone();
    for tool in body["tools"].as_array_mut().unwrap() {
        tool.as_object_mut().unwrap().remove("strict");
    }
    body
}

/// A sent body with each picture's bytes written as the recorded body names its
/// picture, `<picture WxH>`, once the JPEG it is states the size the test made of
/// that recorded size ([`scale_of`]).
fn pictures_named(sent: &Value, recorded: &Value) -> Value {
    fn walk(value: &mut Value, names: &[String], at: &mut usize) {
        match value {
            Value::Object(map) => {
                if map.get("type") == Some(&json!("base64")) {
                    let data = map["data"].as_str().unwrap();
                    let read = image::load_from_memory(&unbase64(data)).unwrap();
                    let name = &names[*at];
                    let (w, h) = name
                        .strip_prefix("<picture ")
                        .and_then(|rest| rest.strip_suffix('>'))
                        .and_then(|size| size.split_once('x'))
                        .unwrap();
                    let (w, h): (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
                    let made = (w / scale_of(w), h / scale_of(w));
                    assert_eq!((read.width(), read.height()), made);
                    map.insert("data".to_owned(), Value::String(name.clone()));
                    *at += 1;
                }
                map.values_mut().for_each(|value| walk(value, names, at));
            }
            Value::Array(values) => values.iter_mut().for_each(|value| walk(value, names, at)),
            _ => {}
        }
    }
    let names: Vec<String> = images_of(recorded)
        .into_iter()
        .map(|(data, _)| data)
        .collect();
    let mut body = sent.clone();
    let mut at = 0;
    walk(&mut body, &names, &mut at);
    assert_eq!(at, names.len());
    body
}

#[test]
fn a_request_is_written_as_the_recorded_request_was() {
    let exchanges = exchanges();
    let written: Vec<(String, JudgeRequest, Option<String>, Value)> = exchanges
        .iter()
        .filter_map(|(name, exchange)| {
            let body = &exchange["request"]["body"];
            let (request, effort) = request_of(body)?;
            Some((name.clone(), request, effort, exchange.clone()))
        })
        .collect();
    // Three name a control a request of the core does not carry: a forced tool
    // choice (one) and a removal of tools (two).
    assert_eq!(written.len(), exchanges.len() - 3);
    let pki = Arc::new(Pki::loopback());
    let answers = written
        .iter()
        .map(|(_, _, _, exchange)| {
            super::answer("200 OK", "", &bytes(&exchange["response"]["body"]))
        })
        .collect();
    let (listener, log) = serving(&pki, answers);
    for (name, request, effort, exchange) in &written {
        let recorded = &exchange["request"];
        let model = recorded["body"]["model"].as_str().unwrap();
        let backend = messages(listener.port(), model, effort.as_deref());
        super::ask(&pki, backend, request).unwrap_or_else(|fault| panic!("{name}: {fault:?}"));
        let seen = log.lock().unwrap().last().unwrap().clone();
        let sent: Value = serde_json::from_slice(&seen.body).unwrap();
        assert_eq!(
            pictures_named(&sent, &recorded["body"]),
            without_strict(&recorded["body"]),
            "{name}"
        );
        assert_eq!(
            images_of(&sent).len(),
            images_of(&recorded["body"]).len(),
            "{name}"
        );
        // The recorded fields, but the key's, are among those sent, with their values.
        let head = seen.head.to_ascii_lowercase();
        for (field, value) in recorded["headers"].as_object().unwrap_or(&Map::new()) {
            if field == "x-api-key" || field == "anthropic-beta" && value != FIELDS[2].1 {
                continue;
            }
            let line = format!("{field}: {}\r\n", value.as_str().unwrap()).to_ascii_lowercase();
            assert!(head.contains(&line), "{name}: {field}");
        }
    }
    assert_eq!(listener.finish(), written.len());
}

#[test]
fn the_input_count_is_at_least_the_usage_of_every_recorded_exchange() {
    // The count as the adapter makes it, of each recorded request: its words as they
    // were written, less each picture's data, and each picture by its recorded size,
    // fitted as the adapter fits it; against the input the provider reported for it,
    // read from its cache and written to it included.
    use super::super::{LONG_EDGE, spend};
    use whizbang_core::egress::fitted;
    for (name, exchange) in exchanges() {
        let mut body = exchange["request"]["body"].clone();
        let pictures: Vec<(u32, u32)> = images_of(&body)
            .iter()
            .map(|(data, _)| {
                let (w, h) = data
                    .strip_prefix("<picture ")
                    .and_then(|rest| rest.strip_suffix('>'))
                    .and_then(|size| size.split_once('x'))
                    .unwrap();
                fitted(w.parse().unwrap(), h.parse().unwrap(), LONG_EDGE)
            })
            .collect();
        fn emptied(value: &mut Value) {
            match value {
                Value::Object(map) => {
                    if map.get("type") == Some(&json!("base64")) {
                        map.insert("data".to_owned(), Value::String(String::new()));
                    }
                    map.values_mut().for_each(emptied);
                }
                Value::Array(values) => values.iter_mut().for_each(emptied),
                _ => {}
            }
        }
        emptied(&mut body);
        let words = serde_json::to_vec(&body).unwrap().len() as u64;
        let messages = body["messages"].as_array().unwrap().len() as u64;
        let counted = spend::input_count(words, messages, &pictures);
        let usage = &exchange["response"]["body"]["usage"];
        let used = [
            "input_tokens",
            "cache_read_input_tokens",
            "cache_creation_input_tokens",
        ]
        .iter()
        .map(|field| usage[field].as_u64().unwrap_or(0))
        .sum::<u64>();
        assert!(counted >= used, "{name}: counted {counted}, used {used}");
    }
}
