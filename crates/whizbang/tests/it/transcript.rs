//! A stored run of `check` written as a transcript (plan 2C, task E2, item 5; plan m3, task
//! B1, item 3): two runs of one job under the same recorded answers write transcripts the
//! comparison finds no difference between, and a run answered otherwise differs at its
//! places.

use serde_json::{Value as Json, json};
use whizbang::door;
use whizbang_eval::baseline::{VOLATILE, compare};

use crate::anthropic::{Judged, checked_job, checklist_of, import_picture, ok, submission_of};

/// An answer of 200 whose turn, as Anthropic answers, looks closer at the middle of
/// `image` with `view`.
fn looking_at(image: &str) -> Vec<u8> {
    ok(&json!({
        "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
        "content": [
            { "type": "text", "text": "I will look at the middle." },
            { "type": "tool_use", "id": "toolu_0", "name": "view", "caller": { "type": "direct" },
              "input": { "image": image, "bbox": [0.25, 0.25, 0.75, 0.75] } }
        ],
        "stop_reason": "tool_use",
        "usage": { "input_tokens": 2_000, "output_tokens": 60 }
    }))
}

/// Runs `check` under source-fidelity in a world of its own, its judge answering a look at
/// the candidate and then a submission giving every item `disposition`, and writes the
/// stored run as a transcript.
fn transcript_of(label: &str, disposition: &str) -> String {
    let judged = Judged::new(label);
    let scratch = judged.world.beside("pictures");
    std::fs::create_dir_all(&scratch).unwrap();
    let candidate = import_picture(&judged.world.root, &scratch, "candidate", 200);
    let source = import_picture(&judged.world.root, &scratch, "source", 10);
    let asked = door::CheckAsked {
        use_case: "source-fidelity",
        candidate: &candidate,
        source: Some(&source),
        references: &[],
        judge: None,
        brief: None,
        request_id: None,
        details: door::Details::None,
        detail: None,
        projection: "full",
    };
    let (job, images) = door::shorthand(&asked).unwrap();
    let ids = checklist_of(&job);
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let answers = vec![
        looking_at(&candidate),
        submission_of(&ids, disposition, "Looked at."),
    ];
    let (printed, _) = checked_job(&judged, &job, &images, answers);
    printed.unwrap();
    let mut runs = Vec::new();
    whizbang::harness::each_run(&judged.world.root, &mut |record, id, _| {
        if record.asked().action.as_deref() == Some("check") {
            runs.push(id.to_owned());
        }
    })
    .unwrap();
    let [run] = runs.as_slice() else {
        panic!("{runs:?}")
    };
    let subject = json!({"set": "synthetic", "item": "one", "model": "anthropic:claude-haiku-5-5",
                         "arm": "C", "repetition": 1, "use_case": "source-fidelity"});
    let preparation = json!({"version": "whizbang", "settings": {}});
    whizbang::transcript::of_run(&judged.world.root, run, &subject, &preparation).unwrap()
}

#[test]
fn a_run_of_check_is_written_as_a_transcript_the_comparison_reads() {
    let first = transcript_of("transcript-first", "held");
    let written: Json = serde_json::from_str(&first).unwrap();
    // The format's fields, whole: two turns, the look executed and the submission final,
    // the job's two images and the crop the look returned.
    assert_eq!(written["transcript_version"], 1);
    assert_eq!(written["origin"], "stored");
    assert_eq!(written["complete"], true, "{}", written["incomplete"]);
    assert_eq!(written["volatile"], json!(VOLATILE));
    assert_eq!(written["submission_tool"], "submit_verdict");
    assert!(
        written["evidence_tools"]
            .as_array()
            .unwrap()
            .contains(&json!("view"))
    );
    let turns = written["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0]["calls"][0]["disposition"], "executed");
    assert_eq!(turns[1]["calls"][0]["disposition"], "final");
    let origins: Vec<&str> = (written["images"].as_array().unwrap().iter())
        .map(|image| image["origin"].as_str().unwrap())
        .collect();
    assert_eq!(origins, ["job", "job", "toolu_0"]);
    let shown = &turns[0]["calls"][0]["result"]["images"];
    assert_eq!(shown[0], written["images"][2]["id"]);
    assert_eq!(
        written["terminal"],
        json!({"kind": "verdict", "verdict": "pass"})
    );
    assert!(first.ends_with("}\n"));

    // A second run of the same job under the same answers: no difference, though its run,
    // its files and its turns' metadata are its own.
    let again = transcript_of("transcript-again", "held");
    assert_ne!(first, again);
    assert_eq!(compare(&first, &again).unwrap(), Vec::new());

    // A run answered otherwise differs where the answers did, and in its verdict: the
    // second turn's response, by its place in the list of turns, and not the first.
    let otherwise = transcript_of("transcript-otherwise", "violation");
    let places: Vec<String> = (compare(&first, &otherwise).unwrap().iter())
        .map(|difference| difference.path.clone())
        .collect();
    assert!(
        places.iter().any(|p| p == "/terminal/verdict"),
        "{places:?}"
    );
    assert!(
        places.iter().any(|p| p.starts_with("/turns/1/response")),
        "{places:?}"
    );
    assert!(
        places.iter().all(|p| !p.starts_with("/turns/0/")),
        "{places:?}"
    );
}
