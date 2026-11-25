//! `play_result::…`: the result's routes (plan m5, task W2): a run's record as the result
//! screen reads it, where its outputs can be sent, and its command line quoted for a
//! shell. The host runs in process over the binary's wiring and the test kit's `playkit`
//! backends (`play::Kitted`); the copied command of a run that needs no model is run by
//! the shipped binary itself.

use std::path::Path;
use std::sync::Arc;

use serde_json::{Value as Json, json};
use whizbang::play_result::{command_line, quoted, said_safely};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;

use crate::play::{Kitted, fresh_id, play, png, request, send};

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

fn record(kitted: &Kitted, run: &str) -> Json {
    let answer = kitted.api("GET", &format!("/api/runs/{run}/record"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    answer.json()
}

#[test]
fn the_record_route_splits_cost_by_capability_and_backend_and_names_free_local_work() {
    let kitted = Kitted::new("result-cost");
    let arguments =
        json!({"image": kitted.picture(30), "prompt": "the <img src=x onerror=alert(1)>"});
    let run = ran(&kitted, "remove_checked", &arguments);
    let said = record(&kitted, &run);
    assert_eq!(said["outcome"], "completed", "{said}");
    assert_eq!(said["verdict"]["word"], "pass", "{said}");
    // The segmenter is free local work; the filler and the judge are paid, each its own
    // line, by its capability and the config's name of its backend.
    let lines = said["cost"]["lines"].as_array().unwrap();
    let line = |capability: &str| {
        lines
            .iter()
            .find(|line| line["capability"] == capability)
            .unwrap_or_else(|| panic!("no {capability} line: {said}"))
    };
    assert_eq!(line("segment")["free"], true);
    assert_eq!(line("segment")["cost"], 0.0);
    assert_eq!(line("fill")["free"], false);
    assert_eq!(line("judge")["free"], false);
    let fill = whizbang_testkit::playkit::FILL_MICROS as f64 / 1e6;
    let judge = whizbang_testkit::playkit::JUDGE_MICROS as f64 / 1e6;
    assert!(
        (line("fill")["cost"].as_f64().unwrap() - fill).abs() < 1e-9,
        "{said}"
    );
    assert!(
        (line("judge")["cost"].as_f64().unwrap() - judge).abs() < 1e-9,
        "{said}"
    );
    assert!((said["cost"]["total"].as_f64().unwrap() - fill - judge).abs() < 1e-9);
    // The loop's attempt, with its outcome and its picture, marked a diagnostic.
    let attempt = &said["attempts"]["list"][0];
    assert_eq!(attempt["verdict"], "pass");
    assert_eq!(attempt["diagnostic"], true);
    assert!(attempt["picture"].as_str().unwrap().starts_with("sha256:"));
    // The words given are said as text: what the page shows is never markup it parses.
    assert_eq!(
        said["arguments"][0]["value"],
        "the <img src=x onerror=alert(1)>"
    );
    // A run that has not ended has no record yet; no such run is 404.
    assert_eq!(kitted.api("GET", "/api/runs/1-1/record", None).status, 404);
}

#[test]
fn send_to_offers_each_task_whose_ports_a_results_outputs_fill() {
    let kitted = Kitted::new("result-send");
    let picture = kitted.picture(31);
    let run = ran(
        &kitted,
        "remove_prompt",
        &json!({"image": picture, "prompt": "the cup"}),
    );
    let said = record(&kitted, &run);
    let targets = said["send_to"].as_array().unwrap();
    let target = |task: &str| targets.iter().find(|t| t["task"] == task).cloned();
    let outputs = said["outputs"].as_array().unwrap();
    let output = |port: &str| outputs.iter().find(|o| o["port"] == port).unwrap()["id"].clone();
    // By its roles: check an edit, the input as the source, the result as the candidate,
    // and the job the loop writes from the phrase, read from the store by the run's id.
    let check = target("source-fidelity").expect("check an edit is offered");
    assert_eq!(check["kit"], "squint");
    assert_eq!(check["values"]["source"], picture);
    assert_eq!(check["values"]["candidate"], output("image"));
    assert_eq!(check["job_of"], run);
    assert!(
        check["values"]["brief"]
            .as_str()
            .unwrap()
            .contains("the cup")
    );
    // By type: the one port of protect's of each type is filled.
    let protect = target("protect").expect("protect is offered");
    assert_eq!(protect["values"]["image"], output("image"));
    assert_eq!(protect["values"]["mask"], output("mask"));
    // Never itself, and never a tool.
    assert!(target("remove_prompt").is_none());
    assert!(target("rect").is_none());
    // Run again: the same task, its picture and words, and each backend written out.
    let again = &said["again"];
    assert_eq!(again["task"], "remove_prompt");
    assert_eq!(again["values"]["image"], picture);
    assert_eq!(again["values"]["prompt"], "the cup");
    // The kit's config names its one filler by the name a table of one takes.
    assert_eq!(again["values"]["filler"], "default");
    // The check that takes the job runs, and its record keeps the loop's requirements.
    let mut values = check["values"].as_object().unwrap().clone();
    values.remove("brief");
    values.insert("job_of".to_owned(), json!(run));
    let checked = ran(&kitted, "source-fidelity", &Json::Object(values));
    let said = record(&kitted, &checked);
    let job = said["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "job")
        .unwrap();
    assert!(job["value"].as_str().unwrap().contains("\"RQ1\""), "{job}");
}

#[test]
fn send_to_leaves_a_port_empty_when_two_ports_take_the_same_type() {
    let kitted = Kitted::new("result-two-ports");
    let picture = kitted.picture(32);
    let run = ran(
        &kitted,
        "remove_prompt",
        &json!({"image": picture, "prompt": "a cup"}),
    );
    let said = record(&kitted, &run);
    // Check against references takes a picture as its candidate and as its references:
    // the draft opens with neither filled, and the person chooses.
    let target = said["send_to"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["task"] == "reference-consistency")
        .cloned()
        .expect("offered");
    assert_eq!(target["values"], json!({}), "{target}");
    assert_eq!(target["from"], json!([]));
}

#[test]
fn the_command_quotes_every_argument_for_a_posix_shell() {
    let words: Vec<String> = [
        "whizbang",
        "plain-word_1.2:3",
        "it's",
        "$HOME and ${PATH}",
        "`id` and $(id)",
        "two\nlines",
        "a \"quoted\" word; | & < > * ? [x] ~ # !",
        "",
    ]
    .map(str::to_owned)
    .to_vec();
    assert_eq!(quoted("plain-word_1.2:3"), "plain-word_1.2:3");
    assert_eq!(quoted("it's"), r"'it'\''s'");
    // A word that begins with `=`, which zsh would expand as a command's path.
    assert_eq!(quoted("=ls"), "'=ls'");
    assert_eq!(quoted("--box=1"), "--box=1");
    // A POSIX shell reads each word back as it was: printf prints each on a line of its
    // own, closed by a NUL so a word's own new line is kept.
    let line = command_line(&words[1..]);
    let script = format!("printf '%s\\0' {line}");
    let out = std::process::Command::new("/bin/sh")
        .args(["-c", &script])
        .output()
        .unwrap();
    assert!(out.status.success());
    let read: Vec<String> = String::from_utf8(out.stdout)
        .unwrap()
        .split('\0')
        .map(str::to_owned)
        .collect();
    assert_eq!(read[..read.len() - 1], words[1..]);
}

/// The record of the run `run` in the store at `root`.
fn stored(root: &Path, run: &str) -> whizbang_core::contract::RunRecord {
    let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    whizbang_core::contract::record::stored(&generation, run, |e| reader.read(e))
        .unwrap()
        .unwrap()
}

#[test]
fn the_command_copied_from_a_mask_rect_run_makes_the_same_call_with_the_shipped_binary() {
    let scratch = Scratch::new("result-shipped");
    let root = scratch.path().join("store");
    let played = play(&root, &[]);
    let address = played.address();
    let origin = format!("http://127.0.0.1:{}", address.port());
    let post = |path: &str, body: &Json| {
        send(
            address,
            &request(
                address,
                &played.token,
                "POST",
                path,
                Some(&origin),
                Some(body),
            ),
        )
    };
    // A picture, sent as the page sends one.
    let bytes = png(40, 30, 9);
    let mut head = format!(
        "POST /api/pictures?name=box.png HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
         Authorization: Bearer {token}\r\nOrigin: {origin}\r\n\
         Content-Type: image/png\r\nContent-Length: {}\r\n\r\n",
        bytes.len(),
        port = address.port(),
        token = played.token,
    )
    .into_bytes();
    head.extend_from_slice(&bytes);
    let picture = send(address, &head).json()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let arguments = json!({"image": picture, "box": "3 4 20 25"});
    let prepared = post(
        "/api/prepare",
        &json!({"action": "mask_rect", "arguments": arguments}),
    );
    let handle = prepared.json()["handle"].clone();
    let started = post(
        "/api/runs",
        &json!({"action": "mask_rect", "arguments": arguments, "handle": handle,
                "request_id": "shipped-1"}),
    );
    assert_eq!(started.status, 201, "{}", started.text());
    let run = started.json()["run"].as_str().unwrap().to_owned();
    for _ in 0..2000 {
        if played
            .api("GET", &format!("/api/runs/{run}?after=0"))
            .json()["state"]
            == "ended"
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let command = played.api("GET", &format!("/api/runs/{run}/command"));
    assert_eq!(command.status, 200, "{}", command.text());
    let text = command.json()["command"].as_str().unwrap().to_owned();
    assert!(text.starts_with("whizbang rascal mask-rect "), "{text}");
    // The host stops first: the store has one writer. Then the shipped binary runs the
    // copied words (how a shell reads the quoted text is the quoting test's).
    let (code, _, err) = played.ended();
    assert_eq!(code, Some(0), "{err}");
    let words: Vec<String> = serde_json::from_value(command.json()["words"].clone()).unwrap();
    assert_eq!(command_line(&words), text);
    let words: Vec<&str> = words[1..].iter().map(String::as_str).collect();
    let out = crate::wired::whizbang_storeless(&words);
    assert_eq!(out.code, Some(0), "{}", out.err);
    let printed = &out.out;
    let again = printed
        .lines()
        .find_map(|line| {
            line.strip_prefix("run ")?
                .split_whitespace()
                .next()
                .map(str::to_owned)
        })
        .unwrap_or_else(|| panic!("no run printed: {printed}"));
    let (first, second) = (stored(&root, &run), stored(&root, &again));
    assert_ne!(first.identity().run, second.identity().run);
    assert_eq!(first.asked().action, second.asked().action);
    assert_eq!(first.asked().arguments, second.asked().arguments);
    assert_eq!(first.asked().settings, second.asked().settings);
    assert_eq!(first.asked().config, second.asked().config);
    assert_eq!(first.inputs(), second.inputs());
}

#[test]
fn words_holding_an_escape_sequence_a_nul_or_a_format_character_are_refused_422() {
    let said = |word: &str| said_safely(&["whizbang".to_owned(), format!("--prompt={word}")]);
    for word in ["the \u{1b}[2Jhose", "the\0hose", "the hose\u{202e}esoh"] {
        let refused = said(word).unwrap_err();
        assert_eq!(refused.status, 422, "{word:?}");
    }
    // A new line and a tab stay as they are inside the quotes.
    assert!(said("two\nlines\tand a tab").is_ok());
}

#[test]
fn the_record_of_a_run_that_has_not_ended_is_refused_409() {
    let kitted = Kitted::new("result-going");
    let prepared = kitted.prepare("waits", &json!({}));
    let started = kitted.begin("waits", &json!({}), &prepared["handle"], &fresh_id());
    assert_eq!(started.status, 201, "{}", started.text());
    let run = started.json()["run"].as_str().unwrap().to_owned();
    let answer = kitted.api("GET", &format!("/api/runs/{run}/record"), None);
    assert_eq!(answer.status, 409, "{}", answer.text());
    let stopped = kitted.api("POST", &format!("/api/runs/{run}/cancel"), None);
    assert_eq!(stopped.status, 202);
}

#[test]
fn the_record_of_a_run_stopped_while_its_fill_was_out_counts_that_request() {
    let kitted = Kitted::new("result-cancelled-cost");
    let arguments = json!({"image": kitted.picture(33), "prompt": "the hose"});
    let prepared = kitted.prepare("remove_prompt", &arguments);
    assert_eq!(prepared["ready"], true, "{prepared}");
    // The segmenter answers; the fill is held until the run is stopped.
    let before = kitted.kit().requests();
    kitted.kit().set("hold").unwrap();
    let started = kitted.begin(
        "remove_prompt",
        &arguments,
        &prepared["handle"],
        &fresh_id(),
    );
    assert_eq!(started.status, 201, "{}", started.text());
    let run = started.json()["run"].as_str().unwrap().to_owned();
    // The segmenter's request is let go once it has come, and only then: a release said
    // after the fill came would let the fill go too, and the run would end answered.
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
    let stopped = kitted.api("POST", &format!("/api/runs/{run}/cancel"), None);
    assert_eq!(stopped.status, 202, "{}", stopped.text());
    kitted.until_ended(&run);
    let said = record(&kitted, &run);
    assert_eq!(said["outcome"], "cancelled", "{said}");
    // The fill no answer settled is booked at its bound, and the result says so.
    let fill = whizbang_testkit::playkit::FILL_MICROS as f64 / 1e6;
    assert!(
        (said["cost"]["total"].as_f64().unwrap() - fill).abs() < 1e-9,
        "{said}"
    );
    assert!(
        (said["cost"]["unresolved"].as_f64().unwrap() - fill).abs() < 1e-9,
        "{said}"
    );
}

#[test]
fn an_unchecked_run_says_which_variable_its_judge_needs_and_never_the_core_words() {
    let kitted = Kitted::new("result-unchecked");
    let arguments = json!({"image": kitted.picture(34), "prompt": "the hose",
                           "judge": "keyless-judge"});
    let run = ran(&kitted, "remove_checked", &arguments);
    let said = record(&kitted, &run);
    assert_eq!(said["outcome"], "completed", "{said}");
    let text = said.to_string();
    assert!(!text.contains("key_env"), "{said}");
    let ended = said["attempts"]["ended"].as_str().unwrap_or_default();
    assert!(
        ended.contains("the judge needs PLAYKIT_TEST_KEY in your env file"),
        "{said}"
    );
}
