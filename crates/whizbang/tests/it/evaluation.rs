//! `evaluation::…`: what a cell of a matrix sends is its arm's, stated on `check` itself,
//! and never a recipe's default (plan m4, task A1, item 4).

use crate::anthropic::submission;
use crate::fetch::SET;
use crate::matrix::{
    PLENTY, argument, checks, matrix_with, never, ran, run_matrix, runs_of, steps_of, world_with,
};

#[test]
fn an_arm_f_and_an_arm_c_cell_send_what_they_sent_before() {
    // The use cases' recipes changed their defaults to the control arm (task A1, item 2).
    // A cell is a call of `check`, not of a recipe, and states its arm's details, tools
    // and projection itself (`whizbang::evaluation`, `call_of`), so every committed matrix
    // and gate means what it meant: arm C and arm F are held here to what they sent
    // before, written out.
    let (judged, tiny) = world_with("evaluation-arms", &["a"]);
    let matrix = matrix_with(SET, "\"C\", \"F\"");
    let answers = vec![submission("held", "Kept."); 2];
    let (report, log) = run_matrix(&judged, answers, &|e| {
        ran(e, &judged, &tiny, &matrix, PLENTY, &never)
    });
    let report = report.unwrap();
    assert_eq!((report.cells, report.completed), (2, 2));
    let root = &judged.world.root;
    // No cell ran a recipe: two runs of `check`, and no step `details` of any recipe.
    assert!(runs_of(root, "source-fidelity").is_empty());
    assert!(runs_of(root, "details").is_empty());
    let mut runs = checks(root);
    assert_eq!(runs.len(), 2);
    runs.sort_by_key(|(_, record)| argument(record, "details"));
    let (first, none) = (&runs[0].1, &runs[1].1);
    let cell = |record: &serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&argument(record, "cell").unwrap()).unwrap()
    };
    let settings = |record: &serde_json::Value| {
        [
            "details",
            "detail",
            "tools",
            "projection",
            "second_look",
            "set",
        ]
        .map(|name| argument(record, name))
    };
    let text = |value: &str| Some(value.to_owned());
    // Arm C: nothing measured first (the tool `first` would measure with is named all
    // the same, as it always was), `view` and `align`, `visual_only`; no override.
    assert_eq!(cell(none)["arm"], "C");
    assert_eq!(
        settings(none),
        [
            text("none"),
            text("align"),
            text("view,align"),
            text("visual_only"),
            text("off"),
            None
        ]
    );
    assert_eq!(steps_of(none), [] as [&str; 0]);
    // Arm F: the details first, by `align`; `resize` offered too; `full`.
    assert_eq!(cell(first)["arm"], "F");
    assert_eq!(
        settings(first),
        [
            text("first"),
            text("align"),
            text("view,align,resize"),
            text("full"),
            text("off"),
            None
        ]
    );
    assert_eq!(steps_of(first), ["align"]);
    // And on the wire: each arm's tools, and the details in arm F's opening alone.
    let sent: Vec<serde_json::Value> = (log.lock().unwrap().iter())
        .map(|(_, body)| serde_json::from_slice(body).unwrap())
        .collect();
    assert_eq!(sent.len(), 2);
    let offered = |sent: &serde_json::Value| -> Vec<String> {
        (sent["tools"].as_array().unwrap().iter())
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect()
    };
    let measured = |sent: &serde_json::Value| {
        (sent["messages"][0]["content"].as_array().unwrap().iter())
            .any(|part| part["text"] == whizbang_squint::details::DETAILS_INTRO)
    };
    let of_f = sent.iter().find(|sent| measured(sent)).expect("arm F's");
    let of_c = sent.iter().find(|sent| !measured(sent)).expect("arm C's");
    assert_eq!(offered(of_c), ["view", "align", "submit_verdict"]);
    assert_eq!(offered(of_f), ["view", "align", "resize", "submit_verdict"]);
}

// --- the setting `submission` in a matrix (plan e1, task F1, item 1) ---------------------------

use crate::anthropic::submitting;
use crate::matrix::{SF1, SF2, items};

/// A submission of v2, keyed by checklist id, as Anthropic answers.
fn keyed() -> Vec<u8> {
    let answer = serde_json::json!({"disposition": "held", "confidence": "confident",
        "reason": "Kept.", "allowed_by": null, "evidence": [], "location": null});
    submitting(serde_json::json!({"schema_version": 2, "findings": [],
        "items": {SF1: answer.clone(), SF2: answer},
        "opinion": {"verdict": "pass", "reason": "Recorded only."}}))
}

#[test]
fn a_resume_under_another_default_submission_is_refused() {
    let (judged, tiny) = world_with("evaluation-submission", &["a"]);
    let matrix = matrix_with(SET, "\"C\", \"C;submission=v2\"");
    let answers = vec![submission("held", "Kept."), keyed()];
    let (report, log) = run_matrix(&judged, answers, &|e| {
        ran(e, &judged, &tiny, &matrix, PLENTY, &never)
    });
    let report = report.unwrap();
    assert_eq!((report.cells, report.completed), (2, 2));
    let root = &judged.world.root;
    // Each cell records the value it ran under: the arm's, or the binary's default.
    let mut ran_under: Vec<(String, Option<String>)> = checks(root)
        .iter()
        .map(|(_, record)| {
            let cell: serde_json::Value =
                serde_json::from_str(&argument(record, "cell").unwrap()).unwrap();
            (
                cell["arm"].as_str().unwrap().to_owned(),
                argument(record, "submission"),
            )
        })
        .collect();
    ran_under.sort();
    assert_eq!(
        ran_under,
        [
            ("C".to_owned(), Some("v1".to_owned())),
            ("C;submission=v2".to_owned(), Some("v2".to_owned()))
        ]
    );
    // On the wire: v1's list, then v2's keys.
    let sent: Vec<serde_json::Value> = (log.lock().unwrap().iter())
        .map(|(_, body)| serde_json::from_slice(body).unwrap())
        .collect();
    let items_type = |sent: &serde_json::Value| {
        let submit = (sent["tools"].as_array().unwrap().iter())
            .find(|t| t["name"] == "submit_verdict")
            .unwrap();
        submit["input_schema"]["properties"]["items"]["type"].clone()
    };
    assert_eq!(items_type(&sent[0]), "array");
    assert_eq!(items_type(&sent[1]), "object");
    // A resume on this binary is allowed; on a binary whose default is v2 it is refused
    // before anything runs, naming the setting and the arm that names none.
    whizbang::evaluation::submission_resumable(&matrix, root, None, "v1").unwrap();
    let refused =
        whizbang::evaluation::submission_resumable(&matrix, root, None, "v2").unwrap_err();
    assert!(refused.contains("submission"), "{refused}");
    assert!(refused.contains("arm C "), "{refused}");
    assert!(!refused.contains("C;submission=v2"), "{refused}");
    // The binary's values are the matrix's.
    assert_eq!(
        whizbang_squint::check::SUBMISSIONS,
        whizbang_eval::matrix::SUBMISSIONS
    );
}

#[test]
fn a_resume_with_only_failed_cells_under_another_default_is_refused() {
    let (judged, tiny) = world_with("evaluation-submission-failed", &["a"]);
    let matrix = matrix_with(SET, "\"C\"");
    let short = || items(&[(SF1, "held", "confident")]);
    let (report, _) = run_matrix(&judged, vec![short(), short()], &|e| {
        ran(e, &judged, &tiny, &matrix, PLENTY, &never)
    });
    let report = report.unwrap();
    assert_eq!((report.cells, report.completed, report.failed), (1, 0, 1));
    let root = &judged.world.root;
    whizbang::evaluation::submission_resumable(&matrix, root, None, "v1").unwrap();
    let refused =
        whizbang::evaluation::submission_resumable(&matrix, root, None, "v2").unwrap_err();
    assert!(refused.contains("submission"), "{refused}");
}
