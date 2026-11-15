//! Removal of a model (ADR 0017, "Removal"): the model is marked as draining, its
//! holds are waited for until a deadline of the injected clock, and its pointer is
//! cleared; or the removal answers that the model is held, and has changed no
//! pointer. The clock is moved by hand, and what paces a removal's tries is the
//! test's: a barrier, or the clock. One shim stands for one process.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use whizbang_backends::models::{
    Current, DRAINING_FORMAT, DRAINING_MAX_BYTES, HoldRefused, InstallRefused, Mark, ModelName,
    ModelsDir, Refused, RemoveRefused, Removed, Rule, State, What, draining, install_or_attach,
    remove,
};
use whizbang_core::contract::{Cancellation, Clock};
use whizbang_core::store::fs::{Fs, RealFs};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;

use super::holds::{alone, count, hold, holds_over};
use super::publishing::{
    adapter, first, hold_file_of, index_of, installed, installing, is_whole, reading, record_of,
    said, second, tiny,
};
use super::staging::{TERMS, at, graph, names_under, serving_both, touches_only};
use super::{MARKER, model, none, secrets, shows_nothing_of};

/// A clock a test moves by hand: it stands still until it is told to go on.
pub(super) struct Hand {
    start: Instant,
    moved: Mutex<Duration>,
}

impl Hand {
    pub(super) fn new() -> Hand {
        Hand {
            start: Instant::now(),
            moved: Mutex::new(Duration::ZERO),
        }
    }

    pub(super) fn advance(&self, by: Duration) {
        *self.moved.lock().unwrap() += by;
    }
}

impl Clock for Hand {
    fn now_ms(&self) -> u64 {
        0
    }

    fn now(&self) -> Instant {
        self.start + *self.moved.lock().unwrap()
    }
}

const SECOND: Duration = Duration::from_secs(1);

/// A removal of the model `tiny` over this file system, by a process of its own.
pub(super) fn removing(
    fs: Arc<dyn Fs>,
    root: &Path,
    clock: &dyn Clock,
    patience: Duration,
    pause: &mut dyn FnMut(),
) -> Result<Removed, RemoveRefused> {
    let dir = ModelsDir::at(fs, root);
    remove(&dir, &model("tiny"), clock, patience, &none(), pause)
}

/// A removal on the real file system that is given a minute of a clock that stands
/// still, and never has to wait.
pub(super) fn removed(root: &Path) -> Result<Removed, RemoveRefused> {
    let mut never = || panic!("the removal waited");
    removing(
        Arc::new(RealFs),
        root,
        &Hand::new(),
        60 * SECOND,
        &mut never,
    )
}

/// Whether the model is marked, read strictly by one who holds no lock.
pub(super) fn marked(root: &Path) -> bool {
    draining(
        &ModelsDir::at(Arc::new(RealFs), root),
        &model("tiny"),
        &none(),
    )
    .unwrap()
}

/// A removal on a thread, a process of its own, which stops at the barrier `try`
/// between two tries. The clock is the test's.
fn removal_on_a_thread(
    root: &Path,
    barriers: &Barriers,
    clock: &Arc<Hand>,
) -> std::thread::JoinHandle<Result<Removed, RemoveRefused>> {
    let (root, stop, clock) = (root.to_path_buf(), barriers.clone(), clock.clone());
    threads::spawn(move || {
        let fs = Arc::new(RealFs);
        removing(fs, &root, &*clock, 60 * SECOND, &mut || stop.stop_at("try"))
    })
}

// --- a removal drains ---------------------------------------------------------------------------

#[test]
fn a_removal_during_a_held_installation_refuses_a_new_hold_and_goes_on_when_the_hold_is_let_go() {
    let scratch = Scratch::new("remove-held");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    let held = hold(&holds, &record).unwrap();
    let barriers = Barriers::new();
    let clock = Arc::new(Hand::new());
    let remover = removal_on_a_thread(root, &barriers, &clock);
    barriers.arrived("try", 1);
    // The removal has marked the model, has its lock, has said so, and waits for
    // the hold: the pointer names what it named.
    assert!(marked(root));
    assert_eq!(said(root), Some((State::Removing, 0, 0)));
    assert_eq!(reading(root), Current::Installed(record.clone()));
    // A new hold is refused. So is one that was refused and is asked for again,
    // which is what stands for a waiting hold here: asked while the mark is set, it
    // is told that the model is draining, by this process and by another.
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
    barriers.release("try");
    barriers.arrived("try", 2);
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
    let other = holds_over(root);
    assert_eq!(hold(&other, &record).unwrap_err(), HoldRefused::Draining);
    assert_eq!(count(&holds, &record), 1);
    assert_eq!(reading(root), Current::Installed(record.clone()));
    // The hold is let go, short of the deadline: the removal goes on.
    clock.advance(59 * SECOND);
    held.let_go();
    barriers.release("try");
    let removed = remover.join().unwrap().unwrap();
    assert_eq!(removed.installation(), Some(record.identity()));
    assert_eq!(barriers.arrivals("try"), 2);
    assert_eq!(reading(root), Current::NotInstalled);
    assert_eq!(said(root), Some((State::Removed, 0, 0)));
    assert!(!marked(root));
    assert!(!tiny(root).join("DRAINING").exists());
    // "Goes on" ends at the decision: nothing is deleted yet (task M5c).
    is_whole(root, &record);
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::NotNamed);
}

#[test]
fn with_the_hold_kept_past_the_deadline_the_removal_answers_busy_the_mark_is_gone_and_the_next_hold_is_given()
 {
    let scratch = Scratch::new("remove-deadline");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    let held = hold(&holds, &record).unwrap();
    // Thirty seconds of patience, and ten pass between two tries: four tries, the
    // last when the deadline has come.
    let clock = Hand::new();
    let mut pauses = 0;
    let refused = removing(Arc::new(RealFs), root, &clock, 30 * SECOND, &mut || {
        pauses += 1;
        assert!(marked(root));
        assert_eq!(said(root), Some((State::Removing, 0, 0)));
        clock.advance(10 * SECOND);
    });
    assert_eq!(refused, Err(RemoveRefused::Held));
    assert_eq!(pauses, 3);
    // Nothing was changed: the mark is gone, the pointer names what it named, the
    // status says what it said, and the next hold is given.
    assert!(!marked(root));
    assert!(!tiny(root).join("DRAINING").exists());
    assert_eq!(reading(root), Current::Installed(record.clone()));
    assert_eq!(said(root), Some((State::Installed, 2, 2)));
    is_whole(root, &record);
    let next = hold(&holds, &record).unwrap();
    assert_eq!(count(&holds, &record), 2);
    // With no patience at all the removal tries once, and does not pause.
    let refused = removing(Arc::new(RealFs), root, &clock, Duration::ZERO, &mut || {
        panic!("a removal with no patience paused")
    });
    assert_eq!(refused, Err(RemoveRefused::Held));
    // It is the clock that ends the wait, and nothing else: on a clock that stands
    // still the removal tries for as long as the hold is kept, here fifty times.
    let still = Hand::new();
    let mut pauses = 0;
    let removed = removing(Arc::new(RealFs), root, &still, SECOND, &mut || {
        pauses += 1;
        if pauses == 50 {
            held.let_go();
            next.let_go();
        }
    });
    assert_eq!(pauses, 50);
    assert_eq!(removed.unwrap().installation(), Some(record.identity()));
    assert_eq!(reading(root), Current::NotInstalled);
}

#[test]
fn a_removal_waits_for_a_hold_on_a_superseded_installation_too() {
    // A run holds the installation that was in use before the last install. The
    // model is not removed under it: the removal waits for that hold as for one on
    // the installation in use.
    let scratch = Scratch::new("remove-superseded");
    let root = scratch.path();
    let old = installed(root, &first());
    let holds = holds_over(root);
    let held = hold(&holds, &old).unwrap();
    let new = installed(root, &second());
    is_whole(root, &old);
    let clock = Hand::new();
    let patience = 30 * SECOND;
    let refused = removing(Arc::new(RealFs), root, &clock, patience, &mut || {
        clock.advance(30 * SECOND);
    });
    assert_eq!(refused, Err(RemoveRefused::Held));
    assert_eq!(reading(root), Current::Installed(new.clone()));
    let mut pauses = 0;
    let removed = removing(Arc::new(RealFs), root, &clock, patience, &mut || {
        pauses += 1;
        held.let_go();
    });
    assert_eq!(removed.unwrap().installation(), Some(new.identity()));
    assert_eq!(pauses, 1);
    assert_eq!(reading(root), Current::NotInstalled);
}

#[test]
fn a_removal_of_a_model_that_is_not_installed_says_so() {
    // Never installed: there is no folder of the model, and none is made.
    let scratch = Scratch::new("remove-never");
    let root = scratch.path();
    assert_eq!(removed(root), Err(RemoveRefused::NotInstalled));
    assert!(names_under(root).is_empty());
    // A first install that failed left a folder and no pointer.
    let short = FakeDownloader::new()
        .serving(&at("LICENSE.txt"), TERMS.to_vec())
        .serving(&at("graph.onnx"), graph()[..2499].to_vec());
    installing(&Arc::new(ShimFs::new()), root, &second(), &short).unwrap_err();
    assert_eq!(said(root), Some((State::Failed, 1, 2)));
    assert_eq!(removed(root), Err(RemoveRefused::NotInstalled));
    assert!(!marked(root));
    assert_eq!(said(root), Some((State::Failed, 1, 2)));
    // Removed already.
    let record = installed(root, &second());
    assert_eq!(
        removed(root).unwrap().installation(),
        Some(record.identity())
    );
    assert_eq!(removed(root), Err(RemoveRefused::NotInstalled));
    assert!(!marked(root));
    assert_eq!(said(root), Some((State::Removed, 0, 0)));
    assert_eq!(reading(root), Current::NotInstalled);
}

#[test]
fn a_removal_of_a_model_whose_pointer_cannot_be_read_clears_it() {
    // Which installation was in use is not known. The removal waits for the hold
    // file of every installation on the disk, and takes the pointer away.
    let scratch = Scratch::new("remove-damaged");
    let root = scratch.path();
    let record = installed(root, &second());
    std::fs::write(tiny(root).join("CURRENT"), b"damaged").unwrap();
    let foreign = RealFs.try_share(&hold_file_of(root, &record)).unwrap();
    let clock = Hand::new();
    let mut pauses = 0;
    let mut foreign = foreign;
    let removed = removing(Arc::new(RealFs), root, &clock, SECOND, &mut || {
        pauses += 1;
        foreign = None;
    });
    assert_eq!(removed.unwrap().installation(), None);
    assert_eq!(pauses, 1);
    assert_eq!(reading(root), Current::NotInstalled);
    assert!(!marked(root));
}

// --- the mark -----------------------------------------------------------------------------------

#[test]
fn the_mark_is_a_record_read_strictly_and_bounded() {
    assert_eq!(DRAINING_FORMAT, "draining/1");
    assert_eq!(DRAINING_MAX_BYTES, 64);
    let written = Mark.to_bytes();
    assert_eq!(written, br#"{"format":"draining/1"}"#);
    assert_eq!(Mark::parse(&written, &none()), Ok(Mark));
    // Over the bound: refused for its length, before anything of it is looked at.
    let mut long = MARKER.as_bytes().to_vec();
    long.resize(DRAINING_MAX_BYTES as usize + 1, b'x');
    assert_eq!(
        Mark::parse(&long, &secrets()).unwrap_err(),
        Refused::TooLarge {
            what: What::Mark,
            limit: DRAINING_MAX_BYTES
        }
    );
    long.truncate(DRAINING_MAX_BYTES as usize);
    assert_eq!(
        Mark::parse(&long, &secrets()).unwrap_err(),
        Refused::HoldsSecret
    );
    let value = |place: &str, rule| Refused::Value {
        at: place.to_owned(),
        rule,
    };
    let other_form = Refused::OtherForm { what: What::Mark };
    let refused = [
        (
            "",
            Refused::NotJson {
                what: What::Mark,
                line: 1,
            },
        ),
        (
            "draining",
            Refused::NotJson {
                what: What::Mark,
                line: 1,
            },
        ),
        ("[]", value("", Rule::Kind)),
        ("{}", value("format", Rule::Missing)),
        (r#"{"format":7}"#, value("format", Rule::Kind)),
        (r#"{"format":"draining/2"}"#, value("format", Rule::Format)),
        (r#"{"format":"status/2"}"#, value("format", Rule::Format)),
        (
            r#"{"format":"draining/1","pid":7}"#,
            value("", Rule::Unknown),
        ),
        (
            r#"{"format":"draining/1","format":"draining/1"}"#,
            Refused::Twice {
                what: What::Mark,
                line: 1,
            },
        ),
        (r#"{"format": "draining/1"}"#, other_form.clone()),
        (r#"{"format":"draining\/1"}"#, other_form.clone()),
        ("{\"format\":\"draining/1\"}\n", other_form),
    ];
    for (bytes, refusal) in refused {
        let got = Mark::parse(bytes.as_bytes(), &none()).unwrap_err();
        assert_eq!(got, refusal, "{bytes}");
        shows_nothing_of(&got, &["pid"]);
    }
    let told = whizbang_core::contract::Secrets::new(["draining".to_owned()]).unwrap();
    assert_eq!(
        Mark::parse(&written, &told).unwrap_err(),
        Refused::HoldsSecret
    );
}

#[test]
fn a_mark_on_the_disk_is_read_strictly_by_a_reader_and_as_there_by_whoever_decides_by_it() {
    let scratch = Scratch::new("remove-mark");
    let root = scratch.path();
    let record = installed(root, &first());
    let holds = holds_over(root);
    let mark = tiny(root).join("DRAINING");
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    let read = || draining(&dir, &model("tiny"), &none());
    assert_eq!(read(), Ok(false));
    // What a removal writes is the record, and is read as one.
    let held = hold(&holds, &record).unwrap();
    let clock = Hand::new();
    let refused = removing(Arc::new(RealFs), root, &clock, SECOND, &mut || {
        assert_eq!(std::fs::read(&mark).unwrap(), Mark.to_bytes());
        assert_eq!(read(), Ok(true));
        clock.advance(SECOND);
    });
    assert_eq!(refused, Err(RemoveRefused::Held));
    drop(held);
    assert_eq!(read(), Ok(false));
    // What is at the mark's name and is no mark: refused by the reader, with no
    // path; and a mark all the same to a hold and to a publication, which do not
    // go on beside what they cannot read.
    type Put = fn(&Path);
    let puts: [Put; 5] = [
        |mark| std::fs::write(mark, b"").unwrap(),
        |mark| std::fs::write(mark, br#"{"format":"draining/1"} "#).unwrap(),
        |mark| std::fs::write(mark, vec![b'x'; 65]).unwrap(),
        |mark| std::fs::create_dir(mark).unwrap(),
        |mark| {
            let aside = mark.with_file_name("aside");
            std::fs::write(&aside, Mark.to_bytes()).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&aside, mark).unwrap();
        },
    ];
    for (which, put) in puts.into_iter().enumerate() {
        put(&mark);
        let refused = read().unwrap_err();
        shows_nothing_of(&refused, &[&*root.to_string_lossy(), "DRAINING", "tiny"]);
        assert_eq!(
            hold(&holds, &record).unwrap_err(),
            HoldRefused::Draining,
            "put {which}"
        );
        let install = installing(&Arc::new(ShimFs::new()), root, &second(), &serving_both());
        assert_eq!(install, Err(InstallRefused::Draining), "put {which}");
        assert_eq!(reading(root), Current::Installed(record.clone()));
        // A removal replaces whatever is there with its own mark, and takes it off.
        let mut seen = false;
        let foreign = RealFs.try_share(&hold_file_of(root, &record)).unwrap();
        let clock = Hand::new();
        let refused = removing(Arc::new(RealFs), root, &clock, SECOND, &mut || {
            seen = read() == Ok(true);
            clock.advance(SECOND);
        });
        assert_eq!(refused, Err(RemoveRefused::Held), "put {which}");
        assert!(seen, "put {which}");
        drop(foreign);
        assert_eq!(read(), Ok(false), "put {which}");
        let _ = std::fs::remove_file(mark.with_file_name("aside"));
    }
}

// --- the steps of a removal ---------------------------------------------------------------------

/// The events of a removal of the second entry's installation that nothing holds.
fn clean_removal() -> Vec<Event> {
    let scratch = Scratch::new("remove-events");
    installed(scratch.path(), &second());
    let fs = Arc::new(ShimFs::new());
    let mut never = || panic!("the removal waited");
    removing(fs.clone(), scratch.path(), &Hand::new(), SECOND, &mut never).unwrap();
    fs.events()
}

#[test]
fn a_removal_marks_under_the_directorys_lock_says_removing_and_clears_the_pointer_with_every_hold_file_its_own()
 {
    let scratch = Scratch::new("remove-steps");
    let root = scratch.path().to_path_buf();
    let record = installed(&root, &second());
    let before = names_under(&root);
    let model = tiny(&root);
    let (mark, status, new) = (
        model.join("DRAINING"),
        model.join("STATUS"),
        model.join("STATUS.new"),
    );
    let pointer = model.join("CURRENT");
    let clean = clean_removal();
    let cleared = index_of(
        &clean,
        |event| matches!(event, Event::Remove(p) if p.ends_with("CURRENT")),
    );
    let fs = Arc::new(ShimFs::new());
    // When the pointer is cleared: the directory's lock is held, the hold file is
    // the removal's alone, the mark is set and the status says `removing`.
    let (at, file) = (root.clone(), hold_file_of(&root, &record));
    let seen = Arc::new(Mutex::new(None));
    let see = seen.clone();
    fs.pause_before(cleared, move || {
        let directory = RealFs.hold_lock_file(&at.join("LOCK"), false).unwrap();
        let shared = RealFs.try_share(&file).unwrap();
        *see.lock().unwrap() = Some((
            directory.is_none(),
            shared.is_none(),
            marked(&at),
            said(&at),
            reading(&at),
        ));
    });
    let mut never = || panic!("the removal waited");
    removing(fs.clone(), &root, &Hand::new(), SECOND, &mut never).unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        Some((
            true,
            true,
            true,
            Some((State::Removing, 0, 0)),
            Current::Installed(record.clone())
        ))
    );
    let events = fs.events();
    touches_only(&events, &root);
    // What a removal changes, in order: the reads are left out.
    let changes: Vec<Event> = events
        .iter()
        .filter(|event| !matches!(event, Event::Open(_) | Event::Read(_)))
        .cloned()
        .collect();
    let said_in = |to: &Path| {
        [
            Event::Write(new.clone()),
            Event::SyncFile(new.clone()),
            Event::Rename(new.clone(), to.to_path_buf()),
        ]
    };
    let mut expected = vec![
        // The mark, under the directory's lock, before the model's lock.
        Event::Lock(root.join("LOCK")),
        Event::Write(mark.clone()),
        Event::SyncFile(mark.clone()),
        Event::Lock(model.join("LOCK")),
    ];
    expected.extend(said_in(&status));
    expected.extend([
        // The pointer the removal decides from is durable before it is read.
        Event::SyncDir(model.clone()),
        Event::Lock(root.join("LOCK")),
        Event::Remove(pointer),
        Event::SyncDir(model.clone()),
    ]);
    expected.extend(said_in(&status));
    expected.extend([
        Event::Lock(root.join("LOCK")),
        Event::Remove(mark),
        Event::SyncDir(model.clone()),
    ]);
    assert_eq!(changes, expected);
    // Nothing is deleted yet, and nothing is left behind but the status.
    let mut after = before.clone();
    after.retain(|name| name != "models/tiny/CURRENT");
    assert_eq!(names_under(&root), after);
}

#[test]
fn each_step_of_the_file_system_refused_leaves_the_model_installed_or_removed() {
    let clean = clean_removal();
    for event in 0..clean.len() {
        let scratch = Scratch::new("remove-refused");
        let root = scratch.path();
        let record = installed(root, &second());
        let fs = Arc::new(ShimFs::new().fail_before(event));
        let mut never = || panic!("the removal waited");
        let ended = removing(fs.clone(), root, &Hand::new(), SECOND, &mut never);
        let now = reading(root);
        match &ended {
            Ok(removed) => {
                assert_eq!(removed.installation(), Some(record.identity()));
                assert_eq!(now, Current::NotInstalled, "event {event}");
            }
            Err(refused) => {
                assert!(
                    matches!(refused, RemoveRefused::Disk(_)),
                    "event {event}: {refused:?}"
                );
                let printed = format!("{refused} {refused:?}");
                assert!(!printed.contains(&*root.to_string_lossy()), "{printed}");
                assert!(
                    now == Current::Installed(record.clone()) || now == Current::NotInstalled,
                    "event {event}: {now:?}"
                );
            }
        }
        // The status says `removed` only of a model whose pointer is gone, and
        // what it said before of one whose pointer is there.
        match (&now, said(root)) {
            (Current::Installed(_), said) => {
                assert_eq!(said, Some((State::Installed, 2, 2)), "event {event}");
                is_whole(root, &record);
            }
            (_, Some((state, _, _))) => {
                assert!(
                    matches!(state, State::Removed | State::Removing),
                    "event {event}: {state:?}"
                );
            }
            (_, None) => panic!("event {event}: no status"),
        }
        // Asked again, the model is removed, or is said to be: no mark is left.
        let again = removed(root);
        match now {
            Current::Installed(_) => {
                assert_eq!(
                    again.unwrap().installation(),
                    Some(record.identity()),
                    "event {event}"
                );
            }
            _ => assert_eq!(again, Err(RemoveRefused::NotInstalled), "event {event}"),
        }
        assert_eq!(reading(root), Current::NotInstalled, "event {event}");
        assert!(!marked(root), "event {event}");
    }
}

/// The ways a crash loses names that are tried at every event of a removal.
const LOSSES: [Lost; 6] = [
    Lost::Unsynced,
    Lost::ContentOnly,
    Lost::Subset(0x5555_5555_5555_5555),
    Lost::Subset(0xaaaa_aaaa_aaaa_aaaa),
    Lost::Names(0x5555_5555_5555_5555),
    Lost::Names(0xaaaa_aaaa_aaaa_aaaa),
];

#[test]
fn a_crash_at_any_event_of_a_removal_leaves_the_model_installed_or_not_installed() {
    let clean = clean_removal();
    let cleared = index_of(
        &clean,
        |event| matches!(event, Event::Remove(p) if p.ends_with("CURRENT")),
    );
    for (event, lost) in (0..clean.len()).flat_map(|event| LOSSES.map(|lost| (event, lost))) {
        let at = format!("event {event}, {lost:?}");
        let scratch = Scratch::new("remove-crash");
        let root = scratch.path();
        let record = installed(root, &second());
        let fs = Arc::new(ShimFs::new().crash_before(event, lost));
        let mut never = || panic!("the removal waited");
        let ended = removing(fs.clone(), root, &Hand::new(), SECOND, &mut never);
        assert!(fs.crashed(), "{at}");
        // The process is gone. One who holds no lock reads the model as installed,
        // whole, or as not installed: never as damaged.
        let now = reading(root);
        if event <= cleared {
            assert_eq!(now, Current::Installed(record.clone()), "{at}");
            assert!(ended.is_err(), "{at}");
        } else if event > cleared + 1 {
            // The sync after the pointer was cleared has run.
            assert_eq!(now, Current::NotInstalled, "{at}");
        } else {
            assert!(
                now == Current::Installed(record.clone()) || now == Current::NotInstalled,
                "{at}: {now:?}"
            );
        }
        if ended.is_ok() {
            assert_eq!(now, Current::NotInstalled, "{at}");
        }
        if let Current::Installed(record) = &now {
            is_whole(root, record);
            assert!(hold_file_of(root, record).exists(), "{at}");
        }
        // The status is not durable, and is never half a record.
        let _ = said(root);
        // The next removal finds a mark, or none, and ends with the model not
        // installed and no mark; and the model installs again.
        let again = removed(root);
        match now {
            Current::Installed(_) => assert!(again.is_ok(), "{at}: {again:?}"),
            _ => assert_eq!(again, Err(RemoveRefused::NotInstalled), "{at}"),
        }
        assert_eq!(reading(root), Current::NotInstalled, "{at}");
        assert!(!marked(root), "{at}");
        assert!(!tiny(root).join("DRAINING").exists(), "{at}");
        // Under the loss of every name and of none: the model installs again.
        if matches!(lost, Lost::Unsynced | Lost::ContentOnly) {
            assert_eq!(installed(root, &second()), record, "{at}");
            is_whole(root, &record);
        }
    }
}

// --- a removal, an install, and one who attaches ------------------------------------------------

#[test]
fn a_removal_and_an_install_in_one_process_do_not_go_on_beside_each_other() {
    // An install has the model's lock, stopped inside its second file. A removal
    // with no patience tries the lock once: busy. It has changed nothing but the
    // mark, which it takes off; the install publishes.
    let scratch = Scratch::new("remove-install");
    let root = scratch.path().to_path_buf();
    let before = installed(&root, &first());
    let barriers = Barriers::new();
    let fake = Arc::new(
        FakeDownloader::new()
            .serving(&at("LICENSE.txt"), TERMS.to_vec())
            .stopping_after(&at("graph.onnx"), graph(), 1024, barriers.clone(), "mid"),
    );
    let installer = {
        let (fake, root) = (fake.clone(), root.clone());
        threads::spawn(move || installing(&Arc::new(ShimFs::new()), &root, &second(), &fake))
    };
    barriers.arrived("mid", 1);
    let refused = removing(
        Arc::new(RealFs),
        &root,
        &Hand::new(),
        Duration::ZERO,
        &mut || panic!("a removal with no patience paused"),
    );
    assert_eq!(refused, Err(RemoveRefused::Busy));
    assert!(!marked(&root));
    assert_eq!(reading(&root), Current::Installed(before));
    assert_eq!(said(&root), Some((State::Installing, 1, 2)));
    barriers.release("mid");
    let published = installer.join().unwrap().unwrap();
    assert_eq!(reading(&root), Current::Installed(published.clone()));
    // A removal has the model's lock and waits for a hold. An install is busy, and
    // touches nothing.
    let holds = holds_over(&root);
    let held = hold(&holds, &published).unwrap();
    let clock = Arc::new(Hand::new());
    let remover = removal_on_a_thread(&root, &barriers, &clock);
    barriers.arrived("try", 1);
    let fs = Arc::new(ShimFs::new());
    let other = serving_both();
    let refused = installing(&fs, &root, &first(), &other);
    assert_eq!(refused, Err(InstallRefused::Busy));
    assert!(fs.events().is_empty() && other.requests().is_empty());
    held.let_go();
    barriers.open("try");
    assert!(remover.join().unwrap().is_ok());
    assert_eq!(reading(&root), Current::NotInstalled);
}

/// A request for the second entry on the real file system: it installs, or attaches.
fn asking(
    root: &Path,
    fake: &FakeDownloader,
    watch: &mut dyn FnMut(Option<State>),
) -> Result<whizbang_backends::models::InstallationRecord, InstallRefused> {
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    install_or_attach(
        &dir,
        &second(),
        &adapter(),
        fake,
        &Cancellation::new(),
        &none(),
        &mut |status| watch(status.map(|status| status.state())),
    )
}

#[test]
fn one_who_attaches_to_a_removal_is_answered_busy_and_to_one_that_changed_nothing_as_before() {
    // A removal has the model's lock and waits for a hold. A request for the model
    // attaches: it is shown nothing, since no install is running. The hold is let
    // go and the removal clears the pointer: the request is told that the model was
    // busy, and never that it is installed. Asked again, it installs.
    let scratch = Scratch::new("remove-attach");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    let held = hold(&holds, &record).unwrap();
    let barriers = Barriers::new();
    let clock = Arc::new(Hand::new());
    let remover = removal_on_a_thread(root, &barriers, &clock);
    barriers.arrived("try", 1);
    let fake = serving_both();
    let mut seen = Vec::new();
    let attached = asking(root, &fake, &mut |state| {
        if seen.is_empty() {
            held.let_go();
            barriers.open("try");
        }
        seen.push(state);
        std::thread::yield_now();
    });
    assert!(remover.join().unwrap().is_ok());
    assert!(seen.iter().all(Option::is_none), "{seen:?}");
    assert_eq!(attached, Err(InstallRefused::Busy));
    assert!(fake.requests().is_empty());
    assert_eq!(said(root), Some((State::Removed, 0, 0)));
    assert_eq!(asking(root, &fake, &mut |_| {}), Ok(record.clone()));
    // The removal that is attached to keeps the hold past its deadline: it changed
    // nothing, the status says what it said, and the request is answered as it
    // would have been with no removal.
    let held = hold(&holds, &record).unwrap();
    let barriers = Barriers::new();
    let remover = removal_on_a_thread(root, &barriers, &clock);
    barriers.arrived("try", 1);
    let mut shown = false;
    let attached = asking(root, &fake, &mut |_| {
        if !shown {
            shown = true;
            clock.advance(60 * SECOND);
            barriers.open("try");
        }
        std::thread::yield_now();
    });
    assert_eq!(remover.join().unwrap(), Err(RemoveRefused::Held));
    assert_eq!(attached, Ok(record));
    drop(held);
}

// --- what a removal shows -----------------------------------------------------------------------

#[test]
fn a_model_name_that_holds_a_secret_is_not_removed_and_no_refusal_shows_a_path() {
    let scratch = Scratch::new("remove-secret");
    let root = scratch.path();
    installed(root, &second());
    let fs = Arc::new(ShimFs::new());
    let dir = ModelsDir::at(fs.clone(), root);
    let named = ModelName::new(MARKER, &none()).unwrap();
    let mut never = || panic!("the removal waited");
    let refused = remove(&dir, &named, &Hand::new(), SECOND, &secrets(), &mut never);
    assert_eq!(refused, Err(RemoveRefused::HoldsSecret));
    assert!(fs.events().is_empty());
    let all = [
        RemoveRefused::HoldsSecret,
        RemoveRefused::NotInstalled,
        RemoveRefused::Busy,
        RemoveRefused::Held,
        RemoveRefused::DirectoryBusy,
        RemoveRefused::Disk(std::io::ErrorKind::PermissionDenied),
    ];
    for refused in all {
        let printed = format!("{refused} {refused:?}");
        assert!(
            printed.bytes().all(|b| b.is_ascii_graphic() || b == b' '),
            "{printed:?}"
        );
        assert!(printed.len() <= 200, "{printed}");
        for part in [&*root.to_string_lossy(), "models/", "DRAINING", MARKER] {
            assert!(!printed.contains(part), "{printed}");
        }
    }
    // The directory's lock, not let go by whoever has it: refused, before the mark.
    let directory = RealFs.hold_lock_file(&root.join("LOCK"), false).unwrap();
    assert!(directory.is_some());
    assert_eq!(removed(root), Err(RemoveRefused::DirectoryBusy));
    drop(directory);
    assert!(!tiny(root).join("DRAINING").exists());
    assert!(alone(root, &record_of(&second())).is_some());
}
