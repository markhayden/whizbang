//! An install under a crash and under a race (ADR 0017, "For later tasks": the crashes
//! the tests hold). A crash is tried in place of each event of an install, each way
//! the test kit's disk can lose what was not durable and each way it can leave what
//! was appended and not synced; each step is refused and the crash follows; a
//! process dies inside a download; and two installers, each a process of its own,
//! are stopped one after the other's read, and let go together. One shim stands for
//! one process; one test runs a real second one.

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

/// The ways a crash loses names that are tried at every event, under each tail the
/// event is tried under ([`tails_at`]): all that were not synced, none, those of
/// every other folder, and every other name.
const LOSSES: [Lost; 6] = [
    Lost::Unsynced,
    Lost::ContentOnly,
    Lost::Subset(0x5555_5555_5555_5555),
    Lost::Subset(0xaaaa_aaaa_aaaa_aaaa),
    Lost::Names(0x5555_5555_5555_5555),
    Lost::Names(0xaaaa_aaaa_aaaa_aaaa),
];

/// The most names, or folders, a crash of these sweeps may have had pending: a mask
/// has 64 bits, and a sweep that met more would leave some untried.
const PENDING_MOST: usize = 64;

/// The losses chosen by what one crash had pending: each name alone lost and each
/// alone kept, and the same of each folder that held such names. With one pending
/// these are the first two of [`LOSSES`] again, and are left out.
fn one_by_one((folders, names): (usize, usize)) -> Vec<Lost> {
    assert!(folders <= PENDING_MOST && names <= PENDING_MOST);
    let masks = |count: usize| {
        let count = if count < 2 { 0 } else { count };
        (0..count).flat_map(|bit| [1u64 << bit, !(1u64 << bit)])
    };
    masks(names)
        .map(Lost::Names)
        .chain(masks(folders).map(Lost::Subset))
        .collect()
}

/// What a crash leaves of a streamed file's bytes that were not synced.
const TAILS: [Tail; 4] = [Tail::Half, Tail::Zeroed, Tail::All, Tail::None];

/// The tails a crash or a refusal at this event of a clean install is tried under:
/// all four from the first staged file begun to the event after the last finished,
/// and one elsewhere, a different one from event to event. A tail is what a crash
/// leaves of bytes appended to a file and not synced, and there are such bytes only
/// inside that span: each staged file is synced after its last piece and before it
/// is finished, which is held here, and nothing else is written piece by piece.
fn tails_at(clean: &[Event], event: usize) -> Vec<Tail> {
    let of = |event: &Event| match event {
        Event::Begin(path) | Event::Append(path) | Event::Finish(path) => Some(path.clone()),
        _ => None,
    };
    let streamed: Vec<usize> = (0..clean.len())
        .filter(|at| of(&clean[*at]).is_some())
        .collect();
    for at in &streamed {
        if let Event::Finish(path) = &clean[*at] {
            assert_eq!(clean[at - 1], Event::SyncFile(path.clone()));
        }
    }
    let (first, last) = (streamed[0], streamed[streamed.len() - 1]);
    assert!(matches!(clean[first], Event::Begin(_)) && matches!(clean[last], Event::Finish(_)));
    if (first..=last + 1).contains(&event) {
        TAILS.to_vec()
    } else {
        vec![TAILS[event % TAILS.len()]]
    }
}

/// The events of part `part` of `parts` of a clean install, each with a tail it is
/// tried under ([`tails_at`]). The parts are cut so that each tries about as many,
/// and together they are every event under each of its tails, once.
fn part_of(clean: &[Event], part: usize, parts: usize) -> Vec<(usize, Tail)> {
    let all: Vec<(usize, Tail)> = (0..clean.len())
        .flat_map(|event| {
            let tails = tails_at(clean, event);
            tails.into_iter().map(move |tail| (event, tail))
        })
        .collect();
    let cut = all[all.len() * part / parts..all.len() * (part + 1) / parts].to_vec();
    assert!(!cut.is_empty());
    cut
}

/// A list of events with the directory they happened in taken out of every path, so
/// that the events of two installs in two scratch folders can be compared.
fn shape(events: &[Event], root: &Path) -> Vec<String> {
    let root = root.display().to_string();
    events
        .iter()
        .map(|event| format!("{event:?}").replace(&root, ""))
        .collect()
}

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
/// install's events. At each event, under each tail of [`tails_at`]: every loss of
/// [`LOSSES`], and then each name that was pending at that crash alone lost and
/// alone kept, and each folder ([`one_by_one`]). `part` of `parts` cuts the sweep so
/// that each test stays short; together the parts are every event ([`part_of`]).
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
    // One crash, and what it had pending: folders that held names not durable, and
    // such names.
    let crashed = |event: usize, tail: Tail, lost: Lost| {
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
        // repair's pointer names the damaged folder until it is cleared: once
        // the clearing is durable, a pointer names the folder that was renamed in.
        if let Current::Installed(record) = &now {
            assert!(hold_file_of(root, record).exists(), "{at}");
            if cleared.is_none_or(|cleared| event > cleared + 1) {
                is_whole(root, record);
            }
        }
        // The status is not durable, and is never half a record: it is read, and
        // says nothing or one of its states.
        let _ = said(root);
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
        (fs.crash_choices(), fs.crash_names())
    };
    for (event, tail) in part_of(&clean, part, parts) {
        let mut pending = (0, 0);
        for lost in LOSSES {
            pending = crashed(event, tail, lost);
        }
        for lost in one_by_one(pending) {
            assert_eq!(crashed(event, tail, lost), pending);
        }
    }
}

#[test]
fn a_crash_at_any_event_of_a_first_install_leaves_it_not_installed_or_whole_part_1_of_4() {
    crashed_at_each_event(Sweep::First, 0, 4);
}

#[test]
fn a_crash_at_any_event_of_a_first_install_leaves_it_not_installed_or_whole_part_2_of_4() {
    crashed_at_each_event(Sweep::First, 1, 4);
}

#[test]
fn a_crash_at_any_event_of_a_first_install_leaves_it_not_installed_or_whole_part_3_of_4() {
    crashed_at_each_event(Sweep::First, 2, 4);
}

#[test]
fn a_crash_at_any_event_of_a_first_install_leaves_it_not_installed_or_whole_part_4_of_4() {
    crashed_at_each_event(Sweep::First, 3, 4);
}

#[test]
fn a_crash_at_any_event_of_an_install_leaves_the_previous_or_the_new_whole_part_1_of_4() {
    crashed_at_each_event(Sweep::Over, 0, 4);
}

#[test]
fn a_crash_at_any_event_of_an_install_leaves_the_previous_or_the_new_whole_part_2_of_4() {
    crashed_at_each_event(Sweep::Over, 1, 4);
}

#[test]
fn a_crash_at_any_event_of_an_install_leaves_the_previous_or_the_new_whole_part_3_of_4() {
    crashed_at_each_event(Sweep::Over, 2, 4);
}

#[test]
fn a_crash_at_any_event_of_an_install_leaves_the_previous_or_the_new_whole_part_4_of_4() {
    crashed_at_each_event(Sweep::Over, 3, 4);
}

#[test]
fn a_crash_at_any_event_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_1_of_4() {
    crashed_at_each_event(Sweep::Repair, 0, 4);
}

#[test]
fn a_crash_at_any_event_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_2_of_4() {
    crashed_at_each_event(Sweep::Repair, 1, 4);
}

#[test]
fn a_crash_at_any_event_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_3_of_4() {
    crashed_at_each_event(Sweep::Repair, 2, 4);
}

#[test]
fn a_crash_at_any_event_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_4_of_4() {
    crashed_at_each_event(Sweep::Repair, 3, 4);
}

// --- a refused step, and then a crash ----------------------------------------------------------

/// Installs the second entry with each event of one part of the install refused in
/// turn, and crashes straight after: what a refused step left not durable is then
/// lost, each way the disk can lose it ([`LOSSES`], and [`one_by_one`] by what was
/// pending), under each tail of [`tails_at`]. And the same with the install asked a
/// second time before the crash, under the loss of every name that was not synced
/// and of none: what it answers must be what the crash leaves.
///
/// A publication refused after its folder was renamed in leaves that folder, whole,
/// named by no pointer and with no hold file: after such a refusal that is a state
/// like any other, and the crash and the next install each meet it here.
fn crashed_after_each_refused_step(sweep: Sweep, part: usize, parts: usize) {
    let (clean, clean_shape) = {
        let scratch = Scratch::new("crash-events");
        sweep.prepare(scratch.path());
        let fs = Arc::new(ShimFs::new());
        installing(&fs, scratch.path(), &second(), &serving_both()).unwrap();
        (fs.events(), shape(&fs.events(), scratch.path()))
    };
    let next = record_of(&second());
    let last = record_of(&sweep.reopening());
    let crashed = |event: usize, tail: Tail, lost: Lost, again: bool| {
        let at = format!("event {event}, {lost:?}, again {again}, {tail:?}");
        let scratch = Scratch::new("crash-refused");
        let root = scratch.path();
        let was = sweep.prepare(root);
        let fs = Arc::new(ShimFs::new().with_tail(tail).fail_before(event));
        let mut ended = installing(&fs, root, &second(), &serving_both());
        // The refusal was used: a refused call is not recorded, so the events are
        // not those of the clean install.
        assert_ne!(shape(&fs.events(), root), clean_shape, "{at}");
        if again {
            ended = installing(&fs, root, &second(), &serving_both());
            assert_eq!(ended, Ok(next.clone()), "{at}");
        }
        fs.crash(lost);
        let now = reading(root);
        let _ = said(root);
        if ended.is_ok() {
            // What was answered is what the crash left.
            assert_eq!(now, Current::Installed(next.clone()), "{at}");
        } else {
            // A repair that was refused, or that the crash caught, may have
            // cleared the pointer.
            let cleared = sweep == Sweep::Repair && now == Current::NotInstalled;
            assert!(
                now == was || now == Current::Installed(next.clone()) || cleared,
                "{at}: {now:?}"
            );
        }
        if let Current::Installed(record) = &now {
            assert!(hold_file_of(root, record).exists(), "{at}");
            // A repair's pointer names its folder before and after: the folder
            // is whole once the repair has answered.
            if sweep != Sweep::Repair || ended.is_ok() {
                is_whole(root, record);
            }
        }
        assert_eq!(installed(root, &sweep.reopening()), last, "{at}");
        assert_eq!(reading(root), Current::Installed(last.clone()), "{at}");
        is_whole(root, &last);
        assert_eq!(installations(root), [last.identity().hex()], "{at}");
        assert!(!incoming(root).exists(), "{at}");
        assert!(!tiny(root).join("CURRENT.new").exists(), "{at}");
        assert_eq!(said(root), Some((State::Installed, 2, 2)), "{at}");
        (fs.crash_choices(), fs.crash_names())
    };
    for (event, tail) in part_of(&clean, part, parts) {
        let mut pending = (0, 0);
        for lost in LOSSES {
            pending = crashed(event, tail, lost, false);
            // Asked again: under the loss of every name, and of none.
            if matches!(lost, Lost::Unsynced | Lost::ContentOnly) {
                crashed(event, tail, lost, true);
            }
        }
        for lost in one_by_one(pending) {
            assert_eq!(crashed(event, tail, lost, false), pending);
        }
    }
}

/// The parts the sweep after a refused step of a first install is cut in: about one event
/// under one tail to a part (a clean first install is about a hundred of them), the finest
/// the sweep is cut without cutting inside one. [`part_of`] fails a part left with none.
const FIRST_PARTS: usize = 96;

/// One test for each part of that sweep, named by its part, with the part it runs from 0.
/// Each is written `fn name() = part;`, so that a test is found by its name where it is
/// defined, as any other (`scripts/tests/test-notes.sh` looks for one so).
macro_rules! after_a_refused_step_of_a_first_install {
    ($(fn $name:ident() = $part:expr;)*) => {
        $(
            #[test]
            fn $name() {
                crashed_after_each_refused_step(Sweep::First, $part, FIRST_PARTS);
            }
        )*
        /// Every part has its test, once: none of the sweep is left out or run twice.
        #[test]
        fn the_sweep_after_a_refused_step_of_a_first_install_has_a_test_for_each_of_its_parts() {
            let mut parts = vec![$($part),*];
            parts.sort_unstable();
            assert_eq!(parts, (0..FIRST_PARTS).collect::<Vec<usize>>());
            $(assert!(stringify!($name).ends_with(&format!(
                "_part_{:02}_of_{FIRST_PARTS}",
                $part + 1
            )));)*
        }
    };
}

// In 96 parts where the two sweeps after it are in four. The sweeps run two at a time
// (`.config/nextest.toml`, `crash-sweeps`), and this one is the first of them by its name,
// so its parts are the ones that run beside the rest of a full check; the sweeps after it
// run when that is over, at full speed. What slows a part is not its own work, which is
// thousands of small file operations and takes well under a second on an idle machine. In
// each of three full checks on a 16-core Mac (2026-11-19) the parts that were slow were the
// ones that ran in the same twenty seconds or so: those in which the core's
// `spend::fold::every_named_history_written_into_a_store_…` and a dozen tests of the rascal
// suite were writing real stores at once (on macOS each sync of a store has the drive empty
// its cache, `whizbang_core::store::fs::RealFs`). There the sweep went about forty times
// slower than alone. In eight parts, the parts that met those seconds took 13 to 18 seconds
// in one run and 27 in others, and one was ended at the limit of 30; in 32 parts, the two
// that met them took 19, the rest 6 at most; in 96, the four that met them took 9, the
// rest 3 at most. So a part is as small as the sweep allows, and does in those seconds what
// it must and no more. Together the parts are still every event under each of its tails,
// once ([`part_of`]), and the test the macro adds holds that each part has its test.
after_a_refused_step_of_a_first_install! {
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_01_of_96() = 0;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_02_of_96() = 1;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_03_of_96() = 2;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_04_of_96() = 3;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_05_of_96() = 4;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_06_of_96() = 5;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_07_of_96() = 6;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_08_of_96() = 7;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_09_of_96() = 8;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_10_of_96() = 9;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_11_of_96() = 10;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_12_of_96() = 11;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_13_of_96() = 12;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_14_of_96() = 13;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_15_of_96() = 14;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_16_of_96() = 15;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_17_of_96() = 16;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_18_of_96() = 17;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_19_of_96() = 18;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_20_of_96() = 19;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_21_of_96() = 20;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_22_of_96() = 21;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_23_of_96() = 22;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_24_of_96() = 23;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_25_of_96() = 24;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_26_of_96() = 25;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_27_of_96() = 26;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_28_of_96() = 27;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_29_of_96() = 28;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_30_of_96() = 29;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_31_of_96() = 30;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_32_of_96() = 31;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_33_of_96() = 32;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_34_of_96() = 33;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_35_of_96() = 34;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_36_of_96() = 35;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_37_of_96() = 36;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_38_of_96() = 37;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_39_of_96() = 38;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_40_of_96() = 39;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_41_of_96() = 40;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_42_of_96() = 41;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_43_of_96() = 42;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_44_of_96() = 43;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_45_of_96() = 44;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_46_of_96() = 45;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_47_of_96() = 46;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_48_of_96() = 47;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_49_of_96() = 48;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_50_of_96() = 49;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_51_of_96() = 50;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_52_of_96() = 51;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_53_of_96() = 52;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_54_of_96() = 53;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_55_of_96() = 54;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_56_of_96() = 55;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_57_of_96() = 56;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_58_of_96() = 57;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_59_of_96() = 58;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_60_of_96() = 59;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_61_of_96() = 60;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_62_of_96() = 61;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_63_of_96() = 62;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_64_of_96() = 63;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_65_of_96() = 64;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_66_of_96() = 65;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_67_of_96() = 66;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_68_of_96() = 67;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_69_of_96() = 68;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_70_of_96() = 69;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_71_of_96() = 70;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_72_of_96() = 71;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_73_of_96() = 72;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_74_of_96() = 73;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_75_of_96() = 74;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_76_of_96() = 75;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_77_of_96() = 76;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_78_of_96() = 77;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_79_of_96() = 78;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_80_of_96() = 79;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_81_of_96() = 80;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_82_of_96() = 81;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_83_of_96() = 82;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_84_of_96() = 83;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_85_of_96() = 84;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_86_of_96() = 85;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_87_of_96() = 86;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_88_of_96() = 87;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_89_of_96() = 88;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_90_of_96() = 89;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_91_of_96() = 90;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_92_of_96() = 91;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_93_of_96() = 92;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_94_of_96() = 93;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_95_of_96() = 94;
    fn a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_96_of_96() = 95;
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_an_install_leaves_the_previous_or_the_new_whole_part_1_of_4()
 {
    crashed_after_each_refused_step(Sweep::Over, 0, 4);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_an_install_leaves_the_previous_or_the_new_whole_part_2_of_4()
 {
    crashed_after_each_refused_step(Sweep::Over, 1, 4);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_an_install_leaves_the_previous_or_the_new_whole_part_3_of_4()
 {
    crashed_after_each_refused_step(Sweep::Over, 2, 4);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_an_install_leaves_the_previous_or_the_new_whole_part_4_of_4()
 {
    crashed_after_each_refused_step(Sweep::Over, 3, 4);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_1_of_4()
 {
    crashed_after_each_refused_step(Sweep::Repair, 0, 4);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_2_of_4()
 {
    crashed_after_each_refused_step(Sweep::Repair, 1, 4);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_3_of_4()
 {
    crashed_after_each_refused_step(Sweep::Repair, 2, 4);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_a_repair_leaves_it_as_it_was_not_installed_or_whole_part_4_of_4()
 {
    crashed_after_each_refused_step(Sweep::Repair, 3, 4);
}

#[test]
fn a_crash_at_any_event_after_a_refused_step_of_a_publication_leaves_its_folder_for_the_next() {
    // The steps of a publication from the rename of the staged folder to the
    // pointer's rename, each refused in turn; then the crash. Under a crash that
    // loses no name the folder is there, whole, named by no pointer, and with a hold
    // file or with none: a state like any other. The next install of that identity
    // points to it and asks for no byte; the next of another deletes it.
    for over in [false, true] {
        let before: Vec<Entry> = if over { vec![first()] } else { vec![] };
        let clean = events_after(&before.iter().collect::<Vec<_>>(), &second());
        let renamed_in = index_of(
            &clean,
            |event| matches!(event, Event::Rename(_, to) if to.parent().is_some_and(|p| p.ends_with("installations"))),
        );
        let pointed = index_of(
            &clean,
            |event| matches!(event, Event::Rename(_, to) if to.ends_with("CURRENT")),
        );
        assert!(renamed_in < pointed);
        let next = record_of(&second());
        let mut with_no_hold_file = 0;
        for (event, same) in
            (renamed_in + 1..=pointed).flat_map(|event| [true, false].map(|same| (event, same)))
        {
            for lost in LOSSES {
                let at = format!("over {over}, event {event}, {lost:?}, same {same}");
                let scratch = Scratch::new("crash-refused-publication");
                let root = scratch.path();
                let was = match before.first() {
                    Some(entry) => Current::Installed(installed(root, entry)),
                    None => Current::NotInstalled,
                };
                let fs = Arc::new(ShimFs::new().fail_before(event));
                installing(&fs, root, &second(), &serving_both()).unwrap_err();
                fs.crash(lost);
                assert_eq!(reading(root), was, "{at}");
                if lost == Lost::ContentOnly {
                    is_whole(root, &next);
                    with_no_hold_file += usize::from(!hold_file_of(root, &next).exists());
                }
                let entry = if same {
                    second()
                } else {
                    entry_at("2026-11.3")
                };
                let last = record_of(&entry);
                let fake = serving_both();
                let fs = Arc::new(ShimFs::new());
                assert_eq!(
                    installing(&fs, root, &entry, &fake),
                    Ok(last.clone()),
                    "{at}"
                );
                if same && lost == Lost::ContentOnly {
                    assert!(fake.requests().is_empty(), "{at}");
                }
                assert_eq!(reading(root), Current::Installed(last.clone()), "{at}");
                is_whole(root, &last);
                assert!(hold_file_of(root, &last).exists(), "{at}");
                assert_eq!(installations(root), [last.identity().hex()], "{at}");
            }
        }
        assert!(with_no_hold_file > 0, "over {over}");
    }
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
        let removed = index_of(&events, |event| *event == Event::Remove(incoming(&root)));
        assert!(locked < removed && removed < begun, "{at}");
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

/// The event that follows the first read of the pointer by an install of the second
/// entry over the first: the read is of the pointer's file, under the model's lock.
fn after_the_pointer_was_read() -> usize {
    let clean = events_after(&[&first()], &second());
    let locked = index_of(&clean, |event| matches!(event, Event::Lock(_)));
    let read = index_of(
        &clean,
        |event| matches!(event, Event::Read(p) if p.ends_with("CURRENT")),
    );
    assert!(locked < read);
    read + 1
}

#[test]
fn two_installers_stopped_after_one_read_the_pointer_do_not_both_publish() {
    // The first installer is stopped after it took the model's lock and read the
    // pointer, at its next step. The pointer is read under the model's lock, so
    // the second never reads one to decide from: it is busy.
    let written = after_the_pointer_was_read();
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

/// A second process, which does not outlive the test that started it: when this is
/// dropped, by the test's end or by a failed assertion, the process is killed if it
/// is still there, and waited for.
struct Ended(std::process::Child);

impl Drop for Ended {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The variable that names the models directory to the installer in a process of its
/// own, below.
const ROOT_FOR_CHILD: &str = "WHIZBANG_TEST_MODELS_ROOT";

/// Run only by the test below, in a process of its own: it installs the second entry
/// into the directory it is told of, stops after it took the model's lock and read
/// the pointer, says so on its output, and goes on when a line arrives on its input.
#[test]
#[ignore = "run by two_installers_in_two_processes_stopped_after_one_read_the_pointer_do_not_both_publish"]
fn child_installs_and_stops_after_it_read_the_pointer() {
    use std::io::{BufRead, Write};
    let root = std::path::PathBuf::from(std::env::var_os(ROOT_FOR_CHILD).unwrap());
    let written = after_the_pointer_was_read();
    let fs = Arc::new(ShimFs::new());
    fs.pause_before(written, || {
        let mut out = std::io::stdout().lock();
        writeln!(out, "\nstopped").unwrap();
        out.flush().unwrap();
        let mut go = String::new();
        std::io::stdin().lock().read_line(&mut go).unwrap();
        assert_eq!(go, "go\n");
    });
    let ended = installing(&fs, &root, &second(), &serving_both());
    let mut out = std::io::stdout().lock();
    writeln!(
        out,
        "\n{}",
        if ended.is_ok() {
            "published"
        } else {
            "refused"
        }
    )
    .unwrap();
    out.flush().unwrap();
}

#[test]
fn two_installers_in_two_processes_stopped_after_one_read_the_pointer_do_not_both_publish() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    // The test binary runs itself once, for the one test above: a second process.
    // The two talk through its input and its output, a line at a time, and no clock
    // is read. What it says of a failure of its own goes to this test's output.
    //
    // The second process is the test binary run directly, not through the runner,
    // so the runner's guard on threads does not watch it. It is within the rule on
    // threads and processes all the same: it is started once, by no loop; it runs
    // one test on one thread (`--test-threads=1`), and that test makes no thread and
    // no process; and it is killed and waited for when this test ends, however it
    // ends ([`Ended`]).
    let scratch = Scratch::new("race-processes");
    let root = scratch.path().to_path_buf();
    let before = installed(&root, &first());
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "models::crashes::child_installs_and_stops_after_it_read_the_pointer",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ROOT_FOR_CHILD, &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("the test binary runs");
    let mut child = Ended(child);
    let mut said_by_child = BufReader::new(child.0.stdout.take().unwrap()).lines();
    let mut wait_for = |word: &str| {
        let found = said_by_child
            .by_ref()
            .map_while(Result::ok)
            .any(|line| line == word);
        assert!(found, "the other process ended before it said `{word}`");
    };
    wait_for("stopped");
    // The other process has the model's lock and has read the pointer. This one
    // never reads a pointer to decide from: it is busy, asks nothing of its
    // downloader and touches nothing.
    let fake = serving_both();
    let fs = Arc::new(ShimFs::new());
    let refused = installing(&fs, &root, &entry_at("2026-11.3"), &fake).unwrap_err();
    assert_eq!(refused, InstallRefused::Busy);
    assert!(fs.events().is_empty() && fake.requests().is_empty());
    assert_eq!(reading(&root), Current::Installed(before));
    child.0.stdin.take().unwrap().write_all(b"go\n").unwrap();
    wait_for("published");
    assert!(child.0.wait().unwrap().success());
    let published = record_of(&second());
    assert_eq!(reading(&root), Current::Installed(published.clone()));
    is_whole(&root, &published);
    assert_eq!(installations(&root), [published.identity().hex()]);
    // With the other process gone, the model's lock is free.
    assert_eq!(
        installed(&root, &entry_at("2026-11.3")),
        record_of(&entry_at("2026-11.3"))
    );
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
