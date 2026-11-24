//! `prober::…`: the network's prober (plan m6, task R1, item 5; ADR 0019, "A download and
//! a probe"), against a scripted front on loopback.

use std::sync::Arc;

use whizbang_backends::net::testing::{Pki, prober_trusting};
use whizbang_core::capability::{FaultKind, Stage};
use whizbang_core::contract::{Cancellation, Secrets};
use whizbang_core::egress::{Address, Credential, PROBE_BODY_MAX_BYTES, ProbeRequest, Probed};

use crate::remote::scripted::{Step, chunked, scripted};

#[test]
fn a_probe_carries_no_credential_and_no_body_and_follows_no_redirect() {
    let pki = Arc::new(Pki::loopback());
    let document = br#"{"format":"pack-host/1","models":[]}"#.to_vec();
    let answered = document.clone();
    let long = vec![b'x'; PROBE_BODY_MAX_BYTES * 2];
    let front = scripted(
        &pki,
        vec![
            Step::Answer(Box::new(move |_| {
                let mut bytes =
                    b"HTTP/1.1 200 OK\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n"
                        .to_vec();
                bytes.extend(chunked(&[&answered]));
                bytes
            })),
            Step::Answer(Box::new(|_| {
                b"HTTP/1.1 302 Found\r\nLocation: /elsewhere\r\nContent-Length: 0\r\n\r\n".to_vec()
            })),
            Step::Answer(Box::new(move |_| {
                let mut bytes =
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", long.len())
                        .into_bytes();
                bytes.extend(&long);
                bytes
            })),
        ],
    );
    let prober = prober_trusting(&pki);
    let none = Secrets::none();
    let health = Address::of(&format!("{}/pack/1/health", front.base()), &none).unwrap();
    let probe = |request: &ProbeRequest| prober.probe(request, &Cancellation::new());
    // Its status and its body, asked as a GET with no credential and no body.
    assert_eq!(
        probe(&ProbeRequest::new(health.clone())).unwrap(),
        Probed::Reached {
            status: 200,
            body: document
        }
    );
    // A redirect comes back as the address it names, which is not asked.
    let moved = probe(&ProbeRequest::new(health.clone())).unwrap();
    let Probed::Moved(to) = moved else {
        panic!("{moved:?}");
    };
    assert_eq!(to.rest(), "/elsewhere");
    // A long body is handed on to one byte past the bound, for the checked prober to
    // refuse.
    let Probed::Reached { body, .. } = probe(&ProbeRequest::new(health.clone())).unwrap() else {
        panic!("not reached");
    };
    assert_eq!(body.len(), PROBE_BODY_MAX_BYTES + 1);
    // One that comes with a credential is refused before anything leaves.
    let credential =
        Credential::new(health.origin().clone(), "Authorization", "Bearer x", &none).unwrap();
    let carried = ProbeRequest::new(health)
        .with_credential(credential)
        .unwrap();
    let refused = probe(&carried).unwrap_err();
    assert_eq!(
        (refused.kind(), refused.stage()),
        (FaultKind::Rejected, Stage::NotSent)
    );
    // Three requests, each a GET of the health with no credential and no body.
    let seen = front.seen();
    assert_eq!(front.listener.finish(), 3);
    for seen in seen {
        assert!(
            seen.line.starts_with("GET /pack/1/health "),
            "{}",
            seen.line
        );
        assert!(seen.field("authorization").is_none());
        assert!(seen.field("content-length").is_none() && seen.body.is_empty());
    }
}
