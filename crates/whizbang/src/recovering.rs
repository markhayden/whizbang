//! Tests of the writing commands on the one way to a writer (ADR 0015; plan 1B, task
//! I4): each recovers the store before it changes anything, each is refused with the
//! reason after a recovery that failed, and at the limit on a generation file
//! `store delete` and `store prune` alone still work. They call the commands as the
//! binary does, with a fake part in the recovery where the binary has its own.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang::wiring::{self, Make};
use whizbang_core::artifact::{Mask, Origin, Source, Step, UseCase};
use whizbang_core::contract::{Recovery, RecoveryPart};
use whizbang_core::identity::{Binding, Digest, PixelDigest, Space};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Reader};
use whizbang_testkit::recover::FakeRecovery;
use whizbang_testkit::scratch::Scratch;

use crate::store::Where;
use crate::{rascal, runs, store};

/// The runs a killed process left: each made, with one name, and never ended.
const LEFT: [&str; 2] = ["1000-1", "1000-2"];

/// A small mask, and its canonical file in the folder.
fn mask_file(dir: &Scratch) -> (PathBuf, String) {
    let binding = Binding::root(
        PixelDigest(Digest::of_bytes(b"photo")),
        Space::flat(6, 4).unwrap(),
    );
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    let mut core = vec![0; 24];
    core[8] = 255;
    let mask = Mask::from_core(binding, UseCase::Remove, core, Source::default(), origin).unwrap();
    let file = dir.path().join("fixture.mask");
    std::fs::write(&file, mask.to_bytes()).unwrap();
    (file, mask.id().0.to_string())
}

/// A store that a killed process left with two runs made and not ended, and a mask
/// in it. The store's directory, the mask's file and its identity.
fn left(dir: &Scratch) -> (PathBuf, PathBuf, String) {
    let root = dir.path().join("store");
    let (file, id) = mask_file(dir);
    let opening = wiring::opening(&root, Make::TheRoot, Recovery::none()).unwrap();
    let writer = opening.writer().unwrap();
    for run in LEFT {
        let mut publication = writer.begin().unwrap();
        publication.create_run(run).unwrap();
        let admitted = format!("the admission of {run}\n");
        publication
            .put_in(run, "admitted", Kind::Diagnostic, admitted.as_bytes())
            .unwrap();
        publication.publish().unwrap();
    }
    drop((writer, opening));
    let import = store::Command::Import {
        file: Some(file.clone()),
        picture: None,
        at: Where::at(&root),
        create: false,
    };
    store::run_with(&import, Recovery::none()).unwrap();
    (root, file, id)
}

/// A part that marks every run the store holds, and makes one name outside runs.
fn marking() -> Arc<FakeRecovery> {
    let part = FakeRecovery::new("fake");
    part.make("recovered/fake", b"once\n");
    part.make_in_runs("marked", b"left\n");
    Arc::new(part)
}

fn with(part: &Arc<FakeRecovery>) -> Recovery {
    let part: Arc<dyn RecoveryPart> = part.clone();
    Recovery::none().with_part(part)
}

/// The store's generation number, whether the part's name outside runs is there, and
/// each run with whether it is marked.
fn held(root: &Path) -> (u64, bool, Vec<(String, bool)>) {
    let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let runs = generation.runs().iter();
    (
        generation.number(),
        generation.get("recovered/fake").is_some(),
        runs.map(|(id, run)| (id.clone(), run.get("marked").is_some()))
            .collect(),
    )
}

fn delete(root: &Path, run: &str) -> store::Command {
    store::Command::Delete {
        at: Where::at(root),
        runs: vec![run.to_owned()],
    }
}

fn prune(root: &Path) -> store::Command {
    store::Command::Prune {
        at: Where::at(root),
    }
}

fn import(root: &Path, file: &Path) -> store::Command {
    store::Command::Import {
        file: Some(file.to_path_buf()),
        picture: None,
        at: Where::at(root),
        create: false,
    }
}

fn grow(root: &Path, mask: &str) -> rascal::Command {
    rascal::Command::Grow {
        mask: mask.to_owned(),
        px: 1.0,
        down_px: None,
        at: Where::at(root),
        under: rascal::Under::default(),
        request_id: None,
    }
}

/// The same command on another store a killed process left, with the recovery this
/// build is wired with (`wiring::recovery`) in place of the fake: when the command
/// has run, every left run it did not delete is ended, with its record. The runs'
/// admissions here are not ones this build reads, so each is ended from its name.
fn with_the_builds_own_recovery(
    label: &str,
    command: impl FnOnce(&Path, &Path, &str) -> Result<String, String>,
) {
    let dir = Scratch::new(label);
    let (root, file, mask) = left(&dir);
    command(&root, &file, &mask).unwrap();
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let kept: Vec<&str> = LEFT
        .iter()
        .copied()
        .filter(|run| generation.run(run).is_some())
        .collect();
    assert!(!kept.is_empty());
    for run in kept {
        let stored = whizbang_core::contract::record::stored(&generation, run, |e| reader.read(e));
        let record = stored.unwrap().unwrap();
        let error = record.outcome().error.as_ref().unwrap();
        assert_eq!(error.kind, "interrupted", "{run}");
    }
}

/// The part was asked once, was shown the store as the killed process left it, and
/// made its three names there.
fn recovered_first(part: &FakeRecovery, found: u64) {
    let [shown] = part.shown().try_into().unwrap();
    assert_eq!((shown.generation, shown.made), (found, 3));
}

// --- each writing command recovers before it changes anything ----------------------------------

#[test]
fn store_delete_on_a_store_a_killed_process_left_recovers_before_it_deletes() {
    let dir = Scratch::new("recovering-delete");
    let (root, _, _) = left(&dir);
    let found = held(&root).0;
    let part = marking();
    let said = store::run_with(&delete(&root, LEFT[0]), with(&part)).unwrap();
    assert!(said.starts_with("runs deleted: 1\n"), "{said}");
    recovered_first(&part, found);
    // The recovery, then the deletion: two generations, and the run that is left
    // has the mark the run that is gone was given too.
    assert_eq!(
        held(&root),
        (found + 2, true, vec![(LEFT[1].to_owned(), true)])
    );
    with_the_builds_own_recovery("recovering-delete-own", |root, _, _| {
        store::run_with(&delete(root, LEFT[0]), wiring::recovery())
    });
}

#[test]
fn store_prune_on_a_store_a_killed_process_left_recovers_before_it_prunes() {
    let dir = Scratch::new("recovering-prune");
    let (root, _, _) = left(&dir);
    let found = held(&root).0;
    let part = marking();
    let said = store::run_with(&prune(&root), with(&part)).unwrap();
    recovered_first(&part, found);
    // The generation the store was found at is among those the prune removed: the
    // recovery was published before anything was removed.
    assert!(
        said.starts_with(&format!("generations removed: {found}\n")),
        "{said}"
    );
    let marked = LEFT.iter().map(|run| ((*run).to_owned(), true)).collect();
    assert_eq!(held(&root), (found + 1, true, marked));
    let generations = root.join("generations");
    assert!(!generations.join(format!("{found}.json")).exists());
    with_the_builds_own_recovery("recovering-prune-own", |root, _, _| {
        store::run_with(&prune(root), wiring::recovery())
    });
}

#[test]
fn store_import_on_a_store_a_killed_process_left_recovers_before_it_imports() {
    let dir = Scratch::new("recovering-import");
    let (root, _, _) = left(&dir);
    let found = held(&root).0;
    // Another artifact than the one the store holds: a mask of another use.
    let binding = Binding::root(
        PixelDigest(Digest::of_bytes(b"another photo")),
        Space::flat(6, 4).unwrap(),
    );
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    let other = Mask::from_core(
        binding,
        UseCase::Remove,
        vec![255; 24],
        Source::default(),
        origin,
    );
    let file = dir.path().join("other.mask");
    std::fs::write(&file, other.unwrap().to_bytes()).unwrap();
    let part = marking();
    let said = store::run_with(&import(&root, &file), with(&part)).unwrap();
    assert!(said.starts_with("imported mask sha256:"), "{said}");
    recovered_first(&part, found);
    let marked = LEFT.iter().map(|run| ((*run).to_owned(), true)).collect();
    assert_eq!(held(&root), (found + 2, true, marked));
    with_the_builds_own_recovery("recovering-import-own", |root, _, _| {
        store::run_with(&import(root, &file), wiring::recovery())
    });
}

#[test]
fn rascal_grow_on_a_store_a_killed_process_left_recovers_before_its_run_is_admitted() {
    let dir = Scratch::new("recovering-grow");
    let (root, _, mask) = left(&dir);
    let found = held(&root).0;
    let part = marking();
    let said = rascal::run_with(&grow(&root, &mask), with(&part)).unwrap();
    assert!(said.contains(" completed\n"), "{said}");
    recovered_first(&part, found);
    // The recovery, the admission and the end: the run the command made is there,
    // beside the two that were left, and it was not there to be marked.
    let (number, named, runs) = held(&root);
    assert_eq!((number, named), (found + 3, true));
    let marks: Vec<bool> = runs.iter().map(|(_, marked)| *marked).collect();
    assert_eq!(runs.len(), 3, "{runs:?}");
    assert_eq!(marks.iter().filter(|marked| **marked).count(), 2);
    for run in LEFT {
        assert!(runs.contains(&(run.to_owned(), true)), "{runs:?}");
    }
    with_the_builds_own_recovery("recovering-grow-own", |root, _, mask| {
        rascal::run_with(&grow(root, mask), wiring::recovery())
    });
}

// --- after a recovery that failed --------------------------------------------------------------

#[test]
fn after_a_failed_recovery_every_writing_command_is_refused_with_the_reason_and_a_reading_one_works()
 {
    let dir = Scratch::new("recovering-failed");
    let (root, file, mask) = left(&dir);
    let before = held(&root);
    let failing = || {
        let part = marking();
        part.fail("it could not read what was left");
        part
    };
    let reason = "the store could not be recovered, so it is open to read only: recovery \
                  failed before anything of it was published (its part \"fake\" failed: it \
                  could not read what was left); nothing was changed";
    let refused = [
        store::run_with(&delete(&root, LEFT[0]), with(&failing())),
        store::run_with(&prune(&root), with(&failing())),
        store::run_with(&import(&root, &file), with(&failing())),
        rascal::run_with(&grow(&root, &mask), with(&failing())),
    ];
    for answered in refused {
        assert_eq!(answered, Err(reason.to_owned()));
    }
    assert!(!reason.contains(root.to_str().unwrap()));
    // Nothing was changed, by the recovery or by a command.
    assert_eq!(held(&root), before);
    // Reading works: the store's runs are listed, by both commands that list them.
    let listed = store::run_with(
        &store::Command::List {
            at: Where::at(&root),
        },
        with(&failing()),
    );
    assert!(listed.unwrap().contains("runs: 2\n"));
    let listed = runs::run(&runs::Command::List {
        at: Where::at(&root),
    });
    let listed = String::from_utf8(listed.unwrap()).unwrap();
    assert!(listed.starts_with("runs: 2\n"), "{listed}");
}

// --- a store that is held, and what a refusal names ---------------------------------------------

#[test]
fn a_store_another_opening_holds_refuses_every_writing_command_in_words_true_of_this_process_and_of_another()
 {
    let dir = Scratch::new("recovering-held");
    let (root, file, mask) = left(&dir);
    let before = held(&root);
    // The holder is this process: an opening that is never asked for its writer.
    let holder = wiring::opening(&root, Make::Nothing, Recovery::none()).unwrap();
    let part = marking();
    let refused = [
        store::run_with(&delete(&root, LEFT[0]), with(&part)),
        store::run_with(&prune(&root), with(&part)),
        store::run_with(&import(&root, &file), with(&part)),
        rascal::run_with(&grow(&root, &mask), with(&part)),
    ];
    for answered in refused {
        let why = answered.unwrap_err();
        // The store's own sentence, which names the store as the caller gave it,
        // and what is true whoever holds it.
        assert!(why.starts_with("the store at "), "{why}");
        assert!(why.contains(root.to_str().unwrap()), "{why}");
        assert!(
            why.ends_with(
                "one process at a time may write a store (the writer that holds it is \
                 another opening of the store, in another process or in this one)"
            ),
            "{why}"
        );
    }
    // Nothing was recovered and nothing changed: no part was asked.
    assert_eq!((part.asked(), held(&root)), (0, before));
    drop(holder);
    // A store that is not there is said as the store says it, with its path: the
    // complaint is escaped where the binary prints it.
    let nowhere = dir.path().join("nowhere");
    let why = store::run_with(&prune(&nowhere), with(&part)).unwrap_err();
    assert_eq!(
        why,
        wiring::not_opened(&whizbang_core::store::Error::Missing { root: nowhere })
    );
    assert!(why.starts_with("there is no store at "), "{why}");
}

// --- at the limit on a generation file ---------------------------------------------------------

#[test]
fn at_the_generation_limit_store_delete_and_prune_work_and_every_other_writing_command_is_refused_and_told_why()
 {
    let dir = Scratch::new("recovering-limit");
    let (root, file, mask) = left(&dir);
    let found = held(&root).0;
    // Room for one name more than the store holds: less than a recovery adds.
    let generation = root.join("generations").join(format!("{found}.json"));
    let limit = std::fs::metadata(generation).unwrap().len() + 200;
    let part = marking();
    let limited = || with(&part).limiting_generations_to(limit);
    let told = |answered: Result<String, String>| {
        let why = answered.unwrap_err();
        assert!(
            why.starts_with(
                "the store is open for deleting and pruning alone: its recovery could not \
                 be published, because the generation file would be "
            ),
            "{why}"
        );
        assert!(why.ends_with("delete runs and prune, and the next open recovers it"));
    };
    told(store::run_with(&import(&root, &file), limited()));
    told(rascal::run_with(&grow(&root, &mask), limited()));
    assert_eq!(held(&root).0, found, "nothing was published");
    // Pruning works, and deleting, which makes room.
    store::run_with(&prune(&root), limited()).unwrap();
    let said = store::run_with(&delete(&root, LEFT[0]), limited()).unwrap();
    assert!(said.starts_with("runs deleted: 1\n"), "{said}");
    assert_eq!(
        held(&root),
        (found + 1, false, vec![(LEFT[1].to_owned(), false)])
    );
    // The next open recovers, under the same limit, and the command goes on.
    store::run_with(&prune(&root), limited()).unwrap();
    assert_eq!(
        held(&root),
        (found + 2, true, vec![(LEFT[1].to_owned(), true)])
    );
}
