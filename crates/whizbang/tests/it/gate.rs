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

/// How many attempts of cells the store holds of the matrix of this identity, made under
/// the gate of this digest or under none: read in the process, with no binary started.
fn attempts(root: &Path, matrix: &str, gate: Option<&str>) -> usize {
    let identity: String = Matrix::parse(matrix)
        .unwrap()
        .identity()
        .chars()
        .take(16)
        .collect();
    let found = whizbang::evaluation::attempts_under(root, &identity, gate).unwrap();
    found.values().map(Vec::len).sum()
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
    // Each run holds the gate's digest; the matrix's own runs, without a gate, are none.
    let root = &judged.world.root;
    assert_eq!(attempts(root, &gate_matrix(), Some(&digest)), 4);
    assert_eq!(attempts(root, &gate_matrix(), None), 0);
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
    assert_eq!(attempts(root, &gate_matrix(), Some(&digest)), 4);
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

// --- gates frozen in their matrix (plan mE, task M1, items 3 and 4) ---------------------------

/// A gate file of version 3 over `matrix.toml` beside it: `baseline` its `[baseline]` lines,
/// `hypotheses` the count it corrects for.
pub fn gate_v3(baseline: &str, hypotheses: usize) -> String {
    format!(
        "gate_version = 3\n\n[run]\nmatrix = \"matrix.toml\"\n\n[baseline]\n{baseline}\n\
         [measure]\nprimary = \"balanced_accuracy\"\npass = \"pass\"\nfail = \"catch\"\n\
         unsure = \"neither\"\nerror = \"neither\"\nmissing_label = \"refuse\"\n\n\
         [confidence]\nlevel = 0.95\ncorrection = \"bonferroni\"\nhypotheses = {hypotheses}\n\n\
         [families]\nminimum = 20\nresampling = \"family\"\nseed = 20261125\nresamples = 200\n\n\
         [repetitions]\ncount = 1\ncombine = \"first\"\n\n\
         [admission]\nmin_gain = 0.03\nlower_bound_above = 0.0\n\n\
         [guardrails]\nfalse_reject_increase_max = 0.03\nerror_rate_max = 0.02\n\
         false_accept_increase_max = 0.03\n\n\
         [limits]\ncost_ratio_max = 2.0\nlatency_ratio_max = 2.0\n\n\
         [too_little_data]\noutcome = \"inconclusive\"\n"
    )
}

/// The `[baseline]` of two arms.
fn arms(control: &str, treatment: &str) -> String {
    format!("control = \"{control}\"\ntreatment = \"{treatment}\"\n")
}

/// Freezes these gate files in `dir`, then the matrix beside them naming each by its
/// digest: the matrix as read.
fn freeze(dir: &Path, matrix: &str, gates: &[(&str, String)]) -> Matrix {
    std::fs::create_dir_all(dir).unwrap();
    let mut named = String::from("\n[gates]\n");
    for (name, text) in gates {
        std::fs::write(dir.join(name), text).unwrap();
        let digest = Digest::of_bytes(text.as_bytes()).hex();
        named.push_str(&format!("\"{name}\" = \"{digest}\"\n"));
    }
    let text = format!("{matrix}{named}");
    std::fs::write(dir.join("matrix.toml"), &text).unwrap();
    Matrix::parse(&text).unwrap()
}

/// The matrix of the default judge of Haiku over these sets and arms, one run at a time.
fn wide(sets: &[&str], arms: &[&str]) -> String {
    let quoted = |names: &[&str]| -> String {
        let all: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
        all.join(", ")
    };
    format!(
        "[matrix]\nname = \"wide\"\nsets = [{}]\narms = [{}]\nrepetitions = 1\nconcurrency = 1\n\
         [[matrix.judges]]\nbackend = \"default\"\nmodel = \"claude-haiku-5-5\"\n",
        quoted(sets),
        quoted(arms)
    )
}

#[test]
fn a_gate_the_matrix_does_not_name_is_refused() {
    let world = crate::wired::World::new("gate-unnamed");
    let dir = world.beside("gates");
    let gate_text = gate_v3(&arms("C", "C+resize"), 1);
    // A matrix that names no gate: the gate was not frozen in it.
    freeze(&dir, &wide(&[SET], &["C", "C+resize"]), &[]);
    std::fs::write(dir.join("tool.toml"), &gate_text).unwrap();
    let refused = gating::frozen(&dir.join("tool.toml")).unwrap_err();
    assert!(
        refused.contains("does not name this gate file"),
        "{refused}"
    );
    // Named, it holds.
    freeze(
        &dir,
        &wide(&[SET], &["C", "C+resize"]),
        &[("tool.toml", gate_text.clone())],
    );
    let frozen = gating::frozen(&dir.join("tool.toml")).unwrap();
    assert!(frozen.scored_from_its_matrix());
    assert_eq!(frozen.comparisons.len(), 1);
    // A gate changed after its matrix named it is refused, and so is the matrix, by
    // `eval matrix`, before anything runs.
    std::fs::write(
        dir.join("tool.toml"),
        gate_text.replace("seed = 20261125", "seed = 1"),
    )
    .unwrap();
    let refused = gating::frozen(&dir.join("tool.toml")).unwrap_err();
    assert!(
        refused.contains("does not name this gate file"),
        "{refused}"
    );
    let matrix_text = std::fs::read_to_string(dir.join("matrix.toml")).unwrap();
    let matrix = Matrix::parse(&matrix_text).unwrap();
    let refused = gating::gates_on(&dir.join("matrix.toml"), &matrix).unwrap_err();
    assert!(
        refused.contains("changed after it was written"),
        "{refused}"
    );
    // A named gate with an arm the matrix lacks, or version 2's identity, is refused.
    for (text, says) in [
        (gate_v3(&arms("C", "C+outline"), 1), "does not name so"),
        (
            gate_v3(&arms("C", "C+resize"), 1).replace(
                "matrix = \"matrix.toml\"",
                "matrix = \"matrix.toml\"\nmatrix_identity = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"",
            ),
            "by file alone",
        ),
    ] {
        freeze(&dir, &wide(&[SET], &["C", "C+resize"]), &[("tool.toml", text)]);
        let refused = gating::frozen(&dir.join("tool.toml")).unwrap_err();
        assert!(refused.contains(says), "{refused}");
    }
    // The gates of version 2 are read as before: their matrix names no gate and lists no
    // cells. One that does takes gates of version 3.
    let v2 = frozen_gate(&world.beside("v2"), &gate_matrix());
    assert!(!gating::frozen(&v2).unwrap().scored_from_its_matrix());
    let listed = gate_matrix().replace(
        "model = \"claude-haiku-5-5\"\n",
        "model = \"claude-haiku-5-5\"\narms = [\"C\", \"C+resize\"]\n",
    );
    let v2 = frozen_gate(&world.beside("v2-listed"), &listed);
    let refused = gating::frozen(&v2).unwrap_err();
    assert!(refused.contains("takes gates of version 3"), "{refused}");
}

#[test]
fn gates_on_one_matrix_count_their_hypotheses_together() {
    let world = crate::wired::World::new("gate-hypotheses");
    let dir = world.beside("gates");
    // Two judges; Flash runs arm C alone, so a gate of a tool compares Haiku alone, and a
    // gate of the format both: three hypotheses over the matrix.
    let matrix = format!(
        "{}[[matrix.judges]]\nbackend = \"flash\"\nmodel = \"gemini-3.8-flash\"\n\
         arms = [\"C\", \"C;submission=v2\"]\n",
        wide(&[SET], &["C", "C+resize", "C;submission=v2"])
    );
    let tool = |n: usize| gate_v3(&arms("C", "C+resize"), n);
    let format = |n: usize| gate_v3(&arms("C", "C;submission=v2"), n);
    freeze(
        &dir,
        &matrix,
        &[("tool.toml", tool(3)), ("format.toml", format(3))],
    );
    let frozen = gating::frozen(&dir.join("tool.toml")).unwrap();
    let judges: Vec<&str> = (frozen.comparisons.iter())
        .map(|c| c.backend.as_str())
        .collect();
    assert_eq!(judges, ["default"]);
    let frozen = gating::frozen(&dir.join("format.toml")).unwrap();
    assert_eq!(frozen.comparisons.len(), 2);
    let (_, hypotheses) = gating::gates_on(&dir.join("matrix.toml"), &frozen.matrix).unwrap();
    assert_eq!(hypotheses, 3);
    // A gate that counts its own comparisons alone is refused, and so is every gate on the
    // matrix, which `eval matrix` will not run.
    for (one, other) in [(1, 3), (3, 2)] {
        let m = freeze(
            &dir,
            &matrix,
            &[("tool.toml", tool(one)), ("format.toml", format(other))],
        );
        for gate in ["tool.toml", "format.toml"] {
            let refused = gating::frozen(&dir.join(gate)).unwrap_err();
            assert!(refused.contains("compare 3"), "{refused}");
        }
        assert!(gating::gates_on(&dir.join("matrix.toml"), &m).is_err());
    }
    // A gate whose repetitions are not the most of the cells it reads is refused.
    let twice = matrix.replace(
        "arms = [\"C\", \"C;submission=v2\"]",
        "arms = [{ arm = \"C\", repetitions = 2 }, \"C;submission=v2\"]",
    );
    freeze(
        &dir,
        &twice,
        &[("tool.toml", tool(3)), ("format.toml", format(3))],
    );
    let refused = gating::frozen(&dir.join("tool.toml")).unwrap_err();
    assert!(
        refused.contains("repetitions are 1, and the most of any cell it reads are 2"),
        "{refused}"
    );
}

// --- the scorer of a frozen gate, on outcomes made in the test --------------------------------

use whizbang::gating::{Decided, Frozen, Labelled, Read};
use whizbang_eval::matrix::Cell;
use whizbang_eval::metrics::{Computed, Expected};

/// What each set holds, as the scorer reads it: by set id, each item with its family and its
/// label.
type Holding<'a> = &'a [(&'a str, &'a [(&'a str, &'a str, Expected)])];

/// What scoring reads, made in the test: the sets as given, every cell of the matrix, the
/// gate's and the others, decided as `decide` says, and left open where it says nothing.
fn read_of(frozen: &Frozen, sets: Holding<'_>, decide: &dyn Fn(&Cell) -> Option<Computed>) -> Read {
    let mut labelled = Labelled::default();
    let mut items = Vec::new();
    for (set, its) in sets {
        for (item, family, expected) in *its {
            let key = ((*set).to_owned(), (*item).to_owned());
            labelled.expected.insert(key.clone(), *expected);
            labelled.families.insert(key, (*family).to_owned());
        }
        labelled
            .digests
            .insert((*set).to_owned(), format!("digest-of-{set}"));
        items.push((
            (*set).to_owned(),
            its.iter().map(|(item, _, _)| (*item).to_owned()).collect(),
        ));
    }
    let mut ended = BTreeMap::new();
    for cell in frozen.matrix.cells(&items) {
        if let Some(computed) = decide(&cell) {
            let decided = Decided {
                computed,
                cost_usd: 0.01,
                wall_ms: 1_000,
                // 2026-11-25, at noon.
                ended_ms: 1_795_608_000_000,
            };
            ended.insert(cell, decided);
        }
    }
    let splits = (items.iter())
        .map(|(set, _)| (set.clone(), "dev".to_owned()))
        .collect();
    Read {
        items,
        labelled,
        splits,
        ended,
    }
}

/// A matrix of two judges over `u/one` and `u/two`, which runs arm C alone, with these arms;
/// Flash runs neither `C+resize` nor `u/two`.
fn two_judges(arms: &[&str]) -> String {
    format!(
        "{}[[matrix.judges]]\nbackend = \"flash\"\nmodel = \"gemini-3.8-flash\"\n\
         arms = [\"C\", \"C+view\"]\n\n[matrix.set.\"source-fidelity/two\"]\narms = [\"C\"]\n\
         judges = [\"default\"]\n",
        wide(&["source-fidelity/one", "source-fidelity/two"], arms)
    )
}

#[test]
fn a_gate_reads_its_two_arms_of_a_wider_matrix() {
    use Expected::Pass as P;
    let world = crate::wired::World::new("gate-wide");
    let dir = world.beside("gates");
    freeze(
        &dir,
        &two_judges(&["C", "C+resize", "C+view"]),
        &[(
            "tool.toml",
            gate_v3(&arms("C", "C+resize"), 1)
                + "\n[admits]\nrung = 4\nkind = \"tool\"\nname = \"resize\"\n",
        )],
    );
    let frozen = gating::frozen(&dir.join("tool.toml")).unwrap();
    // Only the default judge runs both arms, and only on the first set: the second's table
    // gives it arm C alone.
    assert_eq!(frozen.comparisons.len(), 1);
    assert_eq!(frozen.comparisons[0].sets, ["source-fidelity/one"]);
    assert_eq!(frozen.sets(), ["source-fidelity/one"]);
    let one: &[(&str, &str, Expected)] = &[("a", "f1", P), ("b", "f1", P), ("c", "f2", P)];
    let sets: Holding<'_> = &[("source-fidelity/one", one), ("source-fidelity/two", one)];
    // The third arm fails everything, as does every cell of Flash and of the second set; the
    // treatment fails b alone.
    let decide = |cell: &Cell| {
        Some(match (cell.arm.as_str(), cell.item.as_str()) {
            ("C+resize", "b") | ("C+view", _) => Computed::Fail,
            _ if cell.backend == "flash" || cell.set.ends_with("two") => Computed::Fail,
            _ => Computed::Pass,
        })
    };
    let read = read_of(&frozen, sets, &decide);
    let receipt = gating::score_read(&frozen, &read).unwrap();
    assert_eq!(receipt.cells, 6);
    assert_eq!(receipt.sets, ["source-fidelity/one"]);
    let digests: Vec<&String> = receipt.labels_sha256.keys().collect();
    assert_eq!(digests, ["source-fidelity/one"]);
    assert_eq!(receipt.receipt_version, 2);
    assert_eq!((receipt.arm.as_deref(), receipt.relabelled), (None, None));
    // What `eval admitted` reads: the gate's rung and what it admits, its sets' splits, and
    // the day its last cell ended. A dev set admits nothing, whatever the outcome.
    assert_eq!(receipt.date.as_deref(), Some("2026-11-25"));
    let splits: Vec<(&String, &String)> = receipt.splits.iter().collect();
    assert_eq!(
        splits,
        [(&"source-fidelity/one".to_owned(), &"dev".to_owned())]
    );
    assert_eq!(receipt.admits.as_ref().map(|a| a.rung), Some(4));
    let json = serde_json::to_value(&receipt).unwrap();
    assert_eq!(whizbang::admitted::admits(&"00".repeat(32), &json), None);
    let mut blind = json.clone();
    blind["splits"]["source-fidelity/one"] = "heldout".into();
    blind["outcome"] = "admitted".into();
    let admitted = whizbang::admitted::admits(&"00".repeat(32), &blind).unwrap();
    assert_eq!(
        (admitted.kind.as_str(), admitted.name.as_str()),
        ("tool", "resize")
    );
    assert_eq!(receipt.scored.judges.len(), 1);
    let paired = &receipt.scored.judges[0].paired;
    assert_eq!(
        (
            paired.items,
            paired.families,
            paired.control.counts.false_rejects,
            paired.treatment.counts.false_rejects
        ),
        (3, 2, 0, 1)
    );
    // A cell of the gate left open stops the scoring; one outside it does not.
    let open = |cell: &Cell| {
        (cell.arm != "C+resize" || cell.item != "c")
            .then(|| decide(cell))
            .flatten()
    };
    let refused = gating::score_read(&frozen, &read_of(&frozen, sets, &open)).unwrap_err();
    assert!(
        refused.contains("1 of the gate's 6 cells have not ended"),
        "{refused}"
    );
    let mut outside = read.clone();
    outside
        .ended
        .retain(|cell, _| cell.arm != "C+view" && cell.backend != "flash");
    assert_eq!(gating::score_read(&frozen, &outside).unwrap(), receipt);
    // An item with no label stops it too.
    let mut unlabelled = read;
    unlabelled
        .labelled
        .expected
        .remove(&("source-fidelity/one".to_owned(), "a".to_owned()));
    let refused = gating::score_read(&frozen, &unlabelled).unwrap_err();
    assert!(
        refused.contains("2 cells are of items with no label"),
        "{refused}"
    );
}

/// The `[baseline]` of `u/one` against `u/two` under arm C.
fn one_against_two() -> String {
    "arm = \"C\"\ncontrol_set = \"source-fidelity/one\"\ntreatment_set = \"source-fidelity/two\"\n"
        .to_owned()
}

#[test]
fn a_gate_compares_two_versions_of_a_set_item_by_item() {
    use Expected::{Fail as F, Pass as P};
    let world = crate::wired::World::new("gate-versions");
    let dir = world.beside("gates");
    freeze(
        &dir,
        &two_judges(&["C", "C+view"]),
        &[("requirements.toml", gate_v3(&one_against_two(), 1))],
    );
    let frozen = gating::frozen(&dir.join("requirements.toml")).unwrap();
    // The default judge runs C on both sets; Flash is not given the second.
    let judges: Vec<&str> = frozen
        .comparisons
        .iter()
        .map(|c| c.backend.as_str())
        .collect();
    assert_eq!(judges, ["default"]);
    let items: &[(&str, &str, Expected)] = &[("a", "f1", P), ("b", "f1", F), ("c", "f2", P)];
    let sets: Holding<'_> = &[
        ("source-fidelity/one", items),
        ("source-fidelity/two", items),
    ];
    // The second version catches b, which the first passed, and fails c.
    let decide = |cell: &Cell| {
        let two = cell.set.ends_with("two");
        Some(match cell.item.as_str() {
            "b" if two => Computed::Fail,
            "c" if two => Computed::Fail,
            _ => Computed::Pass,
        })
    };
    let receipt = gating::score_read(&frozen, &read_of(&frozen, sets, &decide)).unwrap();
    assert_eq!(receipt.cells, 6);
    assert_eq!(receipt.arm.as_deref(), Some("C"));
    assert_eq!(receipt.relabelled, Some(0));
    assert_eq!(
        (receipt.control.as_str(), receipt.treatment.as_str()),
        ("source-fidelity/one", "source-fidelity/two")
    );
    let paired = &receipt.scored.judges[0].paired;
    assert_eq!((paired.items, paired.families), (3, 2));
    assert_eq!(
        (
            paired.control.counts.detected,
            paired.treatment.counts.detected
        ),
        (0, 1)
    );
    assert_eq!(paired.treatment.counts.false_rejects, 1);
    // Versions whose families or items differ by id are not two versions of one set.
    let moved: &[(&str, &str, Expected)] = &[("a", "f1", P), ("b", "f2", F), ("c", "f2", P)];
    let other: &[(&str, &str, Expected)] = &[("a", "f1", P), ("b", "f1", F), ("d", "f2", P)];
    for second in [moved, other] {
        let sets: Holding<'_> = &[
            ("source-fidelity/one", items),
            ("source-fidelity/two", second),
        ];
        let refused = gating::score_read(&frozen, &read_of(&frozen, sets, &decide)).unwrap_err();
        assert!(refused.contains("not two versions of one set"), "{refused}");
    }
}

#[test]
fn an_item_whose_label_changed_between_versions_is_reported_apart() {
    use Expected::{Fail as F, Pass as P};
    let world = crate::wired::World::new("gate-relabelled");
    let dir = world.beside("gates");
    freeze(
        &dir,
        &two_judges(&["C"]).replace("\"C\", \"C+view\"", "\"C\""),
        &[("requirements.toml", gate_v3(&one_against_two(), 1))],
    );
    let frozen = gating::frozen(&dir.join("requirements.toml")).unwrap();
    let first: &[(&str, &str, Expected)] = &[("a", "f1", P), ("b", "f1", P), ("c", "f2", P)];
    let second: &[(&str, &str, Expected)] = &[("a", "f1", P), ("b", "f1", F), ("c", "f2", P)];
    let sets: Holding<'_> = &[
        ("source-fidelity/one", first),
        ("source-fidelity/two", second),
    ];
    // Every decision a fail: under either version's truth, b would count differently.
    let receipt =
        gating::score_read(&frozen, &read_of(&frozen, sets, &|_| Some(Computed::Fail))).unwrap();
    assert_eq!(receipt.relabelled, Some(1));
    // Every cell was read, b's among them; a and c alone are compared.
    assert_eq!(receipt.cells, 6);
    let paired = &receipt.scored.judges[0].paired;
    assert_eq!(paired.items, 2);
    assert_eq!(
        (
            paired.control.counts.expected_fail,
            paired.treatment.counts.expected_fail
        ),
        (0, 0)
    );
    assert_eq!(paired.treatment.counts.false_rejects, 2);
    let printed = gating::text(&receipt);
    assert!(
        printed.contains("items whose label differs between the versions, scored under neither: 1"),
        "{printed}"
    );
}

#[test]
fn two_versions_whose_families_differ_are_refused_before_the_matrix_runs() {
    // Their ids match; one item's family does not.
    let world = crate::wired::World::new("gate-families");
    let dir = world.beside("set");
    let tiny = crate::fetch::tiny(&dir, &["a", "b"]);
    let second = "source-fidelity/tiny-v2";
    crate::fetch::tiny_of(
        &dir,
        second,
        "source-fidelity",
        &["candidate", "source"],
        &["a", "b"],
    );
    let gates_dir = world.beside("gates");
    let baseline = format!("arm = \"C\"\ncontrol_set = \"{SET}\"\ntreatment_set = \"{second}\"\n");
    let matrix = freeze(
        &gates_dir,
        &wide(&[SET, second], &["C"]),
        &[("requirements.toml", gate_v3(&baseline, 1))],
    );
    let file = gates_dir.join("matrix.toml");
    let (gates, _) = gating::gates_on(&file, &matrix).unwrap();
    gating::versions_hold(&tiny.root, &gates).unwrap();
    let set = tiny.root.join(second);
    let manifest = std::fs::read_to_string(set.join("set.toml")).unwrap();
    std::fs::write(
        set.join("set.toml"),
        manifest.replace(
            "id = \"b\"\nfamily = \"house\"",
            "id = \"b\"\nfamily = \"barn\"",
        ),
    )
    .unwrap();
    let families = std::fs::read_to_string(set.join("families.toml")).unwrap();
    std::fs::write(
        set.join("families.toml"),
        format!("{families}\n[[family]]\nid = \"barn\"\ndescription = \"a barn\"\n"),
    )
    .unwrap();
    let refused = gating::versions_hold(&tiny.root, &gates).unwrap_err();
    assert!(refused.contains("not two versions of one set"), "{refused}");
    assert!(refused.contains("requirements.toml"), "{refused}");
}

#[test]
fn several_gates_on_one_listed_matrix_score_from_its_runs_alone() {
    // One world, end to end: two versions of a set, b relabelled in the second; two arms on
    // the first, arm C alone on the second; a gate of a tool and a gate of the versions,
    // two hypotheses between them.
    let judged = Judged::new("gate-listed");
    let dir = judged.world.beside("set");
    let items = ["a", "b"];
    let tiny = crate::fetch::tiny(&dir, &items);
    let second = "source-fidelity/tiny-v2";
    crate::fetch::tiny_of(
        &dir,
        second,
        "source-fidelity",
        &["candidate", "source"],
        &items,
    );
    let labels = tiny.root.join(second).join("labels.toml");
    let text = std::fs::read_to_string(&labels).unwrap();
    std::fs::write(
        &labels,
        text.replace(
            "item = \"b\"\nexpected = \"pass\"",
            "item = \"b\"\nexpected = \"fail\"",
        ),
    )
    .unwrap();
    crate::fetch::fetched(&tiny, &judged.world.root);
    crate::fetch::fetched_of(&tiny, &judged.world.root, second);
    let gates = judged.world.beside("gates");
    let baseline = format!("arm = \"C\"\ncontrol_set = \"{SET}\"\ntreatment_set = \"{second}\"\n");
    let matrix_text = format!(
        "{}\n[matrix.set.\"{second}\"]\narms = [\"C\"]\n",
        wide(&[SET, second], &["C", "C+resize"])
    );
    let matrix = freeze(
        &gates,
        &matrix_text,
        &[
            ("tool.toml", gate_v3(&arms("C", "C+resize"), 2)),
            ("requirements.toml", gate_v3(&baseline, 2)),
        ],
    );
    let (named, hypotheses) = gating::gates_on(&gates.join("matrix.toml"), &matrix).unwrap();
    assert_eq!(hypotheses, 2);
    gating::versions_hold(&tiny.root, &named).unwrap();
    // Only the six cells the matrix names run: the first set under two arms, the second under
    // C. Each answer in order: a and b of the first set, each under C and C+resize; then a
    // and b of the second under C.
    let (pass, fail) = (
        submission("held", "Kept."),
        submission("violation", "Gone."),
    );
    let mut answers = Vec::new();
    for item in items {
        answers.push(pass.clone());
        answers.push(if item == "b" {
            fail.clone()
        } else {
            pass.clone()
        });
    }
    answers.extend([pass.clone(), fail]);
    let (report, log) = crate::matrix::run_matrix(&judged, answers, &|e| {
        crate::matrix::ran(
            e,
            &judged,
            &tiny,
            &matrix,
            20_000_000,
            &crate::matrix::never,
        )
    });
    assert_eq!(report.unwrap().cells, 6);
    assert_eq!(log.lock().unwrap().len(), 6);
    let root = &judged.world.root;
    let identity: String = matrix.identity().chars().take(16).collect();
    let ran = whizbang::evaluation::attempts_under(root, &identity, None).unwrap();
    assert_eq!(ran.len(), 6);
    // Each gate reads its own cells; `eval run --gate` runs nothing, under an executor that
    // reaches no judge.
    let executor = judged.runner(&judged.over("https://127.0.0.1:9/v1/messages", None));
    let running = whizbang::evaluation::Running {
        executor: &executor,
        root,
        left_micros: 0,
        stop: &|| false,
    };
    let score = |gate: &str| {
        let receipt = judged.world.beside(&format!("{gate}.json"));
        let gating = Gating {
            file: &gates.join(gate),
            sets_root: &tiny.root,
            labels_root: &tiny.root,
            receipt: &receipt,
        };
        let written = gating::run(&gating, &running).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&receipt).unwrap()).unwrap();
        (written, json)
    };
    let (tool, json) = score("tool.toml");
    assert_eq!(
        (tool.cells, tool.sets.as_slice()),
        (4, [SET.to_owned()].as_slice())
    );
    let paired = &tool.scored.judges[0].paired;
    assert_eq!(
        (
            paired.control.counts.false_rejects,
            paired.treatment.counts.false_rejects
        ),
        (0, 1)
    );
    assert!(json.get("arm").is_none() && json.get("relabelled").is_none());
    let (versions, json) = score("requirements.toml");
    assert_eq!((versions.cells, versions.relabelled), (4, Some(1)));
    assert_eq!(json["arm"], "C");
    let paired = &versions.scored.judges[0].paired;
    assert_eq!(
        (
            paired.items,
            paired.control.counts.false_rejects,
            paired.treatment.counts.false_rejects
        ),
        (1, 0, 0)
    );
    drop(executor);
    assert_eq!(
        whizbang::evaluation::attempts_under(root, &identity, None)
            .unwrap()
            .len(),
        6
    );
    // The receipts hold aggregates and digests only, and no path of the sets' folder, which
    // a round names by `--root <owner storage>` (plan mE, task M1, item 11).
    let roots = [
        tiny.root.display().to_string(),
        tiny.root.canonicalize().unwrap().display().to_string(),
    ];
    for gate in ["tool.toml", "requirements.toml"] {
        let text = std::fs::read_to_string(judged.world.beside(&format!("{gate}.json"))).unwrap();
        for word in [
            "\"a\"", "\"b\"", "house", "Kept", "Gone", &roots[0], &roots[1],
        ] {
            assert!(!text.contains(word), "{gate}: {word}");
        }
    }
    // `eval judges` reads the runs of a matrix that names its gates, which carry none, and
    // with `--set` scores the cells the matrix names for that set alone (item 6).
    let labels = judged.world.beside("labels");
    std::fs::create_dir_all(&labels).unwrap();
    let out = judged.world.beside("judges.toml");
    let files = [gates.join("matrix.toml")];
    let only = [SET.to_owned()];
    let reading = whizbang::judges::Reading {
        matrices: &files,
        store: root,
        sets_root: &tiny.root,
        labels_root: &labels,
        out: &out,
        mode: whizbang::judges::Mode::Judges,
        arm: Some("C"),
        rule: None,
        sets: &only,
        commit: "0000000000000000000000000000000000000000",
    };
    let written = whizbang::judges::run(&reading).unwrap();
    assert_eq!(
        (
            written.scored.cells,
            written.scored.decisions,
            written.scored.open
        ),
        (4, 4, 0)
    );
    let record = std::fs::read_to_string(&out).unwrap();
    assert!(!record.contains(&roots[0]) && !record.contains(&roots[1]));
}

#[test]
fn a_matrix_counts_the_judge_rules_hypotheses_beside_its_gates() {
    // Plan mE, task M1, item 6: a round's correction counts its gates' comparisons and the
    // judge's rule's; the matrix states the whole, and every gate corrects for it at the
    // resamples it needs.
    let world = crate::wired::World::new("gate-declared");
    let dir = world.beside("gates");
    let matrix = format!("hypotheses = 21\n{}", wide(&[SET], &["C", "C+resize"]));
    let at = |n: usize, resamples: usize| {
        gate_v3(&arms("C", "C+resize"), n)
            .replace("resamples = 200", &format!("resamples = {resamples}"))
    };
    freeze(&dir, &matrix, &[("tool.toml", at(21, 20_000))]);
    let frozen = gating::frozen(&dir.join("tool.toml")).unwrap();
    let (_, compared) = gating::gates_on(&dir.join("matrix.toml"), &frozen.matrix).unwrap();
    assert_eq!(compared, 1);
    for (text, says) in [
        (at(1, 20_000), "compare 21"),
        (at(21, 2_000), "resamples 2000 times"),
    ] {
        freeze(&dir, &matrix, &[("tool.toml", text)]);
        let refused = gating::frozen(&dir.join("tool.toml")).unwrap_err();
        assert!(refused.contains(says), "{refused}");
    }
    // A matrix that states fewer than its gates compare is refused.
    let few = format!(
        "hypotheses = 1\n{}",
        wide(&[SET], &["C", "C+resize", "C+view"])
    );
    let gates = [
        ("tool.toml", at(1, 20_000)),
        (
            "view.toml",
            gate_v3(&arms("C", "C+view"), 1).replace("resamples = 200", "resamples = 20000"),
        ),
    ];
    freeze(&dir, &few, &gates);
    let refused = gating::frozen(&dir.join("tool.toml")).unwrap_err();
    assert!(
        refused.contains("counts 1 hypotheses, and its gates compare 2"),
        "{refused}"
    );
}
