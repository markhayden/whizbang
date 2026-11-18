//! What a test of this crate, or of a crate that turns on the feature `fakes`, needs
//! to reach a server on loopback over TLS (ADR 0024, "How a test reaches a server on
//! loopback"): a root made in the test and a server's certificate it signed
//! ([`Pki`]), and the executor's ways out with the client trusting that root and no
//! other ([`ways_trusting`]). It is compiled into the crate's own tests and, under
//! `fakes`, into the tests of the crates that turn it on; the binary that ships is
//! built without `fakes` (`scripts/check-features.sh`), so no source of it can name
//! any of this.
//!
//! The root and the server's certificate are made from fixed made-up seeds, as
//! Ed25519 keys whose PKCS#8 is the seed, signed with `rustls`'s own signing: no key
//! file is committed, and no crate is added for it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{RootCertStore, ServerConfig, ServerConnection, StreamOwned};
use whizbang_core::contract::{Secrets, SystemClock};
use whizbang_core::egress::{NotAllowed, Ways};

use super::client::Client;

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
pub struct Pki {
    root: CertificateDer<'static>,
    leaf: CertificateDer<'static>,
    leaf_seed: u8,
}

impl Pki {
    /// A root from the seed `root_seed`, and a certificate for the address `ip`
    /// alone, signed by it.
    pub fn made(root_seed: u8, ip: [u8; 4]) -> Pki {
        Pki::named(root_seed, der(0x87, &ip))
    }

    /// The same, for the DNS name `name` alone.
    pub fn for_name(root_seed: u8, name: &str) -> Pki {
        Pki::named(root_seed, der(0x82, name.as_bytes()))
    }

    /// A root, and a certificate for the one subject alternative name `alt_name`.
    fn named(root_seed: u8, alt_name: Vec<u8>) -> Pki {
        let root_name = "whizbang test root";
        let constraints = seq(&[der(0x01, &[0xff])]);
        let root = certificate(
            1,
            (root_name, root_seed),
            (root_name, root_seed),
            vec![extension(&[0x55, 0x1d, 0x13], true, constraints)],
        );
        let leaf_seed = root_seed.wrapping_add(1);
        let alt_names = seq(&[alt_name]);
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
    pub fn loopback() -> Pki {
        Pki::made(7, [127, 0, 0, 1])
    }

    pub fn roots(&self) -> RootCertStore {
        let mut roots = RootCertStore::empty();
        roots.add(self.root.clone()).unwrap();
        roots
    }

    /// A server of these TLS versions, which shows this certificate.
    pub fn server(
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
    pub fn both(&self) -> Arc<ServerConfig> {
        self.server(&[&rustls::version::TLS13, &rustls::version::TLS12])
    }
}

impl Pki {
    /// A server's side of one connection that accepted `stream`: TLS of 1.3 and 1.2
    /// with this certificate, read and written as a stream.
    pub fn accept(&self, stream: TcpStream) -> StreamOwned<ServerConnection, TcpStream> {
        StreamOwned::new(ServerConnection::new(self.both()).unwrap(), stream)
    }
}

/// The executor's ways out as [`super::ways`] makes them, the client trusting the root
/// of `pki` and no other.
pub fn ways_trusting(pki: &Pki, store_root: &Path, secrets: &Secrets) -> Result<Ways, NotAllowed> {
    let client = Client::trusting(
        pki.roots(),
        super::sender_timing(),
        Arc::new(SystemClock),
        secrets.clone(),
        None,
    );
    super::ways_over(Arc::new(client), store_root)
}

/// The client as a bare sender trusting the root of `pki`: for the crate's own tests
/// of an adapter, which ask a server on loopback with no executor around them. It is
/// compiled into those tests and nowhere else.
#[cfg(test)]
pub(crate) fn sender_trusting(pki: &Pki) -> Arc<dyn whizbang_core::egress::Sender> {
    Arc::new(Client::trusting(
        pki.roots(),
        super::sender_timing(),
        Arc::new(SystemClock),
        Secrets::none(),
        None,
    ))
}
