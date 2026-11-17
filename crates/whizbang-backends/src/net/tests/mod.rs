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

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{RootCertStore, ServerConfig, ServerConnection, StreamOwned};
use whizbang_core::contract::{Clock, Secrets};
use whizbang_core::egress::{Address, Origin};

use super::client::{Client, Dial};
use super::wire::Timing;

/// A clock a test moves by hand, and that moves itself by `step` each time it is read.
pub(super) struct Hand {
    base: Instant,
    ms: AtomicU64,
    step: u64,
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
        })
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
    }
}

// A few lines of DER, enough for two certificates.

fn der(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = body.len();
    if len < 0x80 {
        out.push(u8::try_from(len).unwrap());
    } else if len < 0x100 {
        out.extend([0x81, u8::try_from(len).unwrap()]);
    } else {
        out.extend([0x82, u8::try_from(len >> 8).unwrap(), (len & 0xff) as u8]);
    }
    out.extend_from_slice(body);
    out
}

fn seq(parts: &[Vec<u8>]) -> Vec<u8> {
    der(0x30, &parts.concat())
}

fn oid(bytes: &[u8]) -> Vec<u8> {
    der(0x06, bytes)
}

const ED25519: [u8; 3] = [0x2b, 0x65, 0x70];

fn name(common: &str) -> Vec<u8> {
    let attribute = seq(&[oid(&[0x55, 0x04, 0x03]), der(0x0c, common.as_bytes())]);
    seq(&[der(0x31, &attribute)])
}

fn extension(id: &[u8], critical: bool, value: Vec<u8>) -> Vec<u8> {
    let mut parts = vec![oid(id)];
    if critical {
        parts.push(der(0x01, &[0xff]));
    }
    parts.push(der(0x04, &value));
    seq(&parts)
}

/// An Ed25519 key from a seed, as PKCS#8 of the first version: the seed is the key.
fn key(seed: u8) -> PrivatePkcs8KeyDer<'static> {
    let mut pkcs8 = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    pkcs8.extend([seed; 32]);
    PrivatePkcs8KeyDer::from(pkcs8)
}

/// A certificate of `subject`'s key, signed with `issuer`'s, valid from 2020 to 2049.
fn certificate(
    serial: u8,
    issuer: (&str, u8),
    subject: (&str, u8),
    extensions: Vec<Vec<u8>>,
) -> CertificateDer<'static> {
    let signing = |seed| rustls::crypto::ring::sign::any_eddsa_type(&key(seed)).unwrap();
    let public = signing(subject.1).public_key().unwrap().to_vec();
    let algorithm = seq(&[oid(&ED25519)]);
    let validity = seq(&[der(0x17, b"200101000000Z"), der(0x17, b"491231235959Z")]);
    let tbs = seq(&[
        der(0xa0, &der(0x02, &[2])),
        der(0x02, &[serial]),
        algorithm.clone(),
        name(issuer.0),
        validity,
        name(subject.0),
        public,
        der(0xa3, &seq(&extensions)),
    ]);
    let signer = signing(issuer.1)
        .choose_scheme(&[rustls::SignatureScheme::ED25519])
        .unwrap();
    let signature = signer.sign(&tbs).unwrap();
    let signature = der(0x03, &[&[0u8][..], &signature].concat());
    CertificateDer::from(seq(&[tbs, algorithm, signature]))
}

/// A root made in a test, and a server's certificate it signed, for one address of
/// loopback.
pub(super) struct Pki {
    root: CertificateDer<'static>,
    leaf: CertificateDer<'static>,
    leaf_seed: u8,
}

impl Pki {
    /// A root from the seed `root_seed`, and a certificate for `127.0.0.1` and the
    /// host `ip` names, signed by it.
    pub(super) fn made(root_seed: u8, ip: [u8; 4]) -> Pki {
        let root_name = "whizbang test root";
        let constraints = seq(&[der(0x01, &[0xff])]);
        let root = certificate(
            1,
            (root_name, root_seed),
            (root_name, root_seed),
            vec![extension(&[0x55, 0x1d, 0x13], true, constraints)],
        );
        let leaf_seed = root_seed.wrapping_add(1);
        let alt_names = seq(&[der(0x87, &ip)]);
        let server_auth = seq(&[oid(&[0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x01])]);
        let leaf = certificate(
            2,
            (root_name, root_seed),
            ("whizbang test server", leaf_seed),
            vec![
                extension(&[0x55, 0x1d, 0x11], false, alt_names),
                extension(&[0x55, 0x1d, 0x25], false, server_auth),
            ],
        );
        Pki {
            root,
            leaf,
            leaf_seed,
        }
    }

    /// The root of the tests, with a server's certificate for `127.0.0.1`.
    pub(super) fn loopback() -> Pki {
        Pki::made(7, [127, 0, 0, 1])
    }

    pub(super) fn roots(&self) -> RootCertStore {
        let mut roots = RootCertStore::empty();
        roots.add(self.root.clone()).unwrap();
        roots
    }

    /// A server of these TLS versions, which shows this certificate.
    pub(super) fn server(
        &self,
        versions: &[&'static rustls::SupportedProtocolVersion],
    ) -> Arc<ServerConfig> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(versions)
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![self.leaf.clone()],
                PrivateKeyDer::Pkcs8(key(self.leaf_seed)),
            )
            .unwrap();
        Arc::new(config)
    }

    /// A server of TLS 1.3 and 1.2.
    pub(super) fn both(&self) -> Arc<ServerConfig> {
        self.server(&[&rustls::version::TLS13, &rustls::version::TLS12])
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
