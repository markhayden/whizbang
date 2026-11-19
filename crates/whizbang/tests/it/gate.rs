//! `gate::…`: a gate frozen in a file, run over the matrix it names through the binary's
//! executor under a judge served on loopback, scored against labels read from a folder of
//! their own, and decided (plan m3, task G1, item 1). Sets of three or four items made in
//! the test; 200 resamples.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use whizbang::evaluation::Running;
use whizbang::gating::{self, Gating};
use whizbang_core::identity::Digest;
use whizbang_eval::gate::Receipt;
use whizbang_eval::matrix::Matrix;

use crate::anthropic::{CHECK_CLASSES, Judged, serving, submission};
use crate::fetch::{SET, Tiny};
use crate::matrix::world_with;

/// A matrix of arm C and arm C offered `resize` over the test's set under the config's default judge, one
/// run at a time, so the judge's answers go to the cells in their order.
pub fn gate_matrix() -> String {
    format!(
        "[matrix]\nname = \"tiny-gate\"\nsets = [\"{SET}\"]\narms = [\"C\", \"C+resize\"]\n\
         repetitions = 1\nconcurrency = 1\n\
         [[matrix.judges]]\nbackend = \"default\"\nmodel = \"claude-haiku-5-5\"\n"
    )
}

/// A gate of `C+resize` against C over `matrix`, frozen in `<dir>/gate.toml` with the matrix beside
/// it as `matrix.toml`: the gate file's path.
pub fn frozen_gate(dir: &Path, matrix: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("matrix.toml"), matrix).unwrap();
    let identity = Matrix::parse(matrix).unwrap().identity();
    let gate = format!(
        "gate_version = 2\n\n[run]\nmatrix = \"matrix.toml\"\nmatrix_identity = \"{identity}\"\n\n\
         [baseline]\ncontrol = \"C\"\ntreatment = \"C+resize\"\n\n\
         [measure]\nprimary = \"balanced_accuracy\"\npass = \"pass\"\nfail = \"catch\"\n\
         unsure = \"neither\"\nerror = \"neither\"\nmissing_label = \"refuse\"\n\n\
         [confidence]\nlevel = 0.95\ncorrection = \"bonferroni\"\nhypotheses = 1\n\n\
         [families]\nminimum = 20\nresampling = \"family\"\nseed = 20261118\nresamples = 200\n\n\
         [repetitions]\ncount = 1\ncombine = \"first\"\n\n\
         [admission]\nmin_gain = 0.03\nlower_bound_above = 0.0\n\n\
         [guardrails]\nfalse_reject_increase_max = 0.03\nerror_rate_max = 0.02\n\n\
         [limits]\ncost_ratio_max = 2.0\nlatency_ratio_max = 2.0\n\n\
         [too_little_data]\noutcome = \"inconclusive\"\n"
    );
    let file = dir.join("gate.toml");
    std::fs::write(&file, gate).unwrap();
    file
}

/// Runs the gate under a judge on loopback that gives these answers, in order: what the
/// gate came to, and how many requests the judge was sent.
pub fn gated(
    judged: &Judged,
    tiny: &Tiny,
    gate: &Path,
    labels: &Path,
    receipt: &Path,
    answers: Vec<Vec<u8>>,
) -> (Result<Receipt, String>, usize) {
    // A listener serves at least one connection: a run that is to send nothing is given
    // one answer it never asks for.
    let answers = if answers.is_empty() {
        vec![submission("held", "Unasked.")]
    } else {
        answers
    };
    let (listener, log) = serving(&judged.pki, answers);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &judged.address(listener.port()),
        Some(&origin),
        &CHECK_CLASSES,
        "",
    );
    let executor = judged.runner(&over);
    let running = Running {
        executor: &executor,
        root: &judged.world.root,
        left_micros: 20_000_000,
        stop: &|| false,
    };
    let gating = Gating {
        file: gate,
        sets_root: &tiny.root,
        labels_root: labels,
        receipt,
    };
    let receipt = gating::run(&gating, &running);
    drop(executor);
    listener.finish();
    let sent = log.lock().unwrap().len();
    (receipt, sent)
}

/// Every file under `dir`, by its path, with its bytes.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                out.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

/// The `cell` of each run of `check` in the store, as JSON.
fn cells(root: &Path) -> Vec<serde_json::Value> {
    let listed = crate::wired::whizbang(&["runs", "list"], root);
    let mut found = Vec::new();
    for line in listed.out.lines().skip(1) {
        let run = line.split('\t').next().unwrap();
        let record = crate::anthropic::record_of(root, run);
        if record["asked"]["action"] != "check" {
            continue;
        }
        let arguments = record["asked"]["arguments"].as_array().unwrap();
        let cell = arguments.iter().find(|a| a["name"] == "cell").unwrap();
        found.push(serde_json::from_str(cell["value"]["text"].as_str().unwrap()).unwrap());
    }
    found
}

#[test]
fn the_frozen_gates_digest_is_in_every_run_it_makes() {
    let (judged, tiny) = world_with("gate-digest", &["a", "b"]);
    let gates = judged.world.beside("gates");
    let gate = frozen_gate(&gates, &gate_matrix());
    let receipt = judged.world.beside("receipt.json");
    let answers = vec![submission("held", "Kept."); 4];
    let (written, sent) = gated(&judged, &tiny, &gate, &tiny.root, &receipt, answers);
    let written = written.unwrap();
    assert_eq!(sent, 4);
    let digest = Digest::of_bytes(&std::fs::read(&gate).unwrap()).hex();
    assert_eq!(written.gate_sha256, digest);
    let found = cells(&judged.world.root);
    assert_eq!(found.len(), 4);
    for cell in &found {
        assert_eq!(cell["gate"], digest.as_str(), "{cell}");
        assert_eq!(cell["matrix"], "tiny-gate");
    }
    // The matrix's own runs, without a gate, are not the gate's: none is there.
    let identity: String = Matrix::parse(&gate_matrix())
        .unwrap()
        .identity()
        .chars()
        .take(16)
        .collect();
    let plain = whizbang::evaluation::attempts(&judged.world.root, &identity).unwrap();
    assert!(plain.is_empty());
    // Run again, the gate resumes from its own runs: nothing is sent, the same receipt.
    let (again, sent) = gated(&judged, &tiny, &gate, &tiny.root, &receipt, Vec::new());
    assert_eq!((again.unwrap(), sent), (written, 0));
    // A matrix changed after the gate was frozen is refused before anything runs.
    std::fs::write(
        gates.join("matrix.toml"),
        gate_matrix().replace("concurrency = 1", "concurrency = 2"),
    )
    .unwrap();
    let (refused, sent) = gated(&judged, &tiny, &gate, &tiny.root, &receipt, Vec::new());
    let refused = refused.unwrap_err();
    assert!(
        refused.contains("not the") && refused.contains("frozen with"),
        "{refused}"
    );
    assert_eq!(sent, 0);
    assert_eq!(cells(&judged.world.root).len(), 4);
}

/// The test's set made blind: its labels moved out of the sets' tree into `labels`, laid
/// out as the sets are, the first `fails` of its items labelled fail; the manifest names
/// the labels by their SHA-256 alone.
fn blinded(tiny: &Tiny, labels: &Path, items: &[&str], fails: usize) {
    let set = tiny.root.join(SET);
    let mut text = String::new();
    for (at, item) in items.iter().enumerate() {
        let expected = if at < fails { "fail" } else { "pass" };
        text.push_str(&format!(
            "[[label]]\nitem = \"{item}\"\nexpected = \"{expected}\"\nwords = \"as drawn\"\n\
             adjudicated_by = \"owner\"\nadjudicated_on = \"2026-11-18\"\n\n"
        ));
    }
    std::fs::create_dir_all(labels.join(SET)).unwrap();
    std::fs::write(labels.join(SET).join("labels.toml"), &text).unwrap();
    std::fs::remove_file(set.join("labels.toml")).unwrap();
    let manifest = std::fs::read_to_string(set.join("set.toml")).unwrap();
    let digest = Digest::of_bytes(text.as_bytes()).hex();
    let manifest = manifest
        .replace("split = \"dev\"", "split = \"blind\"")
        .replace(
            "labels = \"labels.toml\"",
            &format!("labels_sha256 = \"{digest}\""),
        );
    std::fs::write(set.join("set.toml"), manifest).unwrap();
}

/// A copy of the labels in `from` with one label turned over, into `to`.
fn blinded_copy(from: &Path, to: &Path) {
    let text = std::fs::read_to_string(from.join(SET).join("labels.toml")).unwrap();
    std::fs::create_dir_all(to.join(SET)).unwrap();
    let turned = text.replacen("expected = \"fail\"", "expected = \"pass\"", 1);
    std::fs::write(to.join(SET).join("labels.toml"), turned).unwrap();
}

#[test]
fn scoring_a_synthetic_blind_set_leaves_the_sets_tree_unchanged_and_writes_only_aggregates() {
    let items = ["itm-alpha", "itm-bravo"];
    let (judged, tiny) = world_with("gate-blind", &items);
    let labels = judged.world.beside("owner-labels");
    blinded(&tiny, &labels, &items, 1);
    let gate = frozen_gate(&judged.world.beside("gates"), &gate_matrix());
    let (sets_before, labels_before) = (tree(&tiny.root), tree(&labels));
    // A receipt asked for under the sets' folder or the labels' folder is refused before
    // anything runs.
    for inside in [
        tiny.root.join("receipt.json"),
        labels.join(SET).join("r.json"),
    ] {
        let (refused, sent) = gated(&judged, &tiny, &gate, &labels, &inside, Vec::new());
        assert!(refused.unwrap_err().contains("not written under"));
        assert_eq!(sent, 0);
    }
    // A receipt where its folder cannot be resolved is refused: the check fails closed.
    let nowhere = judged.world.beside("no-such-folder/receipt.json");
    let (refused, sent) = gated(&judged, &tiny, &gate, &labels, &nowhere, Vec::new());
    assert!(refused.unwrap_err().contains("cannot be resolved"));
    assert_eq!(sent, 0);
    // A blind set's labels are never read from under the sets' folder, even as the very
    // bytes its seal names, nor through a link that resolves there.
    let private = tiny.root.join("private");
    std::fs::create_dir_all(private.join(SET)).unwrap();
    std::fs::copy(
        labels.join(SET).join("labels.toml"),
        private.join(SET).join("labels.toml"),
    )
    .unwrap();
    let linked = judged.world.beside("linked-labels");
    std::os::unix::fs::symlink(&private, &linked).unwrap();
    let elsewhere = judged.world.beside("elsewhere.json");
    for inside in [&private, &linked] {
        let (refused, sent) = gated(&judged, &tiny, &gate, inside, &elsewhere, Vec::new());
        let refused = refused.unwrap_err();
        assert!(
            refused.contains("never read from under the sets' folder"),
            "{refused}"
        );
        assert_eq!(sent, 0);
    }
    std::fs::remove_file(&linked).unwrap();
    std::fs::remove_dir_all(&private).unwrap();
    // Labels that are not the ones the set seals are refused before anything runs.
    let wrong = judged.world.beside("wrong-labels");
    blinded_copy(&labels, &wrong);
    let (refused, sent) = gated(&judged, &tiny, &gate, &wrong, &elsewhere, Vec::new());
    let refused = refused.unwrap_err();
    assert!(
        refused.contains("do not hash to the set's labels_sha256"),
        "{refused}"
    );
    assert_eq!(sent, 0);
    // Cells in order: each item under the control, then the treatment, which catches the
    // fail the control passed.
    let (pass, fail) = (
        submission("held", "Kept."),
        submission("violation", "Gone."),
    );
    let answers = vec![pass.clone(), fail, pass.clone(), pass];
    let receipt = judged.world.beside("receipt.json");
    let (written, sent) = gated(&judged, &tiny, &gate, &labels, &receipt, answers);
    let written = written.unwrap();
    assert_eq!(sent, 4);
    let paired = &written.scored.judges[0].paired;
    assert_eq!(
        (
            paired.control.counts.detected,
            paired.treatment.counts.detected
        ),
        (0, 1)
    );
    assert_eq!(paired.control.balanced_accuracy, Some(0.5));
    assert_eq!(paired.treatment.balanced_accuracy, Some(1.0));
    // Nothing in the sets' tree or the labels' folder changed, and nothing was added.
    assert_eq!(tree(&tiny.root), sets_before);
    assert_eq!(tree(&labels), labels_before);
    // The receipt holds aggregates and digests: no item, family, label word or verdict.
    let text = std::fs::read_to_string(&receipt).unwrap();
    for word in items
        .iter()
        .chain(&["house", "as drawn", "Kept", "Gone", "verdict"])
    {
        assert!(!text.contains(word), "{word}");
    }
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let keys: Vec<&String> = json.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        [
            "cells",
            "control",
            "gate_sha256",
            "judges",
            "labels_sha256",
            "matrix",
            "matrix_identity",
            "outcome",
            "receipt_version",
            "sets",
            "treatment"
        ]
    );
    let labels_digest = Digest::of_bytes(&labels_before.values().next().unwrap().clone()).hex();
    assert_eq!(json["labels_sha256"][SET], labels_digest.as_str());
    assert!(!std::fs::exists(judged.world.beside("receipt.json.partial")).unwrap());
}

#[test]
fn a_gate_with_too_few_families_is_inconclusive_in_its_receipt() {
    let (judged, tiny) = world_with("gate-few", &["a", "b"]);
    let gate = frozen_gate(&judged.world.beside("gates"), &gate_matrix());
    let receipt = judged.world.beside("receipt.json");
    let answers = vec![submission("held", "Kept."); 4];
    // A set that publishes its labels is scored by its own committed labels file, where it
    // is: the labels' folder given holds nothing of it.
    let empty = judged.world.beside("empty-labels");
    std::fs::create_dir_all(&empty).unwrap();
    let (written, _) = gated(&judged, &tiny, &gate, &empty, &receipt, answers);
    let written = written.unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&receipt).unwrap()).unwrap();
    // One family of the twenty the gate requires: inconclusive, for the judge and the gate.
    assert_eq!(json["outcome"], "inconclusive");
    let judge = &json["judges"][0];
    assert_eq!(judge["outcome"], "inconclusive");
    assert_eq!(judge["paired"]["families"], 1);
    assert!(
        judge["failed"]
            .as_array()
            .unwrap()
            .contains(&"families".into()),
        "{judge}"
    );
    assert_eq!(written.cells, 4);
}

#[test]
fn the_details_first_gate_holds_together_with_its_matrix() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let frozen = gating::frozen(&repository.join("gates/details-first.toml")).unwrap();
    let m = &frozen.matrix.matrix;
    assert_eq!(m.sets.len(), 5);
    let repeated: Vec<(&str, u32)> = (m.judges.iter())
        .map(|j| (j.model.as_str(), j.repeated(m)))
        .collect();
    assert_eq!(
        repeated,
        [
            ("claude-haiku-5-5", 3),
            ("gemini-3.8-flash", 1),
            ("gpt-6.1-terra", 1)
        ]
    );
    // One hypothesis a judge.
    assert_eq!(frozen.gate.confidence.hypotheses, m.judges.len());
}

#[test]
fn the_unsure_gates_hold_together_with_their_matrices() {
    // Plan m3, task U1, item 6: one gate of the second look a use case, each read as
    // `eval run --gate` reads it.
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for use_case in whizbang_eval::matrix::USE_CASES {
        let file = repository.join(format!("gates/unsure-{use_case}.toml"));
        let frozen = gating::frozen(&file).unwrap_or_else(|e| panic!("{use_case}: {e}"));
        assert_eq!(
            frozen.gate.confidence.hypotheses,
            frozen.matrix.matrix.judges.len()
        );
    }
}
