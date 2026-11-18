//! The tests of the judge of a service (plan 2A, task S1), each against a server on
//! loopback that the test kit's listener runs, with a certificate of a root made in
//! the test (`net::testing`). Nothing here contacts a host off the machine.
//!
//! A backend is asked through a judge's handle, and sends through the client as a
//! bare sender: the gate and the executor's credential are the binary's wiring's, and
//! are tested there (`crates/whizbang/tests/it/anthropic.rs`, `wired.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod conformance;

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use whizbang_core::capability::{
    Controls, Declaration, Fault, FaultKind, Judge, JudgeHandle, JudgeRequest, Message, Part,
    Picture, Role, Stage, ToolResult, Turn,
};
use whizbang_core::contract::{Cancellation, ModelIdentity, Secrets};
use whizbang_core::egress::{Address, Sender};
use whizbang_core::identity::CanonicalPixels;
use whizbang_testkit::loopback::Listener;

use super::{
    ADAPTER, BUSY, CREDENTIAL, LONG_EDGE, Messages, RATE_LIMITED, REFUSED, STATUS, VERSION, wire,
};
use crate::net::testing::{Pki, sender_trusting};

/// A judge that asks a backend of the kind through the test's sender.
struct Through {
    messages: Messages,
    sender: Arc<dyn Sender>,
}

impl Judge for Through {
    fn declaration(&self) -> &Declaration {
        &self.messages.declaration
    }

    fn send(
        &self,
        request: &JudgeRequest,
        bounds: &whizbang_core::capability::Bounds,
    ) -> Result<Turn, Fault> {
        self.messages.turn_through(request, &*self.sender, bounds)
    }
}

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
        settings: wire::Settings {
            model: model.to_owned(),
            effort: effort.map(str::to_owned),
            long_edge: LONG_EDGE,
            max_output_tokens: 128_000,
        },
        ceilings: super::spend::Ceilings {
            input_tokens: u64::MAX,
            output_tokens: 128_000,
            pictures: 64,
        },
        prices: super::spend::prices_of("claude-haiku-5-5").unwrap(),
    }
}

/// Asks the backend once, through a handle.
fn ask(pki: &Pki, messages: Messages, request: &JudgeRequest) -> Result<Turn, Fault> {
    let judge = Arc::new(Through {
        messages,
        sender: sender_trusting(pki),
    });
    JudgeHandle::new(judge, Cancellation::new()).send(request)
}

/// What a server saw of one request: its head and its body.
#[derive(Debug, Clone, Default)]
struct Seen {
    head: String,
    body: Vec<u8>,
}

type Log = Arc<Mutex<Vec<Seen>>>;

/// A listener that answers each of its connections with the next of `answers`, and
/// keeps what each request was. The server's side ends after its answer.
fn serving(pki: &Arc<Pki>, answers: Vec<Vec<u8>>) -> (Listener, Log) {
    let log: Log = Arc::default();
    let (kept, pki) = (log.clone(), pki.clone());
    let listener = Listener::serve(answers.len(), move |number, stream| {
        let mut tls = pki.accept(stream);
        let mut head = Vec::new();
        let mut byte = [0];
        while !head.ends_with(b"\r\n\r\n") && head.len() < 64 * 1024 {
            match tls.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                _ => break,
            }
        }
        let head = String::from_utf8_lossy(&head).into_owned();
        let length = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            })
            .unwrap_or(0);
        let mut body = vec![0; length];
        let read = tls.read_exact(&mut body);
        kept.lock().unwrap().push(Seen { head, body });
        if read.is_ok() && tls.write_all(&answers[number]).is_ok() {
            tls.conn.send_close_notify();
            let _ = tls.flush();
        }
    });
    (listener, log)
}

/// An answer of this status line, these fields and this body.
fn answer(status: &str, fields: &str, body: &[u8]) -> Vec<u8> {
    [
        format!(
            "HTTP/1.1 {status}\r\n{fields}Content-Length: {}\r\n\r\n",
            body.len()
        )
        .as_bytes(),
        body,
    ]
    .concat()
}

/// A picture of this size with something in it, every pixel opaque or every third
/// one half clear.
fn picture(width: u32, height: u32, clear: bool) -> Picture {
    let pixels = width as usize * height as usize;
    let rgba = (0..pixels)
        .flat_map(|at| {
            let alpha = if clear && at % 3 == 0 { 100 } else { 255 };
            [(at * 5) as u8, (at / 7) as u8, (at % 199) as u8, alpha]
        })
        .collect();
    Picture::new(CanonicalPixels::from_rgba(width, height, rgba).unwrap()).unwrap()
}

/// A small request of words.
fn words() -> JudgeRequest {
    let message = Message::new(Role::User, vec![Part::Text("Look.".to_owned())]).unwrap();
    JudgeRequest::new(
        "Judge.",
        vec![message],
        Vec::new(),
        Controls::new(64).unwrap(),
    )
    .unwrap()
}

/// The base64 of RFC 4648, read back.
fn unbase64(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => panic!("not base64"),
    };
    let bytes: Vec<u8> = text.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::new();
    for group in bytes.chunks(4) {
        let all = group.iter().enumerate().fold(0u32, |all, (at, &c)| {
            all | u32::from(value(c)) << (18 - 6 * at)
        });
        out.extend_from_slice(&all.to_be_bytes()[1..group.len()]);
    }
    out
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
    let pki = Arc::new(Pki::loopback());
    let marker = "MARKER-7Q2";
    let host = "api.example-host.test";
    // A made-up token, put together here so that no scan of this file finds one.
    let token = &["sk", "made", "up", "token", "0123456789abcdef"].join("-");
    let body = format!(
        "{{\"type\":\"error\",\"error\":{{\"type\":\"{marker}\",\"message\":\"{marker} at \
         https://{host}/v1 with {token}\"}},\"request_id\":\"req_{marker}\"}}"
    );
    let fields = format!("X-Marker: {marker}\r\nRequest-Id: req_{marker}\r\nX-Token: {token}\r\n");
    let cases: Vec<(&str, FaultKind, &str)> = vec![
        ("400 Bad Request", FaultKind::Rejected, REFUSED),
        ("401 Unauthorized", FaultKind::Credential, CREDENTIAL),
        ("403 Forbidden", FaultKind::Credential, CREDENTIAL),
        ("404 Not Found", FaultKind::Rejected, REFUSED),
        ("413 Too Large", FaultKind::Rejected, REFUSED),
        (
            "429 Too Many Requests",
            FaultKind::Unavailable,
            RATE_LIMITED,
        ),
        ("500 Internal Server Error", FaultKind::Unavailable, BUSY),
        ("529 Overloaded", FaultKind::Unavailable, BUSY),
        ("418 Teapot", FaultKind::Malformed, STATUS),
        ("200 OK", FaultKind::Malformed, wire::NOT_A_RESPONSE),
    ];
    let answers = cases
        .iter()
        .map(|(status, _, _)| answer(status, &fields, body.as_bytes()))
        .collect();
    let (listener, _) = serving(&pki, answers);
    let port = listener.port();
    for (status, kind, why) in &cases {
        let fault = ask(&pki, messages(port, "claude-haiku-5-5", None), &words()).unwrap_err();
        assert_eq!(
            (fault.kind(), fault.stage(), fault.sentence()),
            (*kind, Stage::Answered, *why),
            "{status}"
        );
        let printed = format!("{fault} {fault:?}");
        for held in [marker, host, token.as_str(), "127.0.0.1", &port.to_string()] {
            assert!(!printed.contains(held), "{status}: {printed}");
        }
    }
    assert_eq!(listener.finish(), cases.len());
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
    let pieces = wire::request(&request, &backend.settings).unwrap().pieces;
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
    backend.settings.max_output_tokens = output_tokens;
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
