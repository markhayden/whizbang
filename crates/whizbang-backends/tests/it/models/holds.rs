//! Holds on an installation (ADR 0017, "Holds"): a shared lock on the installation's
//! hold file, given only on what the pointer names, and counted inside a process. One
//! `Holds` stands for one process; one test runs a real second one, the test's own
//! program run once more, and one kills it.

use std::path::Path;
use std::sync::Arc;

use whizbang_backends::models::{
    Current, Hold, HoldRefused, Holds, InstallationRecord, ModelName, ModelsDir,
};
use whizbang_core::store::fs::{Fs, Held, RealFs};
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;

use super::attaching::{Other, root_from_parent, say, told};
use super::publishing::{
    entry_at, first, folder_of, hold_file_of, installations, installed, is_whole, reading,
    record_of, second, tiny,
};
use super::staging::names_under;
use super::{MARKER, model, none, secrets};

/// The holds of a process of its own over this directory, on the real file system.
pub(super) fn holds_over(root: &Path) -> Holds {
    Holds::over(ModelsDir::at(Arc::new(RealFs), root))
}

/// A hold on this installation of the model `tiny`.
pub(super) fn hold(holds: &Holds, record: &InstallationRecord) -> Result<Hold, HoldRefused> {
    holds.hold(&model("tiny"), record.identity(), &none())
}

pub(super) fn count(holds: &Holds, record: &InstallationRecord) -> usize {
    holds.count(&model("tiny"), record.identity())
}

/// The installation's hold file taken alone by another process, by one try: nothing
/// when a hold is on it. This is how whoever deletes an installation's folder asks.
pub(super) fn alone(root: &Path, record: &InstallationRecord) -> Option<Box<dyn Held>> {
    RealFs.try_alone(&hold_file_of(root, record)).unwrap()
}

// --- holds are given ---------------------------------------------------------------------------

/// Run only in a process of its own: holds the second entry's installation in the
/// directory it is told of, says so, and lets it go when a line arrives on its input.
#[test]
#[ignore = "run in a process of its own by the tests of a hold in a second process"]
fn child_holds_the_model_until_it_is_told() {
    let holds = holds_over(&root_from_parent());
    let held = hold(&holds, &record_of(&second())).unwrap();
    say(&format!("holds {}", held.installation()));
    told();
    held.let_go();
    say("let go");
}

const CHILD: &str = "models::holds::child_holds_the_model_until_it_is_told";

#[test]
fn two_holds_in_one_process_and_one_in_a_real_second_process_are_all_given() {
    let scratch = Scratch::new("holds-given");
    let root = scratch.path();
    let record = installed(root, &second());
    let fs = Arc::new(ShimFs::new());
    let holds = Holds::over(ModelsDir::at(fs.clone(), root));
    let one = hold(&holds, &record).unwrap();
    let two = hold(&holds, &record).unwrap();
    assert_eq!(one.installation(), record.identity());
    assert_eq!(count(&holds, &record), 2);
    // Two holds of one process are one share of the hold file, and a count.
    let shares = |fs: &ShimFs| {
        let tries = fs.events();
        tries
            .iter()
            .filter(|event| **event == Event::TryShare(hold_file_of(root, &record)))
            .count()
    };
    assert_eq!(shares(&fs), 1);
    let mut other = Other::running(CHILD, root);
    assert_eq!(other.says("holds"), record.identity().to_string());
    // Three holds: nobody takes the hold file alone. With this process's two let
    // go, the other's is still there; with that let go, the file is free.
    assert!(alone(root, &record).is_none());
    drop(one);
    assert_eq!(count(&holds, &record), 1);
    assert!(alone(root, &record).is_none());
    drop(two);
    assert_eq!(count(&holds, &record), 0);
    assert!(alone(root, &record).is_none());
    other.tell("go");
    assert_eq!(other.says("let"), "go");
    other.ended_well();
    assert!(alone(root, &record).is_some());
    // The share went with the last hold: the next hold tries for a new one.
    let again = hold(&holds, &record).unwrap();
    assert_eq!(shares(&fs), 2);
    assert_eq!(count(&holds, &record), 1);
    drop(again);
}

#[test]
fn a_hold_is_taken_under_the_directorys_lock_and_makes_nothing() {
    let scratch = Scratch::new("holds-steps");
    let root = scratch.path().to_path_buf();
    let record = installed(&root, &second());
    let before = names_under(&root);
    let pointer = tiny(&root).join("CURRENT");
    let file = hold_file_of(&root, &record);
    let fs = Arc::new(ShimFs::new());
    // When the share is tried for, the directory's lock is held: nobody else has it.
    let at = root.clone();
    fs.pause_before(3, move || {
        let directory = RealFs.hold_lock_file(&at.join("LOCK"), false).unwrap();
        assert!(directory.is_none());
    });
    let holds = Holds::over(ModelsDir::at(fs.clone(), &root));
    let held = hold(&holds, &record).unwrap();
    assert_eq!(
        fs.events(),
        [
            Event::Lock(root.join("LOCK")),
            Event::Open(pointer.clone()),
            Event::Read(pointer),
            Event::TryShare(file),
        ]
    );
    // And let go once the hold is given or refused.
    assert!(
        RealFs
            .hold_lock_file(&root.join("LOCK"), false)
            .unwrap()
            .is_some()
    );
    assert_eq!(names_under(&root), before);
    drop(held);
    assert_eq!(names_under(&root), before);
}

#[test]
fn a_hold_keeps_a_superseded_installation_on_the_disk_until_it_is_let_go() {
    let scratch = Scratch::new("holds-superseded");
    let root = scratch.path();
    let old = installed(root, &first());
    let holds = holds_over(root);
    let held = hold(&holds, &old).unwrap();
    // The install that replaces it leaves it, whole, and does not wait.
    let new = installed(root, &second());
    assert_eq!(reading(root), Current::Installed(new.clone()));
    is_whole(root, &old);
    held.let_go();
    // Let go: the next to take the model's lock deletes it.
    installed(root, &second());
    assert_eq!(installations(root), [new.identity().hex()]);
}

// --- a hold is refused -------------------------------------------------------------------------

#[test]
fn a_hold_on_an_identity_the_pointer_no_longer_names_is_refused() {
    let scratch = Scratch::new("holds-not-named");
    let root = scratch.path();
    let old = installed(root, &first());
    let holds = holds_over(root);
    let kept = hold(&holds, &old).unwrap();
    let new = installed(root, &second());
    // Superseded, and still held by this process: the count does not stand in for
    // the pointer. A new hold on it is refused, and the one that was given stays.
    assert_eq!(hold(&holds, &old).unwrap_err(), HoldRefused::NotNamed);
    assert_eq!(count(&holds, &old), 1);
    assert!(alone(root, &old).is_none());
    is_whole(root, &old);
    // What the pointer names is held.
    let current = hold(&holds, &new).unwrap();
    drop(current);
    drop(kept);
    // An identity that was never installed, and a model that never was: not named.
    let never = record_of(&entry_at("2026-11.9"));
    assert_eq!(hold(&holds, &never).unwrap_err(), HoldRefused::NotNamed);
    let before = names_under(root);
    let refused = holds.hold(&model("other"), new.identity(), &none());
    assert_eq!(refused.unwrap_err(), HoldRefused::NotNamed);
    assert_eq!(names_under(root), before);
    // With no pointer the model is not installed, whatever its folder holds: the
    // installation is whole, has its hold file, and is not held.
    std::fs::remove_file(tiny(root).join("CURRENT")).unwrap();
    assert_eq!(hold(&holds, &new).unwrap_err(), HoldRefused::NotNamed);
    assert!(folder_of(root, &new).exists());
    assert_eq!(count(&holds, &new), 0);
    assert!(alone(root, &new).is_some());
}

#[test]
fn a_hold_is_refused_while_the_mark_of_draining_is_set_whatever_the_count() {
    let scratch = Scratch::new("holds-draining");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    let kept = hold(&holds, &record).unwrap();
    // The mark, as a removal sets it (task M5b): read as there or not there.
    let mark = tiny(root).join("DRAINING");
    std::fs::write(&mark, b"").unwrap();
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
    assert_eq!(count(&holds, &record), 1);
    // The mark is said before the pointer: a model whose pointer a removal has
    // cleared is draining, not "not named".
    let pointer = std::fs::read(tiny(root).join("CURRENT")).unwrap();
    std::fs::remove_file(tiny(root).join("CURRENT")).unwrap();
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
    std::fs::write(tiny(root).join("CURRENT"), pointer).unwrap();
    // With the mark gone the next hold is given.
    std::fs::remove_file(&mark).unwrap();
    let next = hold(&holds, &record).unwrap();
    assert_eq!(count(&holds, &record), 2);
    drop((kept, next));
}

#[test]
fn a_pointer_that_cannot_be_read_or_a_missing_hold_file_is_damage_and_a_hold_makes_none() {
    let scratch = Scratch::new("holds-damage");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    // The hold file is gone: a hold never makes one.
    std::fs::remove_file(hold_file_of(root, &record)).unwrap();
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Damaged);
    assert!(!hold_file_of(root, &record).exists());
    assert_eq!(count(&holds, &record), 0);
    std::fs::write(hold_file_of(root, &record), b"").unwrap();
    drop(hold(&holds, &record).unwrap());
    // A pointer that is not an identity's text and one line feed.
    let pointer = tiny(root).join("CURRENT");
    let written = std::fs::read(&pointer).unwrap();
    for damaged in [&written[..71], b"damaged".as_slice()] {
        std::fs::write(&pointer, damaged).unwrap();
        assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Damaged);
    }
    // A pointer that holds a secret the holder was told of is damage too.
    std::fs::write(&pointer, &written).unwrap();
    let text = String::from_utf8(written).unwrap();
    let told = whizbang_core::contract::Secrets::new([text[7..40].to_owned()]).unwrap();
    let refused = holds.hold(&model("tiny"), record.identity(), &told);
    assert_eq!(refused.unwrap_err(), HoldRefused::Damaged);
    assert_eq!(count(&holds, &record), 0);
}

#[test]
fn a_hold_file_held_alone_is_a_hold_refused_at_once_and_a_try_that_fails_is_an_error() {
    let scratch = Scratch::new("holds-alone");
    let root = scratch.path();
    let record = installed(root, &second());
    // Whoever deletes an installation's folder has its hold file alone. That is
    // only for an installation the pointer does not name; a hold that meets it
    // does not wait.
    let deleting = alone(root, &record).unwrap();
    let fs = Arc::new(ShimFs::new());
    let holds = Holds::over(ModelsDir::at(fs.clone(), root));
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Deleting);
    assert_eq!(count(&holds, &record), 0);
    drop(deleting);
    // A file system that refuses the try has not said "held alone": an error.
    fs.refuse_matching(0, |event| matches!(event, Event::TryShare(_)));
    let refused = hold(&holds, &record).unwrap_err();
    assert!(matches!(refused, HoldRefused::Disk(_)), "{refused:?}");
    assert_eq!(count(&holds, &record), 0);
    drop(hold(&holds, &record).unwrap());
    // The directory's lock, not let go by whoever has it: refused, not waited for.
    let directory = RealFs.hold_lock_file(&root.join("LOCK"), false).unwrap();
    assert!(directory.is_some());
    assert_eq!(
        hold(&holds, &record).unwrap_err(),
        HoldRefused::DirectoryBusy
    );
    drop(directory);
    drop(hold(&holds, &record).unwrap());
}

#[test]
fn a_model_name_that_holds_a_secret_opens_nothing_and_no_refusal_shows_a_path() {
    let scratch = Scratch::new("holds-secret");
    let root = scratch.path();
    let record = installed(root, &second());
    let fs = Arc::new(ShimFs::new());
    let holds = Holds::over(ModelsDir::at(fs.clone(), root));
    let named = ModelName::new(MARKER, &none()).unwrap();
    let refused = holds.hold(&named, record.identity(), &secrets());
    assert_eq!(refused.unwrap_err(), HoldRefused::HoldsSecret);
    assert!(fs.events().is_empty());
    let all = [
        HoldRefused::HoldsSecret,
        HoldRefused::NotNamed,
        HoldRefused::Draining,
        HoldRefused::Damaged,
        HoldRefused::Deleting,
        HoldRefused::DirectoryBusy,
        HoldRefused::Disk(std::io::ErrorKind::PermissionDenied),
    ];
    for refused in all {
        let printed = format!("{refused} {refused:?}");
        assert!(
            printed.bytes().all(|b| b.is_ascii_graphic() || b == b' '),
            "{printed:?}"
        );
        assert!(printed.len() <= 200, "{printed}");
        for part in [&*root.to_string_lossy(), "holds/", "models/", MARKER] {
            assert!(!printed.contains(part), "{printed}");
        }
    }
    // What is printed of a hold and of the holds is an identity, and no path.
    let held = hold(&holds, &record).unwrap();
    let printed = format!("{held:?} {holds:?}");
    assert_eq!(printed, format!("Hold({}) Holds", record.identity()));
}

// --- a hold is let go --------------------------------------------------------------------------

#[test]
fn a_hold_let_go_twice_is_let_go_once() {
    let scratch = Scratch::new("holds-twice");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    let one = hold(&holds, &record).unwrap();
    let two = hold(&holds, &record).unwrap();
    one.let_go();
    one.let_go();
    // Let go twice, and then dropped: the other hold is counted still, and the
    // installation is held.
    assert_eq!(count(&holds, &record), 1);
    assert!(alone(root, &record).is_none());
    drop(one);
    assert_eq!(count(&holds, &record), 1);
    assert!(alone(root, &record).is_none());
    two.let_go();
    assert_eq!(count(&holds, &record), 0);
    assert!(alone(root, &record).is_some());
    drop(two);
    assert_eq!(count(&holds, &record), 0);
    // And a hold taken after is one hold.
    let three = hold(&holds, &record).unwrap();
    assert_eq!(count(&holds, &record), 1);
    drop(three);
    assert!(alone(root, &record).is_some());
}

#[test]
fn a_child_process_that_holds_a_model_and_is_killed_leaves_no_hold() {
    let scratch = Scratch::new("holds-killed");
    let root = scratch.path();
    let record = installed(root, &second());
    let mut other = Other::running(CHILD, root);
    assert_eq!(other.says("holds"), record.identity().to_string());
    assert!(alone(root, &record).is_none());
    // Superseded while the other process holds it, it is left on the disk.
    let new = installed(root, &entry_at("2026-11.3"));
    is_whole(root, &record);
    // Killed: the hold file is taken alone, and the next to take the model's lock
    // deletes the folder. Nothing was read from a file to learn that it died.
    other.killed();
    assert!(alone(root, &record).is_some());
    installed(root, &entry_at("2026-11.3"));
    assert_eq!(installations(root), [new.identity().hex()]);
    // The same of a process that a disk of its own stands for, which loses power.
    let fs = Arc::new(ShimFs::new());
    let holds = Holds::over(ModelsDir::at(fs.clone(), root));
    let held = hold(&holds, &new).unwrap();
    assert!(alone(root, &new).is_none());
    fs.crash(Lost::Unsynced);
    assert!(alone(root, &new).is_some());
    drop(held);
}

#[test]
fn the_last_hold_let_go_and_a_new_one_taken_by_two_threads_never_leave_a_hold_without_its_share() {
    // Two threads of one process take a hold and let it go, 200 times each. The
    // last hold let go by one and a new one taken by the other go through one guard:
    // whoever has a hold finds the hold file shared, every time.
    let scratch = Scratch::new("holds-threads");
    let root = scratch.path().to_path_buf();
    let record = installed(&root, &second());
    let holds = holds_over(&root);
    let mut workers = Vec::new();
    for _ in 0..2 {
        let (holds, root, record) = (holds.clone(), root.clone(), record.clone());
        workers.push(threads::spawn(move || {
            for round in 0..200 {
                let held = hold(&holds, &record).unwrap();
                assert!(count(&holds, &record) >= 1, "round {round}");
                assert!(alone(&root, &record).is_none(), "round {round}");
                drop(held);
            }
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(count(&holds, &record), 0);
    assert!(alone(&root, &record).is_some());
}
