//! The tests of the judge of a service (plan 2A, task S1), each against a server on
//! loopback that the test kit's listener runs, with a certificate of a root made in
//! the test (`net::testing`). Nothing here contacts a host off the machine.
//!
//! A backend is asked through a judge's handle, and sends through the client as a
//! bare sender: the gate and the executor's credential are the binary's wiring's, and
//! are tested there (`crates/whizbang/tests/it/anthropic.rs`, `wired.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod conformance;

use std::sync::Arc;

use serde_json::{Value, json};
use whizbang_core::capability::{
    Controls, Declaration, FaultKind, JudgeRequest, Message, Part, Role, Stage, ToolResult,
};
use whizbang_core::contract::{ModelIdentity, Secrets};
use whizbang_core::egress::Address;

use super::{ADAPTER, LONG_EDGE, VERSION, wire};
use crate::net::testing::Pki;
use crate::service::testing::{
    answer, ask, faults_hold_nothing, picture, serving, unbase64, words,
};
use crate::service::{Service as Messages, Wire};

/// A backend of the kind at a port of loopback, for this model and effort.
fn messages(port: u16, model: &str, effort: Option<&str>) -> Messages {
    let identity = ModelIdentity::new(ADAPTER, model, VERSION);
    Messages {
        declaration: Declaration::new(identity, &["image", "tool"]).unwrap(),
        address: Address::of(
            &format!("https://127.0.0.1:{port}/v1/messages"),
            &Secrets::none(),
        )
        .unwrap(),
        wire: Wire::Messages(wire::Settings {
            model: model.to_owned(),
            effort: effort.map(str::to_owned),
            long_edge: LONG_EDGE,
            max_output_tokens: 128_000,
        }),
        ceilings: super::spend::Ceilings {
            input_tokens: u64::MAX,
            output_tokens: 128_000,
            pictures: 64,
        },
        prices: super::spend::prices_of("claude-haiku-5-5").unwrap(),
    }
}

/// Every image block's `data` and `media_type` in a body, in order.
fn images_of(body: &Value) -> Vec<(String, String)> {
    fn walk(value: &Value, found: &mut Vec<(String, String)>) {
        match value {
            Value::Object(map) => {
                if map.get("type") == Some(&json!("base64"))
                    && let (Some(Value::String(data)), Some(Value::String(media))) =
                        (map.get("data"), map.get("media_type"))
                {
                    found.push((data.clone(), media.clone()));
                }
                map.values().for_each(|value| walk(value, found));
            }
            Value::Array(values) => values.iter().for_each(|value| walk(value, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(body, &mut found);
    found
}

#[test]
fn a_fault_holds_nothing_of_the_body_the_headers_or_the_address() {
    faults_hold_nothing(
        |port| messages(port, "claude-haiku-5-5", None),
        wire::NOT_A_RESPONSE,
    );
}

#[test]
fn each_picture_in_the_sent_body_decodes_to_the_bytes_of_its_checked_piece() {
    let pki = Arc::new(Pki::loopback());
    let result = ToolResult::new(
        "toolu_1",
        "a crop",
        false,
        vec![picture(40, 30, false), picture(20, 20, true)],
    )
    .unwrap();
    let call = whizbang_core::capability::ToolCall::new("toolu_1", "crop", json!({})).unwrap();
    let conversation = vec![
        Message::new(
            Role::User,
            vec![
                Part::Text("Image one.".to_owned()),
                Part::Image(picture(2700, 100, false)),
                Part::Image(picture(64, 48, false)),
            ],
        )
        .unwrap(),
        Message::new(Role::Assistant, vec![Part::ToolCall(call)]).unwrap(),
        Message::new(Role::User, vec![Part::ToolResult(result)]).unwrap(),
    ];
    let request = JudgeRequest::new(
        "Judge.",
        conversation,
        Vec::new(),
        Controls::new(64).unwrap(),
    )
    .unwrap();
    let turn = json!({
        "id": "msg_1", "type": "message", "role": "assistant", "model": "m",
        "content": [{ "type": "text", "text": "Seen." }],
        "stop_reason": "end_turn", "usage": { "input_tokens": 9, "output_tokens": 2 }
    });
    let (listener, log) = serving(
        &pki,
        vec![answer("200 OK", "", turn.to_string().as_bytes())],
    );
    let backend = messages(listener.port(), "claude-haiku-5-5", None);
    // The pieces the gate is handed for this request: written again, they are the
    // same, since the writer and the encoder make the same bytes of the same request.
    let Wire::Messages(settings) = &backend.wire else {
        unreachable!()
    };
    let pieces = wire::request(&request, settings).unwrap().pieces;
    let pictures: Vec<Vec<u8>> = pieces
        .iter()
        .filter(|piece| piece.is_picture())
        .map(|piece| piece.bytes().to_vec())
        .collect();
    assert_eq!(pictures.len(), 4);
    let got = ask(&pki, backend, &request).unwrap();
    assert_eq!(got.texts().collect::<Vec<_>>(), ["Seen."]);
    assert_eq!(listener.finish(), 1);
    let sent: Value = serde_json::from_slice(&log.lock().unwrap()[0].body).unwrap();
    let images = images_of(&sent);
    assert_eq!(images.len(), 4);
    for (at, ((data, _), piece)) in images.iter().zip(&pictures).enumerate() {
        assert_eq!(&unbase64(data), piece, "picture {at}");
    }
    // The first is fitted to the long edge and sent as a JPEG; the one with clear
    // pixels as a PNG; the media type of each block says which.
    let first = image::load_from_memory(&pictures[0]).unwrap();
    assert_eq!((first.width(), first.height()), (LONG_EDGE, 95));
    assert!(pictures[3].starts_with(b"\x89PNG"));
    let media: Vec<&str> = images.iter().map(|(_, media)| media.as_str()).collect();
    assert_eq!(
        media,
        ["image/jpeg", "image/jpeg", "image/jpeg", "image/png"]
    );
}

/// A backend at this port with these ceilings.
fn ceiled(port: u16, input_tokens: u64, output_tokens: u32, pictures: usize) -> Messages {
    let mut backend = messages(port, "claude-haiku-5-5", None);
    backend.ceilings = super::spend::Ceilings {
        input_tokens,
        output_tokens,
        pictures,
    };
    if let Wire::Messages(settings) = &mut backend.wire {
        settings.max_output_tokens = output_tokens;
    }
    backend
}

#[test]
fn a_request_counted_over_its_input_or_picture_ceiling_is_refused_not_sent() {
    let pki = Arc::new(Pki::loopback());
    // A server that must see no connection: each request is refused before one.
    let (listener, _) = serving(&pki, vec![answer("200 OK", "", b"{}")]);
    let port = listener.port();
    // Words over the input ceiling, by the count of one token a byte.
    let long = Message::new(Role::User, vec![Part::Text("x".repeat(5_000))]).unwrap();
    let request = JudgeRequest::new("Judge.", vec![long], Vec::new(), Controls::new(64).unwrap());
    let fault = ask(&pki, ceiled(port, 4_000, 64, 24), &request.unwrap()).unwrap_err();
    assert_eq!(
        (fault.kind(), fault.stage(), fault.sentence()),
        (FaultKind::TooLarge, Stage::NotSent, super::OVER_INPUT)
    );
    // Three pictures over a ceiling of two.
    let pictures = vec![
        Part::Image(picture(8, 8, false)),
        Part::Image(picture(8, 8, false)),
        Part::Image(picture(8, 8, false)),
    ];
    let message = Message::new(Role::User, pictures).unwrap();
    let request = JudgeRequest::new("", vec![message], Vec::new(), Controls::new(64).unwrap());
    let fault = ask(&pki, ceiled(port, 40_000, 64, 2), &request.unwrap()).unwrap_err();
    assert_eq!(
        (fault.kind(), fault.stage(), fault.sentence()),
        (FaultKind::TooLarge, Stage::NotSent, super::OVER_PICTURES)
    );
    // A picture counts by the formula on its prepared size: one of 2,576 by 1,932 is
    // 4,784 tokens and its overhead, over a ceiling of 4,000.
    let big = Message::new(Role::User, vec![Part::Image(picture(2576, 1932, false))]).unwrap();
    let request = JudgeRequest::new("", vec![big], Vec::new(), Controls::new(64).unwrap());
    let fault = ask(&pki, ceiled(port, 4_000, 64, 24), &request.unwrap()).unwrap_err();
    assert_eq!(fault.sentence(), super::OVER_INPUT);
    // Nothing reached the server.
    assert_eq!(listener.finish(), 0);
}

#[test]
fn a_request_asking_more_output_than_the_ceiling_is_sent_with_the_ceiling() {
    let pki = Arc::new(Pki::loopback());
    let turn = json!({
        "id": "msg_1", "content": [{ "type": "text", "text": "Seen." }],
        "stop_reason": "end_turn", "usage": { "input_tokens": 9, "output_tokens": 2 }
    });
    let ok = answer("200 OK", "", turn.to_string().as_bytes());
    let (listener, log) = serving(&pki, vec![ok.clone(), ok]);
    let port = listener.port();
    let asking = |most: u32| {
        let message = Message::new(Role::User, vec![Part::Text("Look.".to_owned())]).unwrap();
        JudgeRequest::new("", vec![message], Vec::new(), Controls::new(most).unwrap()).unwrap()
    };
    // A first request, and a retry that asks four times as much: each is sent with
    // the ceiling at most.
    ask(&pki, ceiled(port, 40_000, 2_048, 24), &asking(1_024)).unwrap();
    ask(&pki, ceiled(port, 40_000, 2_048, 24), &asking(8_192)).unwrap();
    assert_eq!(listener.finish(), 2);
    let sent: Vec<Value> = log
        .lock()
        .unwrap()
        .iter()
        .map(|seen| serde_json::from_slice(&seen.body).unwrap())
        .collect();
    assert_eq!(sent[0]["max_tokens"], 1_024);
    assert_eq!(sent[1]["max_tokens"], 2_048);
}

#[test]
fn an_unlisted_success_status_is_charged_its_bound() {
    // A status of success other than 200, with a body that is a turn: the adapter does
    // not read it, and the request, which the provider may have charged, settles at
    // its bound, as an unreadable answer of 200 does, and never at nothing.
    let pki = Arc::new(Pki::loopback());
    let turn = json!({
        "id": "msg_1", "content": [{ "type": "text", "text": "Seen." }],
        "stop_reason": "end_turn", "usage": { "input_tokens": 9, "output_tokens": 2 }
    });
    let body = turn.to_string();
    let answers = [
        "201 Created",
        "203 Non-Authoritative Information",
        "299 Odd",
    ]
    .map(|status| answer(status, "", body.as_bytes()));
    let (listener, _) = serving(&pki, answers.to_vec());
    let port = listener.port();
    for _ in 0..answers.len() {
        let fault = ask(&pki, messages(port, "claude-haiku-5-5", None), &words()).unwrap_err();
        assert_eq!(fault.kind(), FaultKind::Malformed);
        assert_eq!(fault.sentence(), super::OTHER_SUCCESS);
        assert_eq!(fault.stage(), whizbang_core::capability::Stage::Answered);
        assert_eq!(fault.charge(), None);
    }
    assert_eq!(listener.finish(), answers.len());
    // An unreadable answer of 200 settles the same way.
    let (listener, _) = serving(&pki, vec![answer("200 OK", "", b"not a turn")]);
    let fault = ask(
        &pki,
        messages(listener.port(), "claude-haiku-5-5", None),
        &words(),
    )
    .unwrap_err();
    assert_eq!(
        (fault.stage(), fault.charge()),
        (whizbang_core::capability::Stage::Answered, None)
    );
    assert_eq!(listener.finish(), 1);
}

#[test]
fn submission_v3_is_sent_as_the_build_wrote_it() {
    // Plan mE, task F1, item 4: the conformance fixture of v3 for this adapter.
    use crate::service::testing::{body_of, v3_request};
    let (request, schema) = v3_request();
    let backend = messages(1, "claude-haiku-5-5", None);
    let Wire::Messages(settings) = &backend.wire else {
        unreachable!()
    };
    let sent = body_of(&wire::request(&request, settings).unwrap().pieces);
    let tools = sent["tools"].as_array().unwrap();
    let submit = tools
        .iter()
        .find(|t| t["name"] == "submit_verdict")
        .unwrap();
    assert_eq!(submit["input_schema"], schema);
}
