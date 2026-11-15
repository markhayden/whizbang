//! One way to a writer, with recovery before it (ADR 0015; plan 1B, task I4): the
//! entry point that opens a store to write, its three states, and what a recovery's
//! parts publish before anyone is handed the writer.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::contract::{
    Clock, NoWriter, Opening, RECOVERY_WHY_LIMIT, Recovery, RecoveryPart,
};
use whizbang_core::store::fs::Fs;
use whizbang_core::store::{self, Reader, Store};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::fs_shim::{Event, ShimFs};
use whizbang_testkit::recover::FakeRecovery;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;

// --- what the tests are made of ----------------------------------------------------------------

struct Stopped(u64);

impl Clock for Stopped {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

/// The runs a process left: each is made, with one name, and never ended.
const LEFT: [&str; 2] = ["1000-1", "1000-2"];

struct World {
    dir: Scratch,
}

/// Each run of a store, with its names.
type RunNames = Vec<(String, Vec<String>)>;

impl World {
    /// A store that holds nothing.
    fn empty() -> World {
        World {
            dir: Scratch::new("recover"),
        }
    }

    /// A store that a process left with two runs made and not ended.
    fn left() -> World {
        let world = World::empty();
        let store = Store::open(&world.root(), Arc::new(ShimFs::new())).unwrap();
        for run in LEFT {
            let mut publication = store.begin().unwrap();
            publication.create_run(run).unwrap();
            let bytes = format!("the admission of {run}\n");
            let kind = store::Kind::Diagnostic;
            publication
                .put_in(run, "admitted", kind, bytes.as_bytes())
                .unwrap();
            publication.publish().unwrap();
        }
        world
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("store")
    }

    fn open(&self, fs: &Arc<ShimFs>, recovery: Recovery) -> Opening {
        Opening::open(&self.root(), fs.clone(), recovery).unwrap()
    }

    /// The number of the store's generation, its names outside runs, and each run's
    /// names, as a reader finds them.
    fn held(&self) -> (u64, Vec<String>, RunNames) {
        let fs: Arc<dyn Fs> = Arc::new(ShimFs::new());
        let reader = Reader::open(&self.root(), fs).unwrap();
        let generation = reader.snapshot().unwrap();
        let runs = generation.runs().iter();
        (
            generation.number(),
            generation.entries().keys().cloned().collect(),
            runs.map(|(id, run)| (id.clone(), run.entries().keys().cloned().collect()))
                .collect(),
        )
    }
}

/// A part that marks every run the store holds, and makes one name outside runs.
fn marking(name: &'static str) -> Arc<FakeRecovery> {
    let part = FakeRecovery::new(name);
    part.make(&format!("recovered/{name}"), b"once\n");
    part.make_in_runs(&format!("marked-by-{name}"), b"left\n");
    Arc::new(part)
}

fn with(parts: &[&Arc<FakeRecovery>]) -> Recovery {
    let mut recovery = Recovery::none().with_clock(Arc::new(Stopped(9000)));
    for part in parts {
        let part: Arc<dyn RecoveryPart> = (*part).clone();
        recovery = recovery.with_part(part);
    }
    recovery
}

/// What a store that two parts recovered holds: each part's name outside runs, and
/// each part's mark in each run.
fn recovered_by_two() -> (Vec<String>, RunNames) {
    let marks = ["admitted", "marked-by-one", "marked-by-two"];
    let marks: Vec<String> = marks.iter().map(|name| (*name).to_owned()).collect();
    (
        vec!["recovered/one".to_owned(), "recovered/two".to_owned()],
        LEFT.iter()
            .map(|run| ((*run).to_owned(), marks.clone()))
            .collect(),
    )
}

fn not_recovered() -> (Vec<String>, RunNames) {
    let admitted = vec!["admitted".to_owned()];
    (
        Vec::new(),
        LEFT.iter()
            .map(|run| ((*run).to_owned(), admitted.clone()))
            .collect(),
    )
}

/// Whether the events hold a step of a publication: something written or renamed.
fn published_something(events: &[Event]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, Event::Write(_) | Event::Rename(..)))
}

// --- with no part ------------------------------------------------------------------------------

#[test]
fn with_no_part_recovery_publishes_nothing_and_the_store_opens_as_before() {
    // What opening a store by itself does to the disk.
    let alone = World::empty();
    let fs = Arc::new(ShimFs::new());
    drop(Store::open(&alone.root(), fs.clone()).unwrap());
    let opened_alone = fs.events().len();

    let world = World::empty();
    let fs = Arc::new(ShimFs::new());
    let opening = world.open(&fs, Recovery::none());
    assert!(opening.created());
    assert_eq!(fs.events().len(), opened_alone, "nothing more at the open");
    let store = opening.writer().unwrap();
    // One thing more than before: the generation it found is confirmed, by a sync of
    // the root. Nothing is published.
    let events = fs.events();
    assert_eq!(
        events[opened_alone..],
        [Event::SyncDir(world.root())],
        "{events:?}"
    );
    assert_eq!(store.snapshot().unwrap().number(), 0);
    assert_eq!(world.held(), (0, vec![], vec![]));
    // The writer is the store: it publishes.
    let mut publication = store.begin().unwrap();
    publication.put("a-name", b"content").unwrap();
    publication.publish().unwrap();
    assert_eq!(world.held().0, 1);
    // Asked again, the same writer is handed out, and nothing is done again.
    let before = fs.events().len();
    assert!(Arc::ptr_eq(&store, &opening.writer().unwrap()));
    assert!(Arc::ptr_eq(&store, &opening.clone().writer().unwrap()));
    assert_eq!(fs.events().len(), before);
    // The lock is the opening's: while it lives the store is held, and once it and
    // the writer are gone the store opens again, and was made by neither open.
    let again = Opening::open(&world.root(), Arc::new(ShimFs::new()), Recovery::none());
    assert!(matches!(again, Err(store::Error::Held { .. })));
    drop((store, opening));
    let again = world.open(&Arc::new(ShimFs::new()), Recovery::none());
    assert!(!again.created());
    assert_eq!(again.writer().unwrap().snapshot().unwrap().number(), 1);
}

#[test]
fn each_way_of_opening_a_store_is_the_stores_own() {
    let world = World::empty();
    let fs: Arc<dyn Fs> = Arc::new(ShimFs::new());
    // A store that must be there is not made.
    let missing = Opening::open_existing(&world.root(), fs.clone(), Recovery::none());
    assert!(matches!(missing, Err(store::Error::Missing { .. })));
    // Only the root is made where its parent must exist.
    let deep = world.root().join("a").join("b");
    let no_parent = Opening::open_in_existing_parent(&deep, fs.clone(), Recovery::none());
    assert!(matches!(no_parent, Err(store::Error::NoParent { .. })));
    let made = Opening::open_in_existing_parent(&world.root(), fs.clone(), Recovery::none());
    assert!(made.unwrap().created());
    let found = Opening::open_existing(&world.root(), fs.clone(), Recovery::none()).unwrap();
    assert!(!found.created());
    assert_eq!(format!("{found:?}"), "Opening(\"not asked\")");
    found.writer().unwrap();
    assert_eq!(format!("{found:?}"), "Opening(\"ready\")");
}

// --- a part's records --------------------------------------------------------------------------

#[test]
fn a_fake_parts_records_are_published_before_the_writer_is_handed_out() {
    let world = World::left();
    let (one, two) = (marking("one"), marking("two"));
    let fs = Arc::new(ShimFs::new());
    let opening = world.open(&fs, with(&[&one, &two]));
    // Nothing is recovered until somebody asks for the writer.
    assert_eq!((one.asked(), two.asked()), (0, 0));
    assert_eq!(world.held(), (2, not_recovered().0, not_recovered().1));
    // Whoever is handed the writer finds the recovery published: the generation the
    // writer reads first is the one that holds every part's records.
    let store = opening.writer().unwrap();
    let generation = store.snapshot().unwrap();
    assert_eq!(generation.number(), 3);
    let (names, runs) = recovered_by_two();
    assert_eq!(world.held(), (3, names, runs));
    let entry = generation.get("recovered/one").unwrap();
    assert_eq!(entry.kind, store::Kind::Diagnostic);
    assert_eq!(store.read(entry).unwrap(), b"once\n");
    // Each part was asked once, in the order given, and shown the generation the
    // store was found at, the time of the recovery, and that no run is open here.
    for part in [&one, &two] {
        let [shown] = part.shown().try_into().unwrap();
        assert_eq!((shown.generation, shown.now_ms, shown.made), (2, 9000, 3));
        let runs: Vec<(String, bool)> = LEFT.iter().map(|run| ((*run).to_owned(), false)).collect();
        assert_eq!(shown.runs, runs);
    }
    // One publication, for both parts: one generation more, and one pointer.
    let pointers = fs
        .events()
        .into_iter()
        .filter(|event| matches!(event, Event::Rename(_, to) if to.ends_with("CURRENT")));
    assert_eq!(pointers.count(), 1);
    // Asked again, nothing is recovered again.
    opening.writer().unwrap();
    opening.remover().unwrap();
    assert_eq!((one.asked(), two.asked()), (1, 1));
    assert_eq!(world.held().0, 3);
}

#[test]
fn recovery_run_twice_changes_nothing_the_second_time() {
    let world = World::left();
    let (one, two) = (marking("one"), marking("two"));
    let first = world.open(&Arc::new(ShimFs::new()), with(&[&one, &two]));
    first.writer().unwrap();
    drop(first);
    let after_one = world.held();
    assert_eq!(after_one.0, 3);
    // The next open recovers again: every part is asked, finds every name it would
    // make there, and adds nothing. Nothing is published.
    let fs = Arc::new(ShimFs::new());
    let second = world.open(&fs, with(&[&one, &two]));
    let opened = fs.events().len();
    second.writer().unwrap();
    assert_eq!((one.asked(), two.asked()), (2, 2));
    assert_eq!((one.shown()[1].made, two.shown()[1].made), (0, 0));
    assert_eq!(one.shown()[1].generation, 3);
    assert_eq!(world.held(), after_one);
    assert!(!published_something(&fs.events()[opened..]));
}

#[test]
fn a_part_that_does_not_keep_to_a_name_made_once_fails_the_recovery() {
    // A name the store holds, outside runs and in a run; a run it does not hold; and
    // a name made twice in one recovery. Each fails the recovery, with nothing
    // published, and says which part it was.
    let blind = |run: Option<&str>, name: &str, twice: bool| {
        let part = FakeRecovery::new("blind");
        part.make_blindly(run, name, b"again\n");
        if twice {
            part.make_blindly(run, name, b"again\n");
        }
        Arc::new(part)
    };
    let cases = [
        (
            blind(None, "recovered/one", false),
            "the name \"recovered/one\", which is there",
        ),
        (
            blind(Some(LEFT[0]), "admitted", false),
            "\"admitted\", which is there",
        ),
        (
            blind(Some("1000-9"), "mark", false),
            "the run \"1000-9\", which the store does not hold",
        ),
        (blind(None, "new", true), "the name \"new\", which is there"),
        (blind(Some(LEFT[0]), "new", true), "\"new\", which is there"),
    ];
    for (part, says) in cases {
        let world = World::left();
        let first = world.open(&Arc::new(ShimFs::new()), with(&[&marking("one")]));
        first.writer().unwrap();
        drop(first);
        let before = world.held();
        let opening = world.open(&Arc::new(ShimFs::new()), with(&[&part]));
        let Err(NoWriter::NothingPublished { why }) = opening.writer() else {
            panic!("{says}: the recovery did not fail");
        };
        assert!(
            why.starts_with("its part \"blind\" failed: the part \"blind\" was to make "),
            "{why}"
        );
        assert!(why.contains(says), "{why}");
        assert_eq!(world.held(), before, "{says}");
    }
}

// --- all of the parts' records, or none --------------------------------------------------------

/// How many events an open of the left store takes, how many more its recovery by
/// two parts, and which of those is the last sync of the recovery's publication: the
/// sync of the root after the pointer was replaced. One event follows it, the removal
/// of the publication's staging, which changes no outcome.
fn events_of_a_recovery() -> (usize, usize, usize) {
    let world = World::left();
    let fs = Arc::new(ShimFs::new());
    let opening = world.open(&fs, with(&[&marking("one"), &marking("two")]));
    let opened = fs.events().len();
    opening.writer().unwrap();
    let events = fs.events()[opened..].to_vec();
    let root = Event::SyncDir(world.root());
    assert_eq!(events[0], root, "the confirming sync comes first");
    let last_sync = events.iter().rposition(|event| *event == root).unwrap();
    assert_eq!(last_sync, events.len() - 2, "{events:?}");
    (opened, events.len(), last_sync)
}

#[test]
fn all_of_the_parts_records_are_published_or_none_shown_by_refusing_each_call_of_the_publication() {
    let (opened, steps, last_sync) = events_of_a_recovery();
    // The confirming sync, then the publication: content, the generation, the pointer.
    assert!(steps > 8, "{steps}");
    let (mut none, mut all) = (0, 0);
    for step in 0..steps {
        let world = World::left();
        let (one, two) = (marking("one"), marking("two"));
        let fs = Arc::new(ShimFs::new());
        fs.refuse_before(opened + step);
        let opening = world.open(&fs, with(&[&one, &two]));
        let answered = opening.writer().map(drop);
        let (number, names, runs) = world.held();
        let root = world.root().display().to_string();
        match &answered {
            // Before its publication was visible: none of either part's records.
            // Nobody is handed a writer, for removing either, and the failure is
            // said without a path.
            Err(refused @ NoWriter::NothingPublished { .. }) => {
                assert!(step < last_sync, "step {step}");
                assert_eq!(
                    (number, names, runs),
                    (2, not_recovered().0, not_recovered().1)
                );
                assert_eq!(opening.remover().err().as_ref(), Some(refused));
                assert!(refused.to_string().contains("nothing was changed"));
                assert!(!refused.to_string().contains(&root), "{refused}");
                assert!(!format!("{refused:?}").contains(&root), "step {step}");
                none += 1;
            }
            // The last sync alone: all of both parts' records, and no writer.
            Err(refused @ NoWriter::NotConfirmed { generation: 3 }) => {
                assert_eq!(step, last_sync, "only the last sync leaves it visible");
                assert_eq!(
                    (number, names, runs),
                    (3, recovered_by_two().0, recovered_by_two().1)
                );
                assert_eq!(opening.remover().err().as_ref(), Some(refused));
                all += 1;
            }
            // After it nothing is left but the removal of the publication's
            // staging, which the next open does: the recovery is published.
            Ok(()) => {
                assert!(step > last_sync, "step {step}");
                assert_eq!(
                    (number, names, runs),
                    (3, recovered_by_two().0, recovered_by_two().1)
                );
                all += 1;
            }
            other => panic!("step {step}: {other:?}"),
        }
        drop(opening);
        // The lock was let go: the next open recovers, and the store then holds all
        // of both parts' records, once.
        let next = world.open(&Arc::new(ShimFs::new()), with(&[&one, &two]));
        next.writer().unwrap();
        assert_eq!(
            world.held(),
            (3, recovered_by_two().0, recovered_by_two().1),
            "step {step}"
        );
    }
    assert_eq!((none, all), (last_sync, steps - last_sync));
}

#[test]
fn a_recovery_whose_last_sync_fails_leaves_no_writer_and_the_next_open_confirms_what_is_there_and_publishes_nothing_more()
 {
    let (opened, _, last_sync) = events_of_a_recovery();
    let world = World::left();
    let (one, two) = (marking("one"), marking("two"));
    let fs = Arc::new(ShimFs::new());
    fs.refuse_before(opened + last_sync);
    let opening = world.open(&fs, with(&[&one, &two]));
    let refused = opening.writer().err().unwrap();
    assert_eq!(refused, NoWriter::NotConfirmed { generation: 3 });
    assert_eq!(
        refused.to_string(),
        "the store is open to read only: its recovery was published, as generation 3, and \
         was not confirmed durable; the next open to write confirms it"
    );
    // The whole generation is there, and a reader reads it.
    let reader = opening.reader().unwrap();
    assert_eq!(reader.snapshot().unwrap().number(), 3);
    // The next open confirms the generation it finds, by a sync of the root, before
    // any part is asked; every part then finds its names made and adds nothing, so
    // nothing more is published and nothing is made twice.
    let fs = Arc::new(ShimFs::new());
    let next = world.open(&fs, with(&[&one, &two]));
    let opened = fs.events().len();
    let asked_at = Arc::new(Mutex::new(None));
    one.when_asked({
        let (fs, asked_at) = (fs.clone(), asked_at.clone());
        move || *asked_at.lock().unwrap() = Some(fs.events().len())
    });
    let store = next.writer().unwrap();
    let events = fs.events();
    assert_eq!(events[opened], Event::SyncDir(world.root()));
    // The sync was made before the part was asked.
    assert!(asked_at.lock().unwrap().unwrap() > opened);
    assert!(!published_something(&events[opened..]), "{events:?}");
    assert_eq!((one.shown()[1].made, two.shown()[1].made), (0, 0));
    assert_eq!(store.snapshot().unwrap().number(), 3);
    assert_eq!(
        world.held(),
        (3, recovered_by_two().0, recovered_by_two().1)
    );
}

#[test]
fn an_open_that_cannot_confirm_the_generation_it_finds_hands_out_no_writer_and_asks_no_part() {
    let (opened, _, _) = events_of_a_recovery();
    let world = World::left();
    let one = marking("one");
    let fs = Arc::new(ShimFs::new());
    fs.refuse_before(opened);
    let opening = world.open(&fs, with(&[&one]));
    let Err(NoWriter::NothingPublished { why }) = opening.writer() else {
        panic!("a writer was handed out");
    };
    assert!(
        why.starts_with("the generation the store was found at could not be confirmed durable"),
        "{why}"
    );
    assert_eq!(one.asked(), 0);
    assert_eq!(world.held(), (2, not_recovered().0, not_recovered().1));
}

// --- a recovery that fails ---------------------------------------------------------------------

#[test]
fn after_a_failed_recovery_nobody_is_handed_a_writer_the_reason_is_said_without_a_path_and_a_reader_works()
 {
    let world = World::left();
    let (one, two) = (marking("one"), marking("two"));
    let root = world.root().display().to_string();
    // The second part fails, after the first made its records and it made its own,
    // and says where the store is, which a part must not: what it says is kept as
    // it is, cut, and a failure of the store that a part passes on has no path.
    two.fail(&format!(
        "it could not read a record{}",
        "!".repeat(RECOVERY_WHY_LIMIT)
    ));
    let fs = Arc::new(ShimFs::new());
    let opening = world.open(&fs, with(&[&one, &two]));
    let refused = opening.writer().err().unwrap();
    let NoWriter::NothingPublished { why } = &refused else {
        panic!("{refused:?}");
    };
    assert!(
        why.starts_with("its part \"two\" failed: it could not read a record!"),
        "{why}"
    );
    assert_eq!(
        why.chars().count(),
        "its part \"two\" failed: ".len() + RECOVERY_WHY_LIMIT
    );
    assert!(
        refused.to_string().starts_with(
            "the store could not be recovered, so it is open to read only: recovery failed \
             before anything of it was published (its part \"two\" failed: "
        ),
        "{refused}"
    );
    assert!(!refused.to_string().contains(&root));
    // None of either part's records: the first part's went with the second's failure.
    assert_eq!(world.held(), (2, not_recovered().0, not_recovered().1));
    // Everyone who asks is answered alike, and nothing is recovered a second time.
    assert_eq!(opening.writer().err(), Some(refused.clone()));
    assert_eq!(opening.clone().remover().err(), Some(refused.clone()));
    assert_eq!((one.asked(), two.asked()), (1, 1));
    assert_eq!(format!("{opening:?}"), "Opening(\"failed\")");
    // A reader works, and reads the store as it was found.
    let reader = opening.reader().unwrap();
    let generation = reader.snapshot().unwrap();
    assert_eq!(generation.number(), 2);
    let entry = generation.run(LEFT[0]).unwrap().get("admitted").unwrap();
    assert_eq!(reader.read(entry).unwrap(), b"the admission of 1000-1\n");
    // The lock was let go, though the opening lives: another opens the store to
    // write, and, with the part put right, recovers it.
    two.allow();
    let next = world.open(&Arc::new(ShimFs::new()), with(&[&one, &two]));
    next.writer().unwrap();
    assert_eq!(
        world.held(),
        (3, recovered_by_two().0, recovered_by_two().1)
    );
}

#[test]
fn a_part_that_panics_is_a_failed_recovery() {
    let world = World::left();
    let (one, two) = (marking("one"), marking("two"));
    one.panic_with("the part went wrong at /a/path/of/its/own");
    let opening = world.open(&Arc::new(ShimFs::new()), with(&[&one, &two]));
    let refused = opening.writer().err().unwrap();
    // What the panic carried is not said: only which part it was.
    assert_eq!(
        refused,
        NoWriter::NothingPublished {
            why: "its part \"one\" panicked".to_owned()
        }
    );
    // The part after it was not asked, nothing was published, and the lock is let go.
    assert_eq!((one.asked(), two.asked()), (1, 0));
    assert_eq!(world.held(), (2, not_recovered().0, not_recovered().1));
    assert_eq!(opening.writer().err(), Some(refused));
    one.allow();
    let next = world.open(&Arc::new(ShimFs::new()), with(&[&one, &two]));
    next.writer().unwrap();
    assert_eq!(world.held().0, 3);
}

// --- the three states --------------------------------------------------------------------------

#[test]
fn two_executors_built_at_once_over_one_store_value_are_both_answered_after_one_recovery_and_a_third_built_while_it_runs_waits()
 {
    let world = World::left();
    let part = marking("one");
    let barriers = Barriers::new();
    part.stop_at(&barriers, "recovering");
    let opening = world.open(&Arc::new(ShimFs::new()), with(&[&part]));
    let answered = AtomicU64::new(0);
    // What an executor is built over is the writer, so building one is asking for it.
    let build = |opening: &Opening| {
        let store = opening.writer().unwrap();
        answered.fetch_add(1, Ordering::SeqCst);
        // Whoever is answered finds the recovery published.
        assert_eq!(store.snapshot().unwrap().number(), 3);
        store
    };
    threads::scope(|scope| {
        let first = scope.spawn(|| build(&opening));
        let second = scope.spawn(|| build(&opening));
        // One of them recovers, and is stopped inside its part; the other waits.
        barriers.arrived("recovering", 1);
        while opening.waiting() < 1 {
            std::thread::yield_now();
        }
        assert_eq!(format!("{opening:?}"), "Opening(\"initializing\")");
        // A third, built while the recovery runs, waits too.
        let third = scope.spawn(|| build(&opening.clone()));
        while opening.waiting() < 2 {
            std::thread::yield_now();
        }
        // Nobody was answered, and nothing is published yet.
        assert_eq!(answered.load(Ordering::SeqCst), 0);
        assert_eq!(world.held().0, 2);
        barriers.open("recovering");
        let stores = [first, second, third].map(|built| built.join().unwrap());
        // All three are handed the one writer, after one recovery.
        assert!(Arc::ptr_eq(&stores[0], &stores[1]) && Arc::ptr_eq(&stores[1], &stores[2]));
    });
    assert_eq!(answered.load(Ordering::SeqCst), 3);
    assert_eq!(part.asked(), 1);
    assert_eq!(barriers.arrivals("recovering"), 1);
    assert_eq!(opening.waiting(), 0);
}

#[test]
fn whoever_waits_for_a_recovery_that_fails_is_told_what_the_one_who_ran_it_is_told() {
    let world = World::left();
    let part = marking("one");
    let barriers = Barriers::new();
    part.stop_at(&barriers, "recovering");
    part.fail("it would not");
    let opening = world.open(&Arc::new(ShimFs::new()), with(&[&part]));
    threads::scope(|scope| {
        let first = scope.spawn(|| opening.writer().map(drop));
        barriers.arrived("recovering", 1);
        let second = scope.spawn(|| opening.remover().map(drop));
        while opening.waiting() < 1 {
            std::thread::yield_now();
        }
        barriers.open("recovering");
        let (first, second) = (first.join().unwrap(), second.join().unwrap());
        assert_eq!(first, second);
        assert!(
            matches!(first, Err(NoWriter::NothingPublished { .. })),
            "{first:?}"
        );
    });
    assert_eq!(part.asked(), 1);
}

// --- at the limit on a generation file ---------------------------------------------------------

#[test]
fn with_the_limit_set_low_a_recovery_that_cannot_be_published_leaves_deleting_and_pruning_working_and_an_admission_refused_and_after_a_deletion_the_next_open_recovers()
 {
    let world = World::left();
    let part = marking("one");
    // The generation file as it is, and room for one name more: less than the three
    // names this recovery adds, and enough for the two it adds once a run is deleted.
    let generation = world.root().join("generations").join("2.json");
    let limit = std::fs::metadata(generation).unwrap().len() + 200;
    let limited = || with(&[&part]).limiting_generations_to(limit);
    let opening = world.open(&Arc::new(ShimFs::new()), limited());
    // No writer for anything that adds names: an executor cannot be built over
    // this store, so nothing is admitted, and whoever asks is told why.
    let refused = opening.writer().err().unwrap();
    let NoWriter::AtTheLimit { bytes, limit: said } = refused else {
        panic!("{refused:?}");
    };
    assert!(bytes > limit && said == limit, "{bytes} {said} {limit}");
    assert_eq!(
        refused.to_string(),
        format!(
            "the store is open for deleting and pruning alone: its recovery could not be \
             published, because the generation file would be {bytes} bytes and one is at \
             most {limit}; delete runs and prune, and the next open recovers it"
        )
    );
    assert_eq!(opening.writer().err(), Some(refused.clone()));
    assert_eq!(
        format!("{opening:?}"),
        "Opening(\"for deleting and pruning alone\")"
    );
    // Nothing of the recovery was published.
    assert_eq!(world.held(), (2, not_recovered().0, not_recovered().1));
    // Deleting and pruning work, and make the file smaller.
    let remover = opening.remover().unwrap();
    assert_eq!(remover.snapshot().unwrap().number(), 2);
    remover.delete(&[LEFT[0]]).unwrap();
    let pruned = remover.prune().unwrap();
    assert!(!pruned.generations_removed.is_empty());
    assert_eq!(world.held().2.len(), 1);
    assert_eq!(part.asked(), 1);
    drop((remover, opening));
    // After the deletion made room, the next open recovers, under the same limit.
    let next = world.open(&Arc::new(ShimFs::new()), limited());
    let store = next.writer().unwrap();
    // The limit is the store's from then on: a publication over it is refused.
    let mut large = store.begin().unwrap();
    large.put("a-large-name", &[0; 16]).unwrap();
    for at in 0..8 {
        large.put(&format!("another-name-{at}"), &[at; 16]).unwrap();
    }
    let too_large = large.publish();
    assert!(
        matches!(too_large, Err(store::Error::GenerationTooLarge { .. })),
        "{too_large:?}"
    );
    let names = store.snapshot().unwrap();
    assert!(names.get("recovered/one").is_some());
    assert!(names.run(LEFT[1]).unwrap().get("marked-by-one").is_some());
    assert_eq!(part.asked(), 2);
}
