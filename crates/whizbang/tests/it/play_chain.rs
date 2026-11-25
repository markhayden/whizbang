//! `play_chain::…`: guided chain replay in the playground (plan m7, task U2, item 2): a
//! chain of a removal and its check, bundled through the host, imported, and each step
//! opened as a draft that runs nothing until it is started with `replay`, its linked input
//! bound to the new run of the step before. The host runs in process over the binary's
//! wiring and the test kit's `playkit` backends (`play::Kitted`).

use serde_json::{Value as Json, json};

use crate::play::{Kitted, fresh_id};

/// Runs a call through the host's prepare and start, and answers its run once it ended.
pub(crate) fn ran(kitted: &Kitted, action: &str, arguments: &Json) -> String {
    started(kitted, action, arguments, None)
}

/// The same, as a chain's step when `step` names one.
pub(crate) fn started(
    kitted: &Kitted,
    action: &str,
    arguments: &Json,
    step: Option<&Json>,
) -> String {
    let prepared = kitted.prepare(action, arguments);
    assert_eq!(prepared["ready"], true, "{prepared}");
    let mut body = json!({"action": action, "arguments": arguments,
                          "handle": prepared["handle"], "request_id": fresh_id()});
    if let Some(step) = step {
        body["replay"] = step.clone();
    }
    let answer = kitted.api("POST", "/api/runs", Some(&body));
    assert_eq!(answer.status, 201, "{}", answer.text());
    let run = answer.json()["run"].as_str().unwrap().to_owned();
    kitted.until_ended(&run);
    run
}

/// A removal of a picture, and the check of an edit that takes its picture: the chain's
/// two runs, the picture, and the removal's output picture.
pub(crate) struct Chain {
    pub removal: String,
    pub check: String,
    pub removed: String,
}

pub(crate) fn chain_in(kitted: &Kitted, shade: u8) -> Chain {
    let picture = kitted.picture(shade);
    let removal = ran(
        kitted,
        "remove_prompt",
        &json!({"image": picture, "prompt": "the cup"}),
    );
    let record = kitted
        .api("GET", &format!("/api/runs/{removal}/record"), None)
        .json();
    let check = (record["send_to"].as_array().unwrap().iter())
        .find(|target| target["task"] == "source-fidelity")
        .expect("check an edit is offered")
        .clone();
    let mut values = check["values"].as_object().unwrap().clone();
    values.remove("brief");
    values.insert("job_of".to_owned(), json!(removal));
    let removed = values["candidate"].as_str().unwrap().to_owned();
    let check = ran(kitted, "source-fidelity", &Json::Object(values));
    Chain {
        removal,
        check,
        removed,
    }
}

/// A chain's bundle, as the host writes one.
pub(crate) fn bundled(kitted: &Kitted, runs: &[&str]) -> Vec<u8> {
    let answer = kitted.api("POST", "/api/bundles/chain", Some(&json!({"runs": runs})));
    assert_eq!(answer.status, 200, "{}", answer.text());
    answer.body
}

/// A bundle taken in through the host: its digest.
pub(crate) fn imported(kitted: &Kitted, bytes: &[u8]) -> String {
    let answer = kitted.send_bytes("/api/bundles", "application/x-tar", bytes);
    assert_eq!(answer.status, 201, "{}", answer.text());
    answer.json()["digest"].as_str().unwrap().to_owned()
}

/// A step's draft, with the new runs of the steps before it.
pub(crate) fn opened(kitted: &Kitted, digest: &str, step: u32, runs: &[Option<&str>]) -> Json {
    let answer = kitted.api(
        "POST",
        &format!("/api/bundles/{digest}/steps/{step}/open"),
        Some(&json!({"runs": runs})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text());
    answer.json()
}

/// How many runs history lists, with replayed families as `replayed` says.
fn listed(kitted: &Kitted, replayed: &str) -> Vec<String> {
    let path = match replayed {
        "" => "/api/runs?limit=200".to_owned(),
        how => format!("/api/runs?limit=200&replayed={how}"),
    };
    let answer = kitted.api("GET", &path, None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    (answer.json()["runs"].as_array().unwrap().iter())
        .map(|row| row["run"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_step_runs_only_when_started_and_continue_binds_the_new_output() {
    let kitted = Kitted::new("chain-binds");
    let chain = chain_in(&kitted, 70);
    let digest = imported(&kitted, &bundled(&kitted, &[&chain.removal, &chain.check]));
    let shown = kitted
        .api("GET", &format!("/api/bundles/{digest}"), None)
        .json();
    assert_eq!(shown["steps"].as_array().unwrap().len(), 2, "{shown}");
    // The check's edited picture is the removal's output: a link, not an identity.
    let linked = (shown["steps"][1]["inputs"].as_array().unwrap().iter())
        .find(|input| input["from"].is_object())
        .unwrap_or_else(|| panic!("no link: {shown}"))
        .clone();
    assert_eq!(linked["from"]["step"], 0, "{shown}");
    assert_eq!(linked["from"]["port"], "image", "{shown}");
    // Each step replays by this build's decision.
    assert_eq!(shown["steps"][0]["replayable"], true, "{shown}");
    // Opening a step starts nothing: no request, no run, nothing going.
    let asked = kitted.kit().requests();
    let before = listed(&kitted, "include");
    let first = opened(&kitted, &digest, 0, &[]);
    assert_eq!(first["task"], "remove_prompt", "{first}");
    assert_eq!(first["stopped"], Json::Null, "{first}");
    assert_eq!(first["values"]["prompt"], "the cup", "{first}");
    let second_early = opened(&kitted, &digest, 1, &[None]);
    assert!(
        second_early["stopped"]["why"]
            .as_str()
            .unwrap()
            .contains("has not run"),
        "{second_early}"
    );
    assert_eq!(kitted.kit().requests(), asked);
    assert_eq!(listed(&kitted, "include"), before);
    let open = kitted.api("GET", "/api/runs?open=1", None).json();
    assert_eq!(open["runs"], json!([]), "{open}");
    // Run: the step is an ordinary run, started from the draft.
    let step = json!({"bundle": digest, "step": 0});
    let made = started(&kitted, "remove_prompt", &first["values"], Some(&step));
    let record = kitted
        .api("GET", &format!("/api/runs/{made}/record"), None)
        .json();
    let output = (record["outputs"].as_array().unwrap().iter())
        .find(|output| output["port"] == "image")
        .unwrap()["id"]
        .clone();
    // Continue: the next step's linked input is the new run's output, beside the one
    // recorded.
    let second = opened(&kitted, &digest, 1, &[Some(&made)]);
    assert_eq!(second["stopped"], Json::Null, "{second}");
    assert_eq!(second["values"]["candidate"], output, "{second}");
    let binding = &second["bindings"][0];
    assert_eq!(binding["port"], linked["port"], "{second}");
    assert_eq!(binding["run"], made.as_str(), "{second}");
    assert_eq!(binding["now"], output, "{second}");
    assert_eq!(binding["recorded"], chain.removed.as_str(), "{second}");
    assert_eq!(
        binding["differs"],
        json!(output != json!(chain.removed)),
        "{second}"
    );
    let step = json!({"bundle": digest, "step": 1});
    let checked = started(&kitted, "source-fidelity", &second["values"], Some(&step));
    let said = kitted
        .api("GET", &format!("/api/runs/{checked}/record"), None)
        .json();
    assert_eq!(said["outcome"], "completed", "{said}");
}

#[test]
fn a_link_that_finds_no_output_or_two_stops_the_chain() {
    let kitted = Kitted::new("chain-stops");
    let chain = chain_in(&kitted, 71);
    let digest = imported(&kitted, &bundled(&kitted, &[&chain.removal, &chain.check]));
    // Step 1's new run found nothing and made no picture: the link finds no output there.
    let first = opened(&kitted, &digest, 0, &[]);
    kitted.kit().set("nothing").unwrap();
    let step = json!({"bundle": digest, "step": 0});
    let nothing = started(&kitted, "remove_prompt", &first["values"], Some(&step));
    kitted.kit().set("reset").unwrap();
    let stopped = opened(&kitted, &digest, 1, &[Some(&nothing)]);
    let why = stopped["stopped"]["why"].as_str().unwrap();
    assert!(why.contains("made no output there"), "{stopped}");
    assert_eq!(stopped["stopped"]["run"], nothing.as_str(), "{stopped}");
    assert!(stopped.get("values").is_none(), "{stopped}");
    // A run that is not this chain's step, named for it, is refused: the host does not
    // take the page's word for which run a step made.
    let crafted = ran(&kitted, "shout", &json!({"words": "not a step"}));
    for run in [crafted.as_str(), chain.removal.as_str()] {
        let answer = kitted.api(
            "POST",
            &format!("/api/bundles/{digest}/steps/1/open"),
            Some(&json!({"runs": [run]})),
        );
        assert_eq!(answer.status, 422, "{}", answer.text());
        assert!(answer.text().contains("is not step 1"), "{}", answer.text());
    }
    // Two outputs that could be the one the step recorded: the chain cannot tell which.
    let two = whizbang::play_bundles::bind(0, 1, &["sha256:a".to_owned(), "sha256:b".to_owned()]);
    assert!(two.unwrap_err().contains("cannot tell which"));
    // One where one was recorded binds; a second place binds the second.
    assert_eq!(
        whizbang::play_bundles::bind(0, 1, &["sha256:a".to_owned()]).unwrap(),
        "sha256:a"
    );
    assert_eq!(
        whizbang::play_bundles::bind(1, 2, &["sha256:a".to_owned(), "sha256:b".to_owned()])
            .unwrap(),
        "sha256:b"
    );
    assert!(whizbang::play_bundles::bind(2, 2, &["sha256:a".to_owned()]).is_err());
}

#[test]
fn a_run_started_from_a_step_records_the_bundle_and_the_step() {
    let kitted = Kitted::new("chain-records");
    let chain = chain_in(&kitted, 72);
    let digest = imported(&kitted, &bundled(&kitted, &[&chain.removal, &chain.check]));
    let first = opened(&kitted, &digest, 0, &[]);
    let step = json!({"bundle": digest, "step": 0});
    let made = started(&kitted, "remove_prompt", &first["values"], Some(&step));
    let record = kitted
        .api("GET", &format!("/api/runs/{made}/record"), None)
        .json();
    assert_eq!(
        record["replayed"]["run"],
        chain.removal.as_str(),
        "{record}"
    );
    assert_eq!(
        record["replayed"]["bundle"]["digest"],
        digest.as_str(),
        "{record}"
    );
    assert_eq!(record["replayed"]["bundle"]["step"], 0, "{record}");
    assert_eq!(record["replayed"]["mode"], "live", "{record}");
    // Its family is a replay's: history and the figures leave it out unless asked.
    assert!(!listed(&kitted, "").contains(&made));
    assert!(listed(&kitted, "include").contains(&made));
    assert_eq!(listed(&kitted, "only"), vec![made.clone()]);
    let figures = kitted.api("GET", "/api/stats?replayed=only", None);
    assert_eq!(figures.status, 200, "{}", figures.text());
    assert_eq!(
        kitted.api("GET", "/api/runs?replayed=some", None).status,
        400
    );
    // The same start under its id again is answered with its run; a step of a bundle
    // the store does not hold, or past the bundle's last, is refused and makes no run.
    let prepared = kitted.prepare("remove_prompt", &first["values"]);
    let id = fresh_id();
    let body = json!({"action": "remove_prompt", "arguments": first["values"],
                      "handle": prepared["handle"], "request_id": id, "replay": step});
    let once = kitted.api("POST", "/api/runs", Some(&body));
    assert_eq!(once.status, 201, "{}", once.text());
    let again = kitted.api("POST", "/api/runs", Some(&body));
    assert_eq!(again.json()["run"], once.json()["run"], "{}", again.text());
    kitted.until_ended(once.json()["run"].as_str().unwrap());
    // The same id under another call, with the step's mark: a conflict, not the step's
    // answer; and the same request after its bundle was deleted is answered with its run.
    let mut other = first["values"].clone();
    other["prompt"] = json!("the kettle");
    let prepared = kitted.prepare("remove_prompt", &other);
    let conflict = json!({"action": "remove_prompt", "arguments": other,
                          "handle": prepared["handle"], "request_id": id, "replay": step});
    assert_eq!(kitted.api("POST", "/api/runs", Some(&conflict)).status, 409);
    // A call of another action under a step's mark is not that step made again.
    let shout = kitted.prepare("shout", &json!({"words": "hi"}));
    let wrong = json!({"action": "shout", "arguments": {"words": "hi"},
                       "handle": shout["handle"], "request_id": fresh_id(), "replay": step});
    let refused = kitted.api("POST", "/api/runs", Some(&wrong));
    assert_eq!(refused.status, 422, "{}", refused.text());
    assert!(
        refused.text().contains("is a run of remove_prompt"),
        "{}",
        refused.text()
    );
    // A replay's and a step's outputs are offered to the binding picker all the same.
    let picker = kitted
        .api("GET", "/api/runs?output=image&limit=200", None)
        .json();
    assert!(picker.to_string().contains(&made), "{picker}");
    let elsewhere = format!("sha256:{}", "0".repeat(64));
    for replay in [
        json!({"bundle": elsewhere, "step": 0}),
        json!({"bundle": digest, "step": 9}),
    ] {
        let prepared = kitted.prepare("remove_prompt", &first["values"]);
        let body = json!({"action": "remove_prompt", "arguments": first["values"],
                          "handle": prepared["handle"], "request_id": fresh_id(), "replay": replay});
        let refused = kitted.api("POST", "/api/runs", Some(&body));
        assert_eq!(refused.status, 422, "{}", refused.text());
        assert!(
            refused.text().contains("was not replayed"),
            "{}",
            refused.text()
        );
    }
}

#[test]
fn a_step_asked_again_after_its_bundle_went_is_answered_with_its_run() {
    let kitted = Kitted::new("chain-repeat");
    let chain = chain_in(&kitted, 73);
    let digest = imported(&kitted, &bundled(&kitted, &[&chain.removal]));
    let first = opened(&kitted, &digest, 0, &[]);
    let prepared = kitted.prepare("remove_prompt", &first["values"]);
    let body = json!({"action": "remove_prompt", "arguments": first["values"],
                      "handle": prepared["handle"], "request_id": fresh_id(),
                      "replay": {"bundle": digest, "step": 0}});
    let once = kitted.api("POST", "/api/runs", Some(&body));
    assert_eq!(once.status, 201, "{}", once.text());
    let run = once.json()["run"].as_str().unwrap().to_owned();
    kitted.until_ended(&run);
    let deleted = kitted.api("DELETE", &format!("/api/bundles/{digest}"), None);
    assert_eq!(deleted.status, 200, "{}", deleted.text());
    // The host restarts, so the store's record of the id answers, not the host's memory.
    let kitted = kitted.restarted_after(|_| ());
    let again = kitted.api("POST", "/api/runs", Some(&body));
    assert_eq!(again.json()["run"], run.as_str(), "{}", again.text());
}
