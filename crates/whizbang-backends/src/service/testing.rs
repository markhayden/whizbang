//! What the tests of each provider of the judge of a service share (plan 2A, task S1;
//! plan m2c, task P1): a judge that asks a backend through the test's sender, a server
//! on loopback with a certificate of a root made in the test (`net::testing`), and what
//! it saw. Nothing here contacts a host off the machine.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use whizbang_core::capability::{
    Controls, Declaration, Fault, FaultKind, Judge, JudgeHandle, JudgeRequest, Message, Part,
    Picture, Role, Stage, ToolDefinition, Turn,
};
use whizbang_core::contract::Cancellation;
use whizbang_core::egress::Sender;
use whizbang_core::identity::CanonicalPixels;
use whizbang_testkit::loopback::Listener;

use super::{BUSY, CREDENTIAL, RATE_LIMITED, REFUSED, STATUS, Service};
use crate::net::testing::{Pki, sender_trusting};

/// Every recorded exchange under `tests/fixtures/<set>`, by its set and its number, in
/// order.
pub(crate) fn exchanges(set: &str) -> Vec<(String, Value)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(set);
    let mut sets: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    sets.sort();
    let mut all = Vec::new();
    for set in sets {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&set)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        files.sort();
        for file in files {
            let name = format!(
                "{}/{}",
                set.file_name().unwrap().to_string_lossy(),
                file.file_name().unwrap().to_string_lossy()
            );
            let read: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
            all.push((name, read));
        }
    }
    all
}

/// The size a recorded picture names, `<picture WxH>`, when the text is one.
pub(crate) fn named_size(text: &str) -> Option<(u32, u32)> {
    let size = text.split("<picture ").nth(1)?.strip_suffix('>')?;
    let (w, h) = size.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// How much smaller a recorded picture is made for a test: a tenth on each side for
/// one over 1,000 pixels wide, none otherwise. Encoding the 21 large pictures that
/// `many-images` carried takes the encoder, built unoptimised as the tests are, longer
/// than a test may run; the request's every other byte, and how many pictures it
/// carries, where and of which type, are compared as they were recorded. Fitting a
/// picture to its long edge is `prepare::…`'s.
pub(crate) fn scale_of(width: u32) -> u32 {
    if width > 1000 { 10 } else { 1 }
}

/// The picture a test makes for a recorded one: of its size, cut by [`scale_of`].
pub(crate) fn picture_named(text: &str) -> Picture {
    let (w, h) = named_size(text).unwrap();
    picture(w / scale_of(w), h / scale_of(w), false)
}

/// A judge that asks a backend of the kind through the test's sender.
struct Through {
    messages: Service,
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

/// Asks the backend once, through a handle.
pub(crate) fn ask(pki: &Pki, messages: Service, request: &JudgeRequest) -> Result<Turn, Fault> {
    let judge = Arc::new(Through {
        messages,
        sender: sender_trusting(pki),
    });
    JudgeHandle::new(judge, Cancellation::new()).send(request)
}

/// What a server saw of one request: its head and its body.
#[derive(Debug, Clone, Default)]
pub(crate) struct Seen {
    pub(crate) head: String,
    pub(crate) body: Vec<u8>,
}

pub(crate) type Log = Arc<Mutex<Vec<Seen>>>;

/// A listener that answers each of its connections with the next of `answers`, and
/// keeps what each request was. The server's side ends after its answer.
pub(crate) fn serving(pki: &Arc<Pki>, answers: Vec<Vec<u8>>) -> (Listener, Log) {
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
pub(crate) fn answer(status: &str, fields: &str, body: &[u8]) -> Vec<u8> {
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
pub(crate) fn picture(width: u32, height: u32, clear: bool) -> Picture {
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
pub(crate) fn words() -> JudgeRequest {
    let message = Message::new(Role::User, vec![Part::Text("Look.".to_owned())]).unwrap();
    JudgeRequest::new(
        "Judge.",
        vec![message],
        Vec::new(),
        Controls::new(64).unwrap(),
    )
    .unwrap()
}

/// The fixtures of submission v3's tool, as the build writes it (plan mE, task F1), which
/// squint's tests hold to `use_case::submit_tool_v3`: for source-fidelity's shorthand
/// checklist, whose items are of one instance each, and for reference-consistency's with
/// one reference, whose item is of several.
pub(crate) const V3_FIXTURES: [&str; 2] = ["v3-source-fidelity", "v3-reference-consistency"];

/// A small request of words that offers the v3 tool of this fixture
/// (`fixtures/submission/<name>.json`). With the tool's schema, as written.
pub(crate) fn v3_request(name: &str) -> (JudgeRequest, Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../fixtures/submission/{name}.json"));
    let tool: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let schema = tool["input_schema"].clone();
    let description = tool["description"].as_str().unwrap();
    let submit = ToolDefinition::new("submit_verdict", description, schema.clone()).unwrap();
    let message = Message::new(Role::User, vec![Part::Text("Look.".to_owned())]).unwrap();
    let controls = Controls::new(64).unwrap();
    let request = JudgeRequest::new("Judge.", vec![message], vec![submit], controls).unwrap();
    (request, schema)
}

/// The body an adapter writes of a request that holds no picture: its pieces, together.
pub(crate) fn body_of(pieces: &[whizbang_core::egress::Piece]) -> Value {
    let bytes: Vec<u8> = pieces
        .iter()
        .flat_map(|piece| piece.bytes().to_vec())
        .collect();
    serde_json::from_slice(&bytes).unwrap()
}

/// The base64 of RFC 4648, read back.
pub(crate) fn unbase64(text: &str) -> Vec<u8> {
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

/// Answers of each status a provider gives, each with a body and fields that hold a
/// marker, a host and a made-up token, to a backend at a port of loopback that `make`
/// makes: each fault is of its kind, stage and sentence, and none holds the marker, the
/// host, the token, the address or the port. An answer of 200 that is no response of
/// the provider's says `not_a_response`.
pub(crate) fn faults_hold_nothing(make: impl Fn(u16) -> Service, not_a_response: &str) {
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
    // A status the kind names is named after the sentence; nothing of the body is.
    let busy_500 = format!("{BUSY}; it answered 500");
    let busy_529 = format!("{BUSY}; it answered 529");
    let status_418 = format!("{STATUS}; it answered 418");
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
        (
            "500 Internal Server Error",
            FaultKind::Unavailable,
            &busy_500,
        ),
        ("529 Overloaded", FaultKind::Unavailable, &busy_529),
        ("418 Teapot", FaultKind::Malformed, &status_418),
        ("200 OK", FaultKind::Malformed, not_a_response),
    ];
    let answers = cases
        .iter()
        .map(|(status, _, _)| answer(status, &fields, body.as_bytes()))
        .collect();
    let (listener, _) = serving(&pki, answers);
    let port = listener.port();
    for (status, kind, why) in &cases {
        let fault = ask(&pki, make(port), &words()).unwrap_err();
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
