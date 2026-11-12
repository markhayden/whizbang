//! `whizbang store list|delete|prune`, and the one writer with real processes.
//!
//! The process tests need three processes on one store with nothing left to timing. The
//! first writer is this test process itself: it stops after staging simply by not having
//! made its next call yet. The reader and the second writer are the real binary, each
//! run to its exit while the first writer stands still. Every step is over before the
//! next begins, so there is nothing to wait for and nothing to sleep on.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Digest, Kind, Reader, Store};
use whizbang_testkit::scratch::Scratch;

const MASK: &[u8] = b"a mask two runs share";
const RENDER: &[u8] = b"what only run one made";
const EXCHANGE: &[u8] = b"the exchange of a turn that failed";
const LEDGER: &[u8] = b"a spend record";

struct Ran {
    code: Option<i32>,
    out: String,
    err: String,
}

fn whizbang(args: &[&str], root: &Path) -> Ran {
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .arg("store")
        .args(args)
        .arg("--store")
        .arg(root)
        .output()
        .expect("the binary runs");
    Ran {
        code: out.status.code(),
        out: String::from_utf8(out.stdout).expect("output is UTF-8"),
        err: String::from_utf8(out.stderr).expect("output is UTF-8"),
    }
}

/// A store with a record outside runs and three runs: `one` and `two` share the mask,
/// `one` has a render of its own, `failed` has one diagnostic.
fn with_runs(dir: &Scratch) -> PathBuf {
    let root = dir.path().join("store");
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("spend/1", LEDGER).unwrap();
    publication
        .put_in("one", "mask", Kind::Output, MASK)
        .unwrap();
    publication
        .put_in("one", "render", Kind::Output, RENDER)
        .unwrap();
    publication
        .put_in("two", "mask", Kind::Output, MASK)
        .unwrap();
    publication
        .put_in("failed", "exchange", Kind::Diagnostic, EXCHANGE)
        .unwrap();
    publication.publish().unwrap();
    root
}

fn object(root: &Path, bytes: &[u8]) -> PathBuf {
    root.join("objects").join(Digest::of(bytes).to_string())
}

/// Every path under a directory, with the content of each file.
fn tree(dir: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            found.push((path.clone(), None));
            stack.extend(
                std::fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else {
            found.push((path.clone(), Some(std::fs::read(&path).unwrap())));
        }
    }
    found.sort();
    found
}

fn listing_of_the_three_runs() -> String {
    format!(
        "generation: 1\nnames outside runs: 1\nruns: 3\n\
         failed\toutputs 0\tdiagnostics 1\tbytes {}\n\
         one\toutputs 2\tdiagnostics 0\tbytes {}\n\
         two\toutputs 1\tdiagnostics 0\tbytes {}\n",
        EXCHANGE.len(),
        MASK.len() + RENDER.len(),
        MASK.len()
    )
}

#[test]
fn with_the_first_writer_paused_after_staging_a_reader_reads_and_a_second_writer_is_refused() {
    let dir = Scratch::new("cli-store");
    let root = with_runs(&dir);

    // The first writer, this process: it opens the store, stages two files, and stops.
    let first = Store::open(&root, Arc::new(RealFs)).unwrap();
    let mut publication = first.begin().unwrap();
    publication
        .put_in("three", "out", Kind::Output, b"staged by the first writer")
        .unwrap();
    publication
        .put_in("three", "journal", Kind::Diagnostic, b"staged as well")
        .unwrap();
    let staged = tree(&root.join("staging"));
    assert_eq!(staged.len(), 4, "staging, one directory, two files");
    let before = tree(&root);

    // A reader, in a process of its own, opens the store and reads it.
    let reader = whizbang(&["list"], &root);
    assert_eq!(reader.code, Some(0), "{}", reader.err);
    assert_eq!(reader.out, listing_of_the_three_runs());

    // A second writer, in a process of its own, is refused, and is told who holds it.
    let holder = format!("process {} (", std::process::id());
    for args in [&["prune"][..], &["delete", "one"]] {
        let second = whizbang(args, &root);
        assert_eq!(second.code, Some(1), "{args:?}: {}", second.out);
        assert_eq!(second.out, "", "{args:?}");
        assert!(second.err.contains(&holder), "{args:?}: {}", second.err);
        assert!(
            second.err.contains("one process at a time"),
            "{args:?}: {}",
            second.err
        );
    }

    // Neither touched anything: the staging is as it was, and so is the rest.
    assert_eq!(tree(&root.join("staging")), staged);
    assert_eq!(tree(&root), before);

    // The first writer goes on, and its publication succeeds.
    assert_eq!(publication.publish().unwrap().generation, 2);
    let generation = first.snapshot().unwrap();
    let out = generation.run("three").unwrap().get("out").unwrap();
    assert_eq!(
        first.read_output(out).unwrap(),
        b"staged by the first writer"
    );

    // Once it has gone, a second writer is not refused.
    drop(first);
    let second = whizbang(&["prune"], &root);
    assert_eq!(second.code, Some(0), "{}", second.err);
    assert_eq!(
        second.out,
        "generations removed: 1\ncontent files removed: 0\n"
    );
}

#[test]
fn a_reading_process_holds_its_generation_through_another_processes_prune() {
    let dir = Scratch::new("cli-store");
    let root = with_runs(&dir);
    // This process reads generation 1. Another deletes two runs and prunes.
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let deleted = whizbang(&["delete", "one", "two"], &root);
    assert_eq!(deleted.code, Some(0), "{}", deleted.err);
    assert_eq!(
        deleted.out,
        "runs deleted: 2\ngeneration: 2\ngenerations removed: 0\ncontent files removed: 0\n\
         generations left for a reader that still holds them: 1 (prune again once it has finished)\n"
    );
    for entry in generation.references() {
        reader.read(entry).unwrap();
    }

    // The reader finishes; the next prune removes what was left for it.
    drop(reader);
    let pruned = whizbang(&["prune"], &root);
    assert_eq!(
        (pruned.code, pruned.out.as_str()),
        (
            Some(0),
            "generations removed: 1\ncontent files removed: 2\n"
        ),
        "{}",
        pruned.err
    );
    assert!(!object(&root, MASK).exists());
}

#[test]
fn list_shows_each_run_with_its_references_by_kind() {
    let dir = Scratch::new("cli-store");
    let root = with_runs(&dir);
    let before = tree(&root);
    let listed = whizbang(&["list"], &root);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    assert_eq!(listed.out, listing_of_the_three_runs());
    assert_eq!(tree(&root), before, "a listing changes nothing");
}

#[test]
fn delete_removes_what_only_the_run_referenced_and_keeps_what_another_run_shares() {
    let dir = Scratch::new("cli-store");
    let root = with_runs(&dir);
    let deleted = whizbang(&["delete", "one"], &root);
    assert_eq!(deleted.code, Some(0), "{}", deleted.err);
    assert_eq!(
        deleted.out,
        "runs deleted: 1\ngeneration: 2\ngenerations removed: 1\ncontent files removed: 1\n"
    );
    assert!(!object(&root, RENDER).exists(), "only run one had it");
    assert!(object(&root, MASK).exists(), "run two shares it");

    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    assert_eq!(
        generation.runs().keys().collect::<Vec<_>>(),
        ["failed", "two"]
    );
    let mask = generation.run("two").unwrap().get("mask").unwrap();
    assert_eq!(reader.read_output(mask).unwrap(), MASK);
    // The failed run's diagnostic, and the record outside runs, are read as before.
    let exchange = generation.run("failed").unwrap().get("exchange").unwrap();
    assert_eq!(reader.read(exchange).unwrap(), EXCHANGE);
    assert_eq!(
        reader.read(generation.get("spend/1").unwrap()).unwrap(),
        LEDGER
    );
}

#[test]
fn deleting_every_run_leaves_the_records_outside_runs() {
    let dir = Scratch::new("cli-store");
    let root = with_runs(&dir);
    let deleted = whizbang(&["delete", "one", "two", "failed"], &root);
    assert_eq!(deleted.code, Some(0), "{}", deleted.err);
    let listed = whizbang(&["list"], &root);
    assert_eq!(
        listed.out,
        "generation: 2\nnames outside runs: 1\nruns: 0\n"
    );
    assert_eq!(std::fs::read(object(&root, LEDGER)).unwrap(), LEDGER);
    assert_eq!(std::fs::read_dir(root.join("objects")).unwrap().count(), 1);
}

#[test]
fn deleting_a_run_the_store_does_not_hold_fails_and_changes_nothing() {
    let dir = Scratch::new("cli-store");
    let root = with_runs(&dir);
    let deleted = whizbang(&["delete", "one", "nine"], &root);
    assert_eq!(deleted.code, Some(1));
    assert_eq!(deleted.out, "");
    assert!(deleted.err.contains("no run \"nine\""), "{}", deleted.err);
    assert_eq!(whizbang(&["list"], &root).out, listing_of_the_three_runs());
    assert!(object(&root, RENDER).exists());

    // No run named at all is a usage error.
    let none = whizbang(&["delete"], &root);
    assert_eq!(none.code, Some(2), "{}", none.err);
}

#[test]
fn no_store_command_makes_a_store_where_there_is_none() {
    let dir = Scratch::new("cli-store");
    let missing = dir.path().join("nothing-here");
    for args in [&["list"][..], &["prune"], &["delete", "one"]] {
        let ran = whizbang(args, &missing);
        assert_eq!(ran.code, Some(1), "{args:?}");
        assert!(ran.err.contains("there is no store at"), "{}", ran.err);
        assert!(!missing.exists(), "{args:?}");
    }
    // Nor in a directory that is not one.
    let other = dir.path().join("other");
    std::fs::create_dir(&other).unwrap();
    std::fs::write(other.join("notes.txt"), b"mine").unwrap();
    for args in [&["list"][..], &["prune"], &["delete", "one"]] {
        let ran = whizbang(args, &other);
        assert_eq!(ran.code, Some(1), "{args:?}");
        assert!(ran.err.contains("is not a store"), "{}", ran.err);
        assert_eq!(std::fs::read_dir(&other).unwrap().count(), 1, "{args:?}");
    }
}
