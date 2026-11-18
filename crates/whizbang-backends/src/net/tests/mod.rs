//! The download client's tests (plan 1C, task N1), against servers on loopback that the
//! test kit's listener runs (`whizbang_testkit::loopback`), each on a thread of the test
//! kit's and joined on every path. Nothing here contacts anything off the machine.
//!
//! They are inside the crate because the client trusts only the roots compiled into
//! it, and a test's server needs a root of the test's own: [`Client::trusting`] takes
//! one, and is compiled into these tests and nowhere else. The root and the server's
//! certificate are made here, from fixed made-up seeds, with `rustls`'s own signing
//! of Ed25519 keys: no key file is committed, and no crate is added for it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod client;
mod sender;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustls::{ServerConfig, ServerConnection, StreamOwned};
use whizbang_core::contract::{Clock, Secrets};
use whizbang_core::egress::{Address, Origin};

use super::client::{Client, Dial};
pub(super) use super::testing::Pki;
use super::wire::Timing;

/// A clock a test moves by hand, and that moves itself by `step` each time it is read.
pub(super) struct Hand {
    base: Instant,
    ms: AtomicU64,
    step: u64,
    /// How many times it has been read.
    reads: AtomicU64,
}

impl Hand {
    pub(super) fn standing() -> Arc<Hand> {
        Hand::stepping(0)
    }

    pub(super) fn stepping(step_ms: u64) -> Arc<Hand> {
        Arc::new(Hand {
            base: Instant::now(),
            ms: AtomicU64::new(0),
            step: step_ms,
            reads: AtomicU64::new(0),
        })
    }

    /// How many times it has been read.
    pub(super) fn reads(&self) -> u64 {
        self.reads.load(Ordering::SeqCst)
    }

    pub(super) fn advance(&self, by: Duration) {
        self.ms
            .fetch_add(u64::try_from(by.as_millis()).unwrap(), Ordering::SeqCst);
    }

    /// The time it says, without moving it.
    pub(super) fn elapsed(&self) -> Duration {
        Duration::from_millis(self.ms.load(Ordering::SeqCst))
    }
}

impl Clock for Hand {
    fn now_ms(&self) -> u64 {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.ms.fetch_add(self.step, Ordering::SeqCst)
    }

    fn now(&self) -> Instant {
        self.base + Duration::from_millis(self.now_ms())
    }
}

/// The limits of a test: the plan's, with a poll of a few milliseconds, so that a
/// test that waits for nothing spends little real time doing it.
pub(super) fn timing() -> Timing {
    Timing {
        connect: Duration::from_secs(10),
        between_reads: Duration::from_secs(30),
        least_bytes_per_second: 256 * 1024,
        poll: Duration::from_millis(5),
        until: None,
    }
}

/// A client of the tests: trusting `pki`'s root and no other, telling the time by
/// `clock`.
pub(super) fn client(
    pki: &Pki,
    timing: Timing,
    clock: Arc<dyn Clock>,
    dial: Option<Dial>,
) -> Client {
    Client::trusting(pki.roots(), timing, clock, Secrets::none(), dial)
}

/// The address of a path at a port of loopback.
pub(super) fn address(port: u16, path: &str) -> Address {
    Address::of(&format!("https://127.0.0.1:{port}{path}"), &Secrets::none()).unwrap()
}

pub(super) fn origin(port: u16) -> Origin {
    Origin::of(&format!("https://127.0.0.1:{port}"), &Secrets::none()).unwrap()
}

/// What a server saw of one connection.
#[derive(Debug, Clone, Default)]
pub(super) struct Seen {
    pub(super) request: Vec<u8>,
    pub(super) version: Option<rustls::ProtocolVersion>,
    pub(super) kind: Option<rustls::HandshakeKind>,
    /// The name the client asked for in the handshake (SNI), when it asked for one.
    pub(super) name: Option<String>,
}

pub(super) type Log = Arc<Mutex<Vec<Seen>>>;

/// Serves one connection: the handshake, the request's head, and `answer` as it is,
/// then the end of TLS. What it saw goes into `log`. A client that leaves early ends
/// it at any point.
pub(super) fn answer(config: Arc<ServerConfig>, stream: TcpStream, answer: &[u8], log: &Log) {
    respond(config, stream, log, |_| answer.to_vec());
}

/// The same, with the answer `choose` gives for the request's head.
pub(super) fn respond(
    config: Arc<ServerConfig>,
    stream: TcpStream,
    log: &Log,
    choose: impl FnOnce(&[u8]) -> Vec<u8>,
) {
    let mut tls = StreamOwned::new(ServerConnection::new(config).unwrap(), stream);
    let request = read_head(&mut tls);
    let answer = choose(&request);
    log.lock().unwrap().push(Seen {
        request,
        version: tls.conn.protocol_version(),
        kind: tls.conn.handshake_kind(),
        name: tls.conn.server_name().map(str::to_owned),
    });
    if tls.write_all(&answer).and_then(|()| tls.flush()).is_ok() {
        tls.conn.send_close_notify();
        let _ = tls.flush();
    }
}

/// The head of a request, up to its empty line, at most 64 KiB; what came when the
/// connection ended first.
pub(super) fn read_head(from: &mut impl Read) -> Vec<u8> {
    let mut head = Vec::new();
    let mut byte = [0];
    while !head.ends_with(b"\r\n\r\n") && head.len() < 64 * 1024 {
        match from.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => break,
        }
    }
    head
}

/// An answer of 200 with this body.
pub(super) fn ok(body: &[u8]) -> Vec<u8> {
    [
        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes(),
        body,
    ]
    .concat()
}
