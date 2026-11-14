//! Cancelling a run (ADR 0008, "Cancellation"): a cancellation that arrives before a
//! run's commit step is accepted, and nothing is committed; one that arrives after
//! returns the completed outcome. What a cancel is answered for a run in every state,
//! the moments at which it could be answered wrongly, each stopped at with a hook, the
//! end of a cancelled run, a cancellation that reaches a capability, and one that
//! reaches an admission.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use whizbang_core::artifact::{self, Artifact, ArtifactType};
use whizbang_core::capability::{FaultKind, FillRequest, Picture, Support};
use whizbang_core::contract::record::{DIAGNOSTICS_PREFIX, JOURNAL_NAME, Status};
use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Backends, Call, CallError, Cancel, Cancellation,
    Capabilities, Capability, Clock, Context, Declined, Ending, Engine, Executor, Failure,
    Finished, InputPort, Kind, Ledger, NoBackends, Outcome, Produced, Refusal, Registry, Request,
    RequestId, Reservation, RunId, RunState, Step, StoreFailure, Value, Watched, read_journal,
};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::store::fs::{Fs, RealFs};
use whizbang_core::store::{self, Reader, Store};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::{FakeFiller, Paint, roster};
use whizbang_testkit::echo::Echo;
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs};
use whizbang_testkit::ledger::FakeLedger;
use whizbang_testkit::schema::validate;
use whizbang_testkit::scratch::Scratch;

use super::commit::{Scripted as Told, World, both, committed, engine_over, held, pair, the_mask};
use super::execute::Taker;
use super::record::schema;
use super::watch::{AfterTheLook, asked, busy, engine, engine_on, the_run, work, work_needing};

/// How long a wait for another thread may last before it is taken for a test that
/// cannot pass: a limit, not a pause.
const LIMIT: Duration = Duration::from_secs(20);

fn request(id: &str) -> Request {
    Request::new(Call::new("pair")).with_id(RequestId::parse(id).unwrap())
}

fn completed() -> RunState {
    RunState::Ended(Outcome::Completed {
        outputs: Vec::new(),
    })
}

fn cancelled() -> RunState {
    RunState::Ended(Outcome::Cancelled)
}

/// The ending recorded for a request id, when the store holds one.
fn ending(root: &Path, fs: Arc<dyn Fs>, id: &str) -> Option<Ending> {
    let reader = Reader::open(root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    let entry = generation.get(&RequestId::parse(id).unwrap().ending_name())?;
    Some(Ending::from_bytes(&reader.read(entry).unwrap()).unwrap())
}

/// `pair` that reports an event and writes a diagnostic, stops at the barrier `busy`,
/// notes whether its signal was set when it went on, and hands back its two artifacts
/// whatever the signal says: an action that ignores a cancellation.
fn stubborn(barriers: &Barriers, saw: &Arc<Mutex<Vec<bool>>>) -> Arc<dyn Action> {
    let (barriers, saw) = (barriers.clone(), saw.clone());
    pair(move |_, context| {
        context.events().progress("began");
        context.diagnostics().write("notes.txt", b"so far").unwrap();
        barriers.stop_at("busy");
        saw.lock()
            .unwrap()
            .push(context.cancellation().is_cancelled());
        Ok(both())
    })
}

/// `pair` that cancels its own run, with the signal it was handed, and then hands back
/// its two artifacts: a cancellation that is accepted, on one thread.
fn cancels_itself() -> Arc<dyn Action> {
    pair(|_, context| {
        context.diagnostics().write("notes.txt", b"so far").unwrap();
        context.cancellation().cancel();
        Ok(both())
    })
}

// --- the rule ---------------------------------------------------------------------------------

#[test]
fn a_cancel_before_the_commit_step_is_accepted_and_the_run_ends_cancelled_with_no_output() {
    let world = World::new();
    let store = world.store();
    let barriers = Barriers::new();
    let saw = Arc::new(Mutex::new(Vec::new()));
    let ledger = Arc::new(FakeLedger::new());
    let first =
        engine_over(store.clone(), vec![stubborn(&barriers, &saw)]).with_ledger(ledger.clone());
    // Whoever cancels has an executor of its own over the same store.
    let second = engine_over(store.clone(), Vec::new());
    let run = whizbang_testkit::threads::scope(|scope| {
        let running = scope.spawn(|| first.request(&request("r-1")));
        barriers.arrived("busy", 1);
        let run = the_run(&world.root());
        assert_eq!(
            second.status(&run).unwrap().state(),
            &RunState::NotEnded { here: true }
        );

        // The action is inside its work: the cancellation is accepted, and is answered
        // at once. The run has not ended: its action has not come back.
        assert_eq!(second.cancel(&run), Ok(Cancel::Accepted));
        let watched = second.status(&run).unwrap();
        assert_eq!(watched.state(), &RunState::NotEnded { here: true });
        assert_eq!(watched.events()[0].message(), "began");
        let waiting = {
            let (second, run) = (&second, run.clone());
            scope.spawn(move || second.wait(&run))
        };

        // The action goes on, sees the signal, and hands back two artifacts all the
        // same. Nothing of them is committed, and the run ends as cancelled.
        barriers.release("busy");
        let Ok(Answer::Ran(finished)) = running.join().unwrap() else {
            panic!("the request did not run");
        };
        assert_eq!(finished.run(), &run);
        assert_eq!(finished.outcome(), &Outcome::Cancelled);
        assert_eq!(waiting.join().unwrap().unwrap().state(), &cancelled());
        run
    });
    assert_eq!(*saw.lock().unwrap(), [true]);

    // One publication admitted the run and one ended it. Neither output is in the
    // store, under its artifact's name or among the run's names.
    let after = held(&world.root(), world.fs.clone());
    assert_eq!(after.generation, 2);
    assert!(!after.mask && !after.place);
    assert!(after.outputs.is_empty());
    let (id, stored, names) = &after.runs[0];
    assert_eq!(id, run.as_str());
    let stored = stored.as_ref().unwrap();
    assert_eq!(stored.outcome().status, Status::Cancelled);
    assert!(stored.outcome().error.is_none() && stored.outcome().invalid.is_none());
    assert!(stored.outputs().is_empty());
    let document: serde_json::Value = serde_json::from_slice(&stored.to_bytes()).unwrap();
    validate(&schema(), &document).unwrap();

    // The journal and the diagnostic are kept, as for any run: with the admission and
    // the record, the run holds four names.
    assert_eq!(*names, 4);
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    let of_run = generation.run(run.as_str()).unwrap();
    let journal = reader.read(of_run.get(JOURNAL_NAME).unwrap()).unwrap();
    assert_eq!(read_journal(&journal).unwrap()[0].message(), "began");
    let notes = of_run.get(&format!("{DIAGNOSTICS_PREFIX}notes.txt"));
    assert_eq!(reader.read(notes.unwrap()).unwrap(), b"so far");

    // The id's ending says cancelled too, and the reservation stays.
    let ended = ending(&world.root(), world.fs.clone(), "r-1").unwrap();
    assert_eq!((ended.run(), ended.status()), (&run, Status::Cancelled));
    let reserved = ledger.reservations(store.as_ref()).unwrap();
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].run(), &run);

    // Asked again, by its request's id or by its own, the run is one that was
    // cancelled; nothing runs, and nothing is published.
    let Ok(Answer::Repeat(earlier)) = first.request(&request("r-1")) else {
        panic!("not answered as a repeat");
    };
    assert_eq!(earlier.state(), &cancelled());
    assert_eq!(barriers.arrivals("busy"), 1);
    assert_eq!(second.status(&run).unwrap().state(), &cancelled());
    assert_eq!(first.wait(&run).unwrap().state(), &cancelled());
    assert_eq!(second.cancel(&run), Ok(Cancel::NotAccepted(cancelled())));
    assert_eq!(store.snapshot().unwrap().number(), 2);
}

#[test]
fn a_cancel_after_the_commit_step_began_waits_and_returns_the_completed_outcome() {
    // Stopped before the first call of the commit that changes the disk: the run has
    // entered its commit step and nothing of it is visible. A cancel that arrives now
    // is too late. It waits for the commit, and is answered with the completed
    // outcome; and while the commit and the cancel are both in progress, a third
    // caller reads the status of this run and of another.
    let world = World::new();
    let store = world.store();
    let asking = engine_over(store.clone(), Vec::new());
    let seen = Arc::new(Mutex::new(None));
    let committing = {
        let (fs, root, seen, asking) =
            (world.fs.clone(), world.root(), seen.clone(), asking.clone());
        pair(move |_, _| {
            let (root, seen, asking) = (root.clone(), seen.clone(), asking.clone());
            fs.pause_before(fs.events().len(), move || {
                let run = the_run(&root);
                let cancelling = {
                    let (asking, run) = (asking.clone(), run.clone());
                    whizbang_testkit::threads::spawn(move || asking.cancel(&run))
                };
                let limit = Instant::now() + LIMIT;
                while asking.waiting_for(&run) == 0 && !cancelling.is_finished() {
                    assert!(
                        Instant::now() < limit,
                        "the cancel neither waits nor answers"
                    );
                    std::thread::yield_now();
                }
                let waits = !cancelling.is_finished();
                let status = {
                    let (asking, run) = (asking.clone(), run.clone());
                    whizbang_testkit::threads::spawn(move || asking.status(&run))
                };
                let status = status.join().unwrap();
                *seen.lock().unwrap() = Some((waits, status, cancelling));
            });
            Ok(both())
        })
    };
    let executor = engine_over(store.clone(), vec![committing]);
    let finished = executor.submit(&Call::new("pair")).unwrap();
    assert_eq!(finished.outcome().outputs(), committed());
    let (waits, status, cancelling) = seen.lock().unwrap().take().expect("the hook ran");
    assert!(
        waits,
        "a cancel in the commit step was answered before the commit"
    );
    assert_eq!(
        status.unwrap().state(),
        &RunState::NotEnded { here: true },
        "a status asked while the run commits and a cancel waits"
    );
    assert_eq!(
        cancelling.join().unwrap(),
        Ok(Cancel::NotAccepted(RunState::Ended(Outcome::Completed {
            outputs: committed()
        })))
    );
    let after = held(&world.root(), world.fs.clone());
    assert!(after.mask && after.place);
    assert_eq!(after.outputs.len(), 2);
    assert_eq!(
        after.runs[0].1.as_ref().unwrap().outcome().status,
        Status::Completed
    );
}

#[test]
fn a_status_of_another_run_is_answered_while_a_cancel_waits_for_a_commit() {
    // As above, with a second run that is executing: its status is read, and it is
    // cancelled, by another thread, while the first run's commit is stopped and a
    // cancel of the first run waits for it. Nothing either holds is needed for the
    // other.
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let barriers = Barriers::new();
    let other = engine(store.clone(), vec![busy(&barriers, "nothing")]);
    let Ok(Answer::Started(executing)) = other.start(&asked("order-1")) else {
        panic!("the request was not started");
    };
    barriers.arrived("busy", 1);

    let seen = Arc::new(Mutex::new(None));
    let asking = engine(store.clone(), Vec::new());
    let ending = {
        let (fs, root, seen, executing) =
            (fs.clone(), root.clone(), seen.clone(), executing.clone());
        work(move |_, _| {
            let (root, seen, executing, asking) = (
                root.clone(),
                seen.clone(),
                executing.clone(),
                asking.clone(),
            );
            fs.pause_before(fs.events().len(), move || {
                let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
                let generation = reader.snapshot().unwrap();
                let this = generation
                    .runs()
                    .keys()
                    .find(|id| **id != executing.as_str());
                let this = RunId::parse(this.unwrap()).unwrap();
                let cancelling = {
                    let (asking, this) = (asking.clone(), this.clone());
                    whizbang_testkit::threads::spawn(move || asking.cancel(&this))
                };
                let limit = Instant::now() + LIMIT;
                while asking.waiting_for(&this) == 0 {
                    assert!(Instant::now() < limit, "the cancel does not wait");
                    std::thread::yield_now();
                }
                let of_other = {
                    let (asking, executing) = (asking.clone(), executing.clone());
                    whizbang_testkit::threads::spawn(move || {
                        (asking.status(&executing), asking.cancel(&executing))
                    })
                };
                *seen.lock().unwrap() = Some((of_other.join().unwrap(), cancelling));
            });
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![ending]);
    let finished = executor.submit(&Call::new("work")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let ((status, cancel), cancelling) = seen.lock().unwrap().take().expect("the hook ran");
    assert_eq!(status.unwrap().state(), &RunState::NotEnded { here: true });
    assert_eq!(cancel, Ok(Cancel::Accepted));
    assert_eq!(
        cancelling.join().unwrap(),
        Ok(Cancel::NotAccepted(completed()))
    );
    barriers.release("busy");
    assert_eq!(other.wait(&executing).unwrap().state(), &cancelled());
}

#[test]
fn two_cancels_at_once_are_both_accepted_and_the_run_ends_once() {
    let world = World::new();
    let store = world.store();
    let barriers = Barriers::new();
    let saw = Arc::new(Mutex::new(Vec::new()));
    let executor = engine_over(store.clone(), vec![stubborn(&barriers, &saw)]);
    whizbang_testkit::threads::scope(|scope| {
        let running = scope.spawn(|| executor.request(&request("r-2")));
        barriers.arrived("busy", 1);
        let run = the_run(&world.root());
        let together = Arc::new(std::sync::Barrier::new(2));
        let cancels: Vec<_> = (0..2)
            .map(|_| {
                let (executor, run, together) = (&executor, run.clone(), together.clone());
                scope.spawn(move || {
                    together.wait();
                    executor.cancel(&run)
                })
            })
            .collect();
        for cancel in cancels {
            assert_eq!(cancel.join().unwrap(), Ok(Cancel::Accepted));
        }
        barriers.release("busy");
        let Ok(Answer::Ran(finished)) = running.join().unwrap() else {
            panic!("the request did not run");
        };
        assert_eq!(finished.outcome(), &Outcome::Cancelled);
    });
    let after = held(&world.root(), world.fs.clone());
    assert_eq!(after.generation, 2);
    assert_eq!(after.runs.len(), 1);
}

#[test]
fn whatever_an_action_does_after_an_accepted_cancel_its_run_ends_cancelled() {
    // The cancellation was accepted, so the run ends as cancelled: whether its action
    // then hands back artifacts, fails, finds its input invalid or panics.
    type After = fn() -> Result<Produced, Failure>;
    let cases: [(&str, After); 4] = [
        ("hands back", || Ok(both())),
        ("fails", || Err(Failure::error("pair", "stopped"))),
        ("invalid", || Err(Failure::invalid("no"))),
        ("panics", || panic!("after the cancel")),
    ];
    for (case, after) in cases {
        let world = World::new();
        let engine = world.engine(vec![pair(move |_, context| {
            context.cancellation().cancel();
            after()
        })]);
        let finished = engine.submit(&Call::new("pair")).unwrap();
        assert_eq!(finished.outcome(), &Outcome::Cancelled, "{case}");
        let held = held(&world.root(), world.fs.clone());
        assert!(
            !held.mask && !held.place && held.outputs.is_empty(),
            "{case}"
        );
        let stored = held.runs[0].1.as_ref().unwrap();
        assert_eq!(stored.outcome().status, Status::Cancelled, "{case}");
        assert!(stored.outcome().error.is_none(), "{case}");
    }
}

// --- every state ------------------------------------------------------------------------------

#[test]
fn a_cancel_is_answered_for_a_run_in_every_state_and_changes_none_that_is_over() {
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    // A run whose end the store refuses, as in the tests of a status.
    let stuck = {
        let fs = fs.clone();
        work(move |_, _| {
            fs.refuse_before(fs.events().len());
            Ok(Produced::nothing())
        })
    };
    let unended = {
        let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
        let executor = engine(store.clone(), vec![stuck]);

        // No such run: never made.
        let unknown = RunId::parse("9000-999999").unwrap();
        let refused = CallError::Refused(Refusal::NoSuchRun {
            run: unknown.clone(),
        });
        assert_eq!(executor.cancel(&unknown), Err(refused));

        // A run made by other code of this process: here, and not the executor's to
        // cancel.
        let mut publication = store.begin().unwrap();
        publication.create_run("by-hand").unwrap();
        publication.publish().unwrap();
        let by_hand = RunId::parse("by-hand").unwrap();
        let here = Cancel::NotAccepted(RunState::NotEnded { here: true });
        assert_eq!(executor.cancel(&by_hand), Ok(here));
        store.delete(&["by-hand"]).unwrap();

        // A run whose end could not be written: it was abandoned, and a cancel is
        // answered without waiting.
        let Err(CallError::Store(failure)) = executor.request(&asked("order-1")) else {
            panic!("the end was published");
        };
        let unended = the_run(&root);
        let failure = Some(failure);
        let abandoned = Cancel::NotAccepted(RunState::Abandoned { failure });
        assert_eq!(executor.cancel(&unended), Ok(abandoned));
        unended
    };

    // The process that admitted it is gone. Nobody will end the run, and nobody can
    // be told to stop.
    let store = Arc::new(Store::open(&root, Arc::new(ShimFs::new())).unwrap());
    let executor = engine(store.clone(), vec![work(|_, _| Ok(Produced::nothing()))]);
    let gone = Cancel::NotAccepted(RunState::NotEnded { here: false });
    assert_eq!(executor.cancel(&unended), Ok(gone.clone()));

    // Runs that ended, one of them with an error: the outcome, and nothing changes.
    let done = executor.submit(&Call::new("work")).unwrap();
    let before = store.snapshot().unwrap().number();
    assert_eq!(
        executor.cancel(done.run()),
        Ok(Cancel::NotAccepted(completed()))
    );
    assert_eq!(store.snapshot().unwrap().number(), before);
    assert_eq!(executor.status(done.run()).unwrap().state(), &completed());

    // An executor that only reads the store cancels nothing, and says what it reads.
    let reading = Engine::new(
        Registry::new(),
        Arc::new(NoBackends),
        Arc::new(Reader::open(&root, Arc::new(RealFs)).unwrap()),
        Digest::of_bytes(b""),
    );
    assert_eq!(reading.cancel(&unended), Ok(gone));
    assert_eq!(
        reading.cancel(done.run()),
        Ok(Cancel::NotAccepted(completed()))
    );

    // A run that was deleted is no run.
    store.delete(&[done.run().as_str()]).unwrap();
    for executor in [&executor, &reading] {
        assert_eq!(
            executor.cancel(done.run()),
            Err(CallError::Refused(Refusal::NoSuchRun {
                run: done.run().clone()
            }))
        );
    }
}

// --- a run with nothing behind it -------------------------------------------------------------

/// A clock that panics, once it is told to, the next time it is asked the time of day.
struct Breaks(AtomicU64);

impl Clock for Breaks {
    fn now_ms(&self) -> u64 {
        assert_eq!(self.0.load(Ordering::SeqCst), 0, "the clock broke");
        9000
    }
}

#[test]
fn a_run_with_nothing_behind_it_is_abandoned_for_whoever_waits_asks_or_cancels() {
    // Three ways a run is left open in this process with nothing that will end it.
    // Each is answered as abandoned, with the store's failure when there was one, by
    // a status, a wait, a cancel and a repeat alike; and a wait does not wait.
    let abandoned = |executor: &Engine, run: &RunId, failure: Option<StoreFailure>| {
        let state = RunState::Abandoned { failure };
        assert_eq!(executor.status(run).unwrap().state(), &state);
        assert_eq!(executor.wait(run).unwrap().state(), &state);
        assert_eq!(executor.cancel(run), Ok(Cancel::NotAccepted(state.clone())));
        let Ok(Answer::Repeat(earlier)) = executor.request(&asked("order-1")) else {
            panic!("not answered as a repeat");
        };
        assert_eq!(earlier.state(), &state);
    };

    // The last sync of the admission's publication is refused: the run is visible,
    // its caller was told of the store's failure, and its action never starts.
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let executor = engine(store, vec![work(|_, _| panic!("the action started"))]);
    fs.refuse_matching(0, {
        let root = root.clone();
        move |event| matches!(event, Event::SyncDir(dir) if *dir == root)
    });
    let Err(CallError::Store(failure)) = executor.request(&asked("order-1")) else {
        panic!("the admission was confirmed");
    };
    let said = CallError::Store(failure.clone()).to_string();
    assert!(said.contains("not confirmed durable"), "{said}");
    abandoned(&executor, &the_run(&root), Some(failure));

    // The end of the run is refused by the store.
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let stuck = {
        let fs = fs.clone();
        work(move |_, _| {
            fs.refuse_before(fs.events().len());
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store, vec![stuck]);
    let Err(CallError::Store(failure)) = executor.request(&asked("order-1")) else {
        panic!("the end was published");
    };
    abandoned(&executor, &the_run(&root), Some(failure));

    // The executor's own code panics after the action came back: its clock breaks.
    // The thread of the started run is gone, and no store failed.
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let store = Arc::new(Store::open(&root, Arc::new(ShimFs::new())).unwrap());
    let clock = Arc::new(Breaks(AtomicU64::new(0)));
    let breaking = {
        let clock = clock.clone();
        work(move |_, _| {
            clock.0.store(1, Ordering::SeqCst);
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store, vec![breaking]).with_clock(clock.clone());
    let Ok(Answer::Started(run)) = executor.start(&asked("order-1")) else {
        panic!("the request was not started");
    };
    assert_eq!(
        executor.wait(&run).unwrap().state(),
        &RunState::Abandoned { failure: None }
    );
    clock.0.store(0, Ordering::SeqCst);
    abandoned(&executor, &run, None);
}

#[test]
fn a_cancel_that_waits_for_a_commit_the_store_then_refuses_is_told_the_run_was_abandoned() {
    // The cancel arrives in the commit step and waits. The store then refuses the
    // commit: the run is neither cancelled nor completed, and the cancel says so.
    let world = World::new();
    let store = world.store();
    let asking = engine_over(store.clone(), Vec::new());
    let seen = Arc::new(Mutex::new(None));
    let committing = {
        let (fs, root, seen, asking) =
            (world.fs.clone(), world.root(), seen.clone(), asking.clone());
        pair(move |_, _| {
            let (hooked, root, seen, asking) =
                (fs.clone(), root.clone(), seen.clone(), asking.clone());
            fs.pause_before(fs.events().len(), move || {
                let run = the_run(&root);
                let cancelling = {
                    let (asking, run) = (asking.clone(), run.clone());
                    whizbang_testkit::threads::spawn(move || asking.cancel(&run))
                };
                let limit = Instant::now() + LIMIT;
                while asking.waiting_for(&run) == 0 {
                    assert!(Instant::now() < limit, "the cancel does not wait");
                    std::thread::yield_now();
                }
                *seen.lock().unwrap() = Some(cancelling);
                // The next call of the commit that changes the disk fails.
                hooked.refuse_before(hooked.events().len() + 1);
            });
            Ok(both())
        })
    };
    let executor = engine_over(store.clone(), vec![committing]);
    let Err(CallError::Store(failure)) = executor.submit(&Call::new("pair")) else {
        panic!("the commit was published");
    };
    let cancelling = seen.lock().unwrap().take().expect("the hook ran");
    let state = RunState::Abandoned {
        failure: Some(failure),
    };
    assert_eq!(
        cancelling.join().unwrap(),
        Ok(Cancel::NotAccepted(state.clone()))
    );
    let after = held(&world.root(), world.fs.clone());
    assert!(!after.mask && !after.place && after.runs[0].1.is_none());
    let run = the_run(&world.root());
    assert_eq!(executor.wait(&run).unwrap().state(), &state);
}

#[test]
fn a_run_that_was_named_again_is_in_the_table_of_open_runs_under_its_last_id_alone() {
    // The name of the run's reservation is taken three times as its admission is
    // about to be written, so the run is named four times. The ids that were given up
    // are no runs, here or in the store, and the run is cancelled by the id it has.
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let (fs, store) = Taker::store_taking_reservations(&root, 3);
    let barriers = Barriers::new();
    let executor = engine(store.clone(), vec![busy(&barriers, "nothing")])
        .with_ledger(Arc::new(FakeLedger::new()));
    let Ok(Answer::Started(run)) = executor.start(&asked("order-1")) else {
        panic!("the request was not started");
    };
    barriers.arrived("busy", 1);
    let given_up = fs.taken();
    assert_eq!(given_up.len(), 3);
    for id in &given_up {
        assert_ne!(id, run.as_str());
        assert!(!store.made_here(id));
        let id = RunId::parse(id).unwrap();
        assert_eq!(
            executor.cancel(&id),
            Err(CallError::Refused(Refusal::NoSuchRun { run: id.clone() }))
        );
    }
    assert!(store.made_here(run.as_str()));
    assert_eq!(executor.cancel(&run), Ok(Cancel::Accepted));
    barriers.release("busy");
    assert_eq!(executor.wait(&run).unwrap().state(), &cancelled());
    assert!(!store.made_here(run.as_str()));
}

// --- the edges --------------------------------------------------------------------------------

#[test]
fn a_cancel_asked_the_moment_a_run_becomes_visible_is_accepted_and_its_action_never_runs() {
    // Stopped straight after the rename that makes the admission visible: the run is
    // in the store, and in the table of open runs. A cancel is accepted, and the
    // action, which has not started, is not run at all.
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let ran = Arc::new(AtomicU64::new(0));
    let counting = {
        let ran = ran.clone();
        work(move |_, _| {
            ran.fetch_add(1, Ordering::SeqCst);
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![counting]);
    let seen = Arc::new(Mutex::new(None));
    fs.pause_after_rename_onto("CURRENT", {
        let (root, asking, seen) = (root.clone(), executor.clone(), seen.clone());
        move || *seen.lock().unwrap() = Some(asking.cancel(&the_run(&root)))
    });
    let finished = executor.submit(&Call::new("work")).unwrap();
    assert_eq!(seen.lock().unwrap().take(), Some(Ok(Cancel::Accepted)));
    assert_eq!(finished.outcome(), &Outcome::Cancelled);
    assert_eq!(ran.load(Ordering::SeqCst), 0);
    assert_eq!(
        executor.status(finished.run()).unwrap().state(),
        &cancelled()
    );
}

/// Stops the publishing thread straight after the rename that makes the end of `work`'s
/// run visible, before its last sync, and has a cancel and a wait of the run asked there
/// by two other threads. Once both are waiting for the run, `then` is done on the
/// publishing thread, which goes on. What comes back: the answer of whoever ran the
/// run, of the cancel, and of the wait.
type Asked = (
    Result<Finished, CallError>,
    Result<Cancel, CallError>,
    Result<Watched, CallError>,
);

fn asked_before_the_last_sync(
    root: &Path,
    fs: &Arc<ShimFs>,
    then: impl Fn(&ShimFs) + Send + Sync + 'static,
) -> Asked {
    let store = Arc::new(Store::open(root, fs.clone()).unwrap());
    let seen = Arc::new(Mutex::new(None));
    let asking = engine(store.clone(), Vec::new());
    let then = Arc::new(then);
    let arming = {
        let (fs, root, seen) = (fs.clone(), root.to_path_buf(), seen.clone());
        work(move |_, _| {
            let (hooked, root, asking, seen, then) = (
                fs.clone(),
                root.clone(),
                asking.clone(),
                seen.clone(),
                then.clone(),
            );
            fs.pause_after_rename_onto("CURRENT", move || {
                let run = the_run(&root);
                let cancelling = {
                    let (asking, run) = (asking.clone(), run.clone());
                    whizbang_testkit::threads::spawn(move || asking.cancel(&run))
                };
                let waiting = {
                    let (asking, run) = (asking.clone(), run.clone());
                    whizbang_testkit::threads::spawn(move || asking.wait(&run))
                };
                let limit = Instant::now() + LIMIT;
                while asking.waiting_for(&run) < 2
                    && !(cancelling.is_finished() || waiting.is_finished())
                {
                    assert!(Instant::now() < limit, "neither waits nor answers");
                    std::thread::yield_now();
                }
                let early = cancelling.is_finished() || waiting.is_finished();
                *seen.lock().unwrap() = Some((early, cancelling, waiting));
                then(&hooked);
            });
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store, vec![arming]);
    let ran = executor.submit(&Call::new("work"));
    let (early, cancelling, waiting) = seen.lock().unwrap().take().expect("the hook ran");
    let answers = (ran, cancelling.join().unwrap(), waiting.join().unwrap());
    assert!(!early, "answered before the end's last sync: {answers:?}");
    answers
}

#[test]
fn a_cancel_and_a_wait_asked_when_an_end_is_visible_wait_for_its_last_sync() {
    // The end is visible and not yet confirmed. A status says what the store says; a
    // cancel and a wait are for the executor, and are answered when it is done.
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let (ran, cancel, wait) = asked_before_the_last_sync(&root, &Arc::new(ShimFs::new()), |_| {});
    assert_eq!(ran.unwrap().outcome().name(), "completed");
    assert_eq!(cancel, Ok(Cancel::NotAccepted(completed())));
    assert_eq!(wait.unwrap().state(), &completed());
}

#[test]
fn what_a_cancel_and_a_wait_say_of_an_end_is_what_the_store_holds_after_a_loss_of_power() {
    // Power is lost between the rename that makes the end visible and its last sync,
    // with a cancel and a wait of the run in flight. Neither may say the run completed:
    // the store that is opened again holds the run as it was admitted, with no record.
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let (ran, cancel, wait) = asked_before_the_last_sync(&root, &fs, |fs| fs.crash(Lost::Unsynced));
    let Err(CallError::Store(failure)) = ran else {
        panic!("the run was reported: {ran:?}");
    };
    let state = RunState::Abandoned {
        failure: Some(failure),
    };
    assert_eq!(cancel, Ok(Cancel::NotAccepted(state.clone())));
    assert_eq!(wait.unwrap().state(), &state);

    let fs: Arc<dyn Fs> = Arc::new(ShimFs::new());
    drop(Store::open(&root, fs.clone()).unwrap());
    let reader = Reader::open(&root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    let run = generation
        .runs()
        .values()
        .next()
        .expect("the run was admitted, durably");
    assert_eq!(
        run.entries().len(),
        1,
        "the run holds more than its admission"
    );
}

#[test]
fn a_cancel_and_a_wait_begun_after_an_end_that_was_not_confirmed_say_abandoned() {
    // The end's last sync is refused: the record is visible, and nobody was told it
    // would survive a loss of power. The executor is done with the run. A cancel and a
    // wait that begin now, by any executor over the store, still say `Abandoned`, as
    // those that were waiting did; a status says what the store shows.
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let unconfirmed = {
        let (fs, root) = (fs.clone(), root.clone());
        work(move |_, _| {
            let root = root.clone();
            fs.refuse_matching(
                0,
                move |event| matches!(event, Event::SyncDir(dir) if *dir == root),
            );
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![unconfirmed]);
    let Err(CallError::Store(failure)) = executor.submit(&Call::new("work")) else {
        panic!("the end was confirmed");
    };
    let run = the_run(&root);
    let state = RunState::Abandoned {
        failure: Some(failure),
    };
    for asking in [executor.clone(), engine(store.clone(), Vec::new())] {
        assert_eq!(asking.cancel(&run), Ok(Cancel::NotAccepted(state.clone())));
        assert_eq!(asking.wait(&run).unwrap().state(), &state);
        assert_eq!(asking.status(&run).unwrap().state(), &completed());
    }
}

/// `work` whose end is stopped before its first call that changes the disk: there a
/// cancel or a wait of the run is begun, by `ask`, and is waiting; then `then` is done,
/// and the end goes on. What comes back is what the run's caller and the one who asked
/// were told.
fn asked_in_the_commit_step<T: Send + 'static>(
    root: &Path,
    fs: &Arc<ShimFs>,
    store: &Arc<Store>,
    ask: impl Fn(&Engine, &RunId) -> T + Send + Sync + 'static,
    then: impl Fn(&ShimFs, &RunId) + Send + Sync + 'static,
) -> (Result<Finished, CallError>, T, RunId) {
    let seen = Arc::new(Mutex::new(None));
    let asking = engine(store.clone(), Vec::new());
    let (ask, then) = (Arc::new(ask), Arc::new(then));
    let arming = {
        let (fs, root, seen) = (fs.clone(), root.to_path_buf(), seen.clone());
        work(move |_, _| {
            let (hooked, root, asking, seen, ask, then) = (
                fs.clone(),
                root.clone(),
                asking.clone(),
                seen.clone(),
                ask.clone(),
                then.clone(),
            );
            fs.pause_before(fs.events().len(), move || {
                let run = the_run(&root);
                let asked = {
                    let (asking, run) = (asking.clone(), run.clone());
                    whizbang_testkit::threads::spawn(move || ask(&asking, &run))
                };
                let limit = Instant::now() + LIMIT;
                while asking.waiting_for(&run) == 0 {
                    assert!(Instant::now() < limit, "it does not wait");
                    std::thread::yield_now();
                }
                then(&hooked, &run);
                *seen.lock().unwrap() = Some((asked, run));
            });
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![arming]);
    let ran = executor.submit(&Call::new("work"));
    let (asked, run) = seen.lock().unwrap().take().expect("the hook ran");
    (ran, asked.join().unwrap(), run)
}

#[test]
fn a_wait_begun_in_the_commit_step_of_an_end_that_is_then_not_confirmed_says_abandoned() {
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let synced = root.clone();
    let (ran, waited, run) = asked_in_the_commit_step(
        &root,
        &fs,
        &store,
        |asking, run| asking.wait(run),
        move |fs, _| {
            let root = synced.clone();
            fs.refuse_matching(
                0,
                move |event| matches!(event, Event::SyncDir(dir) if *dir == root),
            );
        },
    );
    let Err(CallError::Store(failure)) = ran else {
        panic!("the end was confirmed: {ran:?}");
    };
    let state = RunState::Abandoned {
        failure: Some(failure),
    };
    assert_eq!(waited.unwrap().state(), &state);
    // The end is visible all the same, which is what a status says.
    let asking = engine(store.clone(), Vec::new());
    assert_eq!(asking.status(&run).unwrap().state(), &completed());
}

#[test]
fn a_cancel_that_waits_in_the_commit_step_of_a_run_that_is_then_deleted_is_told_there_is_no_run() {
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let deleting = store.clone();
    let (ran, cancelled, run) = asked_in_the_commit_step(
        &root,
        &fs,
        &store,
        |asking, run| asking.cancel(run),
        move |_, run| {
            deleting.delete(&[run.as_str()]).unwrap();
        },
    );
    assert!(matches!(ran, Err(CallError::Store(_))), "{ran:?}");
    let no_run = CallError::Refused(Refusal::NoSuchRun { run });
    assert_eq!(cancelled, Err(no_run));
}

#[test]
fn an_executing_run_that_is_deleted_is_told_to_stop() {
    // The run is deleted from the store while its action is busy. Its end will find no
    // run, so nothing it does can be kept: its signal is set by the deletion itself,
    // and nobody has to cancel a run the store no longer holds.
    let dir = Scratch::new("cancel");
    let root = dir.path().join("store");
    let store = Arc::new(Store::open(&root, Arc::new(ShimFs::new())).unwrap());
    let barriers = Barriers::new();
    let saw = Arc::new(Mutex::new(Vec::new()));
    let looking = {
        let (barriers, saw) = (barriers.clone(), saw.clone());
        work(move |_, context| {
            let before = context.cancellation().is_cancelled();
            barriers.stop_at("busy");
            saw.lock()
                .unwrap()
                .push((before, context.cancellation().is_cancelled()));
            barriers.stop_at("done");
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![looking]);
    let Ok(Answer::Started(run)) = executor.start(&asked("order-1")) else {
        panic!("the request was not started");
    };
    barriers.arrived("busy", 1);
    // A wait that began before the deletion.
    let waiting = {
        let (executor, run) = (executor.clone(), run.clone());
        whizbang_testkit::threads::spawn(move || executor.wait(&run))
    };
    let limit = Instant::now() + LIMIT;
    while executor.waiting_for(&run) == 0 {
        assert!(Instant::now() < limit, "the wait does not wait");
        std::thread::yield_now();
    }
    store.delete(&[run.as_str()]).unwrap();
    barriers.release("busy");
    barriers.arrived("done", 1);
    assert_eq!(*saw.lock().unwrap(), [(false, true)]);
    // The store holds no such run, for a cancel now and for the wait that was there.
    let no_run = CallError::Refused(Refusal::NoSuchRun { run: run.clone() });
    assert_eq!(executor.cancel(&run), Err(no_run.clone()));
    barriers.release("done");
    assert_eq!(waiting.join().unwrap(), Err(no_run));
}

// --- none or all ------------------------------------------------------------------------------

/// The calls that change the disk from the moment `pair` has cancelled its own run and
/// handed back to the moment its run is reported: where the first is among all the
/// calls made on a new store, and how many there are.
fn the_calls_of_a_cancelled_end() -> (usize, usize) {
    let world = World::new();
    let started = Arc::new(AtomicU64::new(0));
    let noting = {
        let (fs, started) = (world.fs.clone(), started.clone());
        pair(move |_, context| {
            context.diagnostics().write("notes.txt", b"so far").unwrap();
            context.cancellation().cancel();
            started.store(fs.events().len() as u64, Ordering::SeqCst);
            Ok(both())
        })
    };
    let engine = world.engine(vec![noting]);
    engine.request(&request("r-3")).unwrap();
    let start = started.load(Ordering::SeqCst) as usize;
    (start, world.fs.events().len() - start)
}

/// What a reader finds of a cancelled run with the request id `r-3` after its end was
/// interrupted: `false` when the run is as it was admitted, with no record and no
/// ending, and `true` when it has both, each saying cancelled. Anything between the
/// two, and any output, fails the test.
fn none_or_all(world: &World, fs: Arc<dyn Fs>, at: &str) -> bool {
    let after = held(&world.root(), fs.clone());
    assert_eq!(after.runs.len(), 1, "{at}");
    assert!(!after.mask && !after.place, "{at}");
    assert!(after.outputs.is_empty(), "{at}");
    let (_, stored, names) = &after.runs[0];
    let ended = ending(&world.root(), fs, "r-3");
    match (stored, ended) {
        (None, None) => {
            assert_eq!(*names, 1, "{at}");
            false
        }
        (Some(stored), Some(ended)) => {
            assert_eq!(stored.outcome().status, Status::Cancelled, "{at}");
            assert_eq!(ended.status(), Status::Cancelled, "{at}");
            assert_eq!(*names, 4, "{at}");
            true
        }
        (stored, ended) => panic!(
            "{at}: the record is {} and the ending is {}",
            if stored.is_some() { "there" } else { "missing" },
            if ended.is_some() { "there" } else { "missing" },
        ),
    }
}

#[test]
fn a_cancelled_end_that_the_store_refuses_at_any_step_publishes_no_status_or_both() {
    let (start, steps) = the_calls_of_a_cancelled_end();
    assert!(steps > 8, "{steps}");
    let mut seen = BTreeSet::new();
    for step in 0..steps {
        let world = World::new();
        let engine = world.engine(vec![cancels_itself()]);
        world.fs.refuse_before(start + step);
        let answer = engine.request(&request("r-3"));
        let all = none_or_all(&world, world.fs.clone(), &format!("step {step}"));
        match answer {
            Err(CallError::Store(failure)) => {
                let said = CallError::Store(failure).to_string();
                assert_eq!(
                    said.contains("not confirmed durable"),
                    all,
                    "{step}: {said}"
                );
                seen.insert(all);
            }
            Ok(Answer::Ran(finished)) => {
                assert_eq!(step, steps - 1);
                assert_eq!(finished.outcome(), &Outcome::Cancelled);
                assert!(all);
            }
            other => panic!("step {step}: {other:?}"),
        }
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), [false, true]);
}

#[test]
fn a_cancelled_run_crashed_inside_its_end_reopens_with_no_status_or_with_both() {
    // Power is lost before each call of the end of a cancelled run, with each way the
    // test kit's disk can lose what was not yet durable, as for a commit
    // (`commit::a_run_crashed_inside_its_commit_...`): the names inside `staging/` all
    // lost and all kept, and every choice of the others.
    let (start, steps) = the_calls_of_a_cancelled_end();
    let mut seen = BTreeSet::new();
    let (mut crashes, mut reported) = (0, 0);
    for step in 0..steps {
        let mut ways = vec![Lost::Unsynced, Lost::ContentOnly, Lost::Subset(0)];
        let mut next = 0;
        while next < ways.len() {
            let lost = ways[next];
            next += 1;
            crashes += 1;
            let world = World::on(ShimFs::new().crash_before(start + step, lost));
            let engine = world.engine(vec![cancels_itself()]);
            let answer = engine.request(&request("r-3"));
            assert!(world.fs.crashed(), "step {step}");
            if lost == Lost::Subset(0) {
                let (dirs, named) = (world.fs.crash_choices(), world.fs.crash_named());
                let staging = world.root().join("staging");
                let bit = |names: &[usize]| names.iter().map(|at| 1u64 << at).sum::<u64>();
                let (inside, outside): (Vec<usize>, Vec<usize>) =
                    (0..named.len()).partition(|at| named[*at].starts_with(&staging));
                assert!(dirs <= 6 && outside.len() <= 7, "step {step}: {named:?}");
                ways.extend((1..1u64 << dirs).map(Lost::Subset));
                for choice in 0..1u64 << outside.len() {
                    let chosen: Vec<usize> = (0..outside.len())
                        .filter(|at| choice >> at & 1 == 1)
                        .map(|at| outside[at])
                        .collect();
                    ways.push(Lost::Names(bit(&chosen)));
                    if !inside.is_empty() {
                        ways.push(Lost::Names(bit(&chosen) | bit(&inside)));
                    }
                }
            }
            drop(engine);
            let fs: Arc<dyn Fs> = Arc::new(ShimFs::new());
            drop(Store::open(&world.root(), fs.clone()).unwrap());
            let at = format!("step {step}, {lost:?}");
            let all = none_or_all(&world, fs, &at);
            seen.insert(all);
            match answer {
                Ok(_) => {
                    assert!(all, "{at}: a run was reported whose end did not survive");
                    reported += 1;
                }
                Err(CallError::Store(_)) => {}
                Err(other) => panic!("{at}: {other:?}"),
            }
        }
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), [false, true]);
    assert!(crashes > 5 * steps, "{crashes} crashes over {steps} steps");
    assert!(reported > 0);
}

// --- a cancellation reaches a capability ------------------------------------------------------

/// A request to paint the top row of a small picture.
fn fill_request() -> FillRequest {
    let rgba = (0..24u8).flat_map(|at| [at, at, at, 255]).collect();
    let picture = Picture::new(CanonicalPixels::from_rgba(6, 4, rgba).unwrap()).unwrap();
    let mut plane = vec![0; 24];
    plane[..6].fill(255);
    let support = Support::from_plane(6, 4, plane).unwrap();
    FillRequest::new(picture, support, None).unwrap()
}

/// Backends that offer this filler as the default: its call then resolves it.
fn offering(filler: &Arc<FakeFiller>) -> Arc<dyn Backends> {
    Arc::new(roster(None, Some(filler.as_ref()), None))
}

/// `work` that asks the filler it was handed twice, and notes what each request came
/// to: the kind of its fault, or none.
fn fills(asked: &Arc<Mutex<Vec<Option<FaultKind>>>>) -> Arc<dyn Action> {
    let asked = asked.clone();
    work_needing(&[Capability::Fill], move |_, context| {
        context.events().progress("asking the filler");
        let fill = context.capabilities().fill();
        let fill = fill.expect("the executor hands the action its filler");
        for _ in 0..2 {
            let fault = fill.fill(&fill_request()).err().map(|fault| fault.kind());
            asked.lock().unwrap().push(fault);
        }
        Ok(Produced::nothing())
    })
}

#[test]
fn while_an_action_is_in_blocking_work_a_second_caller_reads_its_status_and_cancels_it() {
    // The action's request is held open by the fake filler. A second caller reads the
    // run's status and cancels it. The fake sees the cancellation while it holds the
    // request, and stops; and the handle sends nothing more for the run.
    let dir = Scratch::new("cancel");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let barriers = Barriers::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse).stopping_at(&barriers, "fill"));
    let faults = Arc::new(Mutex::new(Vec::new()));
    let first = engine_on(store.clone(), offering(&filler), vec![fills(&faults)])
        .with_capabilities(Capabilities::none().with_fill(filler.clone()));
    let second = engine(store.clone(), Vec::new());
    let Ok(Answer::Started(run)) = first.start(&asked("order-1")) else {
        panic!("the request was not started");
    };
    barriers.arrived("fill", 1);

    let watched = second.status(&run).unwrap();
    assert_eq!(watched.state(), &RunState::NotEnded { here: true });
    assert_eq!(watched.events()[0].message(), "asking the filler");
    assert_eq!(second.cancel(&run), Ok(Cancel::Accepted));
    barriers.release("fill");
    assert_eq!(second.wait(&run).unwrap().state(), &cancelled());

    // The request that was open came back cancelled, and the next was never sent.
    let cancelled = Some(FaultKind::Cancelled);
    assert_eq!(*faults.lock().unwrap(), [cancelled, cancelled]);
    assert_eq!(filler.requests(), 1);
    let observed = filler.observed();
    assert_eq!(observed.len(), 1);
    assert!(!observed[0].cancelled_on_arrival());
    assert!(observed[0].cancelled_on_release());
}

#[test]
fn each_runs_handles_are_bound_to_its_own_signal_and_deadline() {
    // Two runs of one executor, over one filler, each with a request open. One is
    // cancelled, and the other has a deadline. The fake is told of each run what is
    // so of that run and not of the other: no handle is bound to another run's signal.
    let dir = Scratch::new("cancel");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let barriers = Barriers::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse).stopping_at(&barriers, "fill"));
    let faults = Arc::new(Mutex::new(Vec::new()));
    let executor = engine_on(store.clone(), offering(&filler), vec![fills(&faults)])
        .with_capabilities(Capabilities::none().with_fill(filler.clone()));
    let Ok(Answer::Started(one)) = executor.start(&asked("order-1")) else {
        panic!("the request was not started");
    };
    barriers.arrived("fill", 1);
    let deadline = Instant::now() + Duration::from_secs(3600);
    let Ok(Answer::Started(two)) = executor.start(&asked("order-2").until(deadline)) else {
        panic!("the request was not started");
    };
    barriers.arrived("fill", 2);
    assert_eq!(executor.cancel(&one), Ok(Cancel::Accepted));
    barriers.open("fill");
    assert_eq!(executor.wait(&one).unwrap().state(), &cancelled());
    assert_eq!(executor.wait(&two).unwrap().state(), &completed());

    // The first run's one request, and the second run's two, in the order the fake
    // let them go, which is not fixed.
    let observed = filler.observed();
    let mut told: Vec<_> = observed
        .iter()
        .map(|seen| (seen.cancelled_on_release(), seen.deadline()))
        .collect();
    told.sort();
    let second = (false, Some(deadline));
    assert_eq!(told, [second, second, (true, None)]);
    assert_eq!(filler.requests(), 3);
    let asked = faults.lock().unwrap().clone();
    assert_eq!(asked.iter().filter(|fault| fault.is_none()).count(), 2);
}

// --- an admission -----------------------------------------------------------------------------

/// A ledger that does what the test says with the step.
struct Scripted<F>(F);

impl<F> Ledger for Scripted<F>
where
    F: Fn(&Reservation, &mut Step<'_, '_>) -> Result<(), Declined> + Send + Sync,
{
    fn reserve(&self, reservation: &Reservation, step: &mut Step<'_, '_>) -> Result<(), Declined> {
        (self.0)(reservation, step)
    }
}

/// A ledger under which no admission is ever published: each time it is asked, another
/// publication makes, first, the name it then adds to the step, so the store refuses
/// the admission and the ledger is asked again. It counts the times it was asked, and
/// before it answers the `at`th does what the test says.
fn starving(
    store: &Arc<Store>,
    asked: &Arc<AtomicU64>,
    at: u64,
    then: impl Fn() + Send + Sync + 'static,
) -> Arc<dyn Ledger> {
    let (store, asked) = (store.clone(), asked.clone());
    Arc::new(Scripted(move |_: &Reservation, step: &mut Step<'_, '_>| {
        let count = asked.fetch_add(1, Ordering::SeqCst) + 1;
        assert!(
            count <= at,
            "the ledger was asked again after the admission was stopped"
        );
        if count == at {
            then();
        }
        let name = format!("spend/taken/{count}");
        let mut publication = store.begin().unwrap();
        publication
            .create(&name, store::Kind::Diagnostic, b"taken")
            .unwrap();
        publication.publish().unwrap();
        step.make(&name, b"taken")?;
        Ok(())
    }))
}

/// What a store holds after an admission that was stopped: the competing
/// publications, and nothing of the request.
fn nothing_of_the_request(store: &Store, publications: u64) {
    let generation = store.snapshot().unwrap();
    assert_eq!(generation.number(), publications);
    assert!(generation.runs().is_empty());
    let left: Vec<&String> = generation
        .entries()
        .keys()
        .filter(|name| !name.starts_with("spend/taken/"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

/// A clock whose instant the test moves.
struct Moved(Mutex<Instant>);

impl Clock for Moved {
    fn now_ms(&self) -> u64 {
        9000
    }

    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}

#[test]
fn a_cancellation_that_arrives_while_a_ledger_is_asked_again_ends_the_admission() {
    let dir = Scratch::new("cancel");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let ran = Arc::new(AtomicU64::new(0));
    let counting = {
        let ran = ran.clone();
        move || {
            let ran = ran.clone();
            work(move |_, _| {
                ran.fetch_add(1, Ordering::SeqCst);
                Ok(Produced::nothing())
            })
        }
    };
    // The caller keeps the signal its request carries. The third time the ledger is
    // asked, the signal is set: the admission is not tried a fourth time.
    let (signal, times) = (Cancellation::new(), Arc::new(AtomicU64::new(0)));
    let ledger = starving(&store, &times, 3, {
        let signal = signal.clone();
        move || signal.cancel()
    });
    let executor = engine(store.clone(), vec![counting()]).with_ledger(ledger);
    let refused = executor
        .request(&asked("order-1").cancelled_by(signal.clone()))
        .unwrap_err();
    assert_eq!(
        refused,
        CallError::Refused(Refusal::Cancelled {
            action: "work".to_owned()
        })
    );
    assert_eq!(
        refused.to_string(),
        "work was not admitted: its request was cancelled first"
    );
    assert_eq!(times.load(Ordering::SeqCst), 3);
    nothing_of_the_request(&store, 3);
    assert_eq!(ran.load(Ordering::SeqCst), 0);

    // A request that is already cancelled when it is sent is refused before the
    // ledger is asked, by `start` as by `request`.
    let before = times.load(Ordering::SeqCst);
    assert!(matches!(
        executor.start(&asked("order-1").cancelled_by(signal.clone())),
        Err(CallError::Refused(Refusal::Cancelled { .. }))
    ));
    assert_eq!(times.load(Ordering::SeqCst), before);
    nothing_of_the_request(&store, 3);

    // The id was not taken: the same request, not cancelled, under a ledger that lets
    // it through, runs.
    let executor = engine(store.clone(), vec![counting()]).with_ledger(Arc::new(FakeLedger::new()));
    let Ok(Answer::Ran(finished)) = executor.request(&asked("order-1")) else {
        panic!("the request did not run");
    };
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(ran.load(Ordering::SeqCst), 1);
}

#[test]
fn a_deadline_that_passes_while_a_ledger_is_asked_again_ends_the_admission() {
    let dir = Scratch::new("cancel");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let start = Instant::now();
    let clock = Arc::new(Moved(Mutex::new(start)));
    let deadline = start + Duration::from_secs(60);
    let times = Arc::new(AtomicU64::new(0));
    // The third time the ledger is asked, the executor's clock passes the deadline.
    let ledger = starving(&store, &times, 3, {
        let clock = clock.clone();
        move || *clock.0.lock().unwrap() = deadline
    });
    let executor = engine(store.clone(), vec![work(|_, _| Ok(Produced::nothing()))])
        .with_clock(clock.clone())
        .with_ledger(ledger);
    let refused = executor
        .request(&asked("order-1").until(deadline))
        .unwrap_err();
    assert_eq!(
        refused,
        CallError::Refused(Refusal::DeadlinePassed {
            action: "work".to_owned()
        })
    );
    assert_eq!(
        refused.to_string(),
        "work was not admitted: the deadline of its request passed first"
    );
    assert_eq!(times.load(Ordering::SeqCst), 3);
    nothing_of_the_request(&store, 3);

    // Sent after its deadline, a request is refused before the ledger is asked.
    assert!(matches!(
        executor.request(&asked("order-2").until(start)),
        Err(CallError::Refused(Refusal::DeadlinePassed { .. }))
    ));
    assert_eq!(times.load(Ordering::SeqCst), 3);
}

#[test]
fn the_signal_and_the_deadline_a_request_came_with_are_its_runs() {
    // After the admission, the signal the caller kept is the run's signal: setting it
    // cancels the run, by the same rule, and the action is handed the deadline.
    let dir = Scratch::new("cancel");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let barriers = Barriers::new();
    let saw = Arc::new(Mutex::new(Vec::new()));
    let looking = {
        let (barriers, saw) = (barriers.clone(), saw.clone());
        work(move |_, context: &Context| {
            let before = context.cancellation().is_cancelled();
            barriers.stop_at("busy");
            let after = context.cancellation().is_cancelled();
            saw.lock()
                .unwrap()
                .push((before, after, context.deadline()));
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store, vec![looking]);
    let signal = Cancellation::new();
    let deadline = Instant::now() + Duration::from_secs(3600);
    let request = asked("order-1")
        .cancelled_by(signal.clone())
        .until(deadline);
    let Ok(Answer::Started(run)) = executor.start(&request) else {
        panic!("the request was not started");
    };
    barriers.arrived("busy", 1);
    signal.cancel();
    barriers.release("busy");
    assert_eq!(executor.wait(&run).unwrap().state(), &cancelled());
    assert_eq!(*saw.lock().unwrap(), [(false, true, Some(deadline))]);
}

// --- the generation a run reads ---------------------------------------------------------------

#[test]
fn a_run_reads_the_generation_it_was_admitted_against_through_a_deletion_and_a_prune() {
    // Before the run: an earlier run with a diagnostic, and a name outside runs. While
    // the run's action is stopped, the earlier run is deleted, the name is given other
    // content, and the store is pruned. The action still sees the generation it was
    // admitted against, and reads from it what that generation names.
    let dir = Scratch::new("cancel");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let barriers = Barriers::new();
    type Read = (u64, bool, Vec<u8>, Vec<u8>);
    let read: Arc<Mutex<Vec<Read>>> = Arc::new(Mutex::new(Vec::new()));
    let reading = {
        let (barriers, read) = (barriers.clone(), read.clone());
        work(move |_, context| {
            for stop in [false, true] {
                if stop {
                    barriers.stop_at("reading");
                }
                let generation = context.store().snapshot().unwrap();
                let earlier = generation.runs().values().next();
                let notes = earlier.and_then(|run| run.get("diagnostics/echo.txt"));
                let notes = notes.map(|entry| context.store().read_record(entry).unwrap());
                let named = generation.get("notes/one").unwrap();
                read.lock().unwrap().push((
                    generation.number(),
                    earlier.is_some(),
                    notes.unwrap_or_default(),
                    context.store().read(named).unwrap(),
                ));
            }
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![reading, Arc::new(Echo::new())]);
    let echo = Call::new("echo").argument("text", Value::text("kept"));
    let earlier = executor.submit(&echo).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("notes/one", b"first").unwrap();
    publication.publish().unwrap();
    let admitted_against = store.snapshot().unwrap().number();

    let Ok(Answer::Started(run)) = executor.start(&Request::new(Call::new("work"))) else {
        panic!("the request was not started");
    };
    barriers.arrived("reading", 1);
    store.delete(&[earlier.run().as_str()]).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("notes/one", b"second").unwrap();
    publication.publish().unwrap();
    let pruned = store.prune().unwrap();
    assert_eq!(pruned.generations_in_use, [admitted_against]);
    barriers.release("reading");
    assert_eq!(executor.wait(&run).unwrap().state(), &completed());

    let seen = (admitted_against, true, b"kept".to_vec(), b"first".to_vec());
    assert_eq!(*read.lock().unwrap(), [seen.clone(), seen]);

    // The run is over: the next prune takes the generation, and what only it named.
    let pruned = store.prune().unwrap();
    assert!(pruned.generations_removed.contains(&admitted_against));
    assert!(pruned.generations_in_use.is_empty());
    assert!(pruned.objects_removed >= 2, "{pruned:?}");
}

// --- what the review's mutants survived -------------------------------------------------------

#[test]
fn a_request_that_is_cancelled_is_refused_as_cancelled_before_its_call_is_looked_at() {
    // The signal is looked at before the call is prepared: a call that would be
    // refused for itself, as unknown or as invalid, is refused as cancelled.
    let dir = Scratch::new("cancel");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let executor = engine(store.clone(), vec![work(|_, _| Ok(Produced::nothing()))]);
    let signal = Cancellation::new();
    signal.cancel();
    let invalid = Call::new("work").argument("nope", Value::text("x"));
    assert!(matches!(
        executor.submit(&invalid),
        Err(CallError::Refused(Refusal::Invalid { .. }))
    ));
    for call in [Call::new("nothing-of-that-name"), invalid] {
        let action = call.action().to_owned();
        let request = Request::new(call).cancelled_by(signal.clone());
        let refused = CallError::Refused(Refusal::Cancelled { action });
        assert_eq!(executor.request(&request), Err(refused));
    }
    assert_eq!(store.snapshot().unwrap().number(), 0);
}

#[test]
fn a_call_is_prepared_from_the_generation_that_was_held_and_not_from_a_later_one() {
    // The store changes between the hold and the preparation: the mask the call takes
    // is published straight after the held generation was read. The call is prepared
    // from what was held, where there is no such mask, and is refused; the next
    // request, admitted against the later generation, runs.
    let dir = Scratch::new("cancel");
    let fs = Arc::new(AfterTheLook {
        hook: Mutex::new(None),
    });
    let store = Arc::new(Store::open(&dir.path().join("store"), fs.clone()).unwrap());
    let mut publication = store.begin().unwrap();
    publication.put("notes/one", b"first").unwrap();
    publication.publish().unwrap();
    let look = {
        let draft = ActionDescriptor::draft("look", "1.0.0", Kind::Primitive, "Looks.")
            .input(InputPort::required("mask", ArtifactType::Mask, "The mask."));
        Arc::new(Told(
            draft.build().unwrap(),
            Box::new(|_, _| Ok(Produced::nothing())),
        ))
    };
    let executor = engine_over(store.clone(), vec![look]);
    *fs.hook.lock().unwrap() = Some(Box::new({
        let store = store.clone();
        move || {
            let mut publication = store.begin().unwrap();
            artifact::put(&mut publication, &Artifact::from(the_mask())).unwrap();
            publication.publish().unwrap();
        }
    }));
    let call = Call::new("look").input("mask", the_mask().id());
    let refused = executor.submit(&call);
    assert!(fs.hook.lock().unwrap().is_none(), "the hook never ran");
    assert_eq!(store.snapshot().unwrap().number(), 2);
    assert!(
        matches!(refused, Err(CallError::Refused(Refusal::Input { .. }))),
        "{refused:?}"
    );
    assert_eq!(
        executor.submit(&call).unwrap().outcome().name(),
        "completed"
    );
}

#[test]
fn a_handle_holds_its_runs_deadline_to_the_executors_clock() {
    // One clock for a deadline: the one admission goes by. The action asks its filler,
    // moves the executor's clock past the run's deadline, and asks again: the handle
    // sends nothing more, though by the machine's clock the deadline is an hour away.
    let dir = Scratch::new("cancel");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let start = Instant::now();
    let deadline = start + Duration::from_secs(3600);
    let clock = Arc::new(Moved(Mutex::new(start)));
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let faults = Arc::new(Mutex::new(Vec::new()));
    let asking = {
        let (clock, faults) = (clock.clone(), faults.clone());
        work_needing(&[Capability::Fill], move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            for _ in 0..2 {
                let fault = fill.fill(&fill_request()).err().map(|fault| fault.kind());
                faults.lock().unwrap().push(fault);
                *clock.0.lock().unwrap() = deadline;
            }
            Ok(Produced::nothing())
        })
    };
    let executor = engine_on(store, offering(&filler), vec![asking])
        .with_clock(clock.clone())
        .with_capabilities(Capabilities::none().with_fill(filler.clone()));
    executor.request(&asked("order-1").until(deadline)).unwrap();
    assert_eq!(*faults.lock().unwrap(), [None, Some(FaultKind::TimedOut)]);
    assert_eq!(filler.requests(), 1);
}

/// The executor of the test below, for its spawner, which is a plain function.
static NO_THREAD: std::sync::OnceLock<Engine> = std::sync::OnceLock::new();
static CANCELLED_FIRST: Mutex<Option<Result<Cancel, CallError>>> = Mutex::new(None);

/// A spawner that makes no thread, and before it says so cancels the run it was asked
/// to make a thread for.
fn cancels_and_makes_no_thread(
    name: String,
    _: Box<dyn FnOnce() + Send + 'static>,
) -> std::io::Result<()> {
    let run = RunId::parse(name.trim_start_matches("run ")).unwrap();
    let cancel = NO_THREAD.get().unwrap().cancel(&run);
    *CANCELLED_FIRST.lock().unwrap() = Some(cancel);
    Err(std::io::Error::other("no thread is left"))
}

#[test]
fn a_run_that_was_cancelled_before_its_thread_could_not_be_made_ends_cancelled() {
    // The run is admitted, a cancel of it is accepted, and then no thread can be made
    // for it. It is ended where it is, through the same entry to the end: the
    // cancellation came first, so it ends as cancelled, and not as an error.
    let dir = Scratch::new("cancel");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let executor = engine(store, vec![work(|_, _| panic!("the action started"))])
        .with_spawn(cancels_and_makes_no_thread);
    assert!(NO_THREAD.set(executor.clone()).is_ok());
    let Err(CallError::NotStarted { run, .. }) = executor.start(&asked("order-1")) else {
        panic!("the run was started");
    };
    assert_eq!(
        CANCELLED_FIRST.lock().unwrap().take(),
        Some(Ok(Cancel::Accepted))
    );
    assert_eq!(executor.wait(&run).unwrap().state(), &cancelled());
}
