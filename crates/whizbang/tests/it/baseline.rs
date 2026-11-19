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
