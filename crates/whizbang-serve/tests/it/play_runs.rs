//! `play_runs::…`: a start goes through the executor's checks of preparation and of a
//! request id, in the order the plan states (plan m5, "The playground API", the route
//! `POST /api/runs`; ADR 0007).

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;

use crate::{Running, fresh_id};

/// How many runs the store holds, as a reader of its own sees it.
fn runs(running: &Running) -> usize {
    let root = running.scratch.path().join("store");
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    reader.snapshot().unwrap().runs().len()
}

#[test]
fn a_start_without_a_fresh_preparation_of_these_arguments_is_refused_stale_before_admission() {
    let running = Running::start("runs-stale", |_| {});
    let prepared = running.api(
        "POST",
        "/api/prepare",
        Some(&json!({"action": "counts", "arguments": {"n": 1}})),
    );
    let handle = prepared.json()["handle"].as_str().unwrap().to_owned();
    // No handle; a handle the host never made; a handle of other arguments.
    for (arguments, handle) in [
        (json!({"n": 1}), None),
        (
            json!({"n": 1}),
            Some("0123456789abcdef0123456789abcdef".to_owned()),
        ),
        (json!({"n": 2}), Some(handle.clone())),
    ] {
        let mut body = json!({"action": "counts", "arguments": arguments,
                              "request_id": fresh_id()});
        if let Some(handle) = handle {
            body["handle"] = json!(handle);
        }
        let refused = running.api("POST", "/api/runs", Some(&body));
        assert_eq!(refused.status, 409, "{}", refused.text());
        assert_eq!(refused.json()["kind"], "stale", "{}", refused.text());
    }
    // Nothing was admitted: the store holds no run, and every place is free.
    assert_eq!(runs(&running), 0);
    let started = running.api(
        "POST",
        "/api/runs",
        Some(
            &json!({"action": "counts", "arguments": {"n": 1}, "handle": handle,
                     "request_id": fresh_id()}),
        ),
    );
    assert_eq!(started.status, 201, "{}", started.text());
}

#[test]
fn a_preparation_past_its_age_is_stale() {
    let running = Running::start("runs-aged", |settings| {
        settings.preparation_age = Duration::ZERO;
    });
    let call = json!({"action": "counts", "arguments": {"n": 1}});
    let prepared = running.api("POST", "/api/prepare", Some(&call));
    let mut body = call;
    body["handle"] = prepared.json()["handle"].clone();
    body["request_id"] = json!(fresh_id());
    let refused = running.api("POST", "/api/runs", Some(&body));
    assert_eq!(refused.status, 409, "{}", refused.text());
    assert_eq!(refused.json()["kind"], "stale");
    assert_eq!(runs(&running), 0);
}

#[test]
fn a_repeated_request_id_answers_its_run_before_the_places_are_looked_at() {
    let running = Running::start("runs-repeat", |settings| settings.calls = 1);
    let call = json!({"action": "waits", "arguments": {}});
    let prepared = running.api("POST", "/api/prepare", Some(&call));
    let mut body = call;
    body["handle"] = prepared.json()["handle"].clone();
    body["request_id"] = json!("the-page-made-this");
    let started = running.api("POST", "/api/runs", Some(&body));
    assert_eq!(started.status, 201, "{}", started.text());
    let run = started.json()["run"].clone();
    // Every place is taken, and the handle is spent or not: the same request again is
    // answered with its run, and nothing more is admitted.
    for handle in [body["handle"].clone(), serde_json::Value::Null] {
        let mut again = body.clone();
        again["handle"] = handle;
        let repeat = running.api("POST", "/api/runs", Some(&again));
        assert_eq!(repeat.status, 200, "{}", repeat.text());
        assert_eq!(repeat.json()["run"], run);
        assert_eq!(repeat.json()["repeat"], true);
    }
    assert_eq!(runs(&running), 1);
    // Something else under the same id is a conflict.
    let mut other = body.clone();
    other["action"] = json!("quick");
    let conflict = running.api("POST", "/api/runs", Some(&other));
    assert_eq!(conflict.status, 409, "{}", conflict.text());
    assert_eq!(conflict.json()["kind"], "conflict");
    // A start names its request id.
    let mut unnamed = body.clone();
    unnamed.as_object_mut().unwrap().remove("request_id");
    assert_eq!(running.api("POST", "/api/runs", Some(&unnamed)).status, 400);
}

#[test]
fn two_thumbnails_being_made_make_a_third_answer_429_and_polls_go_on() {
    let running = Arc::new(Running::start("runs-thumbs", |_| {}));
    let id = format!("sha256:{}", "a".repeat(64));
    let held: Vec<_> = (0..2)
        .map(|_| {
            let running = running.clone();
            let path = format!("/api/artifacts/{id}/thumb?edge=64");
            whizbang_testkit::threads::spawn(move || running.api("GET", &path, None).status)
        })
        .collect();
    for _ in 0..2000 {
        if running.asked.load(std::sync::atomic::Ordering::SeqCst) == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(running.asked.load(std::sync::atomic::Ordering::SeqCst), 2);
    let third = running.api("GET", &format!("/api/artifacts/{id}/thumb?edge=64"), None);
    assert_eq!(third.status, 429, "{}", third.text());
    assert_eq!(third.header("Retry-After").as_deref(), Some("1"));
    // The door was not asked a third time, and a poll is answered meanwhile.
    assert_eq!(running.asked.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(running.api("GET", "/api/runs?open=1", None).status, 200);
    running
        .released
        .store(true, std::sync::atomic::Ordering::SeqCst);
    for one in held {
        assert_eq!(one.join().unwrap(), 200);
    }
    let again = running.api("GET", &format!("/api/artifacts/{id}/thumb?edge=64"), None);
    assert_eq!(again.status, 200);
}
