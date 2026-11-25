//! `play_compare::…`: saved-run comparisons (plan m7, task U1, item 1): two ended root
//! runs of one task, side by side, from their families, through the host's
//! `GET /api/compare/{baseline}/{attempt}`. The host runs in process over the binary's
//! wiring and the test kit's `playkit` backends (`play::Kitted`).

use serde_json::{Value as Json, json};

use crate::play::{Kitted, fresh_id};

/// Runs a call through the host's prepare and start, and answers its run once it ended.
fn ran(kitted: &Kitted, action: &str, arguments: &Json) -> String {
    let prepared = kitted.prepare(action, arguments);
    assert_eq!(prepared["ready"], true, "{prepared}");
    let started = kitted.begin(action, arguments, &prepared["handle"], &fresh_id());
    assert_eq!(started.status, 201, "{}", started.text());
    let run = started.json()["run"].as_str().unwrap().to_owned();
    kitted.until_ended(&run);
    run
}

fn compared(kitted: &Kitted, baseline: &str, attempt: &str) -> Json {
    let answer = kitted.api("GET", &format!("/api/compare/{baseline}/{attempt}"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    answer.json()
}

/// The row of a section by its name.
fn row<'a>(said: &'a Json, section: &str, name: &str) -> &'a Json {
    said["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["section"] == section && row["name"] == name)
        .unwrap_or_else(|| panic!("no {section} row {name}: {said}"))
}

fn dollars(said: &Json) -> f64 {
    said.as_f64()
        .unwrap_or_else(|| panic!("not dollars: {said}"))
}

#[test]
fn two_runs_of_one_task_compare_by_settings_inputs_outputs_time_and_cost() {
    let kitted = Kitted::new("compare-one-task");
    let picture = kitted.picture(60);
    let baseline = ran(
        &kitted,
        "remove_prompt",
        &json!({"image": picture, "prompt": "the hose"}),
    );
    let attempt = ran(
        &kitted,
        "remove_prompt",
        &json!({"image": picture, "prompt": "the garden hose"}),
    );
    let said = compared(&kitted, &baseline, &attempt);
    assert_eq!(said["baseline"]["run"], baseline.as_str(), "{said}");
    assert_eq!(said["attempt"]["run"], attempt.as_str(), "{said}");
    // What was asked, side by side, the difference marked.
    let prompt = row(&said, "asked", "prompt");
    assert_eq!(prompt["baseline"], "the hose", "{prompt}");
    assert_eq!(prompt["attempt"], "the garden hose", "{prompt}");
    assert_eq!(prompt["differs"], true, "{prompt}");
    // What each resolved: the same segmenter and filler, not marked.
    let resolved: Vec<&Json> = (said["rows"].as_array().unwrap().iter())
        .filter(|row| row["section"] == "resolved")
        .collect();
    assert!(!resolved.is_empty(), "{said}");
    assert!(resolved.iter().all(|row| row["differs"] == false), "{said}");
    // The same picture in, by identity, and an output of each by port.
    let image = row(&said, "inputs", "image");
    assert_eq!(image["baseline"], picture.as_str(), "{image}");
    assert_eq!(image["differs"], false, "{image}");
    let outputs: Vec<&Json> = (said["rows"].as_array().unwrap().iter())
        .filter(|row| row["section"] == "outputs")
        .collect();
    assert!(!outputs.is_empty(), "{said}");
    for side in ["baseline", "attempt"] {
        let one = &said[side];
        assert_eq!(one["outcome"], "completed", "{said}");
        assert!(one["time_ms"].as_u64().is_some(), "{said}");
        assert!(one["cost"]["total"].as_f64().is_some(), "{said}");
        assert_eq!(one["whole"], true, "{said}");
        // A family: the recipe and the runs it called.
        assert!(one["runs"].as_u64().unwrap() > 1, "{said}");
    }
    // No winner and no score.
    let text = said.to_string();
    for word in ["winner", "score", "better"] {
        assert!(!text.contains(word), "{word}: {said}");
    }
}

#[test]
fn two_paid_loops_compare_cost_and_models_over_their_families_with_unresolved_spend_said() {
    let kitted = Kitted::new("compare-two-loops");
    let arguments = json!({"image": kitted.picture(61), "prompt": "the hose"});
    // The baseline is stopped while its fill is out: that request is unresolved, booked
    // at its bound.
    let prepared = kitted.prepare("remove_checked", &arguments);
    assert_eq!(prepared["ready"], true, "{prepared}");
    let before = kitted.kit().requests();
    kitted.kit().set("hold").unwrap();
    let started = kitted.begin(
        "remove_checked",
        &arguments,
        &prepared["handle"],
        &fresh_id(),
    );
    assert_eq!(started.status, 201, "{}", started.text());
    let baseline = started.json()["run"].as_str().unwrap().to_owned();
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while kitted.kit().requests() < before + 1 {
        assert!(
            std::time::Instant::now() < limit,
            "the segment was never asked"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    kitted.kit().set("release,hold").unwrap();
    while kitted.kit().requests() < before + 2 {
        assert!(std::time::Instant::now() < limit, "the fill was never sent");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let stopped = kitted.api("POST", &format!("/api/runs/{baseline}/cancel"), None);
    assert_eq!(stopped.status, 202, "{}", stopped.text());
    kitted.until_ended(&baseline);
    kitted.kit().set("reset").unwrap();
    // The attempt runs whole: a fill and a check, paid, in runs the loop called.
    let attempt = ran(&kitted, "remove_checked", &arguments);
    let said = compared(&kitted, &baseline, &attempt);
    let fill = whizbang_testkit::playkit::FILL_MICROS as f64 / 1e6;
    let judge = whizbang_testkit::playkit::JUDGE_MICROS as f64 / 1e6;
    let base = &said["baseline"];
    assert_eq!(base["outcome"], "cancelled", "{said}");
    assert!(
        (dollars(&base["cost"]["unresolved"]) - fill).abs() < 1e-9,
        "{said}"
    );
    let other = &said["attempt"];
    assert!(
        dollars(&other["cost"]["total"]) >= fill + judge - 1e-9,
        "{said}"
    );
    assert!(dollars(&other["cost"]["unresolved"]).abs() < 1e-9, "{said}");
    // Its models over the family: the loop's own record names none of them.
    let models = other["models"].as_array().unwrap();
    for model in ["fill", "judge"] {
        assert!(models.iter().any(|one| one == model), "{model}: {said}");
    }
    // The family's cost is the record route's, which reads the same family.
    let record = kitted.api("GET", &format!("/api/runs/{attempt}/record"), None);
    assert_eq!(record.status, 200, "{}", record.text());
    assert_eq!(other["cost"], record.json()["cost"], "{said}");
}

#[test]
fn two_tasks_are_refused_422_and_a_run_not_ended_409() {
    let kitted = Kitted::new("compare-refused");
    let picture = kitted.picture(62);
    let removal = ran(
        &kitted,
        "remove_prompt",
        &json!({"image": picture, "prompt": "the hose"}),
    );
    let shout = ran(&kitted, "shout", &json!({"words": "hello"}));
    let two = kitted.api("GET", &format!("/api/compare/{removal}/{shout}"), None);
    assert_eq!(two.status, 422, "{}", two.text());
    assert!(
        two.text().contains("two runs of one task"),
        "{}",
        two.text()
    );
    // A run a recipe called is no run a person compares.
    let generation = kitted.generation();
    let child = (generation.runs().keys())
        .find(|id| {
            let record = whizbang_core::contract::record::stored(&generation, id, |entry| {
                whizbang_core::store::Reader::open(
                    &kitted.root(),
                    std::sync::Arc::new(whizbang_core::store::fs::RealFs),
                )
                .unwrap()
                .read(entry)
            });
            matches!(record, Some(Ok(record)) if record.identity().parent.as_deref() == Some(removal.as_str()))
        })
        .cloned()
        .unwrap();
    let nested = kitted.api("GET", &format!("/api/compare/{removal}/{child}"), None);
    assert_eq!(nested.status, 422, "{}", nested.text());
    // A run still going has no record yet.
    kitted.kit().set("hold").unwrap();
    let arguments = json!({"image": picture, "prompt": "the reel"});
    let prepared = kitted.prepare("remove_prompt", &arguments);
    let started = kitted.begin(
        "remove_prompt",
        &arguments,
        &prepared["handle"],
        &fresh_id(),
    );
    assert_eq!(started.status, 201, "{}", started.text());
    let going = started.json()["run"].as_str().unwrap().to_owned();
    let early = kitted.api("GET", &format!("/api/compare/{removal}/{going}"), None);
    assert_eq!(early.status, 409, "{}", early.text());
    kitted.kit().set("release").unwrap();
    kitted.until_ended(&going);
    // A run the store does not hold.
    let none = kitted.api("GET", &format!("/api/compare/{removal}/nothing-here"), None);
    assert_eq!(none.status, 404, "{}", none.text());
}
