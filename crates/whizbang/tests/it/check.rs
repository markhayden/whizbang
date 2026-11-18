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
