//! The baseline gate (plan 2C, task E2, item 7) on the synthetic replay fixtures of item 3
//! (`fixtures/baseline/synthetic/`): three decisions captured from frozen squint by
//! `scripts/baseline/capture/`, each calling `view` and `align` on synthetic pictures, one
//! with a response cut at its token limit. Never the owner's package.

use std::path::{Path, PathBuf};

use whizbang::baseline::{Asked, Tally, receipt, run};
use whizbang_testkit::scratch::Scratch;

/// A commit of the right form: the gate records it and does not look it up.
pub(crate) const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

pub(crate) fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/baseline/synthetic")
}

fn gate(transcripts: &Path, receipt_at: &Path) -> Tally {
    let package = fixtures().join("package");
    let sets = package.join("evalsets");
    let asked = Asked {
        package: &package,
        transcripts,
        receipt: receipt_at,
        sets: &sets,
        pictures: None,
        commit: COMMIT,
    };
    run(&asked).expect("the gate runs")
}

/// Copies a folder, every file in it at any depth.
fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn each_synthetic_decision_replayed_through_the_loop_preparation_and_tools_compares_with_no_difference()
 {
    let scratch = Scratch::new("baseline-same");
    let at = scratch.path().join("receipt.toml");
    let tally = gate(&fixtures().join("transcripts"), &at);
    assert_eq!(
        (
            tally.fixtures,
            tally.transcripts,
            tally.same,
            tally.differed,
            tally.refused
        ),
        (3, 3, 3, 0, 0),
        "{}",
        tally.text()
    );
    assert!(tally.passed());
    let written = std::fs::read_to_string(&at).unwrap();
    assert!(written.ends_with("result = \"pass\"\n"), "{written}");
}

#[test]
fn a_replay_that_differs_at_one_place_fails_and_names_the_place() {
    let scratch = Scratch::new("baseline-differs");
    let transcripts = scratch.path().join("transcripts");
    copy(&fixtures().join("transcripts"), &transcripts);
    // One word of what squint's `view` answered, as the recorded call of the decision `kept`
    // holds it (the history of the next turn holds it too, and is left as it was): the
    // replay answers with squint's words, so it now differs there and only there.
    let kept = transcripts.join("historical/synthetic/kept");
    let file = std::fs::read_dir(&kept)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        text.matches("and the same of raw:source").count(),
        2,
        "one call, two turns"
    );
    let at = text.find("and the same of raw:source").unwrap();
    let mut changed = text.clone();
    changed.replace_range(at..at + 3, "but");
    std::fs::write(&file, changed).unwrap();

    let at = scratch.path().join("receipt.toml");
    let tally = gate(&transcripts, &at);
    assert_eq!((tally.same, tally.differed, tally.refused), (2, 1, 0));
    assert!(!tally.passed());
    let places: Vec<(&String, &usize)> = tally.places.iter().collect();
    assert_eq!(
        places,
        [(&"/turns/*/calls/*/result/content".to_owned(), &1)],
        "{}",
        tally.text()
    );
    assert!(
        tally
            .text()
            .contains("differences at /turns/*/calls/*/result/content: 1\n")
    );
    assert!(tally.text().ends_with("result: fail\n"));
    let written = std::fs::read_to_string(&at).unwrap();
    assert!(written.ends_with("result = \"fail\"\n"), "{written}");
}

#[test]
fn the_receipt_holds_exactly_the_four_fields() {
    // No transcripts: nothing is replayed, and a gate that compared nothing fails.
    let scratch = Scratch::new("baseline-receipt");
    let none = scratch.path().join("none");
    std::fs::create_dir(&none).unwrap();
    let at = scratch.path().join("receipt.toml");
    let tally = gate(&none, &at);
    assert_eq!((tally.fixtures, tally.transcripts), (3, 0));
    let written = std::fs::read_to_string(&at).unwrap();
    let digest = std::fs::read_to_string(fixtures().join("package/PACKAGE.digest")).unwrap();
    assert_eq!(
        written,
        format!(
            "package_digest = \"sha256:{}\"\ncomparison_version = \"{}\"\n\
             whizbang_commit = \"{COMMIT}\"\nresult = \"fail\"\n",
            digest.trim_end(),
            whizbang_eval::baseline::COMPARISON_VERSION
        )
    );
    let fields: Vec<&str> = written
        .lines()
        .map(|line| line.split(" = ").next().unwrap())
        .collect();
    assert_eq!(
        fields,
        [
            "package_digest",
            "comparison_version",
            "whizbang_commit",
            "result"
        ]
    );
    assert_eq!(
        receipt("sha256:00", COMMIT, true).lines().last(),
        Some("result = \"pass\"")
    );
}

#[test]
fn a_transcript_of_another_package_or_a_package_whose_listing_was_changed_is_refused() {
    let scratch = Scratch::new("baseline-refused");
    // A package whose listing no longer hashes to its digest is refused before anything.
    let package = scratch.path().join("package");
    copy(&fixtures().join("package"), &package);
    let listing = package.join("PACKAGE.sha256");
    let mut text = std::fs::read_to_string(&listing).unwrap();
    text.push('\n');
    std::fs::write(&listing, text).unwrap();
    let sets = package.join("evalsets");
    let transcripts = fixtures().join("transcripts");
    let at = scratch.path().join("receipt.toml");
    let asked = Asked {
        package: &package,
        transcripts: &transcripts,
        receipt: &at,
        sets: &sets,
        pictures: None,
        commit: COMMIT,
    };
    assert!(
        run(&asked)
            .unwrap_err()
            .contains("does not hash to its digest")
    );
    assert!(!at.exists());
    // A commit that is not forty hexadecimal digits is refused.
    let short = Asked {
        commit: "abc",
        package: &fixtures().join("package"),
        ..asked
    };
    assert!(run(&short).unwrap_err().contains("--commit"));
}

#[test]
fn a_package_that_spells_the_resize_otherwise_is_refused() {
    let scratch = Scratch::new("baseline-resize");
    let package = scratch.path().join("package");
    copy(&fixtures().join("package"), &package);
    let transcripts = scratch.path().join("transcripts");
    copy(&fixtures().join("transcripts"), &transcripts);
    // The synthetic package spells it as the 2026-11 package does.
    let file = "assembly/generated/preparation.json";
    let text = std::fs::read_to_string(package.join(file)).unwrap();
    let spelled = format!("\"resize\": \"{}\"", whizbang::baseline::RESIZE);
    assert_eq!(text.matches(&spelled).count(), 1, "{text}");
    // Spelled otherwise, with the listing, the digest and the transcripts made to agree,
    // so that the spelling is the one thing that changed.
    let changed = text.replace(&spelled, "\"resize\": \"lanczos3\"");
    std::fs::write(package.join(file), &changed).unwrap();
    let sha = |bytes: &[u8]| whizbang_core::identity::Digest::of_bytes(bytes).hex();
    let listing_at = package.join("PACKAGE.sha256");
    let listing = std::fs::read_to_string(&listing_at)
        .unwrap()
        .replace(&sha(text.as_bytes()), &sha(changed.as_bytes()));
    std::fs::write(&listing_at, &listing).unwrap();
    let old = std::fs::read_to_string(package.join("PACKAGE.digest")).unwrap();
    let new = sha(listing.as_bytes());
    std::fs::write(package.join("PACKAGE.digest"), format!("{new}\n")).unwrap();
    for set in ["cut-short", "kept", "moved"] {
        let folder = transcripts.join("historical/synthetic").join(set);
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let t = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, t.replace(old.trim_end(), &new)).unwrap();
        }
    }
    let (sets, at) = (
        package.join("evalsets"),
        scratch.path().join("receipt.toml"),
    );
    let asked = Asked {
        package: &package,
        transcripts: &transcripts,
        receipt: &at,
        sets: &sets,
        pictures: None,
        commit: COMMIT,
    };
    let tally = run(&asked).expect("the gate runs");
    assert_eq!((tally.same, tally.refused), (0, 3), "{}", tally.text());
    assert_eq!(
        tally.reasons.keys().collect::<Vec<_>>(),
        ["the model's preparation does not resize as whizbang does"]
    );
    assert!(!tally.passed());
}

/// The gate over one of the fixtures' packages (`synthetic` or `scaled`).
fn gate_over(which: &str, transcripts: &Path, receipt_at: &Path) -> Result<Tally, String> {
    let package = fixtures().join("..").join(which).join("package");
    let sets = package.join("evalsets");
    run(&Asked {
        package: &package,
        transcripts,
        receipt: receipt_at,
        sets: &sets,
        pictures: None,
        commit: COMMIT,
    })
}

/// A copy of a fixture's transcripts, each of them, or the first only, in version 2: what
/// `convert.py --transcript-version 2` writes, which differs from version 1 in that field
/// alone (`scripts/baseline/tests/test_baseline.py`, `Versions`).
fn in_version_2(which: &str, to: &Path, all: bool) {
    copy(&fixtures().join("..").join(which).join("transcripts"), to);
    let mut found = Vec::new();
    let mut folders = vec![to.join("historical")];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    for path in found.iter().take(if all { found.len() } else { 1 }) {
        let text = std::fs::read_to_string(path).unwrap();
        let line = "\n  \"transcript_version\": 1,\n";
        assert_eq!(text.matches(line).count(), 1);
        std::fs::write(path, text.replace(line, "\n  \"transcript_version\": 2,\n")).unwrap();
    }
}

#[test]
fn a_version_2_conversion_of_the_synthetic_package_replays_the_same() {
    let scratch = Scratch::new("baseline-v2");
    let transcripts = scratch.path().join("transcripts");
    in_version_2("synthetic", &transcripts, true);
    let tally = gate_over("synthetic", &transcripts, &scratch.path().join("r.toml")).unwrap();
    assert_eq!(
        (tally.version, tally.same, tally.differed),
        (Some(2), 3, 0),
        "{}",
        tally.text()
    );
    assert!(tally.text().contains("\ntranscript version: 2\n"));
    assert!(tally.passed());
}

#[test]
fn a_scaled_cut_differs_under_version_1_and_is_the_same_under_version_2() {
    let scratch = Scratch::new("baseline-scaled");
    let at = scratch.path().join("r.toml");
    // Version 1 compares the crop's prepared bytes, which squint's triangle filter made and
    // whizbang's mean does not; nothing else differs.
    let recorded = fixtures().join("../scaled/transcripts");
    let one = gate_over("scaled", &recorded, &at).unwrap();
    assert_eq!(
        (one.version, one.same, one.differed),
        (Some(1), 0, 1),
        "{}",
        one.text()
    );
    let places: Vec<&str> = one.places.keys().map(String::as_str).collect();
    assert_eq!(
        places,
        ["/images/*/prepared/bytes", "/images/*/prepared/sha256"]
    );
    // Version 2 leaves them out and compares the crop's size and mapping.
    let transcripts = scratch.path().join("v2");
    in_version_2("scaled", &transcripts, true);
    let two = gate_over("scaled", &transcripts, &at).unwrap();
    assert_eq!(
        (two.version, two.same, two.differed),
        (Some(2), 1, 0),
        "{}",
        two.text()
    );
    assert!(
        std::fs::read_to_string(&at)
            .unwrap()
            .ends_with("result = \"pass\"\n")
    );
}

#[test]
fn a_folder_of_transcripts_of_two_versions_is_refused() {
    let scratch = Scratch::new("baseline-mixed");
    let transcripts = scratch.path().join("transcripts");
    in_version_2("synthetic", &transcripts, false);
    let at = scratch.path().join("r.toml");
    let refused = gate_over("synthetic", &transcripts, &at).unwrap_err();
    assert!(refused.contains("not all of one version"), "{refused}");
    assert!(!at.exists());
}

#[test]
fn the_breakdown_counts_scaled_cuts_models_use_cases_and_first_places() {
    let scratch = Scratch::new("baseline-breakdown-scaled");
    let at = scratch.path().join("r.toml");
    let scaled = gate_over("scaled", &fixtures().join("../scaled/transcripts"), &at).unwrap();
    let b = &scaled.breakdown;
    assert_eq!((b.only_scaled, b.elsewhere), (1, 0));
    assert_eq!(b.models["claude-haiku-5-5"], (0, 1));
    assert_eq!(b.use_cases["source-fidelity"], (0, 1));
    assert_eq!(
        b.first.iter().collect::<Vec<_>>(),
        [(&"/images/*/prepared/bytes".to_owned(), &1)]
    );
    assert!(
        scaled
            .text()
            .contains("differed only at scaled cuts: 1\ndiffered elsewhere: 0\n")
    );
    let same = gate_over("synthetic", &fixtures().join("transcripts"), &at).unwrap();
    assert_eq!(same.breakdown.models["claude-haiku-5-5"], (3, 0));
    assert_eq!(same.breakdown.first.len(), 0);
}

#[test]
fn the_breakdown_names_endings_dispositions_and_refusals_from_closed_lists() {
    let scratch = Scratch::new("baseline-breakdown");
    let transcripts = scratch.path().join("transcripts");
    copy(&fixtures().join("transcripts"), &transcripts);
    // The decision `kept` as if squint had refused its first call for its arguments, and
    // ended in another verdict: the replay runs the call and ends in its own.
    let kept = transcripts.join("historical/synthetic/kept");
    let file = std::fs::read_dir(&kept)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut t: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    t["terminal"]["verdict"] = "fail".into();
    let call = &mut t["turns"][0]["calls"][0];
    assert_eq!(call["name"], "view");
    call["disposition"] = "rejected".into();
    call["result"] = serde_json::json!({"content": "invalid arguments: missing required argument \"frame\"",
                                        "is_error": true, "images": []});
    std::fs::write(&file, t.to_string()).unwrap();

    let tally = gate_over("synthetic", &transcripts, &scratch.path().join("r.toml")).unwrap();
    let b = &tally.breakdown;
    let listed = |m: &std::collections::BTreeMap<String, usize>| {
        m.iter().map(|(k, v)| (k.clone(), *v)).collect::<Vec<_>>()
    };
    assert_eq!((b.only_scaled, b.elsewhere), (0, 1));
    assert_eq!(
        listed(&b.endings),
        [("verdict fail -> verdict pass".to_owned(), 1)]
    );
    assert_eq!(
        listed(&b.dispositions),
        [("view rejected -> executed".to_owned(), 1)]
    );
    assert_eq!(
        listed(&b.results),
        [("view rejected, schema".to_owned(), 1)]
    );
    assert_eq!(listed(&b.first), [("/terminal/verdict".to_owned(), 1)]);
    assert_eq!(b.models["claude-haiku-5-5"], (2, 1));
    let text = tally.text();
    for line in [
        "ending verdict fail -> verdict pass: 1\n",
        "disposition view rejected -> executed: 1\n",
        "result text of view rejected, schema: 1\n",
        "first difference at /terminal/verdict: 1\n",
    ] {
        assert!(text.contains(line), "{text}");
    }
    assert!(
        !text.contains("frame\\\"") && !text.contains("missing required"),
        "{text}"
    );
}

#[test]
fn squints_judgement_of_each_kind_of_submission_replays_the_same() {
    // Eleven decisions captured from frozen squint (`scripts/baseline/capture/`, the set of
    // submissions): squint's schema refusals (a bad enum, an extra field, a missing field, two
    // faults at once), its rules (an unknown frame, a box outside its frame, a missing item,
    // `observed` on an item, evidence that is no completed call), a violation located on a
    // crop, which squint took, and two refusals in a row, which end the run.
    let scratch = Scratch::new("baseline-submissions");
    let recorded = fixtures().join("../submissions/transcripts");
    let tally = gate_over("submissions", &recorded, &scratch.path().join("r.toml")).unwrap();
    assert_eq!(
        (tally.transcripts, tally.same, tally.differed, tally.refused),
        (11, 11, 0, 0),
        "{}",
        tally.text()
    );
    // The recorded words are squint's own: the replay matched them, schema refusals included.
    let mut said = String::new();
    for entry in std::fs::read_dir(recorded.join("historical/synthetic")).unwrap() {
        for file in std::fs::read_dir(entry.unwrap().path()).unwrap() {
            said.push_str(&std::fs::read_to_string(file.unwrap().path()).unwrap());
        }
    }
    for words in [
        "is not valid under any of the schemas listed in the 'oneOf' keyword at /items/0/disposition",
        "Additional properties are not allowed ('note' was unexpected) at /items/0",
        "\\\"evidence\\\" is a required property at /items/0",
        "location invalid: submission: bbox [0.5, 0.5, 1.2, 0.9] outside [0, 1]",
        "is not a tool call that completed in this run",
    ] {
        assert!(said.contains(words), "{words}");
    }
}
