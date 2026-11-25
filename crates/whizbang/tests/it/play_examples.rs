//! `play_examples::…`: saved examples (plan m7, task U1, item 2): a run's task, inputs and
//! settings kept as `examples/<id>` (ADR 0048), listed, renamed, deleted, and opened as a
//! draft that starts nothing, through the host's routes. The host runs in process over the
//! binary's wiring and the test kit's `playkit` backends (`play::Kitted`).

use std::sync::Arc;

use serde_json::{Value as Json, json};
use whizbang::wiring::{self, Make};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;

use crate::play::{Kitted, fresh_id};

fn ran(kitted: &Kitted, action: &str, arguments: &Json) -> String {
    let prepared = kitted.prepare(action, arguments);
    assert_eq!(prepared["ready"], true, "{prepared}");
    let started = kitted.begin(action, arguments, &prepared["handle"], &fresh_id());
    assert_eq!(started.status, 201, "{}", started.text());
    let run = started.json()["run"].as_str().unwrap().to_owned();
    kitted.until_ended(&run);
    run
}

fn saved(kitted: &Kitted, run: &str, name: &str, notes: &str) -> Json {
    let body = json!({"run": run, "name": name, "notes": notes});
    let answer = kitted.api("POST", "/api/examples", Some(&body));
    assert_eq!(answer.status, 201, "{}", answer.text());
    answer.json()
}

fn opened(kitted: &Kitted, id: &str) -> Json {
    let answer = kitted.api("POST", &format!("/api/examples/{id}/open"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    answer.json()
}

/// The image a run made, by its identity.
fn made_image(kitted: &Kitted, run: &str) -> String {
    let record = kitted.api("GET", &format!("/api/runs/{run}/record"), None);
    assert_eq!(record.status, 200, "{}", record.text());
    let outputs = record.json()["outputs"].as_array().unwrap().clone();
    let image = outputs.iter().find(|output| output["type"] == "image");
    image.unwrap()["id"].as_str().unwrap().to_owned()
}

#[test]
fn an_example_keeps_its_inputs_after_its_run_is_deleted_and_pruned() {
    let kitted = Kitted::new("examples-keep");
    // The input is a picture another run made, which only runs hold: no import marks it.
    let first = ran(
        &kitted,
        "remove_prompt",
        &json!({"image": kitted.picture(70), "prompt": "the hose"}),
    );
    let made = made_image(&kitted, &first);
    let second = ran(
        &kitted,
        "remove_prompt",
        &json!({"image": made, "prompt": "the reel"}),
    );
    let example = saved(&kitted, &second, "Porch hose, then the reel", "");
    let id = example["id"].as_str().unwrap().to_owned();
    assert_eq!(example["picture"], made.as_str(), "{example}");
    // Both runs deleted, and what they left pruned, while no host holds the store.
    let kitted = kitted.restarted_after(|root| {
        let opening = wiring::opening(root, Make::Nothing, wiring::recovery()).unwrap();
        let remover = opening.remover().unwrap();
        remover
            .delete_with_descendants(&[first.as_str(), second.as_str()])
            .unwrap();
        remover.prune().unwrap();
    });
    let generation = kitted.generation();
    let named = |id: &str| {
        let hex = id.strip_prefix("sha256:").unwrap();
        generation.get(&format!("artifacts/image/{hex}")).is_some()
    };
    assert!(!named(&made), "the made picture's name went with its runs");
    // The example is listed still, and opens with its picture given back.
    let listed = kitted.api("GET", "/api/examples", None).json();
    assert_eq!(listed["examples"][0]["id"], id.as_str(), "{listed}");
    let draft = opened(&kitted, &id);
    assert_eq!(draft["values"]["image"], made.as_str(), "{draft}");
    assert_eq!(draft["values"]["prompt"], "the reel", "{draft}");
    let thumb = kitted.api("GET", &format!("/api/artifacts/{made}/thumb?edge=32"), None);
    assert_eq!(thumb.status, 200, "{}", thumb.text());
    // And a run takes it.
    ran(
        &kitted,
        "remove_prompt",
        &json!({"image": made, "prompt": "the reel"}),
    );
}

#[test]
fn opening_an_example_starts_no_run() {
    let kitted = Kitted::new("examples-open");
    let arguments = json!({"image": kitted.picture(71), "prompt": "the hose",
                           "backend": "default"});
    let run = ran(&kitted, "remove_prompt", &arguments);
    let example = saved(&kitted, &run, "Porch hose", "the first try");
    let id = example["id"].as_str().unwrap();
    let runs = kitted.generation().runs().len();
    let draft = opened(&kitted, id);
    assert_eq!(draft["task"], "remove_prompt", "{draft}");
    assert_eq!(draft["example"]["name"], "Porch hose", "{draft}");
    assert_eq!(draft["values"]["prompt"], "the hose", "{draft}");
    assert_eq!(draft["values"]["image"], arguments["image"], "{draft}");
    // Its choice of segmenter is offered here, so it stands, and nothing fell back.
    assert_eq!(draft["values"]["backend"], "default", "{draft}");
    assert_eq!(draft["fell_back"], json!([]), "{draft}");
    assert_eq!(kitted.generation().runs().len(), runs, "{draft}");
    assert_eq!(
        kitted.api("GET", "/api/runs?open=1", None).json()["runs"],
        json!([])
    );
    // A loop's example, whose judge this config offers and has ready, opens with nothing
    // fallen back, and starts nothing either.
    let check = json!({"image": kitted.picture(72), "prompt": "the reel",
                       "judge": "cloud-judge"});
    let checked = ran(&kitted, "remove_checked", &check);
    let saved_check = saved(&kitted, &checked, "Reel, checked", "");
    let generation_before = kitted.generation().runs().len();
    let draft = opened(&kitted, saved_check["id"].as_str().unwrap());
    assert_eq!(draft["fell_back"], json!([]), "{draft}");
    assert_eq!(kitted.generation().runs().len(), generation_before);
}

#[test]
fn an_example_whose_backend_is_not_offered_now_names_it_and_falls_back() {
    let kitted = Kitted::new("examples-fell-back");
    let picture = kitted.picture(73);
    let judged = ran(
        &kitted,
        "judged",
        &json!({"image": picture, "judge": "cloud-judge"}),
    );
    let removal = ran(
        &kitted,
        "remove_prompt",
        &json!({"image": picture, "prompt": "the hose", "backend": "default"}),
    );
    let judged = saved(&kitted, &judged, "Judged", "");
    let removal = saved(&kitted, &removal, "Removal", "");
    // The config changes: the judge chosen now needs a key no env file sets, and the
    // segmenter chosen is named otherwise.
    let kitted = kitted.restarted_after(|root| {
        let file = root.parent().unwrap().join("whizbang.toml");
        let text = std::fs::read_to_string(&file).unwrap();
        let text = text
            .replace(
                "[backends.judge.cloud-judge]\n",
                "[backends.judge.cloud-judge]\nkey_env = \"PLAYKIT_GONE_KEY\"\n",
            )
            .replace(
                "[backends.segment]\n",
                "[backends.segment]\ndefault = \"local-seg\"\n\n[backends.segment.local-seg]\n",
            );
        std::fs::write(&file, text).unwrap();
    });
    let draft = opened(&kitted, judged["id"].as_str().unwrap());
    let fell = draft["fell_back"].as_array().unwrap();
    assert_eq!(fell.len(), 1, "{draft}");
    assert_eq!(fell[0]["argument"], "judge", "{draft}");
    assert_eq!(fell[0]["value"], "cloud-judge", "{draft}");
    assert_eq!(fell[0]["kind"], "no_key", "{draft}");
    assert!(draft["values"].get("judge").is_none(), "{draft}");
    let draft = opened(&kitted, removal["id"].as_str().unwrap());
    let fell = draft["fell_back"].as_array().unwrap();
    assert_eq!(fell.len(), 1, "{draft}");
    assert_eq!(fell[0]["value"], "default", "{draft}");
    assert_eq!(fell[0]["kind"], "not_in_config", "{draft}");
    assert!(draft["values"].get("backend").is_none(), "{draft}");
}

#[test]
fn a_name_or_notes_holding_markup_are_shown_as_text() {
    let kitted = Kitted::new("examples-markup");
    let run = ran(&kitted, "shout", &json!({"words": "hi"}));
    let name = "<img src=x onerror=alert(1)>\u{1b}[2J";
    let notes = "<script>alert(2)</script>";
    let example = saved(&kitted, &run, name, notes);
    let listed = kitted.api("GET", "/api/examples", None);
    // JSON, as text: the markup as it was given, a control character escaped, and no
    // document to run it.
    assert!(listed.text().starts_with('{'), "{}", listed.text());
    let one = &listed.json()["examples"][0];
    assert_eq!(one["id"], example["id"]);
    assert_eq!(
        one["name"], "<img src=x onerror=alert(1)>\\u{1b}[2J",
        "{one}"
    );
    assert_eq!(one["notes"], notes, "{one}");
    // A rename is held to the same rules, and kept.
    let id = one["id"].as_str().unwrap();
    let long = "x".repeat(81);
    let too_long = kitted.api(
        "PATCH",
        &format!("/api/examples/{id}"),
        Some(&json!({"name": long})),
    );
    assert_eq!(too_long.status, 400, "{}", too_long.text());
    let renamed = kitted.api(
        "PATCH",
        &format!("/api/examples/{id}"),
        Some(&json!({"name": "<b>bold</b>"})),
    );
    assert_eq!(renamed.status, 200, "{}", renamed.text());
    assert_eq!(renamed.json()["name"], "<b>bold</b>");
    assert_eq!(renamed.json()["notes"], notes);
    // Deleted, it is gone, and its run stays.
    let deleted = kitted.api("DELETE", &format!("/api/examples/{id}"), None);
    assert_eq!(deleted.status, 200, "{}", deleted.text());
    let again = kitted.api("DELETE", &format!("/api/examples/{id}"), None);
    assert_eq!(again.status, 404, "{}", again.text());
    let record = kitted.api("GET", &format!("/api/runs/{run}/record"), None);
    assert_eq!(record.status, 200, "{}", record.text());
}

#[test]
fn past_1000_examples_a_new_one_is_refused_naming_the_bound() {
    let kitted = Kitted::new("examples-bound");
    let run = ran(&kitted, "shout", &json!({"words": "hi"}));
    // A thousand examples' names, put while no host holds the store.
    let kitted = kitted.restarted_after(|root| {
        let store = Store::open(root, Arc::new(RealFs)).unwrap();
        let mut publication = store.begin().unwrap();
        for at in 0..1000u32 {
            publication
                .put(&format!("examples/{at:016x}"), b"{}")
                .unwrap();
        }
        publication.publish().unwrap();
    });
    let body = json!({"run": run, "name": "one too many"});
    let refused = kitted.api("POST", "/api/examples", Some(&body));
    assert_eq!(refused.status, 409, "{}", refused.text());
    assert!(refused.text().contains("(1000)"), "{}", refused.text());
}

#[test]
fn store_list_names_an_example_and_store_delete_example_lets_it_go() {
    let kitted = Kitted::new("examples-commands");
    let run = ran(&kitted, "shout", &json!({"words": "hi"}));
    let example = saved(&kitted, &run, "Shout \"once\"", "");
    let id = example["id"].as_str().unwrap().to_owned();
    let store = |root: &std::path::Path, words: &[&str]| {
        let ran = crate::store::whizbang(words, root);
        (ran.code == Some(0), ran.out, ran.err)
    };
    let kitted = kitted.restarted_after(|root| {
        let (ok, out, err) = store(root, &["list"]);
        assert!(ok, "{err}");
        assert!(out.contains("examples: 1\n"), "{out}");
        assert!(
            out.contains(&format!("example {id}\tshout\tShout \\\"once\\\"\n")),
            "{out}"
        );
        let (ok, _, err) = store(root, &["delete", "--example", "0123"]);
        assert!(!ok);
        assert!(err.contains("16 hex digits"), "{err}");
        let (ok, out, err) = store(root, &["delete", "--example", &id]);
        assert!(ok, "{err}");
        assert!(out.contains(&format!("example deleted: {id}")), "{out}");
        let (_, out, _) = store(root, &["list"]);
        assert!(!out.contains("example"), "{out}");
    });
    // The run it was saved from stays.
    let record = kitted.api("GET", &format!("/api/runs/{run}/record"), None);
    assert_eq!(record.status, 200, "{}", record.text());
}
