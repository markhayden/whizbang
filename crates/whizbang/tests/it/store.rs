//! `whizbang store list|delete|prune`, and the one writer with real processes.
//!
//! The process tests need three processes on one store with nothing left to timing. The
//! first writer is this test process itself: it stops after staging simply by not having
//! made its next call yet. The reader and the second writer are the real binary, each
//! run to its exit while the first writer stands still. Every step is over before the
//! next begins, so there is nothing to wait for and nothing to sleep on.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Digest, Kind, Reader, Store};
use whizbang_testkit::scratch::Scratch;

const MASK: &[u8] = b"a mask two runs share";
const RENDER: &[u8] = b"what only run one made";
const EXCHANGE: &[u8] = b"the exchange of a turn that failed";
const LEDGER: &[u8] = b"a spend record";

pub(crate) struct Ran {
    pub(crate) code: Option<i32>,
    pub(crate) out: String,
    pub(crate) err: String,
}

/// `whizbang store <args> --store <root>`.
pub(crate) fn whizbang(args: &[&str], root: &Path) -> Ran {
    let out = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
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
    publication.put("notes/1", LEDGER).unwrap();
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
    root.join("objects").join(Digest::of_bytes(bytes).hex())
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
        "generation: 1\nnames outside runs: 1 (request ids 0, spend 0, other 1)\nruns: 3\n\
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

    // Once it has gone, a second writer is not refused. Its publication already removed
    // generation 1's file (ADR 0036), so the prune finds none to remove.
    drop(first);
    assert!(!root.join("generations/1.json").exists());
    let second = whizbang(&["prune"], &root);
    assert_eq!(second.code, Some(0), "{}", second.err);
    assert_eq!(
        second.out,
        "generations removed: 0\ncontent files removed: 0\n"
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
    // The deletion's own publication removed generation 1's file (ADR 0036); the prune
    // that follows it removes content.
    assert_eq!(
        deleted.out,
        "runs deleted: 1\ngeneration: 2\ngenerations removed: 0\ncontent files removed: 1\n"
    );
    assert!(!root.join("generations/1.json").exists());
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
        reader.read(generation.get("notes/1").unwrap()).unwrap(),
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
        "generation: 2\nnames outside runs: 1 (request ids 0, spend 0, other 1)\nruns: 0\n"
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
    assert!(deleted.err.contains("no run nine"), "{}", deleted.err);
    assert_eq!(whizbang(&["list"], &root).out, listing_of_the_three_runs());
    assert!(object(&root, RENDER).exists());

    // A run named twice is one run.
    let twice = whizbang(&["delete", "one", "one"], &root);
    assert_eq!(twice.code, Some(0), "{}", twice.err);
    assert!(
        twice.out.starts_with("runs deleted: 1\ngeneration: 2\n"),
        "{}",
        twice.out
    );
    // And once deleted it is a run the store does not hold.
    let again = whizbang(&["delete", "one"], &root);
    assert_eq!(again.code, Some(1));
    assert!(again.err.contains("no run one"), "{}", again.err);

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
    // Nor in a directory that is not one, and each says what it found there.
    let other = dir.path().join("other");
    std::fs::create_dir(&other).unwrap();
    std::fs::write(other.join(".DS_Store"), b"").unwrap();
    for args in [&["list"][..], &["prune"], &["delete", "one"]] {
        let ran = whizbang(args, &other);
        assert_eq!(ran.code, Some(1), "{args:?}");
        assert!(ran.err.contains("is not a store"), "{}", ran.err);
        let found = other.join(".DS_Store").display().to_string();
        assert!(ran.err.contains(&found), "{args:?}: {}", ran.err);
        assert_eq!(std::fs::read_dir(&other).unwrap().count(), 1, "{args:?}");
    }
    // Nor in an empty directory, where a writer would make one.
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    for args in [&["list"][..], &["prune"], &["delete", "one"]] {
        let ran = whizbang(args, &empty);
        assert_eq!(ran.code, Some(1), "{args:?}");
        assert!(ran.err.contains("it has no CURRENT"), "{}", ran.err);
        assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0, "{args:?}");
    }
}

/// Replaces the current generation's file of a store with one made by hand.
fn hand_made(root: &Path, runs: &str) {
    let text = format!(r#"{{"format": 4, "number": 1, "entries": {{}}, "runs": {{{runs}}}}}"#);
    std::fs::write(root.join("generations/1.json"), text).unwrap();
}

#[test]
fn list_adds_sizes_without_overflowing() {
    let dir = Scratch::new("cli-store");
    let root = with_runs(&dir);
    let digest = Digest::of_bytes(MASK).hex();
    let huge = u64::MAX - 1;
    hand_made(
        &root,
        &format!(
            r#""big": {{"entries": {{
                "a": {{"digest": "{digest}", "bytes": {huge}, "kind": "output"}},
                "b": {{"digest": "{digest}", "bytes": {huge}, "kind": "diagnostic"}}
            }}}}"#
        ),
    );
    let listed = whizbang(&["list"], &root);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    assert_eq!(
        listed.out,
        format!(
            "generation: 1\nnames outside runs: 0 (request ids 0, spend 0, other 0)\nruns: 1\n\
             big\toutputs 1\tdiagnostics 1\tbytes {}\n",
            u64::MAX
        ),
        "the sum stops at the largest number"
    );
}

#[test]
fn list_escapes_a_run_id_that_would_break_its_lines() {
    // The store refuses such an id; a generation file made by hand can still hold one.
    let dir = Scratch::new("cli-store");
    let root = with_runs(&dir);
    hand_made(
        &root,
        r#""two\nlines\tand a tab \u001b[31m": {"entries": {}}, "café \"7\"": {"entries": {}}"#,
    );
    let listed = whizbang(&["list"], &root);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    assert_eq!(
        listed.out,
        "generation: 1\nnames outside runs: 0 (request ids 0, spend 0, other 0)\nruns: 2\n\
         caf\u{e9} \\\"7\\\"\toutputs 0\tdiagnostics 0\tbytes 0\n\
         two\\nlines\\tand a tab \\u{1b}[31m\toutputs 0\tdiagnostics 0\tbytes 0\n"
    );
}

#[test]
fn runs_whose_ids_the_store_would_refuse_are_listed_and_deleted() {
    // An empty id, and one with a control character: no publication makes such a run,
    // and a generation file made by hand can hold one. It is not refused on reading,
    // or nothing could remove it.
    let dir = Scratch::new("cli-store");
    let root = with_runs(&dir);
    let entry = format!(
        r#"{{"digest": "{}", "bytes": {}, "kind": "output"}}"#,
        Digest::of_bytes(MASK).hex(),
        MASK.len()
    );
    hand_made(
        &root,
        &format!(
            r#""": {{"entries": {{"mask": {entry}}}}},
               "a\u0001b": {{"entries": {{"mask": {entry}}}}}"#
        ),
    );
    let listed = whizbang(&["list"], &root);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    assert_eq!(
        listed.out,
        format!(
            "generation: 1\nnames outside runs: 0 (request ids 0, spend 0, other 0)\nruns: 2\n\
             \toutputs 1\tdiagnostics 0\tbytes {size}\n\
             a\\u{{1}}b\toutputs 1\tdiagnostics 0\tbytes {size}\n",
            size = MASK.len()
        )
    );

    // Each is deleted by its id, given as it is.
    let deleted = whizbang(&["delete", ""], &root);
    assert_eq!(deleted.code, Some(0), "{}", deleted.err);
    assert!(
        deleted.out.starts_with("runs deleted: 1\n"),
        "{}",
        deleted.out
    );
    let listed = whizbang(&["list"], &root);
    assert_eq!(
        listed.out,
        format!(
            "generation: 2\nnames outside runs: 0 (request ids 0, spend 0, other 0)\nruns: 1\n\
             a\\u{{1}}b\toutputs 1\tdiagnostics 0\tbytes {}\n",
            MASK.len()
        )
    );
    assert!(object(&root, MASK).exists(), "the other run still names it");
    let deleted = whizbang(&["delete", "a\u{1}b"], &root);
    assert_eq!(deleted.code, Some(0), "{}", deleted.err);
    assert!(
        deleted.out.starts_with("runs deleted: 1\n"),
        "{}",
        deleted.out
    );
    let listed = whizbang(&["list"], &root);
    assert_eq!(
        listed.out,
        "generation: 3\nnames outside runs: 0 (request ids 0, spend 0, other 0)\nruns: 0\n"
    );
    assert!(!object(&root, MASK).exists(), "nothing names it any more");
}

#[test]
fn delete_means_the_run_that_show_means_and_refuses_a_text_that_could_mean_two() {
    // `delete` finds a run as `runs show` does, through the same function: by its
    // id, or as `runs list` prints it when no run has that id. So what was looked at
    // is what is deleted. A text that is one run's id and how another is listed, and
    // a cut id that two runs share, delete nothing.
    let dir = Scratch::new("cli-store");
    let root = with_runs(&dir);
    let entry = format!(
        r#"{{"digest": "{}", "bytes": {}, "kind": "output"}}"#,
        Digest::of_bytes(MASK).hex(),
        MASK.len()
    );
    let long = format!("long-{}", "z".repeat(70));
    let twin = "t".repeat(64);
    let run = |id: &str| format!(r#"{id}: {{"entries": {{"mask": {entry}}}}}"#);
    let runs = [
        run(r#""a\u0001b""#),
        run(r#""a\\u{1}b""#),
        run(&format!("\"{long}\"")),
        run(&format!("\"{twin}-one\"")),
        run(&format!("\"{twin}-two\"")),
        run("\"plain\""),
    ];
    hand_made(&root, &runs.join(", "));
    let count = || {
        let listed = whizbang(&["list"], &root);
        let runs = listed.out.lines().nth(2).unwrap().to_owned();
        runs.trim_start_matches("runs: ").parse::<usize>().unwrap()
    };
    assert_eq!(count(), 6);
    let refused = |args: &[&str], says: &str| {
        let ran = whizbang(args, &root);
        assert_eq!(ran.code, Some(1), "{args:?}: {}", ran.out);
        assert!(ran.out.is_empty());
        assert!(ran.err.contains(says), "{args:?}: {}", ran.err);
    };
    // One run's id, and how the run with the control character is listed.
    refused(&["delete", "a\\u{1}b"], "is the id of one run");
    // Two runs are listed so.
    let cut = format!("{twin}...");
    refused(&["delete", &cut], "2 runs are listed as tttt");
    // One that is meant plainly beside one that is not: none is deleted.
    refused(&["delete", "plain", "a\\u{1}b"], "is the id of one run");
    refused(
        &["delete", "plain", "absent"],
        "the store holds no run absent",
    );
    assert_eq!(count(), 6);

    // A cut id as it is listed, and the same run by its id: one run.
    let listed_long = format!("{}...", long.chars().take(64).collect::<String>());
    let deleted = whizbang(&["delete", &listed_long, &long], &root);
    assert_eq!(deleted.code, Some(0), "{}", deleted.err);
    assert!(
        deleted.out.starts_with("runs deleted: 1\n"),
        "{}",
        deleted.out
    );
    // The run with the control character by its id, and the other as it is listed.
    let deleted = whizbang(&["delete", "a\u{1}b", "a\\\\u{1}b"], &root);
    assert_eq!(deleted.code, Some(0), "{}", deleted.err);
    assert!(
        deleted.out.starts_with("runs deleted: 2\n"),
        "{}",
        deleted.out
    );
    let listed = whizbang(&["list"], &root);
    let left: Vec<&str> = listed.out.lines().skip(3).collect();
    let left: Vec<&str> = left.iter().map(|l| l.split('\t').next().unwrap()).collect();
    assert_eq!(
        left,
        ["plain", &format!("{twin}-one"), &format!("{twin}-two")]
    );
}

#[test]
fn second_writers_and_readers_beside_a_writer_at_work_are_never_told_of_damage() {
    // This process writes the store without pause: it publishes a run, deletes the one
    // before and prunes, over and over, so the pointer moves and generation files go
    // while other processes look. Each of those is the binary, run to its exit. A
    // writer must be refused as held, every time, and a reader must read. The loop is
    // counted, not timed: the writer stops when the last of them has run.
    //
    // Sixty runs of the binary, thirty of each, and more only while the writer has not
    // yet gone round three times: its rounds sync for real, and on a busy machine three
    // of them can outlast sixty runs, or six hundred. So the loop ends on that event and
    // on no count of its own; what bounds it is the time limit of a test. A writer that
    // has died never gets there, so the loop ends on that too, and the join below then
    // fails the test with the writer's own panic. The runs are
    // a sample, and they are not what
    // holds the rule: the two windows, each opened on purpose, are in the core's tests
    // (`store/locking.rs`: a look that meets a publication and a prune, and a look
    // that meets the end of a store's creation). What this adds is real processes: the
    // answers hold across the boundary of a process, on the real file system and its
    // locks, while generations really come and go. It ran 300 times once, which took
    // up to 28 seconds where the machine is emulated; a fifth of that leaves room
    // under the time limit of a test.
    let dir = Scratch::new("cli-store");
    let root = with_runs(&dir);
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    let stop = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicU64::new(0));
    let writer = {
        let (stop, done) = (stop.clone(), done.clone());
        whizbang_testkit::threads::spawn(move || {
            let mut rounds = 0u64;
            while !stop.load(Ordering::Relaxed) {
                let mut publication = store.begin().unwrap();
                let run = format!("r{rounds}");
                publication
                    .put_in(&run, "out", Kind::Output, run.as_bytes())
                    .unwrap();
                publication.publish().unwrap();
                if rounds > 0 {
                    store.delete(&[&format!("r{}", rounds - 1)]).unwrap();
                }
                store.prune().unwrap();
                rounds += 1;
                done.store(rounds, Ordering::Relaxed);
            }
            rounds
        })
    };
    let holder = format!("process {} (", std::process::id());
    let mut wrong = Vec::new();
    for attempt in 0.. {
        if attempt >= 30 && done.load(Ordering::Relaxed) >= 3 {
            break;
        }
        if writer.is_finished() {
            // Nothing has asked it to stop yet, so it panicked.
            eprintln!("the writer stopped by itself, before run {attempt} of the binary");
            break;
        }
        let second = whizbang(&["prune"], &root);
        if second.code != Some(1) || !second.err.contains("one process at a time") {
            wrong.push(format!("prune {attempt}: {:?} {}", second.code, second.err));
        } else if !second.err.contains(&holder) && !second.err.contains("another process") {
            wrong.push(format!("prune {attempt}: no holder: {}", second.err));
        }
        let reader = whizbang(&["list"], &root);
        if reader.code != Some(0) || !reader.out.starts_with("generation: ") {
            wrong.push(format!("list {attempt}: {:?} {}", reader.code, reader.err));
        }
    }
    stop.store(true, Ordering::Relaxed);
    let rounds = writer
        .join()
        .expect("the writer panicked; its message is above");
    assert!(
        wrong.is_empty(),
        "{} wrong answers: {wrong:#?}",
        wrong.len()
    );
    assert!(rounds >= 3, "the writer was at work: {rounds} rounds");
}

#[test]
fn the_records_of_request_ids_are_left_by_delete_and_prune_and_still_answer() {
    // Request-id records are never deleted by these commands (`SPEC.md` section 3,
    // "Storage"). Two requests with ids run through the binary's own executor; every
    // run is then deleted and the store pruned, by the real commands; and every record
    // of the two ids is read back as it was, and still answers its request. Each call
    // may cost something, so the ledger reserved for it: the reservation of each run,
    // and the ledger's own records, are spend records, and are left as the ids' are.
    // (A call that may cost nothing has none: the ledger is not asked.)
    use whizbang_core::contract::record::Status;
    use whizbang_core::contract::{Answer, Call, Executor, Request, RequestId, RunState, Value};
    use whizbang_core::contract::{Reservation, reservations};

    let world = crate::wired::World::new("store-requests");
    let root = world.root.clone();
    let over = world.config(
        "[backends.fill]\nkind = \"fake\"\n\n[backends.fill.settings]\nat_most_micros = 250000\n",
    );
    let reset = crate::wired::whizbang(&["spend", "reset", "--allowance", "5"], &root);
    assert_eq!(reset.code, Some(0), "{}", reset.err);
    // `paints` asks its filler once, or twice: its need is for one request, so the
    // second is refused it and its run ends in an error.
    let request = |fail: bool, id: &str| {
        let times = Value::Integer(if fail { 2 } else { 1 });
        let call = Call::new("paints").argument("times", times);
        Request::new(call).with_id(RequestId::parse(id).unwrap())
    };
    let records = |root: &Path| -> Vec<(String, Vec<u8>)> {
        let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        let names = generation.entries().iter();
        names
            .map(|(name, entry)| (name.clone(), reader.read(entry).unwrap()))
            .collect()
    };
    let runs: Vec<String> = {
        let executor = world.runner(&over);
        let first = executor.request(&request(false, "order-17")).unwrap();
        let second = executor.request(&request(true, "order-18")).unwrap();
        vec![first.run().to_string(), second.run().to_string()]
    };
    let before = records(&root);
    let names: Vec<&str> = before.iter().map(|(name, _)| name.as_str()).collect();
    let ids = [
        "requests/order-17/admitted",
        "requests/order-17/ended",
        "requests/order-18/admitted",
        "requests/order-18/ended",
    ];
    assert_eq!(names[..4], ids);
    // The rest are the ledger's: the owner's reset, and for each run its
    // reservation, its hold, its dispatch and settlement, and its closing.
    assert!(names[4..].iter().all(|name| name.starts_with("spend/")));
    for run in &runs {
        for name in [
            format!("spend/reservations/{run}"),
            format!("spend/held/1/{run}/250000"),
            format!("spend/closed/{run}/released"),
        ] {
            assert!(names.contains(&name.as_str()), "{name} in {names:?}");
        }
    }
    let reserved = |root: &Path| -> Vec<Reservation> {
        let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
        reservations(&reader.snapshot().unwrap(), |entry| reader.read(entry)).unwrap()
    };
    let reserved_before = reserved(&root);
    assert_eq!(reserved_before.len(), 2);

    let deleted = whizbang(&["delete", &runs[0], &runs[1]], &root);
    assert_eq!(deleted.code, Some(0), "{}", deleted.err);
    let pruned = whizbang(&["prune"], &root);
    assert_eq!(pruned.code, Some(0), "{}", pruned.err);
    let listed = whizbang(&["list"], &root);
    // The records of request ids are named apart from the ledger's (plan 2C, task F1).
    let outside = format!(
        "names outside runs: {} (request ids 4, spend {}, other 0)\nruns: 0\n",
        before.len(),
        before.len() - 4
    );
    assert!(listed.out.contains(&outside), "{}", listed.out);
    assert_eq!(records(&root), before);
    // Each reservation is read back, and still names the run it was made for.
    assert_eq!(reserved(&root), reserved_before);
    for (reservation, run) in reserved_before.iter().zip(&runs) {
        assert_eq!(reservation.run().as_str(), run);
    }

    let executor = world.runner(&over);
    for (id, fail, status, run) in [
        ("order-17", false, Status::Completed, &runs[0]),
        ("order-18", true, Status::Error, &runs[1]),
    ] {
        let Answer::Repeat(earlier) = executor.request(&request(fail, id)).unwrap() else {
            panic!("{id} ran again");
        };
        assert_eq!(earlier.run().as_str(), run);
        assert_eq!(
            earlier.state(),
            &RunState::Removed {
                ended: Some(status)
            }
        );
    }
    assert_eq!(records(&root), before);
}

#[test]
fn an_imported_picture_keeps_where_it_came_from_by_a_name_of_its_own() {
    use image::ImageEncoder;
    // Plan m3, task O5 (the row on where an imported picture came from): each import of
    // a picture leaves the name `imports/picture/` and its file's SHA-256, holding the
    // image's identity, and the pictures of a store are found by those names alone.
    let dir = Scratch::new("store-imported");
    let root = dir.path().join("store");
    let rgba: Vec<u8> = (0..24u8)
        .flat_map(|n| [n * 9, 200 - n, n * 2, 255])
        .collect();
    let png = |extra: &[u8]| {
        let mut file = Vec::new();
        image::codecs::png::PngEncoder::new(&mut file)
            .write_image(&rgba, 6, 4, image::ExtendedColorType::Rgba8)
            .expect("a PNG");
        // The same pixels in another file: a text chunk after the header.
        if !extra.is_empty() {
            let mut chunk = (extra.len() as u32).to_be_bytes().to_vec();
            let mut covered = b"tEXt".to_vec();
            covered.extend_from_slice(extra);
            let mut crc = 0xffff_ffffu32;
            for byte in &covered {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    crc = if crc & 1 == 1 {
                        (crc >> 1) ^ 0xedb8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            chunk.extend_from_slice(&covered);
            chunk.extend_from_slice(&(!crc).to_be_bytes());
            file.splice(33..33, chunk);
        }
        file
    };
    let (first, second) = (png(b""), png(b"Comment\0another file"));
    let mut ids = Vec::new();
    for (name, bytes) in [("one.png", &first), ("two.png", &second)] {
        let file = dir.path().join(name);
        std::fs::write(&file, bytes).expect("the fixture is written");
        let ran = whizbang(
            &[
                "import",
                "--create",
                "--picture",
                file.to_str().expect("UTF-8"),
            ],
            &root,
        );
        assert_eq!(ran.code, Some(0), "{}", ran.err);
        ids.push(
            ran.out
                .split_whitespace()
                .last()
                .expect("an identity")
                .to_owned(),
        );
    }
    // One picture, from two files: the second is already there, and still named.
    assert_eq!(ids[0], ids[1]);
    let reader = Reader::open(&root, Arc::new(RealFs)).expect("a reader");
    let generation = reader.snapshot().expect("a generation");
    for bytes in [&first, &second] {
        let name = format!("imports/picture/{}", Digest::of_bytes(bytes).hex());
        let entry = generation.get(&name).expect("the import's name");
        assert_eq!(reader.read(entry).expect("its content"), ids[0].as_bytes());
    }
    let found = whizbang::import::pictures_by_file(&root).expect("the pictures");
    assert_eq!(found.len(), 2);
    assert!(found.values().all(|id| id.to_string() == ids[0]));
    // A name under the prefix whose tail is no file's digest is passed over.
    let store = Store::open(&root, Arc::new(RealFs)).expect("a store");
    let mut publication = store.begin().expect("a publication");
    publication
        .put("imports/picture/not-a-digest", b"sha256:0")
        .expect("a name");
    publication.publish().expect("published");
    assert_eq!(
        whizbang::import::pictures_by_file(&root)
            .expect("the pictures")
            .len(),
        2
    );
}

/// The real binary with these arguments, as they are: its exit code and its output.
fn binary(args: &[&str]) -> Ran {
    let out = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
        .args(args)
        .output()
        .expect("the binary runs");
    Ran {
        code: out.status.code(),
        out: String::from_utf8(out.stdout).expect("output is UTF-8"),
        err: String::from_utf8(out.stderr).expect("output is UTF-8"),
    }
}

#[test]
fn the_binary_deletes_a_run_with_its_children_while_another_process_reads_the_generation_before() {
    // The deletion's carry-over row (plan m5, task H1, item 7): a recipe's run and the
    // runs its steps made, made by the binary; this process reads the generation from
    // before; another process, the binary, deletes the recipe's run with its children
    // and prunes. What this process holds stays readable; once it lets go, the next
    // prune removes it.
    let dir = Scratch::new("cli-store-tree");
    let root = dir.path().join("store");
    let picture = dir.path().join("p.png");
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/compare");
    std::fs::copy(fixtures.join("before.png"), &picture).unwrap();
    let store = root.to_str().unwrap();
    let imported = binary(&[
        "store",
        "import",
        "--picture",
        picture.to_str().unwrap(),
        "--create",
        "--store",
        store,
    ]);
    assert_eq!(imported.code, Some(0), "{}", imported.err);
    let image = imported.out.lines().last().unwrap();
    let image = image.strip_prefix("imported image ").unwrap();
    let ran = binary(&[
        "rascal",
        "mask-rect",
        "--image",
        image,
        "--box",
        "2 2 10 10",
        "--store",
        store,
    ]);
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    let parent = ran.out.lines().next().unwrap();
    let parent = parent
        .strip_prefix("run ")
        .unwrap()
        .strip_suffix(" completed")
        .unwrap();
    let children: Vec<&str> = ran
        .out
        .lines()
        .filter_map(|line| line.strip_prefix("step "))
        .map(|line| line.rsplit(' ').next().unwrap())
        .collect();
    assert_eq!(children.len(), 2, "{}", ran.out);

    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    assert_eq!(generation.runs().len(), 3);
    let deleted = whizbang(&["delete", parent], &root);
    assert_eq!(deleted.code, Some(0), "{}", deleted.err);
    assert!(
        deleted.out.starts_with("runs deleted: 3\n"),
        "{}",
        deleted.out
    );
    assert!(
        deleted
            .out
            .contains("generations left for a reader that still holds them: 1"),
        "{}",
        deleted.out
    );
    // Every reference of what this process holds reads whole.
    for entry in generation.references() {
        reader.read(entry).unwrap();
    }
    let after = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let now = after.snapshot().unwrap();
    assert!(now.runs().is_empty(), "{:?}", now.runs().keys());
    for child in &children {
        assert!(!now.runs().contains_key(*child));
    }
    drop(after);
    drop(reader);
    let pruned = whizbang(&["prune"], &root);
    assert_eq!(pruned.code, Some(0), "{}", pruned.err);
    assert!(
        pruned.out.starts_with("generations removed: 1\n"),
        "{}",
        pruned.out
    );
}
