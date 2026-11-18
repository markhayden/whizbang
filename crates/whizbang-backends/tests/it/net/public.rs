//! `public::…`: the two ways to the client from outside the crate are a checked
//! downloader (ADR 0024) and ways out whose sender has the check behind a send over it
//! (plan 2A, task S1).

use std::net::{Ipv4Addr, TcpListener};

use whizbang_backends::net::{
    ANSWER_WAIT_LIMIT, BETWEEN_READS_LIMIT, CONNECT_LIMIT, downloader, ways,
};
use whizbang_core::capability::{FaultKind, Stage};
use whizbang_core::config::DOWNLOAD_LEAST_KIB_LIMIT;
use whizbang_core::contract::{Cancellation, Secrets};
use whizbang_core::egress::{
    Address, CheckedDownloader, DownloadRequest, Downloader, NotAllowed, Origin,
};

fn none() -> Secrets {
    Secrets::none()
}

#[test]
fn the_only_public_way_to_the_client_is_a_checked_downloader() {
    // What the crate gives is a checked downloader, by its type.
    let origins = [Origin::of("https://weights.example.org", &none()).unwrap()];
    let made: CheckedDownloader = downloader(256, &origins, &none()).unwrap();
    assert_eq!(format!("{made:?}"), "CheckedDownloader(1 origins)");
    assert_eq!(
        downloader(0, &origins, &none()).unwrap_err(),
        NotAllowed::Empty
    );
    assert_eq!(
        downloader(DOWNLOAD_LEAST_KIB_LIMIT + 1, &origins, &none()).unwrap_err(),
        NotAllowed::TooMany
    );
    assert!(downloader(DOWNLOAD_LEAST_KIB_LIMIT, &origins, &none()).is_ok());
    assert_eq!(CONNECT_LIMIT.as_secs(), 10);
    assert_eq!(BETWEEN_READS_LIMIT.as_secs(), 30);

    // And ways out whose sender is the client, under the policy that approves
    // nothing until a snapshot gives its own; a root of nothing is refused.
    let out = ways(std::path::Path::new("/a/store"), &none()).unwrap();
    assert!(
        format!("{out:?}").starts_with("Ways { sender: true, "),
        "{out:?}"
    );
    assert_eq!(ANSWER_WAIT_LIMIT.as_secs(), 600);
    assert_eq!(
        ways(std::path::Path::new(""), &none()).unwrap_err(),
        NotAllowed::Empty
    );

    // And nothing else of the module is public: every item its files declare `pub`
    // is one of these; `testing` is compiled only into tests and under `fakes`
    // (`#[cfg(any(test, feature = "fakes"))]`), which the binary that ships is built
    // without. The client, its constructors, its wire and its HTTP are the
    // crate's own.
    let net = concat!(env!("CARGO_MANIFEST_DIR"), "/src/net");
    let mut public = Vec::new();
    for file in ["mod.rs", "client.rs", "http.rs", "wire.rs"] {
        let text = std::fs::read_to_string(format!("{net}/{file}")).unwrap();
        for line in text.lines() {
            let line = line.trim_start();
            if line.starts_with("pub ") {
                let item: String = line.split(['(', ':', '=', '<', '{']).next().unwrap().into();
                public.push(format!("{file}: {}", item.trim()));
            }
        }
    }
    assert_eq!(
        public,
        [
            "mod.rs: pub mod testing;",
            "mod.rs: pub const CONNECT_LIMIT",
            "mod.rs: pub const BETWEEN_READS_LIMIT",
            "mod.rs: pub const ANSWER_WAIT_LIMIT",
            "mod.rs: pub fn downloader",
            "mod.rs: pub fn ways",
        ]
    );
    let lib = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).unwrap();
    assert!(lib.contains("pub mod net;"));
}

#[test]
fn through_the_public_way_an_address_off_its_origins_or_not_https_asks_nothing() {
    // A port of loopback nobody listens at: nothing could answer, and nothing is asked.
    let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let here = format!("127.0.0.1:{port}");
    let gate = downloader(
        256,
        &[Origin::of(&format!("https://{here}"), &none()).unwrap()],
        &none(),
    )
    .unwrap();
    let ask = |text: &str| {
        let request = DownloadRequest::new(Address::of(text, &none()).unwrap(), 1).unwrap();
        gate.download(&request, &Cancellation::new(), &mut |_| true)
            .unwrap_err()
    };
    // Off the gate's origins: refused by the gate.
    let off = ask(&format!("http://{here}/f"));
    assert_eq!(
        (off.kind(), off.stage()),
        (FaultKind::Rejected, Stage::NotSent)
    );
    // At its origin, with nobody listening: the client never connected.
    let refused = ask(&format!("https://{here}/f"));
    assert_eq!(refused.kind(), FaultKind::Unavailable);
    assert_eq!(refused.stage(), Stage::NotSent);
    assert!(!format!("{refused:?}").contains(&port.to_string()));
}
