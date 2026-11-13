//! Runs that were started (ADR 0008, "How many runs are started at once"): the bound on
//! them, which the store's writer counts, what a refusal for it leaves behind, what
//! gives a place back, and a run whose thread could not be made.
//!
//! Nothing here makes threads until something gives way. The bound is 1 or 2, a thread
//! that cannot be made is one the test's own spawner refused to make, and every wait is
//! for an event, with a limit.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, BoundRefused, Call, CallError, Context, Executor, Failure,
    Kind, MESSAGE_LIMIT, Outcome, PreparedCall, Produced, Refusal, Request, RequestId, RunId,
    RunState, STARTED_RUNS_CEILING, STARTED_RUNS_DEFAULT, Secrets, Watched, read_journal,
};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;

use super::redact::made_up_key;
use super::watch::{completed, engine, work};

/// How long an answer may take before the test fails: far beyond what any takes here.
const LIMIT: Duration = Duration::from_secs(10);

/// What `work` gives, on a thread of its own, or a failure of the test when it has not
/// come back within the limit: a call that would block for ever fails here, at once.
fn within<T: Send + 'static>(what: &str, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, answer) = mpsc::channel();
    std::thread::spawn(move || drop(done.send(work())));
    answer
        .recv_timeout(LIMIT)
        .unwrap_or_else(|_| panic!("{what} did not come back"))
}

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

struct Named(ActionDescriptor, Body);

impl Action for Named {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(call, context)
    }
}

/// An action of this name, which stops at the barrier of the same name.
fn held(name: &str, barriers: &Barriers) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "Stops.");
    let (barriers, at) = (barriers.clone(), name.to_owned());
    Arc::new(Named(
        draft.build().unwrap(),
        Box::new(move |_, _| {
            barriers.stop_at(&at);
            Ok(Produced::nothing())
        }),
    ))
}

/// An action of this name, which comes straight back.
fn quick(name: &str) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "Returns.");
    Arc::new(Named(
        draft.build().unwrap(),
        Box::new(|_, _| Ok(Produced::nothing())),
    ))
}

fn asked(action: &str, id: &str) -> Request {
    Request::new(Call::new(action)).with_id(RequestId::parse(id).unwrap())
}

fn started(answer: Result<Answer, CallError>) -> RunId {
    match answer {
        Ok(Answer::Started(run)) => run,
        other => panic!("the request was not started: {other:?}"),
    }
}

fn too_many(bound: usize) -> Result<Answer, CallError> {
    Err(CallError::Refused(Refusal::TooManyStarted { bound }))
}

/// Every file under the store, with what it holds.
fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                found.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

fn open(dir: &Scratch) -> (PathBuf, Arc<Store>) {
    let root = dir.path().join("store");
    let store = Arc::new(Store::open(&root, Arc::new(ShimFs::new())).unwrap());
    (root, store)
}

// --- the bound --------------------------------------------------------------------------------

#[test]
fn a_start_over_the_bound_is_refused_and_writes_nothing_and_a_run_that_ends_makes_room() {
    let dir = Scratch::new("started");
    let (root, store) = open(&dir);
    let barriers = Barriers::new();
    let actions = vec![
        held("first", &barriers),
        held("second", &barriers),
        quick("quick"),
    ];
    let executor = engine(store.clone(), actions)
        .with_started_bound(2)
        .unwrap();
    let first = started(executor.start(&asked("first", "order-1")));
    let second = started(executor.start(&asked("second", "order-2")));
    barriers.arrived("first", 1);
    barriers.arrived("second", 1);

    // Two started runs are not over, and two is the bound: a third is refused, with
    // the bound and nothing the caller wrote, and at once, on the caller's thread.
    let before = files(&root);
    let generation = store.snapshot().unwrap().number();
    let refused = executor.start(&asked("first", "order-3"));
    assert_eq!(refused, too_many(2));
    assert_eq!(
        refused.unwrap_err().to_string(),
        "the request was not started: 2 started runs are not over yet, and this executor \
         runs no more than that at once"
    );
    // So is one that has no id.
    assert_eq!(
        executor.start(&Request::new(Call::new("second"))),
        too_many(2)
    );
    // Nothing was written: no run, no admission, no record of the id, no generation.
    assert_eq!(files(&root), before);
    assert_eq!(store.snapshot().unwrap().number(), generation);
    assert_eq!(store.snapshot().unwrap().runs().len(), 2);

    // A request that was sent before starts nothing, and is answered as it always is.
    let Ok(Answer::Repeat(earlier)) = executor.start(&asked("first", "order-1")) else {
        panic!("a request that was sent before was not answered as a repeat");
    };
    assert_eq!(earlier.run(), &first);
    // What runs on the caller's own thread is not counted, and is not refused.
    let Ok(Answer::Ran(ran)) = executor.request(&asked("quick", "order-4")) else {
        panic!("a request on the caller's thread did not run");
    };
    let nothing = Outcome::Completed {
        outputs: Vec::new(),
    };
    assert_eq!(ran.outcome(), &nothing);
    executor.submit(&Call::new("quick")).unwrap();
    assert_eq!(
        executor.start(&asked("first", "order-3")),
        too_many(2),
        "a run on the caller's thread took or gave a place"
    );

    // One of the two ends, and there is room for one: for the request that was
    // refused, under the id it came with, which the refusal did not take.
    barriers.release("first");
    assert_eq!(executor.wait(&first).unwrap().state(), &completed());
    let third = started(executor.start(&asked("first", "order-3")));
    assert!(third != first && third != second);
    barriers.arrived("first", 2);
    assert_eq!(executor.start(&asked("second", "order-5")), too_many(2));

    barriers.open("first");
    barriers.open("second");
    for run in [&second, &third] {
        assert_eq!(executor.wait(run).unwrap().state(), &completed());
    }
}

#[test]
fn two_executors_over_one_store_count_the_same_started_runs() {
    let dir = Scratch::new("started");
    let (_, store) = open(&dir);
    let barriers = Barriers::new();
    let one = engine(store.clone(), vec![held("first", &barriers)])
        .with_started_bound(2)
        .unwrap();
    let other = engine(store.clone(), vec![held("second", &barriers)])
        .with_started_bound(2)
        .unwrap();
    let first = started(one.start(&asked("first", "order-1")));
    let second = started(other.start(&asked("second", "order-2")));
    barriers.arrived("first", 1);
    barriers.arrived("second", 1);
    // Each started one, and the writer of the store has two: neither starts a third.
    assert_eq!(one.start(&asked("first", "order-3")), too_many(2));
    assert_eq!(other.start(&asked("second", "order-4")), too_many(2));
    // An executor that takes fewer is held to its own bound against the same count.
    let fewer = engine(store.clone(), vec![held("first", &barriers)])
        .with_started_bound(1)
        .unwrap();
    assert_eq!(fewer.start(&asked("first", "order-5")), too_many(1));

    // A run the other executor started ends, and this one has its place; the one
    // that takes fewer still has none.
    barriers.open("second");
    assert_eq!(one.wait(&second).unwrap().state(), &completed());
    assert_eq!(fewer.start(&asked("first", "order-5")), too_many(1));
    let third = started(one.start(&asked("first", "order-3")));
    barriers.open("first");
    for run in [&first, &third] {
        assert_eq!(other.wait(run).unwrap().state(), &completed());
    }
}

#[test]
fn a_bound_of_none_or_over_the_ceiling_is_refused_when_it_is_set() {
    assert_eq!((STARTED_RUNS_DEFAULT, STARTED_RUNS_CEILING), (64, 256));
    let dir = Scratch::new("started");
    let (_, store) = open(&dir);
    let executor = || engine(store.clone(), Vec::new());
    for asked in [0, 257, usize::MAX] {
        let refused = executor().with_started_bound(asked).err();
        assert_eq!(refused, Some(BoundRefused { asked }));
    }
    assert_eq!(
        BoundRefused { asked: 257 }.to_string(),
        "an executor starts at least 1 run and at most 256 at once: 257 is not a bound it takes"
    );
    for taken in [1, 64, 256] {
        assert!(executor().with_started_bound(taken).is_ok());
    }
}

// --- what gives a place back ------------------------------------------------------------------

#[test]
fn a_started_run_whose_action_panics_gives_its_place_back() {
    let dir = Scratch::new("started");
    let (_, store) = open(&dir);
    let panics = work(|_, _| panic!("the action gave up"));
    let executor = engine(store, vec![panics]).with_started_bound(1).unwrap();
    let work = |id: &str| asked("work", id);
    let run = started(executor.start(&work("order-1")));
    let waited = executor.wait(&run).unwrap();
    let RunState::Ended(Outcome::Error { kind, .. }) = waited.state() else {
        panic!("the run did not end as an error: {:?}", waited.state());
    };
    assert_eq!(kind, "panic");
    // The one place is free again: the next is started, not refused.
    let next = started(executor.start(&work("order-2")));
    assert!(matches!(
        executor.wait(&next).unwrap().state(),
        RunState::Ended(_)
    ));
}

#[test]
fn a_started_run_whose_end_the_store_refuses_gives_its_place_back() {
    let dir = Scratch::new("started");
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
    let executor = engine(store, vec![stuck, quick("quick")])
        .with_started_bound(1)
        .unwrap();
    let run = started(executor.start(&asked("work", "order-1")));
    let waited = within("the wait for a run whose end was refused", {
        let (executor, run) = (executor.clone(), run.clone());
        move || executor.wait(&run)
    });
    assert_eq!(waited.unwrap().state(), &RunState::NotEnded { here: true });
    let next = started(executor.start(&asked("quick", "order-2")));
    assert_eq!(executor.wait(&next).unwrap().state(), &completed());
}

// --- a thread that cannot be made -------------------------------------------------------------

/// How many times the spawner below was asked for a thread.
static ASKED_FOR_A_THREAD: AtomicUsize = AtomicUsize::new(0);

/// A spawner that makes no thread, and says the system refused: what the system says
/// when it has none left, without anything being used up.
fn no_thread(_: String, _: Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<()> {
    ASKED_FOR_A_THREAD.fetch_add(1, Ordering::SeqCst);
    Err(std::io::Error::other("no thread is left"))
}

#[test]
fn a_run_whose_thread_cannot_be_made_is_ended_as_an_error_and_nobody_waits_for_it() {
    let dir = Scratch::new("started");
    let (root, store) = open(&dir);
    let barriers = Barriers::new();
    let actions = || vec![held("first", &barriers)];
    let executor = engine(store.clone(), actions())
        .with_started_bound(1)
        .unwrap()
        .with_spawn(no_thread);

    // The failure is the caller's answer, and it is not a panic: it names the run,
    // which was admitted, and says what the system said.
    let answer = within("the start of a run whose thread cannot be made", {
        let executor = executor.clone();
        move || executor.start(&asked("first", "order-1"))
    });
    let Err(CallError::NotStarted { run, why }) = &answer else {
        panic!("the failure to make a thread was not handed back: {answer:?}");
    };
    assert_eq!(ASKED_FOR_A_THREAD.load(Ordering::SeqCst), 1);
    assert_eq!(why, "no thread is left");
    assert!(answer.as_ref().unwrap_err().refusal().is_none());
    assert_eq!(
        answer.as_ref().unwrap_err().to_string(),
        format!(
            "the run {run} was admitted and could not be started: no thread could be made \
             for its action: no thread is left"
        )
    );
    // No thread was made, so the action did not run.
    assert_eq!(barriers.arrivals("first"), 0);

    // The run is in the store, ended as an error that says so: a wait for it comes
    // back, and so does a status, and the request's id answers with it.
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    assert_eq!(reader.snapshot().unwrap().runs().len(), 1);
    let waited: Watched = within("the wait for a run that was never started", {
        let (executor, run) = (executor.clone(), run.clone());
        move || executor.wait(&run)
    })
    .unwrap();
    let ended = RunState::Ended(Outcome::Error {
        kind: "start".to_owned(),
        message: "the run was admitted and could not be started: no thread could be made \
                  for its action: no thread is left"
            .to_owned(),
    });
    assert_eq!(waited.state(), &ended);
    assert_eq!(executor.status(run).unwrap(), waited);
    assert!(!store.made_here(run.as_str()));
    let Ok(Answer::Repeat(earlier)) = executor.start(&asked("first", "order-1")) else {
        panic!("the request was not answered as a repeat");
    };
    assert_eq!((earlier.run(), earlier.state()), (run, &ended));
    assert_eq!(ASKED_FOR_A_THREAD.load(Ordering::SeqCst), 1);

    // Its place was given back: an executor that makes threads starts the next run,
    // under a bound of one.
    let real = engine(store, actions()).with_started_bound(1).unwrap();
    let next = started(real.start(&asked("first", "order-2")));
    barriers.arrived("first", 1);
    barriers.open("first");
    assert_eq!(real.wait(&next).unwrap().state(), &completed());
}

// --- started, and waited for ------------------------------------------------------------------

#[test]
fn a_started_run_runs_on_another_thread_and_a_wait_for_it_is_told_when_it_is_over() {
    // Both would hang a test that only called them: a `start` that ran the action
    // itself would stop at the barrier, and a `wait` that nothing told of the end
    // would never return. Each is asked for on a thread of its own and must answer
    // within the limit, so either fault fails here in seconds.
    let dir = Scratch::new("started");
    let (_, store) = open(&dir);
    let barriers = Barriers::new();
    let ran_on = Arc::new(Mutex::new(None));
    let stops = {
        let (barriers, ran_on) = (barriers.clone(), ran_on.clone());
        work(move |_, _| {
            *ran_on.lock().unwrap() = Some(std::thread::current().id());
            barriers.stop_at("busy");
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store, vec![stops]);
    let (run, asked_on) = within("the start of a run whose action stops", {
        let executor = executor.clone();
        move || {
            let answer = executor.start(&asked("work", "order-1"));
            (started(answer), std::thread::current().id())
        }
    });
    barriers.arrived("busy", 1);
    let ran_on = ran_on.lock().unwrap().expect("the action ran");
    assert_ne!(ran_on, asked_on, "the action ran on the caller's thread");
    assert_eq!(
        executor.status(&run).unwrap().state(),
        &RunState::NotEnded { here: true }
    );

    let (asking, waited) = mpsc::channel();
    let waiting = {
        let (executor, run) = (executor.clone(), run.clone());
        std::thread::spawn(move || drop(asking.send(executor.wait(&run))))
    };
    barriers.release("busy");
    let waited = waited
        .recv_timeout(LIMIT)
        .expect("the wait was never told that the run is over");
    assert_eq!(waited.unwrap().state(), &completed());
    waiting.join().unwrap();
}

// --- what a run reports -----------------------------------------------------------------------

#[test]
fn a_progress_message_is_cut_as_every_text_a_run_keeps_is() {
    // Five megabytes in one event, with a secret near its start and one across the
    // cut. What a status shows, and what the journal holds once the run ends, is the
    // first 4,096 characters of the message with the secrets taken out, and three dots.
    let dir = Scratch::new("started");
    let (root, store) = open(&dir);
    let barriers = Barriers::new();
    let key = made_up_key("progress");
    let said = {
        let head = format!("began with {key} ");
        // The second secret begins six characters before the cut of the text as given.
        let filler = "é".repeat(MESSAGE_LIMIT - 6 - head.chars().count());
        let mut said = format!("{head}{filler}{key}");
        said.push_str(&"x".repeat(5 * 1024 * 1024 - said.len()));
        said
    };
    assert_eq!(said.len(), 5 * 1024 * 1024);
    let loud = {
        let (barriers, said) = (barriers.clone(), said.clone());
        work(move |_, context| {
            context.events().progress(&said);
            barriers.stop_at("busy");
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store, vec![loud]).with_secrets(Secrets::new([key.clone()]).unwrap());
    let run = started(executor.start(&asked("work", "order-1")));
    barriers.arrived("busy", 1);

    let watched = executor.status(&run).unwrap();
    let shown = watched.events()[0].message().to_owned();
    assert_eq!(shown.chars().count(), MESSAGE_LIMIT + 3);
    assert!(
        shown.starts_with("began with [redacted] ééé"),
        "{}",
        &shown[..40]
    );
    assert!(shown.ends_with("..."));
    assert!(
        !shown.contains(&key[..12]),
        "a secret, or part of one, was kept"
    );

    barriers.release("busy");
    assert_eq!(executor.wait(&run).unwrap().state(), &completed());
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let entry = generation
        .run(run.as_str())
        .unwrap()
        .get("journal")
        .unwrap();
    let journal = read_journal(&reader.read(entry).unwrap()).unwrap();
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0].message(), shown);
}
