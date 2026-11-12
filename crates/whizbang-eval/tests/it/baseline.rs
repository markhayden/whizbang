//! The comparison the baseline gate uses, on synthetic transcripts
//! (docs/baseline/transcript.md). Each test changes one thing in an otherwise equal pair.

use serde_json::{Value, json};
use whizbang_eval::baseline::{
    COMPARISON_VERSION, Difference, Kind, Refusal, Side, VOLATILE, compare,
};

fn request(history: Value) -> Value {
    json!({
        "system": "You compare a candidate image with its source.",
        "history": history,
        "tools": [
            {"name": "view", "description": "Look closer.", "input_schema": {"type": "object"}},
            {"name": "submit_verdict", "description": "Answer.", "input_schema": {"type": "object"}}
        ],
        "max_output_tokens": 512
    })
}

fn first_message() -> Value {
    json!({"role": "user", "content": [
        {"type": "text", "text": "Compare these."},
        {"type": "image", "image_id": "source"},
        {"type": "image", "image_id": "candidate"}
    ]})
}

fn opaque(call: &str) -> Value {
    json!({"type": "opaque", "adapter": "synthetic", "block": {"call": call, "signature": format!("s-{call}")}})
}

/// Four turns: a response cut at the token limit that held a call (excluded), two evidence
/// calls in one response (both executed), an invalid submission (rejected), the final
/// submission.
fn transcript() -> Value {
    let h1 = json!([first_message()]);
    let assistant2 = json!({"role": "assistant", "content": [opaque("c1"), opaque("c2")]});
    let results2 = json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "c1", "content": "A closer look.", "is_error": false, "images": ["crop:call:0:0"]},
        {"type": "tool_result", "tool_use_id": "c2", "content": "Another look.", "is_error": false, "images": []}
    ]});
    let h3 = json!([first_message(), assistant2, results2]);
    let assistant3 = json!({"role": "assistant", "content": [opaque("c3")]});
    let results3 = json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "c3", "content": "That submission is not valid.", "is_error": true, "images": []}
    ]});
    let mut h4 = h3.as_array().unwrap().clone();
    h4.push(assistant3);
    h4.push(results3);
    json!({
        "transcript_version": 1,
        "origin": "historical",
        "complete": true,
        "incomplete": [],
        "subject": {"set": "set-1", "item": "synthetic-item", "model": "provider:model-x", "arm": "C", "repetition": 1, "use_case": "source-fidelity"},
        "preparation": {"version": "1", "settings": {"long_edge_px": 1024, "jpeg_quality": 90}},
        "evidence_tools": ["view"],
        "submission_tool": "submit_verdict",
        "images": [
            {"id": "source", "frame": "raw:source", "origin": "job", "prepared": {"sha256": "aa", "bytes": 10}},
            {"id": "candidate", "frame": "raw:candidate", "origin": "job", "prepared": {"sha256": "bb", "bytes": 11}},
            {"id": "crop:call:0:0", "frame": "crop:call:0:0", "origin": "c1", "prepared": {"sha256": "cc", "bytes": 5}}
        ],
        "turns": [
            {
                "turn": 1,
                "request": request(h1.clone()),
                "response": {"content": [opaque("c0")], "text": [], "tool_calls": [{"id": "c0", "name": "view", "arguments": {"image": "candidate"}}], "stop_reason": "max_tokens"},
                "excluded": {"reason": "truncated"},
                "calls": [{"id": "c0", "name": "view", "arguments": {"image": "candidate"}, "disposition": "excluded", "result": null, "meta": {}}],
                "meta": {"usage": {"input_tokens": 10, "output_tokens": 512}, "provider_request_id": "req-1", "cost_usd": 0.01}
            },
            {
                "turn": 2,
                "request": request(h1),
                "response": {"content": [opaque("c1"), opaque("c2")], "text": [], "tool_calls": [
                    {"id": "c1", "name": "view", "arguments": {"image": "candidate", "box": [0, 0, 4, 4]}},
                    {"id": "c2", "name": "view", "arguments": {"image": "source", "box": [0, 0, 4, 4]}}
                ], "stop_reason": "tool_use"},
                "excluded": null,
                "calls": [
                    {"id": "c1", "name": "view", "arguments": {"image": "candidate", "box": [0, 0, 4, 4]}, "disposition": "executed",
                     "result": {"content": "A closer look.", "is_error": false, "images": ["crop:call:0:0"]}, "meta": {"duration_ms": 3, "envelope_file": "dd"}},
                    {"id": "c2", "name": "view", "arguments": {"image": "source", "box": [0, 0, 4, 4]}, "disposition": "executed",
                     "result": {"content": "Another look.", "is_error": false, "images": []}, "meta": {"duration_ms": 4, "envelope_file": "ee"}}
                ],
                "meta": {"usage": {"input_tokens": 10, "output_tokens": 20}, "provider_request_id": "req-2", "cost_usd": 0.01}
            },
            {
                "turn": 3,
                "request": request(h3),
                "response": {"content": [opaque("c3")], "text": [], "tool_calls": [{"id": "c3", "name": "submit_verdict", "arguments": {"verdict": "maybe"}}], "stop_reason": "tool_use"},
                "excluded": null,
                "calls": [{"id": "c3", "name": "submit_verdict", "arguments": {"verdict": "maybe"}, "disposition": "rejected",
                           "result": {"content": "That submission is not valid.", "is_error": true, "images": []}, "meta": {"reason": "invalid_submission"}}],
                "meta": {"usage": {"input_tokens": 30, "output_tokens": 5}, "provider_request_id": "req-3", "cost_usd": 0.01}
            },
            {
                "turn": 4,
                "request": request(Value::Array(h4)),
                "response": {"content": [opaque("c4")], "text": [], "tool_calls": [{"id": "c4", "name": "submit_verdict", "arguments": {"verdict": "pass"}}], "stop_reason": "tool_use"},
                "excluded": null,
                "calls": [{"id": "c4", "name": "submit_verdict", "arguments": {"verdict": "pass"}, "disposition": "final", "result": null, "meta": {}}],
                "meta": {"usage": {"input_tokens": 40, "output_tokens": 5}, "provider_request_id": "req-4", "cost_usd": 0.01}
            }
        ],
        "repairs": [
            {"after_turn": 1, "form": "reissue", "reason": "truncated"},
            {"after_turn": 3, "form": "rejected", "reason": "invalid_submission"}
        ],
        "terminal": {"kind": "verdict", "verdict": "pass"},
        "volatile": VOLATILE,
        "files": [{"role": "journal", "path": "replay/a/journal.jsonl", "sha256": "ff", "bytes": 1, "truncated": false}],
        "meta": {"package_digest": "sha256:00", "run": "run-a"}
    })
}

/// Compares the transcript with a copy changed by `change`, and returns the differences.
fn after(change: impl FnOnce(&mut Value)) -> Vec<Difference> {
    let recorded = transcript();
    let mut replayed = transcript();
    change(&mut replayed);
    compare(&recorded, &replayed).unwrap()
}

fn paths(differences: &[Difference]) -> Vec<&str> {
    differences.iter().map(|d| d.path.as_str()).collect()
}

fn one(differences: Vec<Difference>, path: &str, kind: Kind) {
    assert_eq!(
        differences,
        vec![Difference {
            path: path.to_string(),
            kind
        }]
    );
}

#[test]
fn an_unchanged_transcript_matches() {
    assert_eq!(after(|_| {}), vec![]);
}

#[test]
fn the_comparison_has_a_version_for_the_gate_receipt() {
    assert_eq!(COMPARISON_VERSION, "2");
}

#[test]
fn a_changed_prompt_fails() {
    one(
        after(|t| t["turns"][1]["request"]["system"] = json!("You compare two images.")),
        "/turns/1/request/system",
        Kind::Changed,
    );
    one(
        after(|t| {
            t["turns"][3]["request"]["history"][0]["content"][0]["text"] = json!("Compare them.")
        }),
        "/turns/3/request/history/0/content/0/text",
        Kind::Changed,
    );
}

#[test]
fn a_changed_image_fails() {
    one(
        after(|t| t["images"][1]["prepared"]["sha256"] = json!("b0")),
        "/images/1/prepared/sha256",
        Kind::Changed,
    );
    one(
        after(|t| {
            t["turns"][0]["request"]["history"][0]["content"][2]["image_id"] = json!("source")
        }),
        "/turns/0/request/history/0/content/2/image_id",
        Kind::Changed,
    );
}

#[test]
fn a_changed_tool_definition_fails() {
    one(
        after(|t| t["turns"][2]["request"]["tools"][0]["description"] = json!("Look much closer.")),
        "/turns/2/request/tools/0/description",
        Kind::Changed,
    );
    one(
        after(|t| {
            t["turns"][2]["request"]["tools"][0]["input_schema"]["required"] = json!(["image"])
        }),
        "/turns/2/request/tools/0/input_schema/required",
        Kind::Extra,
    );
}

#[test]
fn a_changed_limit_fails() {
    one(
        after(|t| t["turns"][0]["request"]["max_output_tokens"] = json!(1024)),
        "/turns/0/request/max_output_tokens",
        Kind::Changed,
    );
}

#[test]
fn a_changed_opaque_block_fails() {
    one(
        after(|t| t["turns"][1]["response"]["content"][0]["block"]["signature"] = json!("forged")),
        "/turns/1/response/content/0/block/signature",
        Kind::Changed,
    );
    // The same block where it is echoed in a later request.
    one(
        after(|t| {
            t["turns"][2]["request"]["history"][1]["content"][1]["block"]["signature"] =
                json!("forged")
        }),
        "/turns/2/request/history/1/content/1/block/signature",
        Kind::Changed,
    );
}

#[test]
fn a_changed_final_verdict_fails() {
    one(
        after(|t| t["terminal"]["verdict"] = json!("fail")),
        "/terminal/verdict",
        Kind::Changed,
    );
}

#[test]
fn a_verdict_replaced_by_an_error_fails() {
    let differences = after(|t| {
        t["terminal"] =
            json!({"kind": "error", "class": "submission_invalid", "message": "not valid"});
    });
    assert_eq!(
        paths(&differences),
        vec![
            "/terminal/class",
            "/terminal/kind",
            "/terminal/message",
            "/terminal/verdict"
        ]
    );
}

#[test]
fn a_changed_tool_name_fails() {
    one(
        after(|t| t["turns"][1]["calls"][0]["name"] = json!("align")),
        "/turns/1/calls/0/name",
        Kind::Changed,
    );
}

#[test]
fn a_changed_argument_fails() {
    one(
        after(|t| t["turns"][1]["calls"][1]["arguments"]["box"][3] = json!(5)),
        "/turns/1/calls/1/arguments/box/3",
        Kind::Changed,
    );
    one(
        after(|t| {
            t["turns"][1]["calls"][1]["arguments"]
                .as_object_mut()
                .unwrap()
                .remove("box");
        }),
        "/turns/1/calls/1/arguments/box",
        Kind::Missing,
    );
}

#[test]
fn a_changed_result_fails() {
    one(
        after(|t| t["turns"][1]["calls"][0]["result"]["content"] = json!("A different look.")),
        "/turns/1/calls/0/result/content",
        Kind::Changed,
    );
}

#[test]
fn two_calls_in_the_other_order_fail() {
    let differences = after(|t| {
        let calls = t["turns"][1]["calls"].as_array_mut().unwrap();
        calls.swap(0, 1);
    });
    assert!(!differences.is_empty());
    assert!(paths(&differences).contains(&"/turns/1/calls/0/id"));
    assert!(paths(&differences).contains(&"/turns/1/calls/1/id"));
}

#[test]
fn a_missing_turn_fails() {
    let differences = after(|t| {
        t["turns"].as_array_mut().unwrap().pop();
    });
    one(differences, "/turns/3", Kind::Missing);
    // A turn missing from the middle moves every later turn.
    let differences = after(|t| {
        t["turns"].as_array_mut().unwrap().remove(1);
    });
    assert!(differences.contains(&Difference {
        path: "/turns/3".to_string(),
        kind: Kind::Missing
    }));
    assert!(paths(&differences).contains(&"/turns/1/turn"));
}

#[test]
fn an_extra_turn_fails() {
    let differences = after(|t| {
        let last = t["turns"][3].clone();
        t["turns"].as_array_mut().unwrap().push(last);
    });
    one(differences, "/turns/4", Kind::Extra);
}

#[test]
fn a_field_declared_volatile_does_not_fail() {
    let differences = after(|t| {
        t["origin"] = json!("replayed");
        t["meta"] = json!({"whizbang_commit": "abc"});
        t["files"] = json!([]);
        t["turns"][0]["meta"]["usage"]["output_tokens"] = json!(1);
        t["turns"][2]["meta"] = json!({});
        t["turns"][1]["calls"][0]["meta"]["duration_ms"] = json!(900);
        t["turns"][1]["calls"][1]["meta"] = json!(null);
    });
    assert_eq!(differences, vec![]);
}

#[test]
fn the_formats_own_volatile_list_is_accepted() {
    assert_eq!(
        transcript()["volatile"],
        json!([
            "/origin",
            "/meta",
            "/files",
            "/turns/*/meta",
            "/turns/*/calls/*/meta"
        ])
    );
    assert!(compare(&transcript(), &transcript()).is_ok());
}

#[test]
fn a_field_is_volatile_only_where_it_is_declared() {
    // `meta` is declared at three places; the same name anywhere else is compared.
    one(
        after(|t| t["turns"][1]["calls"][0]["arguments"]["meta"] = json!(1)),
        "/turns/1/calls/0/arguments/meta",
        Kind::Extra,
    );
}

#[test]
fn only_the_recorded_transcript_says_what_is_volatile() {
    let differences = after(|t| {
        t["volatile"]
            .as_array_mut()
            .unwrap()
            .push(json!("/terminal"));
        t["terminal"]["verdict"] = json!("fail");
    });
    assert_eq!(
        paths(&differences),
        vec!["/terminal/verdict", "/volatile/5"]
    );
}

#[test]
fn an_excluded_turn_holding_a_call_is_compared_like_any_other() {
    one(
        after(|t| t["turns"][0]["calls"][0]["disposition"] = json!("executed")),
        "/turns/0/calls/0/disposition",
        Kind::Changed,
    );
    one(
        after(|t| t["turns"][0]["excluded"] = json!(null)),
        "/turns/0/excluded",
        Kind::Changed,
    );
    one(
        after(|t| t["turns"][0]["calls"] = json!([])),
        "/turns/0/calls/0",
        Kind::Missing,
    );
    one(
        after(|t| {
            t["turns"][0]["calls"][0]["result"] =
                json!({"content": "x", "is_error": false, "images": []})
        }),
        "/turns/0/calls/0/result",
        Kind::Changed,
    );
}

#[test]
fn a_final_submission_is_compared_and_has_no_result() {
    one(
        after(|t| t["turns"][3]["calls"][0]["arguments"]["verdict"] = json!("fail")),
        "/turns/3/calls/0/arguments/verdict",
        Kind::Changed,
    );
    one(
        after(|t| t["turns"][3]["calls"][0]["disposition"] = json!("rejected")),
        "/turns/3/calls/0/disposition",
        Kind::Changed,
    );
    one(
        after(|t| {
            t["turns"][3]["calls"][0]["result"] =
                json!({"content": "x", "is_error": true, "images": []})
        }),
        "/turns/3/calls/0/result",
        Kind::Changed,
    );
}

#[test]
fn a_response_cut_at_the_token_limit_is_complete_and_compared_as_recorded() {
    // The synthetic transcript's first response stops at the token limit; the pair matches.
    assert_eq!(
        transcript()["turns"][0]["response"]["stop_reason"],
        json!("max_tokens")
    );
    assert_eq!(transcript()["complete"], json!(true));
    assert_eq!(after(|_| {}), vec![]);
    one(
        after(|t| t["turns"][0]["response"]["stop_reason"] = json!("tool_use")),
        "/turns/0/response/stop_reason",
        Kind::Changed,
    );
}

#[test]
fn a_recorded_transcript_with_a_file_cut_short_is_not_a_baseline() {
    let mut recorded = transcript();
    recorded["complete"] = json!(false);
    recorded["incomplete"] = json!(["stored file cut short by the recorder: response 2"]);
    recorded["turns"][1]["response"] = json!(null);
    assert_eq!(
        compare(&recorded, &transcript()),
        Err(Refusal::RecordedIncomplete)
    );
    let mut unmarked = transcript();
    unmarked.as_object_mut().unwrap().remove("complete");
    assert_eq!(
        compare(&unmarked, &transcript()),
        Err(Refusal::RecordedIncomplete)
    );
}

#[test]
fn a_recorded_transcript_marked_complete_that_lists_something_missing_is_not_a_baseline() {
    let mut recorded = transcript();
    recorded["incomplete"] = json!(["missing stored file: response 2"]);
    assert_eq!(
        compare(&recorded, &recorded.clone()),
        Err(Refusal::RecordedIncomplete)
    );
    for bad in [json!(null), json!("none"), json!({})] {
        let mut recorded = transcript();
        recorded["incomplete"] = bad;
        assert_eq!(
            compare(&recorded, &recorded.clone()),
            Err(Refusal::RecordedIncomplete)
        );
    }
    let mut recorded = transcript();
    recorded.as_object_mut().unwrap().remove("incomplete");
    assert_eq!(
        compare(&recorded, &recorded.clone()),
        Err(Refusal::RecordedIncomplete)
    );
}

#[test]
fn a_pair_without_turns_or_a_terminal_outcome_is_refused() {
    for (field, bad) in [
        ("turns", None),
        ("turns", Some(json!(null))),
        ("turns", Some(json!({}))),
        ("terminal", None),
        ("terminal", Some(json!(null))),
        ("terminal", Some(json!("pass"))),
    ] {
        let mut t = transcript();
        match &bad {
            None => {
                t.as_object_mut().unwrap().remove(field);
            }
            Some(value) => t[field] = value.clone(),
        }
        // The same defect on both sides would otherwise compare as equal.
        assert_eq!(
            compare(&t, &t.clone()),
            Err(Refusal::Shape {
                which: Side::Recorded
            }),
            "{field} {bad:?}"
        );
        assert_eq!(
            compare(&transcript(), &t),
            Err(Refusal::Shape {
                which: Side::Replayed
            }),
            "{field} {bad:?}"
        );
    }
    assert_eq!(
        compare(&json!([]), &transcript()),
        Err(Refusal::Version {
            which: Side::Recorded
        })
    );
}

#[test]
fn numbers_are_compared_by_the_text_they_were_written_with() {
    let parse = |number: &str| -> Value {
        let mut t = transcript();
        t["turns"][1]["calls"][0]["arguments"]["scale"] = serde_json::from_str(number).unwrap();
        // Through text, as a transcript read from a file is.
        serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap()
    };
    for (a, b) in [
        ("12345678901234567890123", "12345678901234567890124"),
        ("9007199254740993", "9007199254740992"),
        ("1", "1.0"),
        ("100", "1e2"),
        ("0.1", "0.10"),
        ("0.30000000000000004", "0.3"),
        ("-0.0", "0.0"),
    ] {
        let differences = compare(&parse(a), &parse(b)).unwrap();
        assert_eq!(
            paths(&differences),
            vec!["/turns/1/calls/0/arguments/scale"],
            "{a} and {b}"
        );
        assert_eq!(compare(&parse(a), &parse(a)).unwrap(), vec![], "{a}");
    }
}

#[test]
fn a_replay_that_is_not_complete_fails() {
    let differences = after(|t| {
        t["complete"] = json!(false);
        t["incomplete"] = json!(["missing stored file: response 2"]);
    });
    assert_eq!(paths(&differences), vec!["/complete", "/incomplete/0"]);
}

#[test]
fn a_changed_subject_or_preparation_fails() {
    one(
        after(|t| t["subject"]["model"] = json!("provider:model-y")),
        "/subject/model",
        Kind::Changed,
    );
    one(
        after(|t| t["preparation"]["settings"]["jpeg_quality"] = json!(80)),
        "/preparation/settings/jpeg_quality",
        Kind::Changed,
    );
}

#[test]
fn a_volatile_list_that_is_not_exactly_the_formats_is_refused() {
    // One more path, whatever it names: something compared, something new, something nested.
    for declared in [
        "/turns",
        "/turns/*",
        "/terminal",
        "/terminal/verdict",
        "/turns/*/request",
        "/turns/*/request/system",
        "/turns/*/calls/*/result",
        "/images",
        "/subject",
        "/volatile",
        "/complete",
        "/newtop",
        "/turns/*/extra",
        "/turns/*/calls/*/extra",
        "/*",
        "/meta",
    ] {
        let mut recorded = transcript();
        recorded["volatile"]
            .as_array_mut()
            .unwrap()
            .push(json!(declared));
        assert_eq!(
            compare(&recorded, &recorded.clone()),
            Err(Refusal::Volatile),
            "{declared}"
        );
    }
    // One path fewer, another order, or not a list of text at all.
    let mut fewer = transcript();
    fewer["volatile"].as_array_mut().unwrap().pop();
    assert_eq!(compare(&fewer, &fewer.clone()), Err(Refusal::Volatile));
    let mut reordered = transcript();
    reordered["volatile"].as_array_mut().unwrap().swap(0, 1);
    assert_eq!(
        compare(&reordered, &reordered.clone()),
        Err(Refusal::Volatile)
    );
    for bad in [
        json!(null),
        json!("/meta"),
        json!([1]),
        json!([]),
        json!({}),
    ] {
        let mut recorded = transcript();
        recorded["volatile"] = bad.clone();
        assert_eq!(
            compare(&recorded, &transcript()),
            Err(Refusal::Volatile),
            "{bad}"
        );
    }
}

/// Every place in a value, as JSON pointers, leaves and containers alike.
fn places(value: &Value, at: &str, out: &mut Vec<String>) {
    out.push(at.to_string());
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                places(inner, &format!("{at}/{key}"), out);
            }
        }
        Value::Array(list) => {
            for (index, inner) in list.iter().enumerate() {
                places(inner, &format!("{at}/{index}"), out);
            }
        }
        _ => {}
    }
}

fn is_declared_volatile(place: &str) -> bool {
    let have: Vec<&str> = place.trim_start_matches('/').split('/').collect();
    VOLATILE.iter().any(|declared| {
        let want: Vec<&str> = declared.trim_start_matches('/').split('/').collect();
        have.len() >= want.len() && want.iter().zip(&have).all(|(w, h)| *w == "*" || w == h)
    })
}

#[test]
fn every_place_in_a_transcript_is_compared_unless_the_format_declares_it_volatile() {
    let recorded = transcript();
    let mut all = Vec::new();
    places(&recorded, "", &mut all);
    let (mut compared, mut let_through) = (0, 0);
    for place in all.iter().filter(|p| !p.is_empty()) {
        // A changed value, a removed value and a new field beside it.
        let mut changed = transcript();
        *changed.pointer_mut(place).unwrap() = json!("a value no transcript holds");
        let mut removed = transcript();
        let (parent, key) = place.rsplit_once('/').unwrap();
        match removed.pointer_mut(parent).unwrap() {
            Value::Object(map) => {
                map.remove(key);
            }
            Value::Array(list) => {
                list.remove(key.parse().unwrap());
            }
            _ => unreachable!(),
        }
        let mut added = transcript();
        if let Some(Value::Object(map)) = added.pointer_mut(parent) {
            map.insert("a_field_no_format_names".to_string(), json!(1));
        }
        // A refusal is not a match either: the replay does not pass.
        let matches = |replayed: &Value| compare(&recorded, replayed).is_ok_and(|d| d.is_empty());
        for (what, replayed) in [("changed", changed), ("removed", removed)] {
            if is_declared_volatile(place) {
                assert!(matches(&replayed), "{what} {place} is declared volatile");
            } else {
                assert!(!matches(&replayed), "{what} {place} was let through");
            }
        }
        let parent_volatile = is_declared_volatile(parent) && !parent.is_empty();
        if added != recorded && !parent_volatile {
            assert!(
                !matches(&added),
                "a new field beside {place} was let through"
            );
        }
        if is_declared_volatile(place) {
            let_through += 1;
        } else {
            compared += 1;
        }
    }
    // The walk really covered the transcript, and both kinds of place.
    assert!(compared > 300, "{compared}");
    assert!(let_through > 20, "{let_through}");
}

#[test]
fn every_top_level_field_and_every_field_of_a_turn_and_a_call_is_covered_by_the_walk() {
    let t = transcript();
    let keys = |v: &Value| -> Vec<String> { v.as_object().unwrap().keys().cloned().collect() };
    assert_eq!(
        keys(&t),
        [
            "complete",
            "evidence_tools",
            "files",
            "images",
            "incomplete",
            "meta",
            "origin",
            "preparation",
            "repairs",
            "subject",
            "submission_tool",
            "terminal",
            "transcript_version",
            "turns",
            "volatile"
        ]
    );
    assert_eq!(
        keys(&t["turns"][1]),
        ["calls", "excluded", "meta", "request", "response", "turn"]
    );
    assert_eq!(
        keys(&t["turns"][1]["calls"][0]),
        ["arguments", "disposition", "id", "meta", "name", "result"]
    );
}

#[test]
fn the_example_on_the_formats_page_compares_with_itself_and_not_with_a_changed_copy() {
    let example: Value = serde_json::from_str(include_str!(
        "../../../../docs/baseline/example-transcript.json"
    ))
    .unwrap();
    assert_eq!(example["volatile"], json!(VOLATILE));
    assert_eq!(compare(&example, &example), Ok(vec![]));
    // The example has the same fields as the transcript these tests are written against.
    let keys = |v: &Value| -> Vec<String> { v.as_object().unwrap().keys().cloned().collect() };
    assert_eq!(keys(&example), keys(&transcript()));
    assert_eq!(keys(&example["turns"][1]), keys(&transcript()["turns"][1]));
    assert_eq!(
        keys(&example["turns"][1]["calls"][0]),
        keys(&transcript()["turns"][1]["calls"][0])
    );
    let mut changed = example.clone();
    changed["terminal"]["verdict"] = json!("fail");
    changed["turns"][0]["meta"] = json!({});
    one(
        compare(&example, &changed).unwrap(),
        "/terminal/verdict",
        Kind::Changed,
    );
}

#[test]
fn another_transcript_version_is_refused() {
    let mut recorded = transcript();
    recorded["transcript_version"] = json!(2);
    assert_eq!(
        compare(&recorded, &transcript()),
        Err(Refusal::Version {
            which: Side::Recorded
        })
    );
    let mut replayed = transcript();
    replayed
        .as_object_mut()
        .unwrap()
        .remove("transcript_version");
    assert_eq!(
        compare(&transcript(), &replayed),
        Err(Refusal::Version {
            which: Side::Replayed
        })
    );
}

#[test]
fn a_difference_names_the_place_and_never_the_values() {
    let differences = after(|t| t["terminal"]["verdict"] = json!("fail"));
    assert_eq!(differences[0].to_string(), "changed at /terminal/verdict");
    let all = format!("{differences:?}");
    assert!(!all.contains("fail") && !all.contains("pass"));
}

#[test]
fn every_difference_is_reported_in_the_recorded_order() {
    let differences = after(|t| {
        t["terminal"]["verdict"] = json!("fail");
        t["turns"][2]["calls"][0]["name"] = json!("view");
        t["turns"][0]["request"]["system"] = json!("x");
    });
    assert_eq!(
        paths(&differences),
        vec![
            "/terminal/verdict",
            "/turns/0/request/system",
            "/turns/2/calls/0/name"
        ]
    );
}
