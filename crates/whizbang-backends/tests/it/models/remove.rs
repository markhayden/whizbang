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

use super::attaching::{Deaf, Other, root_from_parent, say, told};
use super::holds::{CHILD, alone, count, hold, holds_over};
use super::publishing::{
    adapter, entry_at, events_after, first, folder_of, hold_file_of, index_of, installations,
    installed, installing, is_whole, reading, record_of, said, second, tiny,
};
use super::staging::{TERMS, at, graph, incoming, names_under, serving_both, touches_only};
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

pub(super) const SECOND: Duration = Duration::from_secs(1);

/// What paces a removal that must not have to wait.
fn never() {
    panic!("the removal waited");
}

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
pub(super) fn removal_on_a_thread(
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
    // And its files are gone; the hold file stays, as every lock file does.
    assert!(installations(root).is_empty());
    assert!(hold_file_of(root, &record).exists());
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
    // The event at which an install of the second entry over the first takes the
    // directory's lock to point.
    let pointing = index_of(
        &events_after(&[&first()], &second()),
        |event| matches!(event, Event::Lock(p) if !p.ends_with("tiny/LOCK")),
    );
    for (which, put) in puts.into_iter().enumerate() {
        let scratch = Scratch::new("remove-mark-put");
        let root = scratch.path();
        let record = installed(root, &first());
        let holds = holds_over(root);
        let mark = tiny(root).join("DRAINING");
        let dir = ModelsDir::at(Arc::new(RealFs), root);
        let read = || draining(&dir, &model("tiny"), &none());
        put(&mark);
        let refused = read().unwrap_err();
        shows_nothing_of(&refused, &[&*root.to_string_lossy(), "DRAINING", "tiny"]);
        assert_eq!(
            hold(&holds, &record).unwrap_err(),
            HoldRefused::Draining,
            "put {which}"
        );
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
        // Put there when an install is about to point, it stops the publication.
        let fs = Arc::new(ShimFs::new());
        let at = mark.clone();
        fs.pause_before(pointing, move || put(&at));
        let install = installing(&fs, root, &second(), &serving_both());
        assert_eq!(install, Err(InstallRefused::Draining), "put {which}");
        assert_eq!(reading(root), Current::Installed(record.clone()));
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
        // The deleting, once the cleared pointer is durable, and its sync.
        Event::Remove(folder_of(&root, &record)),
        Event::SyncDir(model.join("installations")),
    ]);
    expected.extend(said_in(&status));
    expected.extend([
        Event::Lock(root.join("LOCK")),
        Event::Remove(mark),
        Event::SyncDir(model.clone()),
    ]);
    assert_eq!(changes, expected);
    // What is left: the lock files, the hold file, the status and the empty
    // folders. The pointer and the installation are gone.
    let mut after = before.clone();
    after.retain(|name| {
        name != "models/tiny/CURRENT" && !name.starts_with("models/tiny/installations/")
    });
    assert_eq!(names_under(&root), after);
    assert!(after.contains(&format!("models/tiny/holds/{}", record.identity().hex())));
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
        // A removal that was refused after it cleared the pointer is finished by
        // the next: nothing of the installation is left.
        assert!(installations(root).is_empty(), "event {event}");
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
    // Up to the clearing of the pointer the only names that are not durable are the
    // mark's and the status's, which nothing here decides by: those events are
    // tried under the loss of every name and of none. From the clearing on, where
    // the pointer and the folders are at stake, under all six.
    let tried = (0..clean.len()).flat_map(|event| {
        let losses = if event < cleared { 2 } else { LOSSES.len() };
        LOSSES[..losses].iter().map(move |lost| (event, *lost))
    });
    for (event, lost) in tried {
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
        // A limit, pinned: a mark that the crash left stays, and while it does no
        // hold on the model is given, though its installation is named and whole.
        if let Current::Installed(record) = &now {
            let asked = hold(&holds_over(root), record).map(|_| ());
            let left = tiny(root).join("DRAINING").exists();
            let expected = if left {
                Err(HoldRefused::Draining)
            } else {
                Ok(())
            };
            assert_eq!(asked, expected, "{at}");
        }
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
        // Whatever the crash left or brought back of the folder, the next removal
        // deleted it.
        assert!(installations(root).is_empty(), "{at}");
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

// --- draining across processes, and against an install ------------------------------------------

#[test]
fn the_two_cases_of_a_removal_with_the_hold_in_a_real_second_process() {
    let scratch = Scratch::new("remove-held-process");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    let mut other = Other::running(CHILD, root);
    assert_eq!(other.says("holds"), record.identity().to_string());
    // The hold is kept past the deadline: busy, the mark is gone and the next hold
    // is given.
    let clock = Hand::new();
    let mut pauses = 0;
    let refused = removing(Arc::new(RealFs), root, &clock, 30 * SECOND, &mut || {
        pauses += 1;
        assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
        clock.advance(10 * SECOND);
    });
    assert_eq!(refused, Err(RemoveRefused::Held));
    assert_eq!(pauses, 3);
    assert!(!marked(root));
    assert_eq!(reading(root), Current::Installed(record.clone()));
    assert_eq!(said(root), Some((State::Installed, 2, 2)));
    drop(hold(&holds, &record).unwrap());
    // A removal during the held installation refuses a new hold, and one asked for
    // again, and goes on when the other process lets its hold go.
    let barriers = Barriers::new();
    let clock = Arc::new(Hand::new());
    let remover = removal_on_a_thread(root, &barriers, &clock);
    barriers.arrived("try", 1);
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
    barriers.release("try");
    barriers.arrived("try", 2);
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
    assert_eq!(reading(root), Current::Installed(record.clone()));
    other.tell("go");
    assert_eq!(other.says("let"), "go");
    other.ended_well();
    barriers.open("try");
    let removed = remover.join().unwrap().unwrap();
    assert_eq!(removed.installation(), Some(record.identity()));
    assert_eq!(reading(root), Current::NotInstalled);
    assert!(!marked(root));
}

#[test]
fn a_removal_and_an_install_let_go_together_end_in_one_of_the_two_orders() {
    // Two processes, each a thread on a disk of its own, each stopped before its
    // first step and released together, eight times. The model has one
    // installation; the install is of another. Either the install ran first, whole,
    // and the removal removed what it installed; or the removal ran first, of what
    // was there, and the install ran after it, whole, or stopped for the mark, or
    // found the lock taken. Never half of either.
    for round in 0..8 {
        let scratch = Scratch::new("remove-race");
        let root = scratch.path().to_path_buf();
        let old = installed(&root, &first());
        let new = record_of(&second());
        let barriers = Barriers::new();
        let installer = {
            let fs = Arc::new(ShimFs::new());
            let stop = barriers.clone();
            fs.pause_before(0, move || stop.stop_at("start"));
            let root = root.clone();
            threads::spawn(move || installing(&fs, &root, &second(), &serving_both()))
        };
        let remover = {
            let fs = Arc::new(ShimFs::new());
            let stop = barriers.clone();
            fs.pause_before(0, move || stop.stop_at("start"));
            let root = root.clone();
            threads::spawn(move || {
                removing(fs, &root, &Hand::new(), 60 * SECOND, &mut || {
                    std::thread::yield_now();
                })
            })
        };
        barriers.arrived("start", 2);
        barriers.open("start");
        let published = installer.join().unwrap();
        let removed = remover.join().unwrap();
        let was = removed
            .unwrap_or_else(|refused| panic!("round {round}: {refused:?}"))
            .installation();
        let now = reading(&root);
        match published {
            Ok(record) if was == Some(new.identity()) => {
                assert_eq!(record, new, "round {round}");
                assert_eq!(now, Current::NotInstalled, "round {round}");
            }
            Ok(record) => {
                assert_eq!(was, Some(old.identity()), "round {round}");
                assert_eq!(now, Current::Installed(record.clone()), "round {round}");
                is_whole(&root, &record);
            }
            Err(refused) => {
                assert!(
                    matches!(refused, InstallRefused::Draining | InstallRefused::Busy),
                    "round {round}: {refused:?}"
                );
                assert_eq!(was, Some(old.identity()), "round {round}");
                assert_eq!(now, Current::NotInstalled, "round {round}");
            }
        }
        assert!(!marked(&root), "round {round}");
        assert!(!incoming(&root).exists(), "round {round}");
        assert!(!tiny(&root).join("CURRENT.new").exists(), "round {round}");
    }
}

/// The event that follows the first read of the pointer, under the model's lock.
fn after_the_pointer_was_read(events: &[Event]) -> usize {
    let locked = index_of(
        events,
        |event| matches!(event, Event::Lock(p) if p.ends_with("tiny/LOCK")),
    );
    let read = index_of(
        events,
        |event| matches!(event, Event::Read(p) if p.ends_with("CURRENT")),
    );
    assert!(locked < read);
    read + 1
}

#[test]
fn with_an_install_stopped_after_it_read_the_pointer_a_removal_does_not_go_on_beside_it() {
    // The install has the model's lock and has read the pointer. A removal sets its
    // mark and tries for the lock until its deadline: it changes nothing else, and
    // is busy. Its mark is gone, and the install, let go, publishes.
    let stopped = after_the_pointer_was_read(&events_after(&[&first()], &second()));
    let scratch = Scratch::new("remove-beside-install");
    let root = scratch.path().to_path_buf();
    let before = installed(&root, &first());
    let stopping = |barriers: &Barriers| {
        let fs = Arc::new(ShimFs::new());
        let stop = barriers.clone();
        fs.pause_before(stopped, move || stop.stop_at("read"));
        let (root, fake) = (root.clone(), Arc::new(serving_both()));
        let asked = fake.clone();
        let installer = threads::spawn(move || installing(&fs, &root, &second(), &fake));
        barriers.arrived("read", 1);
        (installer, asked)
    };
    let barriers = Barriers::new();
    let (installer, _) = stopping(&barriers);
    let fs = Arc::new(ShimFs::new());
    let clock = Hand::new();
    let mut pauses = 0;
    let refused = removing(fs.clone(), &root, &clock, 30 * SECOND, &mut || {
        pauses += 1;
        assert!(marked(&root));
        assert_eq!(reading(&root), Current::Installed(before.clone()));
        clock.advance(10 * SECOND);
    });
    assert_eq!(refused, Err(RemoveRefused::Busy));
    assert_eq!(pauses, 3);
    // What the removal did on its disk: the directory's lock and the mark, set and
    // taken off. It never had the model's lock.
    let (lock, mark) = (root.join("LOCK"), tiny(&root).join("DRAINING"));
    assert_eq!(
        fs.events(),
        [
            Event::Lock(lock.clone()),
            Event::Write(mark.clone()),
            Event::SyncFile(mark.clone()),
            Event::Lock(lock),
            Event::Remove(mark),
            Event::SyncDir(tiny(&root)),
        ]
    );
    assert!(!marked(&root));
    assert_eq!(reading(&root), Current::Installed(before.clone()));
    assert_eq!(said(&root), Some((State::Installing, 0, 2)));
    barriers.release("read");
    assert_eq!(installer.join().unwrap(), Ok(record_of(&second())));
    // The same, with the removal still trying when the install is let go: the
    // install stops for the mark before it asks for a file, and the removal goes
    // on, of what was installed before it.
    installed(&root, &first());
    let barriers = Barriers::new();
    let (installer, asked) = stopping(&barriers);
    let mut pauses = 0;
    let removed = removing(Arc::new(RealFs), &root, &Hand::new(), SECOND, &mut || {
        pauses += 1;
        if pauses == 1 {
            barriers.release("read");
        }
        std::thread::yield_now();
    });
    assert_eq!(installer.join().unwrap(), Err(InstallRefused::Draining));
    assert!(asked.requests().is_empty());
    assert_eq!(removed.unwrap().installation(), Some(before.identity()));
    assert_eq!(reading(&root), Current::NotInstalled);
    assert_eq!(said(&root), Some((State::Removed, 0, 0)));
    assert!(!marked(&root));
}

#[test]
fn with_a_removal_stopped_after_it_read_the_pointer_an_install_is_busy_and_touches_nothing() {
    let stopped = after_the_pointer_was_read(&clean_removal());
    let scratch = Scratch::new("remove-beside-removal");
    let root = scratch.path().to_path_buf();
    let record = installed(&root, &second());
    let barriers = Barriers::new();
    let remover = {
        let fs = Arc::new(ShimFs::new());
        let stop = barriers.clone();
        fs.pause_before(stopped, move || stop.stop_at("read"));
        let root = root.clone();
        threads::spawn(move || removing(fs, &root, &Hand::new(), SECOND, &mut never))
    };
    barriers.arrived("read", 1);
    let fs = Arc::new(ShimFs::new());
    let fake = serving_both();
    let refused = installing(&fs, &root, &first(), &fake);
    assert_eq!(refused, Err(InstallRefused::Busy));
    assert!(fs.events().is_empty() && fake.requests().is_empty());
    assert_eq!(reading(&root), Current::Installed(record.clone()));
    // And a second removal, with no patience, is busy and leaves the first one's
    // mark as it found it: set.
    assert!(marked(&root));
    let again = removing(
        Arc::new(RealFs),
        &root,
        &Hand::new(),
        Duration::ZERO,
        &mut never,
    );
    assert_eq!(again, Err(RemoveRefused::Busy));
    assert!(marked(&root));
    barriers.release("read");
    let removed = remover.join().unwrap().unwrap();
    assert_eq!(removed.installation(), Some(record.identity()));
    assert_eq!(reading(&root), Current::NotInstalled);
    assert!(!marked(&root));
}

/// Run only in a process of its own: removes the model in the directory it is told
/// of, and stops when it has set its mark, before it takes the model's lock, until
/// a line arrives on its input.
#[test]
#[ignore = "run in a process of its own by the test of a remover that is killed"]
fn child_removes_and_stops_when_it_has_marked() {
    let root = root_from_parent();
    let fs = Arc::new(ShimFs::new());
    fs.pause_before(3, || {
        say("marked yes");
        told();
    });
    let ended = removing(fs, &root, &Hand::new(), SECOND, &mut never);
    say(&format!("ended {}", ended.is_ok()));
}

#[test]
fn a_mark_left_by_a_process_that_died_is_taken_off_by_the_next_to_take_the_lock() {
    let scratch = Scratch::new("remove-stale-mark");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    // A remover that dies when it has set its mark, before it takes the model's
    // lock: a disk of its own stands for its process, and keeps the mark.
    let dies = || {
        let fs = Arc::new(ShimFs::new().crash_before(3, Lost::ContentOnly));
        let ended = removing(fs.clone(), root, &Hand::new(), SECOND, &mut never);
        assert!(ended.is_err() && fs.crashed());
        assert_eq!(fs.events().len(), 3);
        assert!(marked(root));
        assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
        assert_eq!(reading(root), Current::Installed(record.clone()));
    };
    // The next to take the lock is an install, of the identity that is there: it
    // takes the mark off under the directory's lock, as its first change after the
    // status, downloads nothing, and the next hold is given. The sync that follows
    // is the install's own, of the model's folder, before it deletes anything.
    dies();
    let fs = Arc::new(ShimFs::new());
    let fake = serving_both();
    assert_eq!(installing(&fs, root, &second(), &fake), Ok(record.clone()));
    assert!(fake.requests().is_empty());
    assert!(!marked(root));
    let events = fs.events();
    let model = tiny(root);
    let locked = index_of(&events, |event| *event == Event::Lock(model.join("LOCK")));
    let said_first = index_of(
        &events,
        |event| matches!(event, Event::Rename(_, to) if *to == model.join("STATUS")),
    );
    assert!(locked < said_first);
    assert_eq!(
        events[said_first + 1..said_first + 4],
        [
            Event::Lock(root.join("LOCK")),
            Event::Remove(model.join("DRAINING")),
            Event::SyncDir(model.clone()),
        ]
    );
    drop(hold(&holds, &record).unwrap());
    // The next is a removal: it sets its own mark over the one it found, and takes
    // it off when it is done.
    dies();
    assert!(removed(root).is_ok());
    assert!(!marked(root));
    assert_eq!(reading(root), Current::NotInstalled);
    // And the remover is a real process that is killed when it has set its mark.
    installed(root, &second());
    let mut other = Other::running(
        "models::remove::child_removes_and_stops_when_it_has_marked",
        root,
    );
    assert_eq!(other.says("marked"), "yes");
    assert!(marked(root));
    other.killed();
    assert!(marked(root));
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
    assert_eq!(installed(root, &second()), record);
    assert!(!marked(root));
    drop(hold(&holds, &record).unwrap());
}

#[test]
fn a_removal_that_is_not_given_the_models_lock_sets_its_mark_again_and_again_when_it_is() {
    // One who takes the model's lock takes off a mark it finds: it cannot tell a
    // dead remover's from a live one's. A live remover sets its own again: each
    // time it is not given the lock, and when it is.
    let scratch = Scratch::new("remove-mark-again");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    let held = hold(&holds, &record).unwrap();
    let mark = tiny(root).join("DRAINING");
    let lock = RealFs
        .hold_lock_file(&tiny(root).join("LOCK"), false)
        .unwrap();
    assert!(lock.is_some());
    let barriers = Barriers::new();
    let clock = Arc::new(Hand::new());
    let remover = removal_on_a_thread(root, &barriers, &clock);
    barriers.arrived("try", 1);
    assert!(marked(root));
    std::fs::remove_file(&mark).unwrap();
    barriers.release("try");
    barriers.arrived("try", 2);
    assert!(marked(root));
    // The mark is taken off once more and the lock let go: the removal is given
    // the lock, sets its mark, and waits for the hold under it.
    std::fs::remove_file(&mark).unwrap();
    drop(lock);
    barriers.release("try");
    barriers.arrived("try", 3);
    assert!(marked(root));
    assert_eq!(said(root), Some((State::Removing, 0, 0)));
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
    held.let_go();
    barriers.open("try");
    assert!(remover.join().unwrap().is_ok());
    assert!(!marked(root));
}

#[test]
fn an_install_stops_for_a_mark_before_each_file_and_at_each_piece_whatever_the_downloader_does() {
    // Inside a file, of a downloader that looks at nothing: the mark is set after
    // the first piece of the graph, the piece after it is not kept, and the install
    // ends as draining.
    let scratch = Scratch::new("remove-mark-inside");
    let root = scratch.path();
    let before = installed(root, &first());
    let mark = tiny(root).join("DRAINING");
    let set = mark.clone();
    let deaf = Deaf::doing(move || std::fs::write(&set, Mark.to_bytes()).unwrap());
    let fs = Arc::new(ShimFs::new());
    let dir = ModelsDir::at(fs.clone(), root);
    let install = |deaf: &Deaf| {
        let cancellation = Cancellation::new();
        whizbang_backends::models::install(
            &dir,
            &second(),
            &adapter(),
            deaf,
            &cancellation,
            &none(),
        )
    };
    assert_eq!(install(&deaf), Err(InstallRefused::Draining));
    let appended = fs
        .events()
        .iter()
        .filter(|event| matches!(event, Event::Append(p) if p.ends_with("graph.onnx")))
        .count();
    assert_eq!(appended, 1);
    assert_eq!(said(root), Some((State::Failed, 1, 2)));
    assert!(!incoming(root).exists());
    assert_eq!(reading(root), Current::Installed(before.clone()));
    // The install does not take off the mark it stopped for.
    assert!(marked(root));
    std::fs::remove_file(&mark).unwrap();
    // Between files: set when the first file is staged, the second is never asked
    // for.
    let clean = events_after(&[&first()], &second());
    let between = index_of(
        &clean,
        |event| matches!(event, Event::Begin(p) if p.ends_with("graph.onnx")),
    );
    let staged = clean[..between]
        .iter()
        .rposition(|event| matches!(event, Event::Finish(_)))
        .unwrap();
    let deaf = Deaf::doing(|| {});
    let fs = Arc::new(ShimFs::new());
    let set = mark.clone();
    fs.pause_before(staged + 1, move || {
        std::fs::write(&set, Mark.to_bytes()).unwrap();
    });
    let dir = ModelsDir::at(fs.clone(), root);
    let ended = whizbang_backends::models::install(
        &dir,
        &second(),
        &adapter(),
        &deaf,
        &Cancellation::new(),
        &none(),
    );
    assert_eq!(ended, Err(InstallRefused::Draining));
    assert_eq!(deaf.asked(), ["LICENSE.txt"]);
    assert_eq!(said(root), Some((State::Failed, 1, 2)));
    assert!(!incoming(root).exists());
    assert_eq!(reading(root), Current::Installed(before));
}

#[test]
fn a_removal_of_a_model_that_is_installing_cancels_the_install_and_what_was_installed_before_is_removed_or_kept()
 {
    // An install of another identity is stopped inside its second file. A removal
    // sets its mark and tries for the model's lock; the download, let go, reads the
    // mark at its next piece and stops; the install ends as draining, with its
    // staging removed; and the removal goes on, of what was installed before.
    let scratch = Scratch::new("remove-cancels");
    let root = scratch.path().to_path_buf();
    let before = installed(&root, &first());
    let stopped_mid_file = |barriers: &Barriers| {
        let fake = Arc::new(
            FakeDownloader::new()
                .serving(&at("LICENSE.txt"), TERMS.to_vec())
                .stopping_after(&at("graph.onnx"), graph(), 1024, barriers.clone(), "mid"),
        );
        let (served, root) = (fake.clone(), root.clone());
        let installer =
            threads::spawn(move || installing(&Arc::new(ShimFs::new()), &root, &second(), &served));
        barriers.arrived("mid", 1);
        (installer, fake)
    };
    // Kept: the removal's deadline comes while the install is stopped. It is busy,
    // and what was installed before is named and whole; the install goes on.
    let barriers = Barriers::new();
    let (installer, _) = stopped_mid_file(&barriers);
    let clock = Hand::new();
    let refused = removing(Arc::new(RealFs), &root, &clock, SECOND, &mut || {
        clock.advance(SECOND);
    });
    assert_eq!(refused, Err(RemoveRefused::Busy));
    assert_eq!(reading(&root), Current::Installed(before.clone()));
    is_whole(&root, &before);
    barriers.release("mid");
    assert_eq!(installer.join().unwrap(), Ok(record_of(&second())));
    // Removed: the removal is still trying when the download is let go.
    let before = installed(&root, &first());
    let barriers = Barriers::new();
    let (installer, fake) = stopped_mid_file(&barriers);
    let mut pauses = 0;
    let removed = removing(Arc::new(RealFs), &root, &Hand::new(), SECOND, &mut || {
        pauses += 1;
        if pauses == 1 {
            barriers.release("mid");
        }
        std::thread::yield_now();
    });
    assert_eq!(installer.join().unwrap(), Err(InstallRefused::Draining));
    assert_eq!(removed.unwrap().installation(), Some(before.identity()));
    // No byte of the file was kept after the mark, and none is on the disk.
    assert!(fake.handed(&at("graph.onnx")) < 2500);
    assert_eq!(fake.requests().len(), 2);
    assert!(!incoming(&root).exists());
    assert_eq!(reading(&root), Current::NotInstalled);
    assert_eq!(said(&root), Some((State::Removed, 0, 0)));
    assert!(!marked(&root));
}

/// Run only in a process of its own: installs the second entry in the directory it
/// is told of, and stops between the first piece of its second file and the second
/// until a line arrives on its input. It says how it ended.
#[test]
#[ignore = "run in a process of its own by the test of a removal that cancels an install in another process"]
fn child_installs_and_stops_between_two_pieces_of_its_second_file() {
    let root = root_from_parent();
    let clean = events_after(&[&first()], &second());
    let pieces: Vec<usize> = (0..clean.len())
        .filter(|at| matches!(&clean[*at], Event::Append(p) if p.ends_with("graph.onnx")))
        .collect();
    assert_eq!(pieces.len(), 3);
    let fs = Arc::new(ShimFs::new());
    fs.pause_before(pieces[1], || {
        say("stopped inside");
        told();
    });
    let ended = installing(&fs, &root, &second(), &serving_both());
    let appended = fs
        .events()
        .iter()
        .filter(|event| matches!(event, Event::Append(p) if p.ends_with("graph.onnx")))
        .count();
    say(&format!(
        "ended {:?}",
        ended.map(|record| record.identity())
    ));
    say(&format!("appended {appended}"));
}

#[test]
fn a_removal_cancels_an_install_that_runs_in_a_second_process() {
    // Nothing is signalled and no process id is read: the removal sets its mark,
    // and the install in the other process reads it at its next piece.
    let scratch = Scratch::new("remove-cancels-process");
    let root = scratch.path();
    let before = installed(root, &first());
    let mut other = Other::running(
        "models::remove::child_installs_and_stops_between_two_pieces_of_its_second_file",
        root,
    );
    assert_eq!(other.says("stopped"), "inside");
    assert_eq!(said(root), Some((State::Installing, 1, 2)));
    let mut pauses = 0;
    let removed = removing(Arc::new(RealFs), root, &Hand::new(), SECOND, &mut || {
        pauses += 1;
        if pauses == 1 {
            assert!(marked(root));
            other.tell("go");
        }
        std::thread::yield_now();
    });
    assert_eq!(removed.unwrap().installation(), Some(before.identity()));
    assert_eq!(other.says("ended"), "Err(Draining)");
    // The piece it was stopped before was appended; the one after the mark was not.
    assert_eq!(other.says("appended"), "2");
    other.ended_well();
    assert!(!incoming(root).exists());
    assert_eq!(reading(root), Current::NotInstalled);
    assert_eq!(said(root), Some((State::Removed, 0, 0)));
    assert!(!marked(root));
}

// --- removal on disk -----------------------------------------------------------------------------

#[test]
fn a_removal_deletes_every_installation_of_the_model_under_its_hold_files_and_touches_nothing_outside()
 {
    // Two installations are on the disk: the one in use, and one that was
    // superseded while a run held it. Beside them, what no installation is: a file,
    // and a link that leads out of the models directory. And a staging folder that
    // a dead install left.
    let scratch = Scratch::new("remove-deletes");
    let root = scratch.path().to_path_buf();
    let old = installed(&root, &first());
    let foreign = RealFs.try_share(&hold_file_of(&root, &old)).unwrap();
    let new = installed(&root, &second());
    drop(foreign);
    is_whole(&root, &old);
    let outside = Scratch::new("remove-deletes-outside");
    std::fs::write(outside.path().join("kept"), b"not whizbang's").unwrap();
    std::fs::write(tiny(&root).join("installations/notes.txt"), b"x").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), tiny(&root).join("installations/elsewhere"))
        .unwrap();
    std::fs::create_dir_all(incoming(&root).join("files")).unwrap();
    std::fs::write(incoming(&root).join("files/graph.onnx"), b"part").unwrap();
    // When the deleting is synced, both hold files are still the removal's alone.
    let fs = Arc::new(ShimFs::new());
    let synced = Event::SyncDir(tiny(&root).join("installations"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (see, files) = (
        seen.clone(),
        [hold_file_of(&root, &old), hold_file_of(&root, &new)],
    );
    let at = clean_removal_index(&root, &synced);
    fs.pause_before(at, move || {
        for file in &files {
            see.lock()
                .unwrap()
                .push(RealFs.try_share(file).unwrap().is_none());
        }
    });
    let removed = removing(fs.clone(), &root, &Hand::new(), SECOND, &mut never);
    assert_eq!(removed.unwrap().installation(), Some(new.identity()));
    assert_eq!(*seen.lock().unwrap(), [true, true]);
    assert!(installations(&root).is_empty());
    assert!(!incoming(&root).exists());
    assert!(hold_file_of(&root, &old).exists() && hold_file_of(&root, &new).exists());
    assert!(alone(&root, &old).is_some() && alone(&root, &new).is_some());
    assert_eq!(
        std::fs::read(outside.path().join("kept")).unwrap(),
        b"not whizbang's"
    );
    let events = fs.events();
    touches_only(&events, &root);
    // Nothing is deleted before the cleared pointer is durable, and the deleting
    // is synced.
    let cleared = index_of(&events, |event| {
        *event == Event::Remove(tiny(&root).join("CURRENT"))
    });
    assert_eq!(events[cleared + 1], Event::SyncDir(tiny(&root)));
    let deleted: Vec<usize> = (0..events.len())
        .filter(|at| {
            matches!(&events[*at], Event::Remove(p)
                if p.parent() == Some(&tiny(&root).join("installations")))
        })
        .collect();
    assert_eq!(deleted.len(), 4);
    assert!(deleted[0] > cleared + 1);
    assert_eq!(events[deleted[3] + 1], synced);
    // The model installs again, from nothing.
    let fake = serving_both();
    let again = installing(&Arc::new(ShimFs::new()), &root, &second(), &fake);
    assert_eq!(again, Ok(new.clone()));
    assert_eq!(fake.requests().len(), 2);
    is_whole(&root, &new);
}

/// The index of this event among the events of a removal of the directory at
/// `root` as it is now, found on a copy of it that is removed for the purpose.
fn clean_removal_index(root: &Path, event: &Event) -> usize {
    let copy = Scratch::new("remove-events-copy");
    copy_tree(root, copy.path());
    let fs = Arc::new(ShimFs::new());
    removing(fs.clone(), copy.path(), &Hand::new(), SECOND, &mut never).unwrap();
    let there = format!("{event:?}").replace(
        &root.display().to_string(),
        &copy.path().display().to_string(),
    );
    fs.events()
        .iter()
        .position(|event| format!("{event:?}") == there)
        .unwrap()
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(&path).unwrap(), &target).unwrap();
        } else if path.is_dir() {
            std::fs::create_dir(&target).unwrap();
            copy_tree(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

/// How a removal is interrupted once it has cleared the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Interrupted {
    /// It dies before it deletes: the mark, no pointer, the folder.
    CrashBeforeDeleting,
    /// The deleting is refused: no pointer, the folder, and no mark.
    RefusedDeleting,
    /// It dies when it has deleted and not synced, and the crash brings the folder
    /// back.
    CrashBeforeTheSync,
}

#[test]
fn a_removal_interrupted_after_the_pointer_was_cleared_is_finished_by_the_next_install_or_removal()
{
    let clean = clean_removal();
    let deleting = index_of(&clean, |event| {
        matches!(event, Event::Remove(p)
            if p.parent().is_some_and(|folder| folder.ends_with("installations")))
    });
    let hows = [
        Interrupted::CrashBeforeDeleting,
        Interrupted::RefusedDeleting,
        Interrupted::CrashBeforeTheSync,
    ];
    for (how, by_install) in hows.into_iter().flat_map(|how| [(how, false), (how, true)]) {
        let at = format!("{how:?}, by an install {by_install}");
        let scratch = Scratch::new("remove-interrupted");
        let root = scratch.path();
        let record = installed(root, &second());
        let fs = match how {
            Interrupted::CrashBeforeDeleting => {
                ShimFs::new().crash_before(deleting, Lost::ContentOnly)
            }
            Interrupted::RefusedDeleting => ShimFs::new().fail_before(deleting),
            Interrupted::CrashBeforeTheSync => {
                ShimFs::new().crash_before(deleting + 1, Lost::Unsynced)
            }
        };
        let ended = removing(Arc::new(fs), root, &Hand::new(), SECOND, &mut never);
        assert!(
            matches!(ended, Err(RemoveRefused::Disk(_))),
            "{at}: {ended:?}"
        );
        assert_eq!(reading(root), Current::NotInstalled, "{at}");
        is_whole(root, &record);
        // A crash leaves the mark; a refusal does not.
        assert_eq!(
            tiny(root).join("DRAINING").exists(),
            how != Interrupted::RefusedDeleting,
            "{at}"
        );
        if by_install {
            // The next install, of another identity: it takes the mark off and
            // deletes what no pointer names, before it stages.
            let other = entry_at("2026-11.3");
            let fs = Arc::new(ShimFs::new());
            let installed = installing(&fs, root, &other, &serving_both()).unwrap();
            assert_eq!(installations(root), [installed.identity().hex()], "{at}");
            let events = fs.events();
            let deleted = index_of(&events, |event| {
                *event == Event::Remove(folder_of(root, &record))
            });
            let begun = index_of(&events, |event| matches!(event, Event::Begin(_)));
            assert!(deleted < begun, "{at}");
            assert_eq!(reading(root), Current::Installed(installed), "{at}");
        } else {
            // The next removal: the model is not installed, and it says so; what
            // was left is deleted, and the mark is gone.
            let fs = Arc::new(ShimFs::new());
            let again = removing(fs.clone(), root, &Hand::new(), SECOND, &mut never);
            assert_eq!(again, Err(RemoveRefused::NotInstalled), "{at}");
            assert!(installations(root).is_empty(), "{at}");
            let events = fs.events();
            let deleted = index_of(&events, |event| {
                *event == Event::Remove(folder_of(root, &record))
            });
            assert_eq!(
                events[deleted + 1],
                Event::SyncDir(tiny(root).join("installations")),
                "{at}"
            );
            // Nothing is deleted on the word of a name that is not durable: the
            // model's folder was synced before the pointer was read.
            assert!(
                events[..deleted].contains(&Event::SyncDir(tiny(root))),
                "{at}"
            );
            touches_only(&events, root);
            assert_eq!(reading(root), Current::NotInstalled, "{at}");
        }
        assert!(!marked(root), "{at}");
        assert!(!tiny(root).join("DRAINING").exists(), "{at}");
    }
}

#[test]
fn a_removal_that_finds_no_pointer_leaves_a_folder_a_run_still_holds_and_deletes_nothing_when_its_sync_is_refused()
 {
    // No pointer, and a folder that a run holds: as after a held repair that could
    // not put its pointer back. The removal says that the model is not installed,
    // tries the hold file once, and leaves the folder.
    let scratch = Scratch::new("remove-no-pointer-held");
    let root = scratch.path();
    let record = installed(root, &second());
    let foreign = RealFs.try_share(&hold_file_of(root, &record)).unwrap();
    std::fs::remove_file(tiny(root).join("CURRENT")).unwrap();
    assert_eq!(removed(root), Err(RemoveRefused::NotInstalled));
    is_whole(root, &record);
    drop(foreign);
    // The sync of the model's folder is refused: which pointer is durable is not
    // known, and nothing is deleted.
    let fs = Arc::new(ShimFs::new());
    let model = tiny(root);
    fs.refuse_matching(0, move |event| *event == Event::SyncDir(model.clone()));
    let refused = removing(fs, root, &Hand::new(), SECOND, &mut never);
    assert!(
        matches!(refused, Err(RemoveRefused::Disk(_))),
        "{refused:?}"
    );
    is_whole(root, &record);
    // Free, and synced: deleted.
    assert_eq!(removed(root), Err(RemoveRefused::NotInstalled));
    assert!(installations(root).is_empty());
}

#[test]
fn a_second_removal_that_is_busy_leaves_the_mark_of_the_one_that_has_the_lock_and_no_hold_is_given_meanwhile()
 {
    // Removal A has the model's lock and waits for a hold. Removal B of the same
    // model, with patience or with none, ends busy: it never had the model's lock,
    // and the mark it found is not its own to take off. A hold asked for before, in
    // between and after is refused as draining, throughout A's wait.
    let scratch = Scratch::new("remove-two");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    let held = hold(&holds, &record).unwrap();
    let barriers = Barriers::new();
    let clock = Arc::new(Hand::new());
    let a = removal_on_a_thread(root, &barriers, &clock);
    barriers.arrived("try", 1);
    let draining = || {
        assert!(marked(root));
        assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
    };
    draining();
    let b = removing(
        Arc::new(RealFs),
        root,
        &Hand::new(),
        Duration::ZERO,
        &mut never,
    );
    assert_eq!(b, Err(RemoveRefused::Busy));
    draining();
    let patient = Hand::new();
    let mut pauses = 0;
    let b = removing(Arc::new(RealFs), root, &patient, 2 * SECOND, &mut || {
        pauses += 1;
        draining();
        patient.advance(SECOND);
    });
    assert_eq!(b, Err(RemoveRefused::Busy));
    assert_eq!(pauses, 2);
    draining();
    assert_eq!(said(root), Some((State::Removing, 0, 0)));
    held.let_go();
    barriers.open("try");
    assert!(a.join().unwrap().is_ok());
    assert!(!marked(root));
    assert_eq!(reading(root), Current::NotInstalled);
}

#[test]
fn a_removal_that_waits_for_a_hold_sets_its_mark_again_when_it_is_gone() {
    // A removal has the model's lock and waits for a hold. Its mark is taken off
    // under it, as a second removal that made the mark first and then ended busy
    // takes its own off. At its next try the removal sets the mark again.
    let scratch = Scratch::new("remove-mark-in-wait");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    let held = hold(&holds, &record).unwrap();
    let barriers = Barriers::new();
    let clock = Arc::new(Hand::new());
    let remover = removal_on_a_thread(root, &barriers, &clock);
    barriers.arrived("try", 1);
    assert_eq!(said(root), Some((State::Removing, 0, 0)));
    std::fs::remove_file(tiny(root).join("DRAINING")).unwrap();
    barriers.release("try");
    barriers.arrived("try", 2);
    assert!(marked(root));
    assert_eq!(hold(&holds, &record).unwrap_err(), HoldRefused::Draining);
    held.let_go();
    barriers.open("try");
    assert!(remover.join().unwrap().is_ok());
    assert!(!marked(root));
}

#[test]
fn a_removal_that_made_the_mark_and_never_had_the_lock_takes_its_own_mark_off_and_no_other() {
    // Busy behind an install, with no mark there before: the mark is the
    // removal's own, and it takes it off, so the install is not stopped by a mark
    // nobody stands behind. (Held by `remove::a_removal_of_a_model_that_is_
    // installing_…` too, whose install goes on.) Busy with a mark already there, as
    // a dead remover leaves one: it is left as it was found.
    let scratch = Scratch::new("remove-own-mark");
    let root = scratch.path();
    installed(root, &second());
    let lock = RealFs
        .hold_lock_file(&tiny(root).join("LOCK"), false)
        .unwrap();
    assert!(lock.is_some());
    let busy = || {
        removing(
            Arc::new(RealFs),
            root,
            &Hand::new(),
            Duration::ZERO,
            &mut never,
        )
    };
    assert_eq!(busy(), Err(RemoveRefused::Busy));
    assert!(!tiny(root).join("DRAINING").exists());
    std::fs::write(tiny(root).join("DRAINING"), b"").unwrap();
    assert_eq!(busy(), Err(RemoveRefused::Busy));
    assert_eq!(std::fs::read(tiny(root).join("DRAINING")).unwrap(), b"");
    drop(lock);
    // With the lock, a removal makes whatever is there its own mark, and takes it
    // off at its end.
    assert!(removed(root).is_ok());
    assert!(!tiny(root).join("DRAINING").exists());
}

#[test]
fn a_removal_waits_for_a_hold_on_the_installation_the_pointer_names_though_its_folder_is_gone() {
    // The pointer names an installation whose folder is not on the disk: damage.
    // A hold is given on what the pointer names, so a run may hold it; the removal
    // waits for that hold file all the same.
    let scratch = Scratch::new("remove-no-folder");
    let root = scratch.path();
    let record = installed(root, &second());
    let holds = holds_over(root);
    let held = hold(&holds, &record).unwrap();
    std::fs::remove_dir_all(
        tiny(root)
            .join("installations")
            .join(record.identity().hex()),
    )
    .unwrap();
    let clock = Hand::new();
    let refused = removing(Arc::new(RealFs), root, &clock, SECOND, &mut || {
        clock.advance(SECOND);
    });
    assert_eq!(refused, Err(RemoveRefused::Held));
    assert_eq!(count(&holds, &record), 1);
    let mut pauses = 0;
    let removed = removing(Arc::new(RealFs), root, &Hand::new(), SECOND, &mut || {
        pauses += 1;
        held.let_go();
    });
    assert_eq!(removed.unwrap().installation(), Some(record.identity()));
    assert_eq!(pauses, 1);
    assert_eq!(reading(root), Current::NotInstalled);
}
