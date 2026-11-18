//! Evaluation sets (plan 2C, task E1, items 1 and 2): squint's sets as copied, and a set's
//! checks, on copies of a published set of two items made in the test.

use std::path::{Path, PathBuf};

use whizbang_core::identity::Digest;
use whizbang_eval::sets::{self, Judging, Set};
use whizbang_testkit::scratch::Scratch;

/// The repository's root.
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The rule files `check` ships, by use case, as the binary hands them in.
fn rules(base: &str) -> Option<&'static str> {
    Some(match base {
        "source-fidelity" => include_str!("../../../whizbang-squint/rules/source-fidelity.toml"),
        "reference-consistency" => {
            include_str!("../../../whizbang-squint/rules/reference-consistency.toml")
        }
        "reality-checks" => include_str!("../../../whizbang-squint/rules/reality-checks.toml"),
        "brief-compliance" => include_str!("../../../whizbang-squint/rules/brief-compliance.toml"),
        "product-fidelity" => include_str!("../../../whizbang-squint/rules/product-fidelity.toml"),
        _ => return None,
    })
}

fn any_job(_: &str) -> Result<(), String> {
    Ok(())
}

fn judging() -> Judging<'static> {
    Judging {
        rules: &rules,
        job: &any_job,
    }
}

/// Every file under `from`, copied to `to`.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let into = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_tree(&path, &into);
        } else {
            std::fs::copy(&path, &into).unwrap();
        }
    }
}

/// A copy of a published set of two items, `<root>/source-fidelity/adversarial-v1`.
fn a_set(label: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new(label);
    let dir = scratch.path().join("source-fidelity/adversarial-v1");
    copy_tree(
        &repo().join("evalsets/source-fidelity/adversarial-v1"),
        &dir,
    );
    (scratch, dir)
}

/// The list of squint's files and their digests, as committed.
fn listed() -> Vec<(String, String)> {
    let list = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/squint-evalsets.sha256"),
    )
    .unwrap();
    list.lines()
        .map(|line| {
            let (digest, path) = line.split_once("  ").unwrap();
            (digest.to_owned(), path.to_owned())
        })
        .collect()
}

#[test]
fn every_file_squint_tracked_under_evalsets_is_here_byte_for_byte() {
    let listed = listed();
    // Squint tracked 374 files under `evalsets/` at the commit `docs/sources.toml` pins.
    assert_eq!(listed.len(), 374);
    for (digest, path) in &listed {
        assert!(path.starts_with("evalsets/"), "{path}");
        let bytes = std::fs::read(repo().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(
            &Digest::of_bytes(&bytes).hex(),
            digest,
            "{path} is not squint's"
        );
    }
    // And nothing else is under `evalsets/`: no label, no picture.
    let mut found = Vec::new();
    let mut dirs = vec![repo().join("evalsets")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                found.push(path.strip_prefix(repo()).unwrap().display().to_string());
            }
        }
    }
    found.sort();
    let mut names: Vec<String> = listed.into_iter().map(|(_, path)| path).collect();
    names.sort();
    assert_eq!(found, names);
}

#[test]
fn every_published_set_checks_and_its_seal_holds() {
    // The seals squint wrote are reproduced here offline: the canonical jobs, the rules,
    // the labels, and the pictures by their pins.
    let report = sets::check(&repo().join("evalsets"), &judging()).unwrap();
    assert_eq!(report.sets, 13);
}

#[test]
fn a_set_whose_seal_does_not_match_is_refused() {
    let (_scratch, dir) = a_set("sets-seal");
    Set::load(&dir).unwrap().check(&judging()).unwrap();
    // A job's words changed after the seal.
    let job = dir.join("items/garden-shed-sign-pass/job.toml");
    let text = std::fs::read_to_string(&job).unwrap();
    let changed = text.replacen("brief = \"", "brief = \"Also: ", 1);
    assert_ne!(changed, text);
    std::fs::write(&job, changed).unwrap();
    let refused = Set::load(&dir).unwrap().check(&judging()).unwrap_err();
    assert!(
        refused.contains("the seal does not match the set; changed: jobs"),
        "{refused}"
    );
    std::fs::write(&job, text).unwrap();
    // A picture's pin changed: the images.
    let set = Set::load(&dir).unwrap();
    let pin = set.pictures().unwrap()[0].sha256.clone();
    let other = format!("{}0", &pin[..63]);
    let other = if other == pin {
        format!("{}1", &pin[..63])
    } else {
        other
    };
    std::fs::write(
        &job,
        std::fs::read_to_string(&job).unwrap().replace(&pin, &other),
    )
    .unwrap();
    let refused = Set::load(&dir).unwrap().check(&judging()).unwrap_err();
    assert!(refused.contains("changed: jobs, images"), "{refused}");
}

#[test]
fn corrections_are_applied_as_a_versioned_file_beside_the_set() {
    let (_scratch, dir) = a_set("sets-corrections");
    let sealed = Set::load(&dir).unwrap();
    let before = sealed.labels.clone().unwrap();
    let first = &before.label[0];
    let (was, now) = match first.expected {
        sets::Expected::Pass => ("pass", "fail"),
        sets::Expected::Fail => ("fail", "pass"),
    };
    let correction = format!(
        "[[correction]]\nitem = \"{}\"\nwas = \"{was}\"\nexpected = \"{now}\"\n\
         failure_types = []\nwords = \"corrected in the test\"\nobligation_ids = []\n\
         adjudicated_by = \"owner\"\nadjudicated_on = \"2026-11-18\"\nfound_by = \"a test\"\n",
        first.item
    );
    std::fs::write(dir.join(sets::CORRECTIONS_FILE), &correction).unwrap();
    let corrected = Set::load(&dir).unwrap();
    let label = &corrected.labels.as_ref().unwrap().label[0];
    assert_eq!(label.item, first.item);
    assert_ne!(label.expected, first.expected);
    assert_eq!(label.words, "corrected in the test");
    // The labels file stays as sealed, so the seal still holds.
    corrected.check(&judging()).unwrap();
    assert_eq!(corrected.labels.as_ref().unwrap().label[1], before.label[1]);
    // A correction that does not say what the label was is refused, as is one of an item
    // that is not labelled.
    let wrong = correction.replace(&format!("was = \"{was}\""), &format!("was = \"{now}\""));
    std::fs::write(dir.join(sets::CORRECTIONS_FILE), wrong).unwrap();
    assert!(Set::load(&dir).unwrap_err().contains("says it was"));
    let stray = correction.replace(&first.item, "no-such-item");
    std::fs::write(dir.join(sets::CORRECTIONS_FILE), stray).unwrap();
    assert!(Set::load(&dir).unwrap_err().contains("which has no label"));
}

#[test]
fn no_set_without_published_labels_has_a_labels_file() {
    let mut blind = 0;
    for dir in sets::discover(&repo().join("evalsets")).unwrap() {
        let set = Set::load(&dir).unwrap();
        let meta = &set.manifest.set;
        let files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        if meta.labels.is_none() {
            blind += 1;
            assert_eq!(meta.split, sets::Split::Blind, "{}", meta.id);
            assert!(meta.labels_sha256.is_some(), "{}", meta.id);
            assert!(
                !files.iter().any(|name| name.contains("label")),
                "{}: {files:?}",
                meta.id
            );
            assert!(set.labels.is_none());
        } else {
            assert_ne!(meta.split, sets::Split::Blind, "{}", meta.id);
        }
    }
    assert_eq!(blind, 1);
    // A blind set that carries a labels file is refused.
    let (_scratch, dir) = a_set("sets-blind");
    let manifest = dir.join(sets::MANIFEST_FILE);
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        text.replace("split = \"dev\"", "split = \"blind\""),
    )
    .unwrap();
    let refused = Set::load(&dir).unwrap().check(&judging()).unwrap_err();
    assert!(
        refused.contains("a blind set carries no labels file"),
        "{refused}"
    );
}

#[test]
fn a_set_file_or_folder_that_is_a_link_is_refused_and_nothing_it_leads_to_is_said() {
    use std::os::unix::fs::symlink;
    let (scratch, dir) = a_set("sets-links");
    let outside = scratch.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("words.toml"), "an outside line\n").unwrap();
    // A file of the set that is a link.
    let families = dir.join("families.toml");
    let kept = std::fs::read(&families).unwrap();
    std::fs::remove_file(&families).unwrap();
    symlink(outside.join("words.toml"), &families).unwrap();
    let refused = Set::load(&dir).unwrap_err();
    assert!(refused.contains("is a link"), "{refused}");
    assert!(!refused.contains("an outside line"), "{refused}");
    std::fs::remove_file(&families).unwrap();
    std::fs::write(&families, kept).unwrap();
    Set::load(&dir).unwrap();
    // A folder of the set that is a link to a copy of itself elsewhere.
    let items = dir.join("items");
    std::fs::rename(&items, outside.join("items")).unwrap();
    symlink(outside.join("items"), &items).unwrap();
    let refused = Set::load(&dir).unwrap_err();
    assert!(refused.contains("is a link"), "{refused}");
    // And corrections beside the labels that are a link.
    std::fs::remove_file(&items).unwrap();
    std::fs::rename(outside.join("items"), &items).unwrap();
    symlink(outside.join("words.toml"), dir.join(sets::CORRECTIONS_FILE)).unwrap();
    assert!(Set::load(&dir).unwrap_err().contains("is a link"));
}
