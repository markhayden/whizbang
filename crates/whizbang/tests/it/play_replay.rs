//! `play_replay::…`: the replay of a run from the playground (plan m7, task U2, item 3):
//! started on the host's one executor through `POST /api/runs/{id}/replay`, polled as any
//! run there, and its report read once it has ended.

use serde_json::json;

use crate::play::{Kitted, fresh_id};
use crate::play_chain::ran;

#[test]
fn an_offline_replay_from_the_page_spends_nothing_and_shows_reproduced() {
    let kitted = Kitted::new("replay-offline");
    let arguments = json!({"image": kitted.picture(90), "prompt": "the cup"});
    // A paid run: the loop's fill and its judge.
    let run = ran(&kitted, "remove_checked", &arguments);
    let asked = kitted.kit().requests();
    let spent = kitted.api("GET", "/api/hello", None).json()["allowance"]["spent"].clone();
    let id = fresh_id();
    let body = json!({"live": false, "request_id": id});
    let started = kitted.api("POST", &format!("/api/runs/{run}/replay"), Some(&body));
    assert_eq!(started.status, 201, "{}", started.text());
    let replay = started.json()["run"].as_str().unwrap().to_owned();
    // Polled as any run of the host, and the same id answers the same replay.
    let polled = kitted.until_ended(&replay);
    assert_eq!(polled["outcome"], "completed", "{polled}");
    let again = kitted.api("POST", &format!("/api/runs/{run}/replay"), Some(&body));
    assert_eq!(again.json()["run"], replay.as_str(), "{}", again.text());
    let report = kitted.api("GET", &format!("/api/runs/{replay}/replay"), None);
    assert_eq!(report.status, 200, "{}", report.text());
    let report = report.json();
    assert_eq!(report["report"], "reproduced", "{report}");
    assert_eq!(report["of"], run.as_str(), "{report}");
    assert_eq!(report["mode"], "offline", "{report}");
    assert_eq!(report["spent_micros"], 0, "{report}");
    // Nothing was asked of a backend, and nothing spent.
    assert_eq!(kitted.kit().requests(), asked);
    let now = kitted.api("GET", "/api/hello", None).json()["allowance"]["spent"].clone();
    assert_eq!(now, spent);
    // The record says it is a replay; an ordinary run has no replay's report.
    let record = kitted
        .api("GET", &format!("/api/runs/{replay}/record"), None)
        .json();
    assert_eq!(record["replayed"]["run"], run.as_str(), "{record}");
    assert_eq!(record["replayed"]["mode"], "offline", "{record}");
    assert_eq!(
        kitted
            .api("GET", &format!("/api/runs/{run}/replay"), None)
            .status,
        422
    );
    // A run that is not a root is refused, with the engine's reason.
    let child = record_child(&kitted, &run);
    let refused = kitted.api(
        "POST",
        &format!("/api/runs/{child}/replay"),
        Some(&json!({"request_id": fresh_id()})),
    );
    assert_eq!(refused.status, 422, "{}", refused.text());
    assert!(
        refused.json()["error"]
            .as_str()
            .unwrap()
            .contains("another run called"),
        "{}",
        refused.text()
    );
}

/// A run the run `run` called.
fn record_child(kitted: &Kitted, run: &str) -> String {
    let generation = kitted.generation();
    (generation.runs().keys())
        .find(|id| {
            let reader = whizbang_core::store::Reader::open(
                &kitted.root(),
                std::sync::Arc::new(whizbang_core::store::fs::RealFs),
            )
            .unwrap();
            whizbang_core::contract::record::stored(&generation, id, |e| reader.read(e))
                .and_then(Result::ok)
                .is_some_and(|record| record.identity().parent.as_deref() == Some(run))
        })
        .cloned()
        .expect("the loop called a run")
}
