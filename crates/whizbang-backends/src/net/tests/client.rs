//! `client::…`: the plan's tests of task N1, and the owner's safety properties of the
//! client, each against servers on loopback.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustls::{HandshakeKind, ProtocolVersion};
use whizbang_core::capability::{Fault, FaultKind, Stage};
use whizbang_core::contract::{Cancellation, Clock, Secrets, SystemClock};
use whizbang_core::egress::{
    Address, CheckedDownloader, Credential, DownloadRequest, Downloaded, Downloader,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::loopback::Listener;
use whizbang_testkit::scratch::Scratch;

use super::super::client::{
    CONNECT_TIMED_OUT, CREDENTIAL, Client, Dial, MOVED_SECRET, NOT_CONNECTED, NOT_HTTPS, STATUS,
};
use super::super::http;
use super::super::wire::{CANCELLED, CUT, STALLED, TOO_SLOW, Timing, UNTRUSTED};
use super::{
    Hand, Log, Pki, Seen, address, answer, client, ok, origin, read_head, respond, timing,
};
use crate::models::{
    CatalogueFile, Entry, EntryParts, FileName, Licence, ModelName, ModelsDir, Platform, Revision,
    Title, install,
};

/// What a download ended as, with every piece its sink was handed.
struct Got {
    ended: Result<Downloaded, Fault>,
    pieces: Vec<Vec<u8>>,
}

impl Got {
    fn bytes(&self) -> Vec<u8> {
        self.pieces.concat()
    }

    fn fault(&self) -> &Fault {
        self.ended
            .as_ref()
            .expect_err("the download ended in a fault")
    }
}

fn download(
    client: &dyn Downloader,
    request: &DownloadRequest,
    cancellation: &Cancellation,
) -> Got {
    let mut pieces = Vec::new();
    let ended = client.download(request, cancellation, &mut |piece| {
        pieces.push(piece.to_vec());
        true
    });
    Got { ended, pieces }
}

fn get(client: &dyn Downloader, from: Address, most: u64) -> Got {
    download(
        client,
        &DownloadRequest::new(from, most).unwrap(),
        &Cancellation::new(),
    )
}

/// A listener of `connections` connections, each answered with the next of
/// `answers` (the last again when there are fewer), and a log of what each saw.
fn serving(pki: &Pki, connections: usize, answers: Vec<Vec<u8>>) -> (Listener, Log) {
    let config = pki.both();
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let kept = log.clone();
    let listener = Listener::serve(connections, move |number, stream| {
        let which = number.min(answers.len() - 1);
        answer(config.clone(), stream, &answers[which], &kept);
    });
    (listener, log)
}

fn seen(log: &Log) -> Vec<Seen> {
    log.lock().unwrap().clone()
}

fn the_tests_client(pki: &Pki) -> Client {
    client(pki, timing(), Arc::new(SystemClock), None)
}

/// What is printed of a fault, both ways.
fn printed(fault: &Fault) -> String {
    format!("{fault} {fault:?}")
}

/// The request the client writes for this address, as a server sees it.
fn request_text(port: u16, path: &str, credential: Option<&str>) -> Vec<u8> {
    let mut text = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUser-Agent: whizbang\r\nAccept: */*\r\n\
         Accept-Encoding: identity\r\nConnection: close\r\n"
    );
    if let Some(value) = credential {
        text.push_str(&format!("Authorization: {value}\r\n"));
    }
    text.push_str("\r\n");
    text.into_bytes()
}

/// Bytes of a made-up file: each byte its place, so a piece out of order shows.
fn file_bytes(length: usize) -> Vec<u8> {
    (0..length).map(|at| (at % 251) as u8).collect()
}

#[test]
fn a_download_is_streamed_to_its_sink_and_stopped_at_the_bytes_it_was_asked_for() {
    let pki = Pki::loopback();
    let body = file_bytes(300_000);
    let (listener, log) = serving(&pki, 2, vec![ok(&body)]);
    let port = listener.port();
    let client = the_tests_client(&pki);

    // Asked for less than the file: exactly that many, in more than one piece.
    let got = get(&client, address(port, "/model.bin"), 200_000);
    assert_eq!(got.ended, Ok(Downloaded::Bytes(200_000)));
    assert_eq!(got.bytes(), body[..200_000]);
    assert!(got.pieces.len() > 1, "{} pieces", got.pieces.len());
    assert!(got.pieces.iter().all(|piece| piece.len() <= 64 * 1024));

    // Asked for more than the file: the file, by its stated length.
    let got = get(&client, address(port, "/model.bin"), 1 << 40);
    assert_eq!(got.ended, Ok(Downloaded::Bytes(300_000)));
    assert_eq!(got.bytes(), body);
    assert_eq!(listener.finish(), 2);
    // One request on each connection, which the client asked to close.
    for seen in seen(&log) {
        assert_eq!(seen.request, request_text(port, "/model.bin", None));
    }
}

#[test]
fn an_answer_that_points_elsewhere_is_handed_back_as_moved_and_nothing_is_asked_of_the_second_address()
 {
    let pki = Pki::loopback();
    let (elsewhere, elsewhere_log) = serving(&pki, 1, vec![ok(b"not asked")]);
    let whole = format!("https://127.0.0.1:{}/there?sig=1", elsewhere.port());
    let (listener, _) = serving(
        &pki,
        3,
        vec![
            format!("HTTP/1.1 302 Found\r\nLocation: {whole}\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
            b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /same/origin\r\n\r\n".to_vec(),
            b"HTTP/1.1 301 Moved\r\nLocation: there\r\n\r\n".to_vec(),
        ],
    );
    let client = the_tests_client(&pki);
    let got = get(&client, address(listener.port(), "/f"), 10);
    assert_eq!(
        got.ended,
        Ok(Downloaded::Moved(
            Address::of(&whole, &Secrets::none()).unwrap()
        ))
    );
    assert!(got.pieces.is_empty());
    // A path at the same origin.
    let got = get(&client, address(listener.port(), "/f"), 10);
    assert_eq!(
        got.ended,
        Ok(Downloaded::Moved(address(listener.port(), "/same/origin")))
    );
    // A relative path of another form is not taken.
    let got = get(&client, address(listener.port(), "/f"), 10);
    assert_eq!(got.fault().kind(), FaultKind::Malformed);
    assert_eq!(listener.finish(), 3);
    assert_eq!(elsewhere.finish(), 0);
    assert!(seen(&elsewhere_log).is_empty());
}

#[test]
fn a_credential_is_sent_as_its_header_to_its_own_origin_and_a_request_without_one_sends_none() {
    let pki = Pki::loopback();
    let (listener, log) = serving(&pki, 2, vec![ok(b"abc")]);
    let port = listener.port();
    let client = the_tests_client(&pki);
    let credential = Credential::new(
        origin(port),
        "Authorization",
        "Bearer made-up-token",
        &Secrets::none(),
    )
    .unwrap();
    let carried = DownloadRequest::new(address(port, "/a"), 3)
        .unwrap()
        .with_credential(credential)
        .unwrap();
    let got = download(&client, &carried, &Cancellation::new());
    assert_eq!(got.ended, Ok(Downloaded::Bytes(3)));
    let got = get(&client, address(port, "/b"), 3);
    assert_eq!(got.ended, Ok(Downloaded::Bytes(3)));
    assert_eq!(listener.finish(), 2);
    let seen = seen(&log);
    assert_eq!(
        seen[0].request,
        request_text(port, "/a", Some("Bearer made-up-token"))
    );
    assert_eq!(seen[1].request, request_text(port, "/b", None));
}

#[test]
fn a_fault_holds_nothing_of_the_body_the_headers_or_the_address_that_answered() {
    const MARKER: &str = "sk-marker-7f3a9c";
    let pki = Pki::loopback();
    let answers = vec![
        format!(
            "HTTP/1.1 500 {MARKER}\r\nX-Marker: {MARKER}\r\nContent-Length: 16\r\n\r\n{MARKER}"
        ),
        format!("HTTP/1.1 403 Forbidden\r\nX-Reason: {MARKER}\r\n\r\n"),
        format!("HTTP/1.1 302 Found\r\nLocation: /{MARKER}/file\r\n\r\n"),
        format!("HTTP/1.1 418 {MARKER}\r\n\r\n"),
        format!("HTTP/1.1 200 OK\r\nX-{MARKER}: \x01\r\n\r\n"),
    ];
    let (listener, _) = serving(
        &pki,
        answers.len(),
        answers.into_iter().map(String::into_bytes).collect(),
    );
    let port = listener.port();
    let secrets = Secrets::new([MARKER.to_owned()]).unwrap();
    let client = Client::trusting(pki.roots(), timing(), Arc::new(SystemClock), secrets, None);
    let mut kinds = Vec::new();
    for _ in 0..5 {
        let got = get(&client, address(port, "/private/path"), 100);
        let fault = got.fault();
        kinds.push((fault.kind(), fault.sentence().to_owned()));
        let printed = printed(fault);
        for held in [
            MARKER,
            "127.0.0.1",
            &port.to_string(),
            "private",
            "Forbidden",
        ] {
            assert!(!printed.contains(held), "{printed:?} holds {held:?}");
        }
        assert_eq!(fault.stage(), Stage::Unresolved);
    }
    assert_eq!(listener.finish(), 5);
    assert_eq!(kinds[0].0, FaultKind::Unavailable);
    assert_eq!(kinds[1], (FaultKind::Credential, CREDENTIAL.to_owned()));
    assert_eq!(kinds[2], (FaultKind::Malformed, MOVED_SECRET.to_owned()));
    assert_eq!(kinds[3], (FaultKind::Malformed, STATUS.to_owned()));
    assert_eq!(kinds[4], (FaultKind::Malformed, http::BAD_FIELD.to_owned()));
}

/// A port of loopback nobody listens at: one the system gave, let go of at once.
fn closed_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[test]
fn a_request_that_never_connected_is_not_sent_and_one_cut_after_it_was_written_is_unresolved() {
    let pki = Pki::loopback();
    let client = the_tests_client(&pki);

    // Refused to connect.
    let got = get(&client, address(closed_port(), "/f"), 10);
    assert_eq!(got.fault().stage(), Stage::NotSent);
    assert_eq!(got.fault().kind(), FaultKind::Unavailable);
    assert_eq!(got.fault().sentence(), NOT_CONNECTED);

    // An address that is not https, and a cancellation before connecting: nothing
    // is connected to, and the listener sees nothing.
    let (listener, log) = serving(&pki, 2, vec![Vec::new()]);
    let port = listener.port();
    let plain = Address::of(&format!("http://127.0.0.1:{port}/f"), &Secrets::none()).unwrap();
    let got = get(&client, plain, 10);
    assert_eq!(got.fault().stage(), Stage::NotSent);
    assert_eq!(got.fault().sentence(), NOT_HTTPS);
    let cancelled = Cancellation::new();
    cancelled.cancel();
    let request = DownloadRequest::new(address(port, "/f"), 10).unwrap();
    let got = download(&client, &request, &cancelled);
    assert_eq!(got.fault().stage(), Stage::NotSent);
    assert_eq!(got.fault().kind(), FaultKind::Cancelled);
    assert_eq!(listener.accepted(), 0);

    // Connected, written, and cut with no answer: unresolved.
    let got = get(&client, address(port, "/f"), 10);
    assert_eq!(got.fault().stage(), Stage::Unresolved);
    assert_eq!(got.fault().sentence(), CUT);
    assert_eq!(seen(&log)[0].request, request_text(port, "/f", None));

    // A handshake that fails is unresolved too: a server whose root the client does
    // not trust.
    let stranger = Pki::made(41, [127, 0, 0, 1]);
    let (strange, _) = serving(&stranger, 1, vec![ok(b"x")]);
    let got = get(&client, address(strange.port(), "/f"), 10);
    assert_eq!(got.fault().stage(), Stage::Unresolved);
    assert_eq!(got.fault().sentence(), UNTRUSTED);
    assert_eq!(strange.finish(), 1);
    assert_eq!(listener.finish(), 1);
}

#[test]
fn the_limit_to_connect_is_for_all_of_a_hosts_addresses_and_a_connect_out_of_its_time_is_not_sent()
{
    // A connect that takes the whole limit it is given and fails, by the clock: the
    // socket's own limit is the time left, and nothing is tried after it.
    let pki = Pki::loopback();
    let clock = Hand::standing();
    let given: Arc<Mutex<Vec<Duration>>> = Arc::default();
    let dial: Dial = {
        let (clock, given) = (clock.clone(), given.clone());
        Arc::new(move |_: &SocketAddr, limit: Duration| {
            given.lock().unwrap().push(limit);
            clock.advance(limit);
            Err(io::Error::from(io::ErrorKind::TimedOut))
        })
    };
    let client = client(&pki, timing(), clock.clone(), Some(dial));
    let got = get(&client, address(closed_port(), "/f"), 10);
    assert_eq!(got.fault().kind(), FaultKind::TimedOut);
    assert_eq!(got.fault().stage(), Stage::NotSent);
    assert_eq!(got.fault().sentence(), CONNECT_TIMED_OUT);
    assert_eq!(*given.lock().unwrap(), [Duration::from_secs(10)]);
    assert_eq!(clock.elapsed(), Duration::from_secs(10));
}

#[test]
fn a_cancellation_is_looked_at_between_pieces() {
    let pki = Pki::loopback();
    let body = file_bytes(300_000);
    let (listener, _) = serving(&pki, 1, vec![ok(&body)]);
    let client = the_tests_client(&pki);
    let cancellation = Cancellation::new();
    let request = DownloadRequest::new(address(listener.port(), "/f"), 300_000).unwrap();
    let mut pieces = 0;
    let ended = client.download(&request, &cancellation, &mut |_| {
        pieces += 1;
        cancellation.cancel();
        true
    });
    let fault = ended.unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Cancelled);
    assert_eq!(fault.sentence(), CANCELLED);
    assert_eq!(fault.stage(), Stage::Unresolved);
    assert_eq!(pieces, 1);
    assert_eq!(listener.finish(), 1);
}

/// A listener whose one connection is read from and never answered, until the client
/// leaves: no TLS, no byte back.
fn mute() -> Listener {
    Listener::serve(1, |_, mut stream| {
        let mut sink = [0; 1024];
        while matches!(stream.read(&mut sink), Ok(read) if read > 0) {}
    })
}

#[test]
fn a_peer_that_accepts_and_sends_nothing_ends_the_request_as_timed_out_at_the_limit_between_reads()
{
    // The clock moves a second each time it is read; the limit is 30 seconds of it.
    let pki = Pki::loopback();
    let listener = mute();
    let clock = Hand::stepping(1000);
    let client = client(&pki, timing(), clock.clone(), None);
    let got = get(&client, address(listener.port(), "/f"), 10);
    let fault = got.fault();
    assert_eq!(fault.kind(), FaultKind::TimedOut);
    assert_eq!(fault.sentence(), STALLED);
    assert_eq!(fault.stage(), Stage::Unresolved);
    // At the limit: 30 seconds of the clock, and not more than the reads of a few
    // more polls.
    let spent = clock.elapsed();
    assert!(spent >= Duration::from_secs(30), "{spent:?}");
    assert!(spent <= Duration::from_secs(36), "{spent:?}");
    assert_eq!(listener.finish(), 1);
}

#[test]
fn under_a_clock_that_stands_still_a_mute_peer_ends_by_the_ceiling_of_polls() {
    // A limit between two reads of 40 ms and a poll of 5 ms: nine polls with nothing.
    let pki = Pki::loopback();
    let listener = mute();
    let quick = Timing {
        between_reads: Duration::from_millis(40),
        ..timing()
    };
    let client = client(&pki, quick, Hand::standing(), None);
    let got = get(&client, address(listener.port(), "/f"), 10);
    assert_eq!(got.fault().sentence(), STALLED);
    assert_eq!(listener.finish(), 1);
}

#[test]
fn a_cancellation_set_while_no_byte_arrives_ends_the_request_by_that_limit_at_the_latest() {
    // The server sets the cancellation once the client has connected, and sends
    // nothing; the clock moves 100 ms each time it is read.
    let pki = Pki::loopback();
    let cancellation = Cancellation::new();
    let listener = {
        let cancellation = cancellation.clone();
        Listener::serve(1, move |_, mut stream| {
            cancellation.cancel();
            let mut sink = [0; 1024];
            while matches!(stream.read(&mut sink), Ok(read) if read > 0) {}
        })
    };
    let clock = Hand::stepping(100);
    let client = client(&pki, timing(), clock.clone(), None);
    let request = DownloadRequest::new(address(listener.port(), "/f"), 10).unwrap();
    let got = download(&client, &request, &cancellation);
    let fault = got.fault();
    assert_eq!(fault.kind(), FaultKind::Cancelled);
    assert_eq!(fault.stage(), Stage::Unresolved);
    assert!(
        clock.elapsed() < Duration::from_secs(30),
        "{:?}",
        clock.elapsed()
    );
    assert_eq!(listener.finish(), 1);
}

#[test]
fn a_peer_that_sends_a_byte_now_and_then_ends_the_download_as_timed_out_by_the_least_rate() {
    // A byte for every ten seconds of the clock, which only the server moves; a least
    // rate of 1 KiB a second, and a limit between two reads that is never reached.
    let pki = Pki::loopback();
    let clock = Hand::standing();
    let config = pki.both();
    let listener = {
        let clock = clock.clone();
        Listener::serve(1, move |_, stream| {
            let mut tls = rustls::StreamOwned::new(
                rustls::ServerConnection::new(config.clone()).unwrap(),
                stream,
            );
            read_head(&mut tls);
            let head = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n";
            if tls.write_all(head).and_then(|()| tls.flush()).is_err() {
                return;
            }
            for _ in 0..30 {
                clock.advance(Duration::from_secs(10));
                if tls.write_all(b"x").and_then(|()| tls.flush()).is_err() {
                    return;
                }
            }
        })
    };
    let slow = Timing {
        between_reads: Duration::from_secs(3600),
        least_bytes_per_second: 1024,
        ..timing()
    };
    let client = client(&pki, slow, clock.clone(), None);
    let got = get(&client, address(listener.port(), "/f"), 100);
    let fault = got.fault();
    assert_eq!(fault.kind(), FaultKind::TimedOut);
    assert_eq!(fault.sentence(), TOO_SLOW);
    assert_eq!(fault.stage(), Stage::Unresolved);
    assert!(got.bytes().len() < 100);
    assert!(clock.elapsed() >= Duration::from_secs(60));
    assert_eq!(listener.finish(), 1);
}

#[test]
fn the_client_trusts_the_roots_compiled_into_it_and_asks_no_other_service() {
    // The client the crate makes trusts the compiled roots: a server whose root is
    // the test's is refused in the handshake, before a byte of the request, and
    // nothing else is asked: the one listener saw one connection, and no request.
    let pki = Pki::loopback();
    let (listener, log) = serving(&pki, 1, vec![ok(b"x")]);
    let shipped = Client::new(timing(), Arc::new(SystemClock), Secrets::none());
    let got = get(&shipped, address(listener.port(), "/f"), 1);
    assert_eq!(got.fault().kind(), FaultKind::Rejected);
    assert_eq!(got.fault().sentence(), UNTRUSTED);
    assert_eq!(listener.finish(), 1);
    assert!(seen(&log)[0].request.is_empty());

    // A certificate the root trusts, for another host than the address's, is
    // refused the same way.
    let other_host = Pki::made(7, [127, 0, 0, 2]);
    let (listener, log) = serving(&other_host, 1, vec![ok(b"x")]);
    let got = get(
        &the_tests_client(&other_host),
        address(listener.port(), "/f"),
        1,
    );
    assert_eq!(got.fault().sentence(), UNTRUSTED);
    assert_eq!(listener.finish(), 1);
    assert!(seen(&log)[0].request.is_empty());

    // The roots are compiled in, and nothing in the build could ask the platform:
    // the lock file holds no verifier of the platform and no reader of its roots,
    // and `rustls` is built with the features ADR 0024 names and no other.
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let lock = std::fs::read_to_string(format!("{root}/Cargo.lock")).unwrap();
    let names: Vec<&str> = lock
        .lines()
        .filter_map(|line| line.strip_prefix("name = \""))
        .map(|name| name.trim_end_matches('"'))
        .collect();
    for refused in [
        "rustls-platform-verifier",
        "rustls-native-certs",
        "openssl-probe",
        "openssl",
        "openssl-sys",
        "native-tls",
        "security-framework",
        "schannel",
        "aws-lc-rs",
        "aws-lc-sys",
        "log",
    ] {
        assert!(!names.contains(&refused), "the lock file holds {refused}");
    }
    assert!(names.contains(&"webpki-roots"));
    let manifest = std::fs::read_to_string(format!("{root}/Cargo.toml")).unwrap();
    assert!(manifest.contains(
        "rustls = { version = \"=0.23.46\", default-features = false, features = [\"ring\", \"std\", \"tls12\"] }"
    ));
    assert!(manifest.contains("webpki-roots = \"=1.0.12\""));
    // And no source of the client turns the check off or reads the environment.
    for file in ["mod.rs", "client.rs", "http.rs", "wire.rs"] {
        let text =
            std::fs::read_to_string(format!("{}/src/net/{file}", env!("CARGO_MANIFEST_DIR")))
                .unwrap();
        for refused in ["dangerous", "std::env", "env::var", "KeyLogFile", "set_var"] {
            assert!(!text.contains(refused), "src/net/{file} names {refused}");
        }
    }
}

#[test]
fn the_client_speaks_tls_1_3_and_1_2_and_each_connection_is_a_full_handshake() {
    let pki = Pki::loopback();
    let client = the_tests_client(&pki);
    for (version, expected) in [
        (&rustls::version::TLS13, ProtocolVersion::TLSv1_3),
        (&rustls::version::TLS12, ProtocolVersion::TLSv1_2),
    ] {
        let config = pki.server(&[version]);
        let log: Log = Arc::default();
        let listener = {
            let log = log.clone();
            Listener::serve(2, move |_, stream| {
                answer(config.clone(), stream, &ok(b"abc"), &log);
            })
        };
        for _ in 0..2 {
            let got = get(&client, address(listener.port(), "/f"), 3);
            assert_eq!(got.ended, Ok(Downloaded::Bytes(3)));
        }
        assert_eq!(listener.finish(), 2);
        // No session is resumed: the second connection is a full handshake too.
        for seen in seen(&log) {
            assert_eq!(seen.version, Some(expected));
            assert_eq!(seen.kind, Some(HandshakeKind::Full));
        }
    }
}

#[test]
fn the_answers_the_subset_does_not_handle_are_each_refused_by_name() {
    let long = "a".repeat(http::LINE_MAX_BYTES);
    let many = "X-A: b\r\n".repeat(http::FIELDS_LIMIT + 1);
    let cases: Vec<(String, &str)> = vec![
        (
            "HTTP/1.1 103 Early Hints\r\nLink: </a>\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\nx".into(),
            http::INTERIM,
        ),
        ("HTTP/1.1 200 OK\r\n\r\nxyz".into(), http::NO_LENGTH),
        ("HTTP/1.1 200 OK\r\nContent-Length: 3, 3\r\n\r\nxyz".into(), http::BAD_LENGTH),
        (
            // Twenty digits: more than a length of 64 bits holds, refused, and not
            // read as the most there is.
            "HTTP/1.1 200 OK\r\nContent-Length: 99999999999999999999\r\n\r\nxyz".into(),
            http::BAD_LENGTH,
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nContent-Length: 3\r\n\r\nxyz".into(),
            http::LENGTH_TWICE,
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nxyz\r\n0\r\n\r\n".into(),
            http::CODED,
        ),
        (
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nxyz\r\n0\r\n\r\n".into(),
            http::CODED,
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 3\r\n\r\nxyz".into(),
            http::COMPRESSED,
        ),
        (
            format!("HTTP/1.1 200 OK\r\nX-Long: {long}\r\nContent-Length: 1\r\n\r\nx"),
            http::LINE_TOO_LONG,
        ),
        (
            "HTTP/1.1 200 OK\r\nX-A: b\rc\r\nContent-Length: 1\r\n\r\nx".into(),
            http::BARE_ENDING,
        ),
        (
            "HTTP/1.1 200 OK\nContent-Length: 1\r\n\r\nx".into(),
            http::BARE_ENDING,
        ),
        (
            "HTTP/1.1 200 OK\r\nX-A: b\r\n c\r\nContent-Length: 1\r\n\r\nx".into(),
            http::FOLDED,
        ),
        (
            format!("HTTP/1.1 200 OK\r\n{many}Content-Length: 1\r\n\r\nx"),
            http::TOO_MANY_FIELDS,
        ),
        ("HTTP/2 200\r\n\r\n".into(), http::NOT_HTTP),
        (
            "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\n\r\nx".into(),
            STATUS,
        ),
        ("HTTP/1.1 304 Not Modified\r\n\r\n".into(), STATUS),
        (
            "HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nxyz".into(),
            CUT,
        ),
    ];
    let pki = Pki::loopback();
    let (listener, _) = serving(
        &pki,
        cases.len(),
        cases
            .iter()
            .map(|(bytes, _)| bytes.clone().into_bytes())
            .collect(),
    );
    let client = the_tests_client(&pki);
    for (bytes, why) in &cases {
        let got = get(&client, address(listener.port(), "/f"), 100);
        let fault = got.fault();
        assert_eq!(fault.sentence(), *why, "{bytes:?}");
        assert_eq!(fault.stage(), Stage::Unresolved);
        if *why != CUT {
            assert!(got.pieces.is_empty(), "{bytes:?}");
        }
    }
    assert_eq!(listener.finish(), cases.len());
}

#[test]
fn a_download_through_the_checked_downloader_that_is_moved_off_its_entrys_origins_is_refused() {
    // The client hands the redirect back to the gate, which refuses the second origin:
    // the second listener sees no connection at all.
    let pki = Pki::loopback();
    let (elsewhere, _) = serving(&pki, 1, vec![ok(b"not asked")]);
    let moved = format!(
        "HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:{}/f\r\n\r\n",
        elsewhere.port()
    );
    let (listener, _) = serving(&pki, 1, vec![moved.into_bytes()]);
    let gate = CheckedDownloader::new(Arc::new(the_tests_client(&pki)), &[origin(listener.port())])
        .unwrap();
    let got = get(&gate, address(listener.port(), "/f"), 10);
    assert_eq!(got.fault().kind(), FaultKind::Rejected);
    assert!(got.pieces.is_empty());
    assert_eq!(listener.finish(), 1);
    assert_eq!(elsewhere.finish(), 0);
}

#[test]
fn a_credentialed_download_moved_to_another_allowed_origin_is_asked_there_with_no_credential() {
    // Review of N4: the gate and the real client together. A request that carries a
    // credential for its origin is answered with a move to another origin the entry
    // names; the gate follows it, and what the second listener reads carries no
    // `Authorization`.
    let pki = Pki::loopback();
    let (second, second_log) = serving(&pki, 1, vec![ok(b"abc")]);
    let moved = format!(
        "HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:{}/stored\r\n\r\n",
        second.port()
    );
    let (first, first_log) = serving(&pki, 1, vec![moved.into_bytes()]);
    let origins = [origin(first.port()), origin(second.port())];
    let gate = CheckedDownloader::new(Arc::new(the_tests_client(&pki)), &origins).unwrap();
    let credential = Credential::new(
        origin(first.port()),
        "Authorization",
        "Bearer made-up-token",
        &Secrets::none(),
    )
    .unwrap();
    let request = DownloadRequest::new(address(first.port(), "/f"), 3)
        .unwrap()
        .with_credential(credential)
        .unwrap();
    let got = download(&gate, &request, &Cancellation::new());
    assert_eq!(got.ended, Ok(Downloaded::Bytes(3)));
    let (first_port, second_port) = (first.port(), second.port());
    assert_eq!(first.finish(), 1);
    assert_eq!(second.finish(), 1);
    assert_eq!(
        seen(&first_log)[0].request,
        request_text(first_port, "/f", Some("Bearer made-up-token"))
    );
    let asked = seen(&second_log)[0].request.clone();
    assert_eq!(asked, request_text(second_port, "/stored", None));
    assert!(!String::from_utf8_lossy(&asked).contains("Authorization"));
}

#[test]
fn the_client_makes_no_thread_and_one_socket_for_each_request() {
    // Two downloads through the gate, one of them moved once within its origin:
    // three requests, three connections, each asked to close.
    let pki = Pki::loopback();
    let (listener, log) = serving(
        &pki,
        3,
        vec![
            ok(b"one"),
            b"HTTP/1.1 308 Permanent Redirect\r\nLocation: /two\r\n\r\n".to_vec(),
            ok(b"two"),
        ],
    );
    let gate = CheckedDownloader::new(Arc::new(the_tests_client(&pki)), &[origin(listener.port())])
        .unwrap();
    // Each piece is handed over on the caller's own thread.
    let caller = std::thread::current().id();
    for (path, expected) in [("/one", b"one"), ("/f", b"two")] {
        let request = DownloadRequest::new(address(listener.port(), path), 3).unwrap();
        let mut got = Vec::new();
        let ended = gate.download(&request, &Cancellation::new(), &mut |piece| {
            assert_eq!(std::thread::current().id(), caller);
            got.extend_from_slice(piece);
            true
        });
        assert_eq!(ended, Ok(Downloaded::Bytes(3)));
        assert_eq!(got, expected);
    }
    assert_eq!(listener.finish(), 3);
    for seen in seen(&log) {
        let text = String::from_utf8(seen.request).unwrap();
        assert_eq!(text.matches("GET ").count(), 1);
        assert!(text.contains("\r\nConnection: close\r\n"));
    }
    // No source of the client makes a thread or a process.
    for file in ["mod.rs", "client.rs", "http.rs", "wire.rs"] {
        let text =
            std::fs::read_to_string(format!("{}/src/net/{file}", env!("CARGO_MANIFEST_DIR")))
                .unwrap();
        for refused in ["thread::", "spawn", "Command"] {
            assert!(!text.contains(refused), "src/net/{file} names {refused}");
        }
    }
}

#[test]
fn the_installer_installs_a_model_of_two_files_from_the_listener_through_the_checked_downloader() {
    let none = Secrets::none();
    let pki = Pki::loopback();
    let graph = file_bytes(70_000);
    let licence = b"made-up terms".to_vec();
    // Answered by path. One file is moved once within the origin, and the gate
    // follows it.
    let log: Log = Arc::default();
    let listener = {
        let (config, log) = (pki.both(), log.clone());
        let (graph, licence) = (graph.clone(), licence.clone());
        Listener::serve(3, move |_, stream| {
            respond(config.clone(), stream, &log, |request| {
                if request.starts_with(b"GET /files/graph.onnx ") {
                    ok(&graph)
                } else if request.starts_with(b"GET /old/LICENSE.txt ") {
                    b"HTTP/1.1 302 Found\r\nLocation: /files/LICENSE.txt\r\n\r\n".to_vec()
                } else if request.starts_with(b"GET /files/LICENSE.txt ") {
                    ok(&licence)
                } else {
                    b"HTTP/1.1 404 Not Found\r\n\r\n".to_vec()
                }
            });
        })
    };
    let port = listener.port();
    let file = |name: &str, path: &str, bytes: &[u8]| {
        CatalogueFile::new(
            FileName::new(name, &none).unwrap(),
            bytes.len() as u64,
            Digest::of_bytes(bytes),
            address(port, path),
            &none,
        )
        .unwrap()
    };
    let entry = Entry::new(
        EntryParts {
            name: ModelName::new("tiny", &none).unwrap(),
            title: Title::new("Tiny, for tests", &none).unwrap(),
            revision: Revision::new("2026-11.1", &none).unwrap(),
            adapter: ModelName::new("owlv2-sam2", &none).unwrap(),
            licence: Licence::new("Apache-2.0", &none).unwrap(),
            platforms: vec![Platform::MacosArm64, Platform::LinuxX86_64],
            features: vec!["phrase".to_owned()],
            origins: vec![origin(port)],
            files: vec![
                file("graph.onnx", "/files/graph.onnx", &graph),
                file("LICENSE.txt", "/old/LICENSE.txt", &licence),
            ],
        },
        &none,
    )
    .unwrap();
    let gate = CheckedDownloader::new(Arc::new(the_tests_client(&pki)), entry.origins()).unwrap();
    let scratch = Scratch::new("net-install");
    let root = scratch.path().join("models");
    let dir = ModelsDir::open(
        Arc::new(RealFs),
        &root,
        Arc::new(SystemClock) as Arc<dyn Clock>,
    )
    .unwrap();
    let record = install(
        &dir,
        &entry,
        &Revision::new("7", &none).unwrap(),
        &gate,
        &Cancellation::new(),
        &none,
    )
    .unwrap();
    assert_eq!(record.files().len(), 2);
    let installed = root
        .join("models/tiny/installations")
        .join(record.identity().hex())
        .join("files");
    assert_eq!(std::fs::read(installed.join("graph.onnx")).unwrap(), graph);
    assert_eq!(
        std::fs::read(installed.join("LICENSE.txt")).unwrap(),
        licence
    );
    assert_eq!(listener.finish(), 3);
    let mut paths: Vec<String> = seen(&log)
        .iter()
        .map(|seen| {
            String::from_utf8_lossy(&seen.request)
                .split(' ')
                .nth(1)
                .unwrap()
                .to_owned()
        })
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            "/files/LICENSE.txt",
            "/files/graph.onnx",
            "/old/LICENSE.txt"
        ]
    );
}

#[test]
fn a_name_is_checked_against_a_certificate_for_that_name_and_asked_for_by_it() {
    // `localhost`, from the hosts file: resolved to loopback, and nothing else asked.
    let named =
        |port: u16| Address::of(&format!("https://localhost:{port}/f"), &Secrets::none()).unwrap();
    // A certificate for the name `localhost` alone is taken when it is asked as
    // `localhost`, and the server is told that name.
    let pki = Pki::for_name(7, "localhost");
    let (listener, log) = serving(&pki, 1, vec![ok(b"abc")]);
    let got = get(&the_tests_client(&pki), named(listener.port()), 3);
    assert_eq!(got.ended, Ok(Downloaded::Bytes(3)));
    assert_eq!(listener.finish(), 1);
    assert_eq!(seen(&log)[0].name.as_deref(), Some("localhost"));

    // A certificate for the address `127.0.0.1` alone is refused when the address
    // asked is `localhost`, though that is where the name leads.
    let pki = Pki::loopback();
    let (listener, log) = serving(&pki, 1, vec![ok(b"abc")]);
    let got = get(&the_tests_client(&pki), named(listener.port()), 3);
    assert_eq!(got.fault().sentence(), UNTRUSTED);
    assert_eq!(listener.finish(), 1);
    assert!(seen(&log)[0].request.is_empty());
}

#[test]
fn a_server_that_sends_tls_records_and_none_of_the_answer_ends_by_the_limit_between_reads() {
    // A server whose certificate is good, that answers nothing and keeps the
    // connection busy with records of TLS alone: a key update, then ten seconds of a
    // clock only it moves, once the client has read the update, and again, 25 times.
    // (`rustls` takes 32 key updates in a row at most; session tickets of TLS 1.3, which
    // it takes without a limit, come only in one flight after the handshake, which
    // cannot be spread over the clock.) Only bytes of the answer count, toward the limit
    // between two reads and toward the least rate, so the download ends at that limit,
    // 30 seconds after its request; were the updates counted, each would put the limit
    // off again, and it would end only after the last of them.
    let pki = Pki::loopback();
    let clock = Hand::standing();
    let config = pki.server(&[&rustls::version::TLS13]);
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let listener = {
        let (clock, done) = (clock.clone(), done.clone());
        Listener::serve(1, move |_, stream| {
            let mut tls = rustls::StreamOwned::new(
                rustls::ServerConnection::new(config.clone()).unwrap(),
                stream,
            );
            read_head(&mut tls);
            // The client has read what was sent once it has looked at the clock twice
            // since: it looks before each read of its socket. A bounded wait, ended at
            // once when the test is past the download.
            let read_since = |before: u64| {
                let patience = std::time::Instant::now() + Duration::from_secs(5);
                while clock.reads() < before + 2
                    && !done.load(std::sync::atomic::Ordering::SeqCst)
                    && std::time::Instant::now() < patience
                {
                    std::thread::yield_now();
                }
            };
            for _ in 0..25 {
                let before = clock.reads();
                if tls.conn.refresh_traffic_keys().is_err() || tls.flush().is_err() {
                    return;
                }
                read_since(before);
                clock.advance(Duration::from_secs(10));
            }
            // Then a minute more, and nothing, until the client leaves.
            clock.advance(Duration::from_secs(60));
            let mut rest = [0; 1024];
            while matches!(tls.read(&mut rest), Ok(read) if read > 0) {}
        })
    };
    // A least rate a key update alone would keep.
    let slow = Timing {
        least_bytes_per_second: 1,
        ..timing()
    };
    let client = client(&pki, slow, clock.clone(), None);
    let got = get(&client, address(listener.port(), "/f"), 100);
    let spent = clock.elapsed();
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    let fault = got.fault();
    assert_eq!(fault.kind(), FaultKind::TimedOut);
    assert_eq!(fault.sentence(), STALLED);
    assert!(got.pieces.is_empty());
    // At the limit: three updates, ten seconds apart, and no fourth.
    assert!(spent >= Duration::from_secs(30), "{spent:?}");
    assert!(spent <= Duration::from_secs(40), "{spent:?}");
    assert_eq!(listener.finish(), 1);
}
