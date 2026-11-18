//! `conformance::…`: squint's recorded exchanges of its Gemini adapter
//! (`fixtures/adapters/gemini-generate-content/`, at the commit `docs/sources.toml`
//! pins), served by a recorded fake on loopback.
//!
//! Ported as `tests/fixtures/gemini-generate-content/`, reshaped as the Messages
//! exchanges were: each request's pictures are written as `<picture WxH>`, their sizes
//! as the recording's JPEG states them, in place of their bytes; each keeps its request's
//! body and headers (the key's written as `<redacted>`) and its answer's body and status;
//! nothing else was kept or changed.

use std::sync::Arc;

use serde_json::{Value, json};
use whizbang_core::capability::{
    Controls, JudgeRequest, Message, Opaque, Part, Role, StopReason, ToolCall, ToolDefinition,
    ToolResult,
};
use whizbang_core::egress::fitted;

use super::super::{ADAPTER, FIELDS, LONG_EDGE, spend};
use super::generate;
use crate::net::testing::Pki;
use crate::service::testing::{
    answer, ask, exchanges, named_size, picture_named, scale_of, serving, unbase64, words,
};

const SET: &str = "gemini-generate-content";

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

fn opaque(part: &Value) -> Part {
    Part::Opaque(Opaque::new(ADAPTER, &serde_json::to_string(part).unwrap()).unwrap())
}

/// A part of words alone.
fn is_words(part: &Value) -> bool {
    part.as_object().is_some_and(|map| map.len() == 1)
        && part
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty())
}

/// A model's part as the reader keeps it: words as a text part; a call as its part,
/// kept, and the call beside it; every other part kept as it came.
fn model_parts(part: &Value) -> Vec<Part> {
    if is_words(part) {
        return vec![Part::Text(part["text"].as_str().unwrap().to_owned())];
    }
    match part.get("functionCall") {
        Some(call) => {
            let id = call["id"].as_str().unwrap();
            let call = ToolCall::new(id, call["name"].as_str().unwrap(), call["args"].clone());
            vec![opaque(part), Part::ToolCall(call.unwrap())]
        }
        None => vec![opaque(part)],
    }
}

#[test]
fn each_recorded_exchange_of_squint_is_read_back_to_its_turn() {
    let exchanges = exchanges(SET);
    assert_eq!(exchanges.len(), 22);
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
            generate(listener.port(), "gemini-3.8-flash", None),
            &words(),
        )
        .unwrap_or_else(|fault| panic!("{name}: {fault:?}"));
        let candidate = &recorded["candidates"][0];
        let expected: Vec<Part> = candidate["content"]["parts"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(model_parts)
            .collect();
        assert_eq!(turn.parts(), expected.as_slice(), "{name}");
        let calls = turn.tool_calls().count();
        let stop = match (candidate["finishReason"].as_str().unwrap(), calls) {
            ("STOP", 0) => StopReason::EndTurn,
            ("STOP", _) => StopReason::ToolUse,
            ("MAX_TOKENS", _) => StopReason::MaxTokens,
            (other, _) => panic!("{name}: a finish reason the test does not know: {other}"),
        };
        assert_eq!(turn.stop(), &stop, "{name}");
        let usage = &recorded["usageMetadata"];
        let count = |field: &str| usage[field].as_u64().unwrap_or(0);
        let read = turn.usage().unwrap();
        assert_eq!(
            (
                read.input_tokens(),
                read.output_tokens(),
                read.cache_read_tokens(),
                read.cache_write_tokens()
            ),
            (
                count("promptTokenCount") - count("cachedContentTokenCount"),
                count("candidatesTokenCount") + count("thoughtsTokenCount"),
                count("cachedContentTokenCount"),
                0
            ),
            "{name}"
        );
        assert_eq!(turn.request_id(), recorded["responseId"].as_str(), "{name}");
    }
    assert_eq!(listener.finish(), exchanges.len());
}

/// The request a recorded body was written from, as whizbang holds one, with the
/// thinking level it names; none for a body whose function mode is not `AUTO`, a
/// forced call that a [`JudgeRequest`] does not carry.
fn request_of(body: &Value) -> Option<(JudgeRequest, Option<String>)> {
    if body["toolConfig"]["functionCallingConfig"]["mode"] != "AUTO" {
        return None;
    }
    let system = body["systemInstruction"]["parts"][0]["text"]
        .as_str()
        .unwrap();
    let mut messages = Vec::new();
    for content in body["contents"].as_array().unwrap() {
        let parts = content["parts"].as_array().unwrap();
        let message = match content["role"].as_str().unwrap() {
            "user" => {
                let mut said = Vec::new();
                for part in parts {
                    if let Some(text) = part.get("text") {
                        said.push(Part::Text(text.as_str().unwrap().to_owned()));
                    } else if let Some(data) = part.get("inlineData") {
                        said.push(Part::Image(picture_named(data["data"].as_str().unwrap())));
                    } else {
                        let response = &part["functionResponse"];
                        let (text, error) = match response["response"].get("error") {
                            Some(error) => (error.as_str().unwrap(), true),
                            None => (response["response"]["output"].as_str().unwrap(), false),
                        };
                        let images = response
                            .get("parts")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .map(|part| picture_named(part["inlineData"]["data"].as_str().unwrap()))
                            .collect();
                        let id = response["id"].as_str().unwrap();
                        said.push(Part::ToolResult(
                            ToolResult::new(id, text, error, images).unwrap(),
                        ));
                    }
                }
                Message::new(Role::User, said)
            }
            "model" => Message::new(
                Role::Assistant,
                parts.iter().flat_map(model_parts).collect(),
            ),
            other => panic!("a role of {other}"),
        };
        messages.push(message.unwrap());
    }
    let tools = body["tools"][0]["functionDeclarations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| {
            ToolDefinition::new(
                tool["name"].as_str().unwrap(),
                tool["description"].as_str().unwrap(),
                tool["parametersJsonSchema"].clone(),
            )
            .unwrap()
        })
        .collect();
    let generation = &body["generationConfig"];
    let most = u32::try_from(generation["maxOutputTokens"].as_u64().unwrap()).unwrap();
    let request = JudgeRequest::new(system, messages, tools, Controls::new(most).unwrap()).unwrap();
    let effort = generation["thinkingConfig"]["thinkingLevel"]
        .as_str()
        .map(str::to_owned);
    Some((request, effort))
}

/// A sent body with each picture's bytes written as the recorded body names its picture,
/// once the JPEG it is states the size the test made of that recorded size.
fn pictures_named(sent: &Value, recorded: &Value) -> Value {
    fn names(value: &Value, found: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(data)) = map.get("inlineData").map(|d| &d["data"]) {
                    found.push(data.clone());
                }
                map.values().for_each(|value| names(value, found));
            }
            Value::Array(values) => values.iter().for_each(|value| names(value, found)),
            _ => {}
        }
    }
    fn walk(value: &mut Value, names: &[String], at: &mut usize) {
        match value {
            Value::Object(map) => {
                if let Some(data) = map.get_mut("inlineData") {
                    let read =
                        image::load_from_memory(&unbase64(data["data"].as_str().unwrap())).unwrap();
                    let (w, h) = named_size(&names[*at]).unwrap();
                    assert_eq!(
                        (read.width(), read.height()),
                        (w / scale_of(w), h / scale_of(w))
                    );
                    data["data"] = json!(names[*at]);
                    *at += 1;
                    return;
                }
                map.values_mut().for_each(|value| walk(value, names, at));
            }
            Value::Array(values) => values.iter_mut().for_each(|value| walk(value, names, at)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    names(recorded, &mut found);
    let mut body = sent.clone();
    let mut at = 0;
    walk(&mut body, &found, &mut at);
    assert_eq!(at, found.len());
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
    // One forces the call of a function, which a request of the core does not carry.
    assert_eq!(written.len(), exchanges.len() - 1);
    let pki = Arc::new(Pki::loopback());
    let answers = written
        .iter()
        .map(|(_, _, _, exchange)| answer("200 OK", "", &bytes(&exchange["response"]["body"])))
        .collect();
    let (listener, log) = serving(&pki, answers);
    for (name, request, effort, exchange) in &written {
        let recorded = &exchange["request"];
        let backend = generate(listener.port(), "gemini-3.8-flash", effort.as_deref());
        ask(&pki, backend, request).unwrap_or_else(|fault| panic!("{name}: {fault:?}"));
        let seen = log.lock().unwrap().last().unwrap().clone();
        let sent: Value = serde_json::from_slice(&seen.body).unwrap();
        assert_eq!(
            pictures_named(&sent, &recorded["body"]),
            recorded["body"],
            "{name}"
        );
        // The recorded fields, but the key's, are among those sent, with their values;
        // the key's is the executor's to attach, and no field of the request names it.
        let head = seen.head.to_ascii_lowercase();
        for (field, value) in recorded["headers"].as_object().unwrap() {
            if field == "x-goog-api-key" {
                assert!(!head.contains("x-goog-api-key"), "{name}");
                continue;
            }
            let line = format!("{field}: {}\r\n", value.as_str().unwrap());
            assert!(head.contains(&line), "{name}: {field}");
        }
        assert_eq!(
            FIELDS.len(),
            recorded["headers"].as_object().unwrap().len() - 1
        );
        assert!(head.starts_with("post /v1beta/models/gemini-3.8-flash:generatecontent "));
    }
    assert_eq!(listener.finish(), written.len());
}

#[test]
fn the_input_count_is_at_least_the_usage_of_every_recorded_exchange() {
    // The count as the adapter makes it, of each recorded request: its words as they
    // were written, less each picture's data, and each picture by its recorded size,
    // fitted as the adapter fits it; against the whole prompt the provider reported for
    // it, what was read from its cache included.
    fn emptied(value: &mut Value, pictures: &mut Vec<(u32, u32)>) {
        match value {
            Value::Object(map) => {
                if let Some(data) = map.get_mut("inlineData") {
                    let (w, h) = named_size(data["data"].as_str().unwrap()).unwrap();
                    pictures.push(fitted(w, h, LONG_EDGE));
                    data["data"] = json!("");
                    return;
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
        let messages = body["contents"].as_array().unwrap().len() as u64;
        let counted = spend::input_count(words, messages, &pictures);
        let used = exchange["response"]["body"]["usageMetadata"]["promptTokenCount"]
            .as_u64()
            .unwrap();
        assert!(counted >= used, "{name}: counted {counted}, used {used}");
    }
}
