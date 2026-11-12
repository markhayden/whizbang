//! Runs and the two kinds of reference (plan 1A, task A3): what a run references, what
//! deleting it removes, and a diagnostic that is never an output.

use std::path::Path;
use std::sync::Arc;

use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Digest, Error, Kind, Reader, Store};
use whizbang_testkit::fs_shim::{Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;

use super::{B, C, after_restart, contents, for_every_loss, root};

pub(super) const MASK: &[u8] = b"a mask two runs share";
pub(super) const EXCHANGE: &[u8] = b"the exchange of a turn that failed";
pub(super) const ONLY_ONE: &[u8] = b"what only run one made";
pub(super) const LEDGER: &[u8] = b"a spend record";

/// A store with a record outside any run and three runs: `one` and `two` share the mask,
/// `one` has an output of its own, and `failed` holds diagnostics only, one of them the
/// very bytes `two` holds as an output.
pub(super) fn with_runs(root: &Path) {
    let store = Store::open(root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("spend/1", LEDGER).unwrap();
    publication
        .put_in("one", "mask", Kind::Output, MASK)
        .unwrap();
    publication
        .put_in("one", "render", Kind::Output, ONLY_ONE)
        .unwrap();
    publication
        .put_in("two", "mask", Kind::Output, MASK)
        .unwrap();
    publication
        .put_in("two", "exchange", Kind::Output, EXCHANGE)
        .unwrap();
    publication
        .put_in("failed", "exchange", Kind::Diagnostic, EXCHANGE)
        .unwrap();
    publication
        .put_in("failed", "journal", Kind::Diagnostic, B)
        .unwrap();
    assert_eq!(publication.publish().unwrap().generation, 1);
}

pub(super) fn object(root: &Path, bytes: &[u8]) -> std::path::PathBuf {
    root.join("objects").join(Digest::of_bytes(bytes).hex())
}

/// The ids of the runs a reader sees.
pub(super) fn runs(root: &Path) -> Vec<String> {
    let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
    reader.snapshot().unwrap().runs().keys().cloned().collect()
}

#[test]
fn a_run_holds_its_references_under_names_of_its_own() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    assert_eq!(runs(&root), ["failed", "one", "two"]);
    let names = |run: &str| -> Vec<String> {
        let run = generation.run(run).unwrap();
        run.entries().keys().cloned().collect()
    };
    assert_eq!(names("one"), ["mask", "render"]);
    assert_eq!(names("two"), ["exchange", "mask"]);
    assert_eq!(names("failed"), ["exchange", "journal"]);
    assert!(generation.run("three").is_none());
    // The same name in two runs is two references; here to the same bytes, stored once.
    let one = generation.run("one").unwrap().get("mask").unwrap();
    let two = generation.run("two").unwrap().get("mask").unwrap();
    assert_eq!(one, two);
    assert_eq!(reader.read_output(one).unwrap(), MASK);
    assert_eq!(std::fs::read_dir(root.join("objects")).unwrap().count(), 5);
    // Outside any run.
    assert_eq!(generation.entries().keys().collect::<Vec<_>>(), ["spend/1"]);
    assert_eq!(generation.get("spend/1").unwrap().kind, Kind::Output);
}

#[test]
fn the_same_bytes_are_a_diagnostic_to_one_run_and_an_output_to_another() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    // After reopening, through a writer and through a reader.
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    for generation in [store.snapshot().unwrap(), reader.snapshot().unwrap()] {
        let diagnostic = generation.run("failed").unwrap().get("exchange").unwrap();
        let output = generation.run("two").unwrap().get("exchange").unwrap();
        assert_eq!(diagnostic.digest, output.digest, "one stored file");
        assert_eq!(
            (diagnostic.kind, output.kind),
            (Kind::Diagnostic, Kind::Output)
        );

        // Each is treated as it says. A diagnostic is read, and is not an output.
        assert_eq!(store.read(diagnostic).unwrap(), EXCHANGE);
        assert_eq!(reader.read(diagnostic).unwrap(), EXCHANGE);
        for result in [
            store.read_output(diagnostic),
            reader.read_output(diagnostic),
        ] {
            match result {
                Err(Error::NotAnOutput { digest }) => {
                    assert_eq!(digest, Digest::of_bytes(EXCHANGE))
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(store.read_output(output).unwrap(), EXCHANGE);
        assert_eq!(reader.read_output(output).unwrap(), EXCHANGE);
    }
    assert!(object(&root, EXCHANGE).exists());
}

#[test]
fn a_generation_file_says_the_kind_of_each_reference_and_the_run_it_belongs_to() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("a", B).unwrap();
    publication
        .put_in("r", "journal", Kind::Diagnostic, C)
        .unwrap();
    publication.publish().unwrap();
    let (b, c) = (Digest::of_bytes(B).hex(), Digest::of_bytes(C).hex());
    let expected = format!(
        r#"{{
  "format": 2,
  "number": 1,
  "entries": {{
    "a": {{
      "digest": "{b}",
      "bytes": {},
      "kind": "output"
    }}
  }},
  "runs": {{
    "r": {{
      "entries": {{
        "journal": {{
          "digest": "{c}",
          "bytes": {},
          "kind": "diagnostic"
        }}
      }}
    }}
  }}
}}
"#,
        B.len(),
        C.len()
    );
    assert_eq!(
        std::fs::read_to_string(root.join("generations/1.json")).unwrap(),
        expected
    );
}

#[test]
fn a_later_publication_adds_to_a_run_and_replaces_a_name_of_it() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .put_in("one", "render", Kind::Output, C)
        .unwrap();
    publication
        .put_in("one", "journal", Kind::Diagnostic, B)
        .unwrap();
    publication.publish().unwrap();
    let generation = store.snapshot().unwrap();
    let one = generation.run("one").unwrap();
    assert_eq!(
        one.entries().keys().collect::<Vec<_>>(),
        ["journal", "mask", "render"]
    );
    assert_eq!(store.read(one.get("render").unwrap()).unwrap(), C);
    assert_eq!(generation.run("two").unwrap().entries().len(), 2);
}

#[test]
fn deleting_a_run_removes_its_references_and_no_others() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    assert_eq!(store.delete(&["one"]).unwrap().generation, 2);

    let generation = store.snapshot().unwrap();
    assert_eq!(runs(&root), ["failed", "two"]);
    // The mask that run two shares is still referenced, and read.
    let mask = generation.run("two").unwrap().get("mask").unwrap();
    assert_eq!(store.read_output(mask).unwrap(), MASK);
    assert!(
        !generation
            .runs()
            .values()
            .flat_map(|run| run.entries().values())
            .any(|entry| entry.digest == Digest::of_bytes(ONLY_ONE)),
        "what only the deleted run referenced is referenced no more"
    );
    // Records outside runs are not a run's to lose.
    assert_eq!(
        store.read(generation.get("spend/1").unwrap()).unwrap(),
        LEDGER
    );
}

#[test]
fn a_record_outside_runs_outlives_the_deletion_of_every_run() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    store.delete(&["one", "two", "failed"]).unwrap();
    drop(store);
    assert!(runs(&root).is_empty());
    assert_eq!(
        after_restart(&root),
        (2, contents(&[("spend/1", LEDGER)])),
        "the spend and request-id records are kept this way"
    );
}

#[test]
fn deleting_a_run_the_store_does_not_hold_is_refused_and_publishes_nothing() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    // One that exists beside one that does not: neither goes.
    match store.delete(&["one", "three"]) {
        Err(Error::NoSuchRun { run }) => assert_eq!(run, "three"),
        other => panic!("{other:?}"),
    }
    assert_eq!(store.snapshot().unwrap().number(), 1);
    assert_eq!(runs(&root), ["failed", "one", "two"]);
    assert!(!root.join("generations/2.json").exists());
    assert_eq!(std::fs::read_dir(root.join("staging")).unwrap().count(), 0);
}

#[test]
fn a_crash_at_any_event_of_a_deletion_leaves_the_run_whole_or_gone() {
    let count = {
        let dir = Scratch::new("store-runs");
        let root = root(&dir);
        with_runs(&root);
        let fs = Arc::new(ShimFs::new());
        Store::open(&root, fs.clone())
            .unwrap()
            .delete(&["one"])
            .unwrap();
        fs.events().len()
    };
    for crash_at in 0..=count {
        for_every_loss(|lost| {
            let case = format!("{lost:?}, crash before event {crash_at}");
            let dir = Scratch::new("store-runs");
            let root = root(&dir);
            with_runs(&root);
            let fs = Arc::new(ShimFs::new().crash_before(crash_at, lost));
            let result = Store::open(&root, fs.clone()).and_then(|store| store.delete(&["one"]));
            fs.crash(lost);

            let (number, _) = after_restart(&root);
            match number {
                1 => {
                    assert!(result.is_err(), "{case}: success was reported");
                    assert_eq!(runs(&root), ["failed", "one", "two"], "{case}");
                    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
                    let generation = reader.snapshot().unwrap();
                    for entry in generation.run("one").unwrap().entries().values() {
                        reader.read(entry).unwrap();
                    }
                }
                2 => assert_eq!(runs(&root), ["failed", "two"], "{case}"),
                other => panic!("{case}: generation {other}"),
            }
            if matches!(lost, Lost::Unsynced) && crash_at == count {
                assert_eq!(number, 2, "{case}");
            }
            fs.crash_choices()
        });
    }
}

#[test]
fn a_run_id_is_not_empty_and_holds_no_control_character() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    let mut publication = store.begin().unwrap();
    let before = fs.events().len();
    for id in [
        "",
        "a\nb",
        "tab\there",
        "\u{1b}[31mred",
        "bell\u{7}",
        "nul\0",
    ] {
        match publication.put_in(id, "out", Kind::Output, MASK) {
            Err(Error::InvalidRun { run }) => assert_eq!(run, id),
            other => panic!("{id:?}: {other:?}"),
        }
    }
    assert_eq!(fs.events().len(), before, "nothing was staged for them");
    let message = publication
        .put_in("a\nb", "out", Kind::Output, MASK)
        .unwrap_err()
        .to_string();
    assert!(
        message.starts_with("\"a\\nb\" cannot be a run's id"),
        "{message}"
    );
    // Anything else is an id, as far as the store goes.
    for id in ["one", "run 2", "caf\u{e9}/7", "-", "."] {
        publication.put_in(id, "out", Kind::Output, MASK).unwrap();
    }
    publication.publish().unwrap();
    assert_eq!(runs(&root), ["-", ".", "caf\u{e9}/7", "one", "run 2"]);
}

#[test]
fn a_generation_file_of_another_format_is_refused_as_another_format_not_as_damage() {
    let dir = Scratch::new("store-runs");
    let root = super::with_old(&dir);
    // Generation 1 exactly as the first format wrote it: no kinds, no runs.
    let first_format = format!(
        r#"{{
  "format": 1,
  "number": 1,
  "entries": {{
    "a": {{
      "digest": "{}",
      "bytes": {}
    }}
  }}
}}
"#,
        Digest::of_bytes(super::A1).hex(),
        super::A1.len()
    );
    std::fs::write(root.join("generations/1.json"), first_format).unwrap();
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let by_reader = reader.snapshot().map(|_| ());
    let by_writer = Store::open(&root, Arc::new(ShimFs::new())).map(|_| ());
    for result in [by_reader, by_writer] {
        let error = result.unwrap_err();
        match &error {
            Error::OtherFormat {
                path,
                format: 1,
                reads: 2,
            } => assert_eq!(*path, root.join("generations/1.json")),
            other => panic!("{other:?}"),
        }
        let message = error.to_string();
        assert!(
            message.ends_with("is of another format: 1, and this build reads format 2"),
            "{message}"
        );
        assert!(!message.contains("damaged"), "{message}");
    }
}
