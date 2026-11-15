//! An install under a crash and under a race (ADR 0017, "For later tasks": the crashes
//! the tests hold). A crash is tried in place of each event of an install, each way
//! the test kit's disk can lose what was not durable and each way it can leave what
//! was appended and not synced; a process dies inside a download; and two installers,
//! each a process of its own, are let go together.

use std::path::Path;
use std::sync::Arc;

use whizbang_backends::models::{Current, Entry, InstallRefused, ModelsDir, State, install};
use whizbang_core::contract::Cancellation;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs, Tail};
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;

use super::none;
use super::publishing::{
    adapter, entry_at, events_after, first, flip, folder_of, hold_file_of, index_of, installations,
    installed, installing, is_whole, reading, record_of, said, second, tiny,
};
use super::staging::{TERMS, at, graph, incoming, serving_both};

/// The ways a crash loses names: all that were not synced, none, those of some
/// folders, and some names of a folder without the others.
const LOSSES: [Lost; 6] = [
    Lost::Unsynced,
    Lost::ContentOnly,
    Lost::Subset(0b01),
    Lost::Subset(0b10),
    Lost::Names(0b0101),
    Lost::Names(0b1010),
];

/// What a crash leaves of a streamed file's bytes that were not synced.
const TAILS: [Tail; 4] = [Tail::Half, Tail::Zeroed, Tail::All, Tail::None];

/// What is swept.
#[derive(Clone, Copy, PartialEq)]
enum Sweep {
    /// An install into a directory that holds nothing. The process that reopens it
    /// installs the same identity: what the crash left under `installations/` is
    /// what it finds there.
    First,
    /// An install over another installation. The process that reopens it installs
    /// a third identity: what the crash left is what it deletes.
    Over,
    /// A repair: the installation in use has a file with one byte another, and the
    /// same identity is installed again.
    Repair,
}

impl Sweep {
    /// The directory as it is before the install that crashes, and what its pointer
    /// says then.
    fn prepare(self, root: &Path) -> Current {
        match self {
            Sweep::First => Current::NotInstalled,
            Sweep::Over => Current::Installed(installed(root, &first())),
            Sweep::Repair => {
                let record = installed(root, &second());
                flip(&folder_of(root, &record).join("files/graph.onnx"), 77);
                Current::Installed(record)
            }
        }
    }

    fn reopening(self) -> Entry {
        match self {
            Sweep::Over => entry_at("2026-11.3"),
            _ => second(),
        }
    }
}

/// Installs the second entry with a crash in place of each event of one part of the
/// install's events, under each loss and each tail. `part` of `parts` cuts the sweep
/// so that each test stays short; together the parts are every event.
fn crashed_at_each_event(sweep: Sweep, part: usize, parts: usize) {
    let clean = {
        let scratch = Scratch::new("crash-events");
        sweep.prepare(scratch.path());
        let fs = Arc::new(ShimFs::new());
        installing(&fs, scratch.path(), &second(), &serving_both()).unwrap();
        fs.events()
    };
    let pointer = |event: &Event| matches!(event, Event::Rename(_, to) if to.ends_with("CURRENT"));
    let renamed = index_of(&clean, pointer);
    // A repair takes the pointer away before it sets it again.
    let cleared = clean
        .iter()
        .position(|event| matches!(event, Event::Remove(p) if p.ends_with("CURRENT")));
    assert_eq!(cleared.is_some(), sweep == Sweep::Repair);
    let next = Current::Installed(record_of(&second()));
    let last = record_of(&sweep.reopening());
    let events = clean.len() * part / parts..clean.len() * (part + 1) / parts;
    assert!(!events.is_empty());
    for (event, tail) in events.flat_map(|event| TAILS.map(|tail| (event, tail))) {
        for lost in LOSSES {
            let at = format!("event {event}, {lost:?}, {tail:?}");
            let scratch = Scratch::new("crash");
            let root = scratch.path();
            let was = sweep.prepare(root);
            let fs = Arc::new(ShimFs::new().with_tail(tail).crash_before(event, lost));
            let ended = installing(&fs, root, &second(), &serving_both());
            assert!(fs.crashed(), "{at}");
            // The process is gone. One that holds no lock reads the model as it
            // was or as the new installation, and for a repair as not installed
            // in between: never damage, and never a pointer to what is not there.
            let now = reading(root);
            let between = cleared.is_some_and(|cleared| event > cleared);
            if event <= cleared.unwrap_or(renamed) {
                assert_eq!(now, was, "{at}");
                assert!(ended.is_err(), "{at}");
            } else if event > renamed + 1 {
                // The sync after the pointer's rename has run.
                assert_eq!(now, next, "{at}");
            } else {
                let allowed = now == was || now == next;
                assert!(allowed || (between && now == Current::NotInstalled), "{at}");
            }
            // What the pointer names is whole, and its hold file is there. A
            // repair's pointer names the damaged folder until it is cleared.
            if let Current::Installed(record) = &now {
                assert!(hold_file_of(root, record).exists(), "{at}");
                if sweep != Sweep::Repair || event > renamed + 1 {
                    is_whole(root, record);
                }
            }
            // The next to take the model's lock removes the staging that was left
            // and whatever the pointer does not name, and ends with one
            // installation, whole.
            assert_eq!(installed(root, &sweep.reopening()), last, "{at}");
            assert_eq!(reading(root), Current::Installed(last.clone()), "{at}");
            is_whole(root, &last);
            assert_eq!(installations(root), [last.identity().hex()], "{at}");
            assert!(!incoming(root).exists(), "{at}");
            assert!(!tiny(root).join("CURRENT.new").exists(), "{at}");
            assert_eq!(said(root), Some((State::Installed, 2, 2)), "{at}");
        }
    }
}

#[test]
fn a_crash_at_any_event_of_a_first_install_leaves_it_not_installed_or_whole_part_1_of_2() {
    crashed_at_each_event(Sweep::First, 0, 2);
}

#[test]
fn a_crash_at_any_event_of_a_first_install_leaves_it_not_installed_or_whole_part_2_of_2() {
    crashed_at_each_event(Sweep::First, 1, 2);
}

#[test]
fn a_crash_at_any_event_of_an_install_leaves_the_previous_or_the_new_whole_part_1_of_3() {
    crashed_at_each_event(Sweep::Over, 0, 3);
}

#[test]
fn a_crash_at_any_event_of_an_install_leaves_the_previous_or_the_new_whole_part_2_of_3() {
    crashed_at_each_event(Sweep::Over, 1, 3);
}

#[test]
fn a_crash_at_any_event_of_an_install_leaves_the_previous_or_the_new_whole_part_3_of_3() {
    crashed_at_each_event(Sweep::Over, 2, 3);
}

#[test]
fn a_crash_at_any_event_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_1_of_3() {
    crashed_at_each_event(Sweep::Repair, 0, 3);
}

#[test]
fn a_crash_at_any_event_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_2_of_3() {
    crashed_at_each_event(Sweep::Repair, 1, 3);
}

#[test]
fn a_crash_at_any_event_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_3_of_3() {
    crashed_at_each_event(Sweep::Repair, 2, 3);
}

// --- a refused step, and then a crash ----------------------------------------------------------

/// Installs the second entry with each event of one part of the install refused in
/// turn, and crashes straight after: what a refused step left not durable is then
/// lost, each way the disk can lose it. Under the loss of every name that was not
/// synced, the same again with the install asked a second time before the crash:
/// what it answers must be what the crash leaves.
fn crashed_after_each_refused_step(sweep: Sweep, part: usize, parts: usize) {
    let clean = {
        let scratch = Scratch::new("crash-events");
        sweep.prepare(scratch.path());
        let fs = Arc::new(ShimFs::new());
        installing(&fs, scratch.path(), &second(), &serving_both()).unwrap();
        fs.events().len()
    };
    let next = record_of(&second());
    let last = record_of(&sweep.reopening());
    let cases = [
        (Lost::Unsynced, false),
        (Lost::Unsynced, true),
        (Lost::Names(0b0101), false),
        (Lost::Names(0b1010), false),
    ];
    let events = clean * part / parts..clean * (part + 1) / parts;
    for (event, tail) in events.flat_map(|event| TAILS.map(|tail| (event, tail))) {
        for (lost, again) in cases {
            let at = format!("event {event}, {lost:?}, again {again}, {tail:?}");
            let scratch = Scratch::new("crash-refused");
            let root = scratch.path();
            let was = sweep.prepare(root);
            let fs = Arc::new(ShimFs::new().with_tail(tail).fail_before(event));
            let mut ended = installing(&fs, root, &second(), &serving_both());
            if again {
                ended = installing(&fs, root, &second(), &serving_both());
                assert_eq!(ended, Ok(next.clone()), "{at}");
            }
            fs.crash(lost);
            let now = reading(root);
            if ended.is_ok() {
                // What was answered is what the crash left.
                assert_eq!(now, Current::Installed(next.clone()), "{at}");
            } else {
                assert!(
                    now == was || now == Current::Installed(next.clone()),
                    "{at}: {now:?}"
                );
            }
            if let Current::Installed(record) = &now {
                is_whole(root, record);
                assert!(hold_file_of(root, record).exists(), "{at}");
            }
            assert_eq!(installed(root, &sweep.reopening()), last, "{at}");
            assert_eq!(reading(root), Current::Installed(last.clone()), "{at}");
            is_whole(root, &last);
            assert_eq!(installations(root), [last.identity().hex()], "{at}");
            assert!(!incoming(root).exists(), "{at}");
        }
    }
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_1_of_2()
 {
    crashed_after_each_refused_step(Sweep::First, 0, 2);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_2_of_2()
 {
    crashed_after_each_refused_step(Sweep::First, 1, 2);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_an_install_leaves_the_previous_or_the_new_whole_part_1_of_3()
 {
    crashed_after_each_refused_step(Sweep::Over, 0, 3);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_an_install_leaves_the_previous_or_the_new_whole_part_2_of_3()
 {
    crashed_after_each_refused_step(Sweep::Over, 1, 3);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_an_install_leaves_the_previous_or_the_new_whole_part_3_of_3()
 {
    crashed_after_each_refused_step(Sweep::Over, 2, 3);
}

// --- a process that dies inside a download -----------------------------------------------------

#[test]
fn a_process_that_dies_inside_a_download_leaves_the_previous_installation_and_the_next_installs() {
    // Four deaths, one for each tail, each with another loss of names. The installer
    // is a thread on a disk of its own, which stands for its process; it is stopped
    // inside the second file, 1,024 bytes of 2,500 handed over and not synced.
    for (tail, lost) in TAILS.into_iter().zip(LOSSES) {
        let at = format!("{tail:?}, {lost:?}");
        let scratch = Scratch::new("crash-download");
        let root = scratch.path().to_path_buf();
        let before = installed(&root, &first());
        let barriers = Barriers::new();
        let fake = Arc::new(
            FakeDownloader::new()
                .serving(&at_file("LICENSE.txt"), TERMS.to_vec())
                .stopping_after(
                    &at_file("graph.onnx"),
                    graph(),
                    1024,
                    barriers.clone(),
                    "mid",
                ),
        );
        let dying = Arc::new(ShimFs::new().with_tail(tail));
        let installer = {
            let (fs, fake, root) = (dying.clone(), fake.clone(), root.clone());
            threads::spawn(move || installing(&fs, &root, &second(), &fake))
        };
        barriers.arrived("mid", 1);
        // Stopped there, it has the model's lock: another process is busy, asks
        // nothing of its downloader and touches nothing, the staging least of all.
        assert_eq!(said(&root), Some((State::Installing, 1, 2)), "{at}");
        let other = serving_both();
        let fs = Arc::new(ShimFs::new());
        let refused = installing(&fs, &root, &second(), &other).unwrap_err();
        assert_eq!(refused, InstallRefused::Busy, "{at}");
        assert!(
            fs.events().is_empty() && other.requests().is_empty(),
            "{at}"
        );
        assert!(incoming(&root).join("files/graph.onnx").exists(), "{at}");
        // The process dies: its locks are gone, and its disk answers nothing more.
        dying.crash(lost);
        barriers.release("mid");
        let ended = installer.join().unwrap();
        assert!(
            matches!(ended, Err(InstallRefused::Disk(_))),
            "{at}: {ended:?}"
        );
        // The previous installation is named and whole. The status is not durable:
        // it says that an install is running, or what it said before, as the crash
        // kept its name or lost it. With the model's lock free, "installing" is an
        // install that died.
        assert_eq!(reading(&root), Current::Installed(before.clone()), "{at}");
        is_whole(&root, &before);
        assert_eq!(installations(&root), [before.identity().hex()], "{at}");
        let state = said(&root).map(|said| said.0);
        assert!(
            matches!(state, Some(State::Installing | State::Installed)),
            "{at}: {state:?}"
        );
        // The next to take the lock removes what was left in staging, under the
        // lock, before it stages: whatever the crash left of the file is not gone
        // on from. Every byte is asked for again.
        let fake = serving_both();
        let fs = Arc::new(ShimFs::new());
        let after = installing(&fs, &root, &second(), &fake).unwrap();
        let events = fs.events();
        let locked = index_of(&events, |event| {
            *event == Event::Lock(tiny(&root).join("LOCK"))
        });
        let begun = index_of(&events, |event| matches!(event, Event::Begin(_)));
        if let Some(removed) = events
            .iter()
            .position(|event| *event == Event::Remove(incoming(&root)))
        {
            assert!(locked < removed && removed < begun, "{at}");
        }
        assert_eq!(fake.handed(&at_file("graph.onnx")), 2500, "{at}");
        assert_eq!(reading(&root), Current::Installed(after.clone()), "{at}");
        is_whole(&root, &after);
        assert_eq!(installations(&root), [after.identity().hex()], "{at}");
    }
}

fn at_file(name: &str) -> whizbang_core::egress::Address {
    at(name)
}

// --- two installers over one directory ---------------------------------------------------------

#[test]
fn two_installers_stopped_after_one_read_the_pointer_do_not_both_publish() {
    // The first installer is stopped after it took the model's lock and read the
    // pointer, at its first write. The pointer is read under the model's lock, so
    // the second never reads one to decide from: it is busy.
    let clean = events_after(&[&first()], &second());
    let written = index_of(&clean, |event| matches!(event, Event::Write(_)));
    assert!(
        clean[..written]
            .iter()
            .any(|event| matches!(event, Event::Read(p) if p.ends_with("CURRENT")))
    );
    let scratch = Scratch::new("race-stopped");
    let root = scratch.path().to_path_buf();
    let before = installed(&root, &first());
    let barriers = Barriers::new();
    let one = Arc::new(ShimFs::new());
    let stop = barriers.clone();
    one.pause_before(written, move || stop.stop_at("read"));
    let installer = {
        let (fs, root) = (one.clone(), root.clone());
        threads::spawn(move || installing(&fs, &root, &second(), &serving_both()))
    };
    barriers.arrived("read", 1);
    let fake = serving_both();
    let two = Arc::new(ShimFs::new());
    let refused = installing(&two, &root, &entry_at("2026-11.3"), &fake).unwrap_err();
    assert_eq!(refused, InstallRefused::Busy);
    assert!(two.events().is_empty() && fake.requests().is_empty());
    assert_eq!(reading(&root), Current::Installed(before));
    barriers.release("read");
    let published = installer.join().unwrap().unwrap();
    assert_eq!(reading(&root), Current::Installed(published.clone()));
    assert_eq!(installations(&root), [published.identity().hex()]);
}

#[test]
fn two_installers_let_go_together_end_in_one_of_the_two_orders_with_one_installation_whole() {
    // Two processes, each a thread on a disk of its own, each stopped before its
    // first step and released together, eight times. Each installs another
    // identity. Either one was busy, or they ran one after the other: the pointer
    // names what the last to publish installed, whole, and nothing else is left.
    for round in 0..8 {
        let scratch = Scratch::new("race-together");
        let root = scratch.path().to_path_buf();
        installed(&root, &first());
        let barriers = Barriers::new();
        let mut installers = Vec::new();
        for entry in [second(), entry_at("2026-11.3")] {
            let fs = Arc::new(ShimFs::new());
            let stop = barriers.clone();
            fs.pause_before(0, move || stop.stop_at("start"));
            let root = root.clone();
            let work = move || {
                let dir = ModelsDir::at(fs.clone(), &root);
                let ended = install(
                    &dir,
                    &entry,
                    &adapter(),
                    &serving_both(),
                    &Cancellation::new(),
                    &none(),
                );
                // Whether this one replaced the pointer, and when its disk says so.
                let pointed = fs
                    .events()
                    .iter()
                    .any(|event| matches!(event, Event::Rename(_, to) if to.ends_with("CURRENT")));
                (ended, pointed)
            };
            installers.push(threads::spawn(work));
        }
        barriers.arrived("start", 2);
        barriers.open("start");
        let ended: Vec<_> = installers
            .into_iter()
            .map(|installer| installer.join().unwrap())
            .collect();
        let published: Vec<_> = ended
            .iter()
            .filter_map(|(ended, _)| ended.as_ref().ok())
            .collect();
        for (ended, pointed) in &ended {
            match ended {
                Ok(_) => assert!(pointed, "round {round}"),
                Err(refused) => {
                    assert_eq!(*refused, InstallRefused::Busy, "round {round}");
                    assert!(!pointed, "round {round}");
                }
            }
        }
        assert!(!published.is_empty(), "round {round}: both were busy");
        let now = reading(&root);
        assert!(
            published
                .iter()
                .any(|record| now == Current::Installed((*record).clone())),
            "round {round}: {now:?}"
        );
        let Current::Installed(record) = now else {
            unreachable!()
        };
        is_whole(&root, &record);
        assert_eq!(
            installations(&root),
            [record.identity().hex()],
            "round {round}"
        );
        assert!(!incoming(&root).exists(), "round {round}");
        assert_eq!(said(&root), Some((State::Installed, 2, 2)), "round {round}");
    }
}
