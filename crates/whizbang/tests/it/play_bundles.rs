//! `play_bundles::…`: bundles in the playground (plan m7, task U2, item 1): written from a
//! run or a chain, taken in through the host's bytes and read whole by the core's reader
//! before anything is published, held to the bundle's bound and to the host's two upload
//! places, deleted, and never named by a path.

use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;

use serde_json::json;

use crate::play::Kitted;
use crate::play_chain::{bundled, chain_in, imported, opened, started};

/// The bundles a store holds, by their digests.
fn held(kitted: &Kitted) -> Vec<String> {
    whizbang_core::bundle::held(&kitted.generation())
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Where the bytes of a tar's last member start: its headers walked, each size read.
fn last_member_data(tar: &[u8]) -> usize {
    let mut at = 0;
    let mut last = 0;
    while at + 512 <= tar.len() && tar[at..at + 512].iter().any(|b| *b != 0) {
        let size = std::str::from_utf8(&tar[at + 124..at + 135]).unwrap();
        let size =
            usize::from_str_radix(size.trim_matches(|c: char| c == '\0' || c == ' '), 8).unwrap();
        last = at + 512;
        at += 512 + size.div_ceil(512) * 512;
    }
    last
}

#[test]
fn a_bundle_uploaded_is_read_whole_before_anything_is_imported() {
    let kitted = Kitted::new("bundles-whole");
    let chain = chain_in(&kitted, 80);
    let bytes = bundled(&kitted, &[&chain.removal, &chain.check]);
    // One run's bundle downloads too, as a tar named for its task and run.
    let one = kitted.api("GET", &format!("/api/runs/{}/bundle", chain.removal), None);
    assert_eq!(one.status, 200, "{}", one.text());
    assert!(whizbang_core::bundle::check(std::io::Cursor::new(&one.body)).is_ok());
    let about = kitted.api(
        "GET",
        &format!("/api/runs/{}/bundle/about", chain.removal),
        None,
    );
    assert!(
        about.json()["about_bytes"].as_u64().unwrap() > 0,
        "{}",
        about.text()
    );
    // A byte changed in its last member: the reader refuses it at the end of its reading,
    // and nothing of it was published.
    let mut broken = bytes.clone();
    let at = last_member_data(&broken);
    broken[at] ^= 0x20;
    let generation = kitted.generation().number();
    let refused = kitted.send_bytes("/api/bundles", "application/x-tar", &broken);
    assert_eq!(refused.status, 422, "{}", refused.text());
    assert!(
        refused.json()["error"]
            .as_str()
            .unwrap()
            .contains("nothing was imported and nothing ran"),
        "{}",
        refused.text()
    );
    assert_eq!(kitted.generation().number(), generation);
    assert!(held(&kitted).is_empty());
    // Whole, it is taken in once; a second time changes nothing.
    let digest = imported(&kitted, &bytes);
    assert_eq!(held(&kitted), vec![digest.clone()]);
    let again = kitted.send_bytes("/api/bundles", "application/x-tar", &bytes);
    assert_eq!(again.json()["imported"], false, "{}", again.text());
    let listed = kitted.api("GET", "/api/bundles", None).json();
    assert_eq!(listed["bundles"][0]["digest"], digest.as_str(), "{listed}");
    // Not a tar's type: refused before it is read.
    let typed = kitted.send_bytes("/api/bundles", "image/png", &bytes);
    assert_eq!(typed.status, 415, "{}", typed.text());
}

#[test]
fn a_bundle_over_256_mib_is_refused_413_before_it_is_read_whole() {
    let kitted = Kitted::new("bundles-bound");
    let over = whizbang_core::bundle::MAX_BYTES + 1;
    // Only the head is sent: the answer comes without one byte of the body.
    let head = kitted.head_of("POST", "/api/bundles", "application/x-tar", over);
    let answer = crate::play::send(kitted.address(), &head);
    assert_eq!(answer.status, 413, "{}", answer.text());
    assert!(answer.text().contains("256 MiB"), "{}", answer.text());
    assert!(held(&kitted).is_empty());
}

#[test]
fn with_two_bundle_uploads_stalled_a_third_is_refused_429_and_polling_and_stop_are_answered() {
    let kitted = Kitted::new("bundles-stalled");
    // Two uploads that send their head and a few bytes, and then nothing.
    let stall = || {
        let mut stream = TcpStream::connect(kitted.address()).unwrap();
        let mut head = kitted.head_of("POST", "/api/bundles", "application/x-tar", 4096);
        head.extend_from_slice(&[0u8; 16]);
        stream.write_all(&head).unwrap();
        stream.set_nonblocking(true).unwrap();
        stream
    };
    let mut stalled = vec![stall(), stall()];
    // Each holds its place once its head is read. A probe, sent whole so that one taken
    // before then is answered at once (it is no bundle), is refused once both are held;
    // a stalled upload that a probe beat to a place was answered, and is sent again.
    let mut third = None;
    for _ in 0..200 {
        for one in &mut stalled {
            let mut byte = [0u8; 1];
            if !matches!(one.peek(&mut byte), Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock)
            {
                *one = stall();
            }
        }
        let mut head = kitted.head_of("POST", "/api/bundles", "application/x-tar", 1024);
        head.extend_from_slice(&[0u8; 1024]);
        let answer = crate::play::send(kitted.address(), &head);
        if answer.status == 429 {
            third = Some(answer);
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let third = third.expect("a third upload is refused while two are stalled");
    assert!(third.text().contains("at most two"), "{}", third.text());
    // Polls and Stop find a worker.
    let open = kitted.api("GET", "/api/runs?open=1", None);
    assert_eq!(open.status, 200, "{}", open.text());
    let stop = kitted.api("POST", "/api/runs/1-1/cancel", None);
    assert_eq!(stop.status, 404, "{}", stop.text());
    let hello = kitted.api("GET", "/api/hello", None);
    assert_eq!(hello.status, 200);
    drop(stalled);
}

#[test]
fn a_deleted_bundle_leaves_the_artifacts_a_run_names() {
    let first = Kitted::new("bundles-made");
    let chain = chain_in(&first, 81);
    let bytes = bundled(&first, &[&chain.removal]);
    drop(first);
    // Another store takes it in, and a step of it runs there.
    let kitted = Kitted::new("bundles-elsewhere");
    let digest = imported(&kitted, &bytes);
    let draft = opened(&kitted, &digest, 0, &[]);
    let picture = draft["values"]["image"].as_str().unwrap().to_owned();
    let step = json!({"bundle": digest, "step": 0});
    let made = started(&kitted, "remove_prompt", &draft["values"], Some(&step));
    let deleted = kitted.api("DELETE", &format!("/api/bundles/{digest}"), None);
    assert_eq!(deleted.status, 200, "{}", deleted.text());
    assert!(held(&kitted).is_empty());
    assert_eq!(
        kitted
            .api("GET", &format!("/api/bundles/{digest}"), None)
            .status,
        404
    );
    // Once a prune has run, the run still holds the picture it took and what it made.
    let shown = kitted.api("GET", &format!("/api/artifacts/{picture}"), None);
    assert_eq!(shown.status, 200, "{}", shown.text());
    let record = kitted
        .api("GET", &format!("/api/runs/{made}/record"), None)
        .json();
    for output in record["outputs"].as_array().unwrap() {
        let id = output["id"].as_str().unwrap();
        let shown = kitted.api("GET", &format!("/api/artifacts/{id}"), None);
        assert_eq!(shown.status, 200, "{}", shown.text());
    }
    // A second deletion finds nothing.
    let again = kitted.api("DELETE", &format!("/api/bundles/{digest}"), None);
    assert_eq!(again.status, 404, "{}", again.text());
}

#[test]
fn no_bundle_route_takes_a_path() {
    let kitted = Kitted::new("bundles-no-path");
    // A digest is the only name of a bundle; anything else is no bundle.
    for path in [
        "/api/bundles/..%2F..%2Fetc%2Fpasswd",
        "/api/bundles/%2Ftmp%2Fx.tar",
        "/api/bundles/sha256:..",
    ] {
        assert_eq!(kitted.api("GET", path, None).status, 404, "{path}");
        assert_eq!(kitted.api("DELETE", path, None).status, 404, "{path}");
    }
    // A run is named by its id; a field that names a file is not taken.
    let chain = kitted.api(
        "POST",
        "/api/bundles/chain",
        Some(&json!({"runs": ["/etc/passwd"]})),
    );
    assert_eq!(chain.status, 404, "{}", chain.text());
    let file = kitted.api(
        "POST",
        "/api/bundles/chain",
        Some(&json!({"runs": ["1-1"], "to": "/tmp/out.tar"})),
    );
    assert_eq!(file.status, 400, "{}", file.text());
    let from = kitted.send_bytes("/api/bundles?path=/tmp/in.tar", "application/x-tar", b"");
    assert_eq!(from.status, 400, "{}", from.text());
    let opened = kitted.api(
        "POST",
        &format!("/api/bundles/sha256:{}/steps/0/open", "0".repeat(64)),
        Some(&json!({"runs": [], "file": "/tmp/in.tar"})),
    );
    assert_eq!(opened.status, 400, "{}", opened.text());
    let replayed = kitted.api(
        "POST",
        "/api/runs/1-1/replay",
        Some(&json!({"request_id": "page-x", "bundle": "/tmp/in.tar"})),
    );
    assert_eq!(replayed.status, 400, "{}", replayed.text());
}

#[test]
fn a_chain_is_checked_from_its_records_without_being_written() {
    let kitted = Kitted::new("bundles-check");
    let chain = chain_in(&kitted, 83);
    let body = json!({"runs": [chain.removal, chain.check], "check": true});
    let checked = kitted.api("POST", "/api/bundles/chain", Some(&body));
    assert_eq!(checked.status, 200, "{}", checked.text());
    let said = checked.json();
    let linked = said["steps"][1]["inputs"].as_array().unwrap();
    assert!(
        linked.iter().any(|input| input["from"]["step"] == 0),
        "{said}"
    );
    // The same run twice is refused, as the writer refuses it.
    let twice = json!({"runs": [chain.removal, chain.removal], "check": true});
    let refused = kitted.api("POST", "/api/bundles/chain", Some(&twice));
    assert_eq!(refused.status, 422, "{}", refused.text());
    assert!(refused.text().contains("given twice"), "{}", refused.text());
    assert!(held(&kitted).is_empty());
}
