//! `google::…`: the Google judge through the binary's wiring (plan m2c, task P1), as
//! `anthropic::…` does for Anthropic's: a config that names the kind `service` with the
//! provider `google`, its key in the env file, and a server on loopback with a
//! certificate of a root made in the test. The shipped kind's address is never asked.

use whizbang_backends::google::KEY_HEADER;

use crate::anthropic::{GOOGLE, Provider, asked_once};

#[test]
fn the_key_goes_only_to_the_approved_origin_in_its_header() {
    let (judged, head, _) = asked_once(GOOGLE, "google-key");
    // In its one header, `x-goog-api-key`, and in no query of the address.
    let line = format!("{KEY_HEADER}: {}\r\n", judged.key);
    assert!(head.contains(&line), "{head}");
    assert!(
        head.starts_with("POST /v1beta/models/gemini-3.8-flash:generateContent HTTP/1.1\r\n"),
        "{head}"
    );
    assert!(
        !head.to_ascii_lowercase().contains("authorization"),
        "{head}"
    );
}

#[test]
fn a_google_judge_whose_table_sets_no_effort_thinks_at_the_low_level() {
    // The gate run of the details first sent no thinking level, and 117 of its 131 turns
    // cut off at the output limit held nothing but thinking: Gemini counts its thoughts
    // against `maxOutputTokens`. A table that sets no `effort` asks for `low`, the level
    // squint's recorded requests of the study's models asked for.
    let (_, _, body) = asked_once(GOOGLE, "google-low");
    let sent: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        sent["generationConfig"]["thinkingConfig"]["thinkingLevel"], "low",
        "{sent}"
    );
}

/// An answer of 200 with this body.
fn ok(body: &serde_json::Value) -> Vec<u8> {
    let body = body.to_string();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// A call of each tool this build links, by name and arguments, each as its schema says:
/// `view` of the candidate (its optional `also` left out), `align` of the candidate onto
/// the source, `resize` of the candidate's raw frame, and each comparison of the raw frames.
fn every_call(candidate: &str) -> Vec<(&'static str, serde_json::Value)> {
    use serde_json::json;
    let linked = whizbang_squint::check::linked();
    let calls = vec![
        (
            "view",
            json!({"image": candidate, "bbox": [0.1, 0.1, 0.6, 0.6]}),
        ),
        ("align", json!({"image": "candidate", "against": "source"})),
        ("resize", json!({"frame": "raw:candidate", "long_edge": 64})),
        (
            "change_map",
            json!({"frame": "raw:source", "against": "raw:candidate"}),
        ),
        (
            "edge_similarity",
            json!({"frame": "raw:source", "against": "raw:candidate"}),
        ),
    ];
    // Every tool the build links has its call here: a tool added to the build without
    // one fails this test until it is given one.
    let named: Vec<&str> = calls.iter().map(|(name, _)| *name).collect();
    assert_eq!(named, linked, "a call for each tool this build links");
    calls
}

/// The first answer of a judge of this provider: a call of each tool, in one turn.
fn calling_every_tool(provider: Provider, candidate: &str) -> Vec<u8> {
    use serde_json::json;
    let calls = every_call(candidate);
    let body = match provider.name {
        "google" => {
            let parts: Vec<serde_json::Value> = (calls.iter().enumerate())
                .map(|(at, (name, args))| {
                    json!({ "functionCall": { "id": format!("call_t{at}"), "name": name, "args": args } })
                })
                .collect();
            json!({
                "candidates": [{ "content": { "role": "model", "parts": parts }, "finishReason": "STOP" }],
                "usageMetadata": { "promptTokenCount": 3_000, "candidatesTokenCount": 200 },
                "responseId": "resp_1"
            })
        }
        _ => {
            let output: Vec<serde_json::Value> = (calls.iter().enumerate())
                .map(|(at, (name, args))| {
                    json!({ "type": "function_call", "id": format!("fc_t{at}"),
                            "call_id": format!("call_t{at}"), "name": name,
                            "arguments": args.to_string(), "status": "completed" })
                })
                .collect();
            json!({
                "id": "resp_1", "status": "completed", "output": output,
                "usage": { "input_tokens": 3_000, "output_tokens": 200 }
            })
        }
    };
    ok(&body)
}

/// Every key whose value is `null`, taken out at any depth: how Gemini writes a call's
/// arguments, leaving out the fields it would set to null.
fn without_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            map.values_mut().for_each(without_nulls);
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(without_nulls),
        _ => {}
    }
}

/// A run of `check` under a judge of this provider on loopback that calls every tool of the
/// build in its first turn and submits in its second: what was printed, and the bodies of
/// the two requests. Each tool is offered; the submission is the provider's own form, as
/// Gemini writes one without its nulls.
pub fn every_tool_sent(provider: Provider, label: &str) -> (String, serde_json::Value, String) {
    use crate::anthropic::{CHECK_CLASSES, Judged, pair, serving, submission_by};
    let judged = Judged::of(label, provider);
    let (source, candidate) = pair(&judged);
    let mut submitted = submission_by(provider, "held", "Kept.");
    if provider.name == "google" {
        let text = String::from_utf8(submitted).unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap();
        let mut body: serde_json::Value = serde_json::from_str(body).unwrap();
        without_nulls(&mut body["candidates"][0]["content"]["parts"][0]["functionCall"]["args"]);
        let _ = head;
        submitted = ok(&body);
    }
    let answers = vec![calling_every_tool(provider, &candidate), submitted];
    let (listener, log) = serving(&judged.pki, answers);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &judged.address(listener.port()),
        Some(&origin),
        &CHECK_CLASSES,
        "",
    );
    let executor = judged.runner(&over);
    let asked = whizbang::door::CheckAsked {
        use_case: "source-fidelity",
        source: Some(&source),
        candidate: &candidate,
        references: &[],
        judge: None,
        brief: None,
        request_id: None,
        details: whizbang::door::Details::None,
        detail: None,
        projection: "full",
    };
    let (job, images) = whizbang::door::shorthand(&asked).unwrap();
    let more: Vec<String> = whizbang_squint::check::linked()
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let measuring = whizbang::door::Measuring {
        details: whizbang::door::Details::None,
        detail: None,
        projection: "full",
        tools: &more,
        second_look: "off",
        submission: None,
    };
    let root = judged.world.root.clone();
    let printed =
        whizbang::door::check_job(&executor, &job, &images, None, None, &measuring, &|run| {
            whizbang::door::verdict(&root, run)
        });
    drop(executor);
    assert_eq!(listener.finish(), 2);
    let log = log.lock().unwrap();
    let first: serde_json::Value = serde_json::from_slice(&log[0].1).unwrap();
    let second = String::from_utf8(log[1].1.clone()).unwrap();
    (printed.unwrap(), first, second)
}

/// The schema each tool of the build is described with, by name, the submission among them.
pub fn schemas() -> Vec<(String, serde_json::Value)> {
    let mut out: Vec<(String, serde_json::Value)> = (whizbang_squint::check::linked().iter())
        .map(|name| {
            let tool = whizbang_squint::check::definition(name).unwrap();
            (tool.name().to_owned(), tool.schema().clone())
        })
        .collect();
    let submit = whizbang_squint::use_case::submit_tool().unwrap();
    out.push((submit.name().to_owned(), submit.schema().clone()));
    out
}

#[test]
fn every_tool_of_the_build_is_sent_and_its_arguments_read_by_its_schema() {
    let (printed, first, second) = every_tool_sent(GOOGLE, "google-every-tool");
    // Each tool is declared once, its schema as the build wrote it; the submission's
    // definitions written in place, and nothing else changed.
    let declared = first["tools"][0]["functionDeclarations"]
        .as_array()
        .unwrap();
    let wanted = schemas();
    assert_eq!(declared.len(), wanted.len(), "{declared:?}");
    for (name, schema) in &wanted {
        let found = declared
            .iter()
            .find(|d| d["name"] == name.as_str())
            .unwrap();
        let sent = &found["parametersJsonSchema"];
        if schema.get("$defs").is_some() {
            let text = sent.to_string();
            assert!(!text.contains("$ref") && !text.contains("$defs"), "{name}");
            assert_eq!(sent["required"], schema["required"], "{name}");
        } else {
            assert_eq!(sent, schema, "{name}");
        }
    }
    // Each call was read as its schema says: answered with what the tool gave, never
    // refused or left uncalled, and the submission without its nulls decided the verdict.
    let second: serde_json::Value = serde_json::from_str(&second).unwrap();
    let responses: Vec<&serde_json::Value> = (second["contents"].as_array().unwrap().iter())
        .flat_map(|c| c["parts"].as_array().unwrap())
        .filter_map(|p| p.get("functionResponse"))
        .collect();
    assert_eq!(responses.len(), 5, "{second}");
    for response in responses {
        let text = response.to_string();
        assert!(
            !text.contains("not called") && !text.contains("refused"),
            "{text}"
        );
    }
    assert!(printed.contains("verdict: pass"), "{printed}");
}

/// A run of `check` under submission v2 (plan e1, task F1) under a judge of this provider
/// on loopback that answers in v2's keyed form: what was printed, and the body of the one
/// request, which carried v2's tool.
pub fn v2_sent(provider: Provider, label: &str) -> (String, serde_json::Value) {
    use crate::anthropic::{CHECK_CLASSES, Judged, pair, serving, submitting_by};
    let judged = Judged::of(label, provider);
    let (source, candidate) = pair(&judged);
    let answer = serde_json::json!({"disposition": "held", "confidence": "confident",
        "reason": "Kept.", "allowed_by": null, "evidence": [], "location": null});
    let keyed = submitting_by(
        provider,
        serde_json::json!({"schema_version": 2, "findings": [],
            "items": {"chk_ob_SF1_candidate": answer.clone(), "chk_ob_SF2_candidate": answer},
            "opinion": {"verdict": "pass", "reason": "Recorded only."}}),
    );
    let (listener, log) = serving(&judged.pki, vec![keyed]);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &judged.address(listener.port()),
        Some(&origin),
        &CHECK_CLASSES,
        "",
    );
    let executor = judged.runner(&over);
    let asked = whizbang::door::CheckAsked {
        use_case: "source-fidelity",
        source: Some(&source),
        candidate: &candidate,
        references: &[],
        judge: None,
        brief: None,
        request_id: None,
        details: whizbang::door::Details::None,
        detail: None,
        projection: "visual_only",
    };
    let (job, images) = whizbang::door::shorthand(&asked).unwrap();
    let measuring = whizbang::door::Measuring {
        submission: Some("v2"),
        ..whizbang::door::Measuring::arm(whizbang::door::Details::None, None)
    };
    let root = judged.world.root.clone();
    let printed =
        whizbang::door::check_job(&executor, &job, &images, None, None, &measuring, &|run| {
            whizbang::door::verdict(&root, run)
        });
    drop(executor);
    assert_eq!(listener.finish(), 1);
    let log = log.lock().unwrap();
    let first: serde_json::Value = serde_json::from_slice(&log[0].1).unwrap();
    (printed.unwrap(), first)
}

/// Every key of every object a schema names under `properties`, at any depth, holds to
/// what every provider takes as a property's name: `^[a-zA-Z0-9_.-]{1,64}$`.
pub fn keys_are_provider_safe(schema: &serde_json::Value) -> bool {
    let safe = |key: &str| {
        (1..=64).contains(&key.len())
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
    };
    match schema {
        serde_json::Value::Object(fields) => fields.iter().all(|(name, value)| {
            let named = name != "properties"
                || value.as_object().is_some_and(|p| p.keys().all(|k| safe(k)));
            named && keys_are_provider_safe(value)
        }),
        serde_json::Value::Array(values) => values.iter().all(keys_are_provider_safe),
        _ => true,
    }
}

/// v2's tool for the checklist of source-fidelity's shorthand job.
pub fn v2_schema() -> serde_json::Value {
    let job = whizbang_squint::job::Job::shorthand("source-fidelity", true, 0, None);
    let resolved = whizbang_squint::use_case::resolve(&job).unwrap();
    let checklist = whizbang_squint::use_case::Checklist::build(&resolved, &job);
    let tool = whizbang_squint::use_case::submit_tool_v2(&checklist).unwrap();
    tool.schema().clone()
}

#[test]
fn a_v2_keyed_submission_is_sent_with_its_definitions_in_place_and_read() {
    let (printed, first) = v2_sent(GOOGLE, "google-v2");
    let declared = first["tools"][0]["functionDeclarations"]
        .as_array()
        .unwrap();
    let submit = declared
        .iter()
        .find(|d| d["name"] == "submit_verdict")
        .unwrap();
    let sent = &submit["parametersJsonSchema"];
    let text = sent.to_string();
    assert!(!text.contains("$ref") && !text.contains("$defs"), "{text}");
    let wanted = v2_schema();
    assert!(keys_are_provider_safe(sent), "{text}");
    assert_eq!(sent["required"], wanted["required"]);
    assert_eq!(
        sent["properties"]["items"]["required"],
        serde_json::json!(["chk_ob_SF1_candidate", "chk_ob_SF2_candidate"])
    );
    assert_eq!(
        sent["properties"]["items"]["properties"]["chk_ob_SF1_candidate"]["required"],
        wanted["properties"]["items"]["properties"]["chk_ob_SF1_candidate"]["required"]
    );
    assert!(printed.contains("verdict: pass"), "{printed}");
}
