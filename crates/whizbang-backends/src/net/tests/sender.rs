//! `sender::…`: the plan's tests of the sender (plan 2A, task S1, its first part), each
//! against servers on loopback that the test kit's listener runs.
//!
//! A sender is handed the bounds of a backend's request, and only a handle makes
//! those, so each send here is made from inside a judge of the test's, through a
//! judge's handle: the judge sends with the client the test made, and keeps what came
//! back.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustls::{ServerConfig, ServerConnection, StreamOwned};
use whizbang_core::capability::{
    Bounds, Controls, Declaration, Fault, FaultKind, Judge, JudgeHandle, JudgeRequest, Message,
    Part, Picture, Role, Stage, StopReason, Turn,
};
use whizbang_core::contract::{Cancellation, ModelIdentity, Secrets, SystemClock};
use whizbang_core::egress::{Address, Credential, DataClass, Outgoing, Piece, Reply, Sender};
use whizbang_core::identity::CanonicalPixels;
use whizbang_testkit::loopback::Listener;

use super::super::client::{Client, NOT_CONNECTED, NOT_HTTPS, PAST_DEADLINE};
use super::super::http;
use super::super::wire::CUT;
use super::{Hand, Pki, address, client, read_head, timing};

/// A judge that sends one request with the test's client, from inside the bounds its
/// handle gives it, and keeps what came back. It cancels the run first, when it is
/// told to: the handle refuses a run that is cancelled before its backend is called.
struct Sends {
    declaration: Declaration,
    client: Client,
    outgoing: Outgoing,
    cancel_first: Option<Cancellation>,
    got: Mutex<Vec<Result<Reply, Fault>>>,
}

impl Judge for Sends {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn send(&self, _: &JudgeRequest, bounds: &Bounds) -> Result<Turn, Fault> {
        if let Some(cancellation) = &self.cancel_first {
            cancellation.cancel();
        }
        let got = Sender::send(&self.client, &self.outgoing, bounds);
        self.got.lock().unwrap().push(got);
        Turn::new(
            vec![Part::Text("sent".to_owned())],
            StopReason::EndTurn,
            None,
            None,
        )
        .map_err(|_| Fault::new(FaultKind::Malformed, "the test's turn is not one"))
    }
}

/// Sends `outgoing` with `client` from inside a judge's handle, under this
/// cancellation, which the judge sets first when `cancel_first` says so, and, when one
/// is given, this deadline.
fn send(
    client: Client,
    outgoing: Outgoing,
    cancellation: Cancellation,
    cancel_first: bool,
    deadline: Option<Instant>,
) -> Result<Reply, Fault> {
    let identity = ModelIdentity::new("sends", "m", "1");
    let judge = Arc::new(Sends {
        declaration: Declaration::new(identity, &[]).unwrap(),
        client,
        outgoing,
        cancel_first: cancel_first.then(|| cancellation.clone()),
        got: Mutex::new(Vec::new()),
    });
    let mut handle = JudgeHandle::new(judge.clone(), cancellation);
    if let Some(deadline) = deadline {
        handle = handle.until(deadline);
    }
    let message = Message::new(Role::User, vec![Part::Text("go".to_owned())]).unwrap();
    let request = JudgeRequest::new("", vec![message], Vec::new(), Controls::new(1).unwrap());
    let _ = handle.send(&request.unwrap());
    let mut got = judge.got.lock().unwrap();
    assert_eq!(got.len(), 1, "the handle called its backend once");
    got.pop().unwrap()
}

/// A picture of two pixels, as the encoder writes it.
fn picture() -> Piece {
    let pixels = CanonicalPixels::from_rgba(2, 1, vec![9, 8, 7, 255, 1, 2, 3, 255]).unwrap();
    Piece::picture(DataClass::SourceImages, &Picture::new(pixels).unwrap()).unwrap()
}

/// A request of words, a picture and words, with two fields of its own.
fn outgoing(to: Address) -> Outgoing {
    let pieces = vec![
        Piece::of(DataClass::Prompts, b"{\"image\":\"".to_vec()),
        picture(),
        Piece::of(DataClass::Prompts, b"\"}".to_vec()),
    ];
    Outgoing::new(to, pieces)
        .unwrap()
        .with_fields(&[("content-type", "application/json"), ("x-test", "1")])
        .unwrap()
}

/// The body `outgoing` is written with: the picture as the base64 of its bytes.
fn body_of(outgoing: &Outgoing) -> Vec<u8> {
    let mut body = Vec::new();
    for piece in outgoing.pieces() {
        if piece.is_picture() {
            http::base64_onto(piece.bytes(), &mut body);
        } else {
            body.extend_from_slice(piece.bytes());
        }
    }
    body
}

/// What a server saw of one connection: the request's head, and every byte that came
/// after it until the client's side ended.
#[derive(Debug, Clone, Default)]
struct Request {
    head: Vec<u8>,
    after: Vec<u8>,
}

type Requests = Arc<Mutex<Vec<Request>>>;

/// The value of a field of a head, by its name, without regard to case.
fn field(head: &[u8], name: &str) -> Option<String> {
    let text = String::from_utf8_lossy(head);
    text.lines().find_map(|line| {
        let (field, value) = line.split_once(':')?;
        field
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_owned())
    })
}

/// Serves one connection: the handshake, the request's head and the body its
/// `Content-Length` states, then `answer` as it is, or nothing when `answer` is none,
/// and then reads on until the client ends its side, so that anything it wrote after
/// its body is seen.
fn serve(config: Arc<ServerConfig>, stream: TcpStream, answer: Option<&[u8]>, log: &Requests) {
    let mut tls = StreamOwned::new(ServerConnection::new(config).unwrap(), stream);
    let head = read_head(&mut tls);
    let length: usize = field(&head, "content-length").map_or(0, |n| n.parse().unwrap());
    let mut after = vec![0; length];
    let read = tls.read_exact(&mut after);
    let mut request = Request { head, after };
    // With no answer, or no whole body, the connection is let go unanswered: cut.
    let answered = answer
        .filter(|_| read.is_ok())
        .is_some_and(|answer| tls.write_all(answer).and_then(|()| tls.flush()).is_ok());
    if answered {
        // Anything the client writes after its body, until it closes.
        let mut more = Vec::new();
        let _ = tls.read_to_end(&mut more);
        request.after.extend(more);
        tls.conn.send_close_notify();
        let _ = tls.flush();
    }
    log.lock().unwrap().push(request);
}

/// A listener of one connection per answer, each answered in turn.
fn serving(pki: &Pki, answers: Vec<Option<Vec<u8>>>) -> (Listener, Requests) {
    let config = pki.both();
    let log: Requests = Arc::default();
    let kept = log.clone();
    let listener = Listener::serve(answers.len(), move |number, stream| {
        serve(config.clone(), stream, answers[number].as_deref(), &kept);
    });
    (listener, log)
}

fn answer(status: &str, body: &[u8]) -> Vec<u8> {
    [
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .as_bytes(),
        body,
    ]
    .concat()
}

fn the_tests_client(pki: &Pki) -> Client {
    client(pki, timing(), Arc::new(SystemClock), None)
}

fn closed_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn fault(got: Result<Reply, Fault>) -> Fault {
    got.expect_err("the send ended in a fault")
}

#[test]
fn a_post_carries_its_body_once_and_follows_no_redirect() {
    let pki = Pki::loopback();
    // A second server, which a redirect points to: it must see no connection.
    let (elsewhere, _) = serving(&pki, vec![Some(answer("200 OK", b"{}"))]);
    let there = format!("https://127.0.0.1:{}/v1/elsewhere", elsewhere.port());
    let moved = format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {there}\r\n\r\n");
    let answers = vec![
        Some(answer("200 OK", b"{\"answer\":1}")),
        Some(answer("400 Bad Request", b"{\"error\":\"made-up\"}")),
        Some(moved.into_bytes()),
        Some(b"HTTP/1.1 302 Found\r\nLocation: /v1/moved\r\n\r\n".to_vec()),
    ];
    let (listener, log) = serving(&pki, answers);
    let port = listener.port();
    let none = Secrets::none();
    let origin = whizbang_core::egress::Origin::of(&format!("https://127.0.0.1:{port}"), &none);
    let credential = Credential::new(origin.unwrap(), "x-api-key", "made-up-value", &none);

    // Answered: the status and the body, as they came.
    let sent = outgoing(address(port, "/v1/messages"))
        .with_credential(credential.unwrap())
        .unwrap();
    let body = body_of(&sent);
    let got = send(
        the_tests_client(&pki),
        sent,
        Cancellation::new(),
        false,
        None,
    )
    .unwrap();
    assert_eq!(got.status(), Some(200));
    assert_eq!(got.body(), b"{\"answer\":1}");
    // Any other final status is answered too, with its body: what it means is the
    // adapter's to say.
    let sent = outgoing(address(port, "/v1/messages"));
    let got = send(
        the_tests_client(&pki),
        sent,
        Cancellation::new(),
        false,
        None,
    )
    .unwrap();
    assert_eq!(got.status(), Some(400));
    assert_eq!(got.body(), b"{\"error\":\"made-up\"}");

    // A redirect comes back as the address it names, and nothing is asked there.
    let sent = outgoing(address(port, "/v1/messages"));
    let got = send(
        the_tests_client(&pki),
        sent,
        Cancellation::new(),
        false,
        None,
    )
    .unwrap();
    assert_eq!(got.status(), None);
    assert_eq!(got.moved_to(), Some(&Address::of(&there, &none).unwrap()));
    // A path at the same origin, made whole.
    let sent = outgoing(address(port, "/v1/messages"));
    let got = send(
        the_tests_client(&pki),
        sent,
        Cancellation::new(),
        false,
        None,
    )
    .unwrap();
    assert_eq!(got.moved_to(), Some(&address(port, "/v1/moved")));

    // What the server saw, once its thread is over: each request went once, on a
    // connection of its own.
    assert_eq!(listener.finish(), 4);
    assert_eq!(elsewhere.finish(), 0);
    let heads = log.lock().unwrap().clone();
    assert_eq!(heads.len(), 4);
    assert!(
        heads
            .iter()
            .all(|seen| seen.head.starts_with(b"POST /v1/messages "))
    );
    let seen = heads[0].clone();
    let head = String::from_utf8(seen.head.clone()).unwrap();
    assert!(head.starts_with("POST /v1/messages HTTP/1.1\r\n"), "{head}");
    assert_eq!(
        field(&seen.head, "content-length"),
        Some(body.len().to_string())
    );
    assert_eq!(
        field(&seen.head, "content-type").unwrap(),
        "application/json"
    );
    assert_eq!(field(&seen.head, "x-test").unwrap(), "1");
    assert_eq!(field(&seen.head, "x-api-key").unwrap(), "made-up-value");
    assert_eq!(field(&seen.head, "connection").unwrap(), "close");
    assert!(field(&seen.head, "transfer-encoding").is_none());
    // The body once, and nothing after it: the picture as the base64 of exactly its
    // bytes, between the words.
    assert_eq!(seen.after, body);
    let text = String::from_utf8(seen.after).unwrap();
    let mut encoded = Vec::new();
    http::base64_onto(picture().bytes(), &mut encoded);
    let encoded = String::from_utf8(encoded).unwrap();
    assert_eq!(text, format!("{{\"image\":\"{encoded}\"}}"));
}

#[test]
fn a_request_that_never_connected_is_not_sent_and_one_cut_after_writing_is_unresolved() {
    let pki = Pki::loopback();

    // Refused to connect.
    let sent = outgoing(address(closed_port(), "/v1/messages"));
    let got = fault(send(
        the_tests_client(&pki),
        sent,
        Cancellation::new(),
        false,
        None,
    ));
    assert_eq!(got.stage(), Stage::NotSent);
    assert_eq!(got.kind(), FaultKind::Unavailable);
    assert_eq!(got.sentence(), NOT_CONNECTED);

    // An address that is not https, a cancellation before connecting and a deadline
    // that has passed: nothing is connected to, and the listener sees nothing. The
    // handle itself refuses a cancelled run and one past its deadline before its
    // backend is called; here each comes while the request is with the backend.
    let (listener, log) = serving(&pki, vec![None]);
    let port = listener.port();
    let plain = Address::of(&format!("http://127.0.0.1:{port}/v1"), &Secrets::none()).unwrap();
    let got = fault(send(
        the_tests_client(&pki),
        outgoing(plain),
        Cancellation::new(),
        false,
        None,
    ));
    assert_eq!(got.stage(), Stage::NotSent);
    assert_eq!(got.sentence(), NOT_HTTPS);
    let sent = outgoing(address(port, "/v1/messages"));
    let got = fault(send(
        the_tests_client(&pki),
        sent,
        Cancellation::new(),
        true,
        None,
    ));
    assert_eq!(got.stage(), Stage::NotSent);
    assert_eq!(got.kind(), FaultKind::Cancelled);
    // A deadline an hour away when the handle looks, and past by the client's clock,
    // which has moved two hours on.
    let clock = Hand::standing();
    clock.advance(Duration::from_secs(7200));
    let late = client(&pki, timing(), clock, None);
    let got = fault(send(
        late,
        outgoing(address(port, "/v1/messages")),
        Cancellation::new(),
        false,
        Some(Instant::now() + Duration::from_secs(3600)),
    ));
    assert_eq!(got.stage(), Stage::NotSent, "{got:?}");
    assert_eq!(got.sentence(), PAST_DEADLINE);
    assert_eq!(listener.accepted(), 0);

    // Connected, written whole, and cut with no answer: unresolved.
    let sent = outgoing(address(port, "/v1/messages"));
    let body = body_of(&sent);
    let got = fault(send(
        the_tests_client(&pki),
        sent,
        Cancellation::new(),
        false,
        None,
    ));
    assert_eq!(got.stage(), Stage::Unresolved);
    assert_eq!(got.sentence(), CUT);
    assert_eq!(listener.finish(), 1);
    let seen = log.lock().unwrap();
    assert!(seen[0].head.starts_with(b"POST /v1/messages "));
    assert_eq!(seen[0].after, body);
}
