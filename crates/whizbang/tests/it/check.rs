//! `check::…`: the judge loop of squint's `check` over the binary's kind `service`, on
//! loopback (plan 1D, task F1, item 2): what a turn cut off at its backend's output
//! ceiling comes to.

use serde_json::Value as Json;
use whizbang_core::contract::{Call, Executor, Outcome, Value};

use crate::anthropic::{Judged, import_picture, record_of, serving, turn_with};

#[test]
fn a_turn_cut_off_at_the_output_ceiling_is_not_retried_with_the_same_limit() {
    // The judge's table holds its output to 1,024 tokens, and `check` asks for 1,024:
    // the turn is cut off, and a retry would ask for four times that, which the
    // ceiling sends as 1,024 again. One request is sent, and the run fails naming the
    // setting to raise.
    let judged = Judged::new("check-ceiling");
    let answers = vec![turn_with("Looking", "max_tokens", Some((10_000, 1_024)))];
    let (listener, log) = serving(&judged.pki, answers);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &format!("{origin}/v1/messages"),
        Some(&origin),
        &["prompts", "source_images"],
        "[backends.judge.settings]\nmax_output_tokens = 1024\n",
    );
    let scratch = judged.world.beside("pictures");
    std::fs::create_dir_all(&scratch).unwrap();
    let picture = import_picture(&judged.world.root, &scratch, "one", 10);
    let executor = judged.runner(&over);
    let call = Call::new("check")
        .input(
            "images",
            picture
                .parse::<whizbang_core::identity::Digest>()
                .map(whizbang_core::artifact::ArtifactId)
                .unwrap(),
        )
        .argument("tools", Value::text(""));
    let ran = executor.submit(&call).unwrap();
    let Outcome::Error { kind, message: why } = ran.outcome() else {
        panic!("the run did not fail: {:?}", ran.outcome());
    };
    assert_eq!(kind, "judge_output_ceiling");
    assert_eq!(
        why,
        "the judge was cut off at its backend's output ceiling of 1024 tokens, and a retry \
         would be sent with no more: raise max_output_tokens in the judge's table of the config"
    );
    drop(executor);
    assert_eq!(listener.finish(), 1);
    let sent: Json = serde_json::from_slice(&log.lock().unwrap()[0].1).unwrap();
    assert_eq!(sent["max_tokens"], 1_024);
    let record = record_of(&judged.world.root, ran.run().as_str());
    assert_eq!(record["judge"].as_array().unwrap().len(), 1, "{record}");
    assert_eq!(record["judge"][0]["excluded"], "truncated");
}

// --- a whole exchange of `align` under squint's arm C (plan m2c, task A2, item 4) -----------

/// An answer of 200 whose turn calls one tool.
pub fn calling(id: &str, name: &str, arguments: Json) -> Vec<u8> {
    let body = serde_json::json!({
        "id": "msg_c", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
        "content": [{ "type": "tool_use", "id": id, "name": name, "input": arguments }],
        "stop_reason": "tool_use",
        "usage": { "input_tokens": 3_000, "output_tokens": 60 }
    })
    .to_string();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// The parts of a transcript (`docs/baseline/transcript.md`) that the exchange of one call
/// of `align` holds: the tool as the judge was told of it, the call, the result in the
/// history and the call's own, the turns' ends and the verdict. Everything else of a
/// transcript (the opening, the system text, `view`'s definition) is left out of both
/// sides: `view` as squint's judge knew it is task E2's.
fn transcript(align_tool: &Json, turns: &[(Json, Json, Json)], verdict: &str) -> String {
    let turns: Vec<Json> = turns
        .iter()
        .enumerate()
        .map(|(at, (history, response, calls))| {
            serde_json::json!({
                "turn": at + 1,
                "request": {"tools": [align_tool], "history": history},
                "response": response,
                "excluded": null,
                "calls": calls,
                "meta": {},
            })
        })
        .collect();
    serde_json::json!({
        "transcript_version": 1, "complete": true, "incomplete": [],
        "volatile": whizbang_eval::baseline::VOLATILE,
        "evidence_tools": ["view", "align"], "submission_tool": "submit_verdict",
        "turns": turns, "terminal": {"kind": "verdict", "verdict": verdict},
    })
    .to_string()
}

#[test]
fn a_whole_exchange_of_align_under_arm_c_has_the_recorded_shape() {
    use crate::anthropic::{Judged, checked_as, submission};
    use whizbang_core::artifact::{Image, ImageRole, Origin, Step};
    use whizbang_core::identity::{DecodeLimits, Projection, Topology};

    let judged = Judged::new("check-arm-c");
    let arguments = serde_json::json!({"image": "candidate", "against": "source"});
    let answers = vec![
        calling("toolu_a", "align", arguments.clone()),
        submission("held", "Nothing protected moved."),
    ];
    // `squint check --details none --projection visual_only`: squint's arm C.
    let (printed, log) = checked_as(
        &judged,
        answers,
        None,
        whizbang::door::Details::None,
        "visual_only",
    );
    assert!(printed.unwrap().contains("verdict: pass"));
    let sent: Vec<Json> = log
        .lock()
        .unwrap()
        .iter()
        .map(|(_, body)| serde_json::from_slice(body).unwrap())
        .collect();
    assert_eq!(sent.len(), 2);

    // What was replayed, read off the wire: the tool `align` as the judge was told of it,
    // and in the second request, the result that answers the call.
    let told = sent[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "align")
        .unwrap();
    let told = serde_json::json!({"name": told["name"], "description": told["description"],
                                  "input_schema": told["input_schema"]});
    let answered = sent[1]["messages"].as_array().unwrap().last().unwrap()["content"][0].clone();
    assert_eq!(answered["type"], "tool_result");
    let content = answered["content"].as_array().unwrap();
    let text = content[0]["text"].as_str().unwrap().to_owned();
    let shown: Json = serde_json::from_str(&text).unwrap();
    let frames: Vec<Json> = shown["images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|image| image["frame"].clone())
        .collect();
    assert_eq!(
        content
            .iter()
            .filter(|part| part["type"] == "image")
            .count(),
        frames.len()
    );
    let result = |text: &str, frames: Json| serde_json::json!({"content": text, "is_error": false, "images": frames});
    let in_history = |text: &str, frames: Json| {
        serde_json::json!([{"role": "user", "content": [{"type": "tool_result",
            "tool_use_id": "toolu_a", "content": text,
            "is_error": answered["is_error"].as_bool().unwrap_or(false), "images": frames}]}])
    };
    let response = |name: &str, id: &str, arguments: &Json| {
        serde_json::json!({"stop_reason": "tool_use",
            "tool_calls": [{"id": id, "name": name, "arguments": arguments}]})
    };
    let submitted = sent[1]["messages"].as_array().unwrap().len();
    assert_eq!(submitted, 3, "the opening, the call, its result");
    let call = |disposition: &str, result: Json| {
        serde_json::json!([{"id": "toolu_a", "name": "align", "arguments": arguments,
                            "disposition": disposition, "result": result}])
    };
    let final_call = serde_json::json!([{"id": "toolu_1", "name": "submit_verdict",
        "arguments": "submission", "disposition": "final", "result": null}]);
    let replayed = transcript(
        &told,
        &[
            (
                serde_json::json!([]),
                response("align", "toolu_a", &arguments),
                call("executed", result(&text, Json::Array(frames.clone()))),
            ),
            (
                in_history(&text, Json::Array(frames.clone())),
                response("submit_verdict", "toolu_1", &Json::from("submission")),
                final_call.clone(),
            ),
        ],
        "pass",
    );

    // What squint's arm C recorded for such an exchange, written here from squint's code:
    // the tool from its manifest (`crates/squint-tools/manifests/align.toml`); the result as
    // its harness presented a `visual_only` envelope of a registered pair
    // (`crates/squint-core/src/run.rs`, `crates/squint-core/src/tool/projection.rs`): the
    // call's id, ok, the fixed sentence, nothing measured, no region, and the aligned frame
    // on the source's raw frame with the normalised homography; one picture.
    let squint_align = serde_json::json!({
        "name": "align",
        "description": "Registers one image onto another of the same scene and viewpoint: a homography for flat images, a rotation on the sphere for panoramas; returns the first image warped into the second's frame with how well the pair agrees.",
        "input_schema": {"type": "object", "additionalProperties": false,
            "required": ["image", "against"],
            "properties": {
                "image": {"type": "string", "description": "The image to register, usually the candidate: its image id (candidate) or its raw frame id (raw:candidate)."},
                "against": {"type": "string", "description": "The image to register onto, usually the source: its image id (source) or its raw frame id (raw:source); the aligned frame shares its grid."}}},
    });
    let pictures = judged.world.beside("pictures");
    let read = |name: &str| {
        let file = std::fs::read(pictures.join(format!("{name}.png"))).unwrap();
        let origin = Origin::new(Step::new("fixture", "1")).unwrap();
        Image::new(
            file,
            Projection::Flat,
            Topology::Bounded,
            ImageRole::Picture,
            origin,
            &DecodeLimits::default(),
        )
        .unwrap()
    };
    let aligned = whizbang_squint::align::align(
        &read("candidate"),
        &read("source"),
        &whizbang_squint::align::Options::default(),
        &DecodeLimits::default(),
    )
    .unwrap();
    let params = Json::from(aligned.transform.unwrap().params.to_vec()).to_string();
    let recorded_text = format!(
        "{{\"call_id\":\"call:0\",\"images\":[{{\"frame\":\"aligned:candidate:t1\",\
         \"mapping\":{{\"homography\":{params}}},\"parent\":\"raw:source\"}}],\
         \"measurements\":[],\"regions\":[],\"status\":\"ok\",\
         \"summary\":\"Result computed; its measurements are withheld.\"}}"
    );
    let squint_frames = serde_json::json!(["aligned:candidate:t1"]);
    let recorded = transcript(
        &squint_align,
        &[
            (
                serde_json::json!([]),
                response("align", "toolu_a", &arguments),
                call("executed", result(&recorded_text, squint_frames.clone())),
            ),
            (
                in_history(&recorded_text, squint_frames.clone()),
                response("submit_verdict", "toolu_1", &Json::from("submission")),
                final_call,
            ),
        ],
        "pass",
    );
    let differences = whizbang_eval::baseline::compare(&recorded, &replayed).unwrap();
    assert!(differences.is_empty(), "{differences:?}");
    // The comparison is not blind: the same exchange answered under `full` differs.
    let full = transcript(
        &squint_align,
        &[(
            serde_json::json!([]),
            response("align", "toolu_a", &arguments),
            call(
                "executed",
                result(
                    &recorded_text.replace("\"ok\"", "\"not_applicable\""),
                    squint_frames,
                ),
            ),
        )],
        "pass",
    );
    assert!(
        !whizbang_eval::baseline::compare(&recorded, &full)
            .unwrap()
            .is_empty()
    );
}
