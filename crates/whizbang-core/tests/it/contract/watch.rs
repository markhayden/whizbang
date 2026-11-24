//! Asking after a run by its id (ADR 0008, "Status"): its status and its progress
//! while its action is busy, a run that was started and left, waiting for one, and the
//! two moments at which a status could be answered wrongly, each stopped at with a hook.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Backends, Call, CallError, Capability, Clock, Context,
    Engine, Executor, Failure, Kind, NoBackends, Outcome, PreparedCall, Produced, Refusal,
    Registry, Request, RequestId, Requirement, RunId, RunState, Secrets, Watched,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::{Fs, Held, LockFile, RealFs};
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;

use super::redact::made_up_key;

struct Stopped(u64);

impl Clock for Stopped {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

struct Scripted(ActionDescriptor, Body);

impl Action for Scripted {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(call, context)
    }
}

/// The action `work`, whose run is whatever the test says.
pub(super) fn work(
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    work_needing(&[], body)
}

/// `work`, with a descriptor that says it needs these capabilities: an action is
/// handed a handle on the backend its call resolved, and on no other (ADR 0009).
pub(super) fn work_needing(
    needs: &[Capability],
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let mut draft = ActionDescriptor::draft("work", "1.0.0", Kind::Primitive, "Works.");
    for need in needs {
        draft = draft.needs(Requirement::new(*need));
    }
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

/// `work` that reports two events around a stop at the barrier `busy`.
pub(super) fn busy(barriers: &Barriers, secret: &str) -> Arc<dyn Action> {
    let (barriers, secret) = (barriers.clone(), secret.to_owned());
    work(move |_, context| {
        context.events().progress(&format!("began with {secret}"));
        barriers.stop_at("busy");
        context.events().progress("went on");
        Ok(Produced::nothing())
    })
}

pub(super) fn engine(store: Arc<Store>, actions: Vec<Arc<dyn Action>>) -> Engine {
    engine_on(store, Arc::new(NoBackends), actions)
}

/// The same executor, whose calls are resolved by these backends.
pub(super) fn engine_on(
    store: Arc<Store>,
    backends: Arc<dyn Backends>,
    actions: Vec<Arc<dyn Action>>,
) -> Engine {
    let mut registry = Registry::new();
    for action in actions {
        registry.register(action).unwrap();
    }
    Engine::over(registry, backends, store, Digest::of_bytes(b""))
        .with_clock(Arc::new(Stopped(9000)))
}

pub(super) fn asked(id: &str) -> Request {
    Request::new(Call::new("work")).with_id(RequestId::parse(id).unwrap())
}

/// The one run a reader of its own sees in the store.
pub(super) fn the_run(root: &Path) -> RunId {
    let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    assert_eq!(generation.runs().len(), 1);
    RunId::parse(generation.runs().keys().next().unwrap()).unwrap()
}

pub(super) fn completed() -> RunState {
    RunState::Ended(Outcome::Completed {
        outputs: Vec::new(),
    })
}

pub(super) fn messages(watched: &Watched) -> Vec<&str> {
    watched.events().iter().map(|line| line.message()).collect()
}

// --- while an action is busy ------------------------------------------------------------------

#[test]
fn a_second_caller_reads_the_status_and_the_progress_of_a_run_whose_action_is_busy() {
    let dir = Scratch::new("watch");
    let root = dir.path().join("store");
    let store = Arc::new(Store::open(&root, Arc::new(ShimFs::new())).unwrap());
    let barriers = Barriers::new();
    let key = made_up_key("watch");
    let first = engine(store.clone(), vec![busy(&barriers, &key)])
        .with_secrets(Secrets::new([key.clone()]).unwrap());
    // The second caller has an executor of its own over the same store.
    let second = engine(store.clone(), Vec::new());
    whizbang_testkit::threads::scope(|scope| {
        let running = scope.spawn(|| first.request(&asked("order-1")));
        barriers.arrived("busy", 1);
        let run = the_run(&root);

        // The action is stopped inside its work. Its status is answered, with what it
        // has reported so far, the secret taken out; by either executor.
        for executor in [&first, &second] {
            let watched = executor.status(&run).unwrap();
            assert_eq!(watched.run(), &run);
            assert_eq!(watched.state(), &RunState::NotEnded { here: true });
            assert_eq!(messages(&watched), ["began with [redacted]"]);
            assert_eq!(watched.events()[0].seq(), 0);
        }

        // A caller that waits is still waiting while the action is stopped, and is
        // answered once it is over.
        let waiting = {
            let (second, run) = (&second, run.clone());
            scope.spawn(move || second.wait(&run))
        };
        assert!(!waiting.is_finished());
        barriers.release("busy");
        let Ok(Answer::Ran(finished)) = running.join().unwrap() else {
            panic!("the request did not run");
        };
        assert_eq!(finished.run(), &run);
        let waited = waiting.join().unwrap().unwrap();
        assert_eq!(waited.state(), &completed());
        // What it reported is in its journal now, and not in the answer.
        assert!(waited.events().is_empty());

        // Ended, the run is answered from its record, by anyone; its events are in
        // its journal, in the store.
        let watched = second.status(&run).unwrap();
        assert_eq!(watched.state(), &completed());
        assert!(watched.events().is_empty());
        assert_eq!(first.wait(&run).unwrap(), watched);
    });
}

#[test]
fn a_caller_that_starts_a_run_and_leaves_finds_it_again_by_its_id() {
    let dir = Scratch::new("watch");
    let root = dir.path().join("store");
    let store = Arc::new(Store::open(&root, Arc::new(ShimFs::new())).unwrap());
    let barriers = Barriers::new();
    let executor = engine(store.clone(), vec![busy(&barriers, "nothing")]);

    // The caller starts the run and is answered while the action is still at work:
    // the action has not passed its stop when the answer is here. Then it leaves,
    // with its executor.
    let run = {
        let caller = executor.clone();
        let Ok(Answer::Started(run)) = caller.start(&asked("order-2")) else {
            panic!("the request was not started");
        };
        run
    };
    barriers.arrived("busy", 1);
    assert_eq!(barriers.arrivals("busy"), 1);
    drop(executor);
    assert_eq!(the_run(&root), run);

    // Someone returns, with another executor over the store, and finds the run by its
    // id; and by the request's id, which is answered as a repeat and starts nothing.
    let returned = engine(store, Vec::new());
    let watched = returned.status(&run).unwrap();
    assert_eq!(watched.state(), &RunState::NotEnded { here: true });
    assert_eq!(messages(&watched), ["began with nothing"]);
    let Ok(Answer::Repeat(earlier)) = returned.start(&asked("order-2")) else {
        panic!("the request was not answered as a repeat");
    };
    assert_eq!(earlier.run(), &run);
    assert_eq!(earlier.state(), &RunState::NotEnded { here: true });

    barriers.release("busy");
    assert_eq!(returned.wait(&run).unwrap().state(), &completed());
    assert_eq!(barriers.arrivals("busy"), 1);
}

#[test]
fn a_started_call_that_is_refused_is_refused_and_starts_nothing() {
    let dir = Scratch::new("watch");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let executor = engine(store.clone(), Vec::new());
    assert!(matches!(
        executor.start(&asked("order-3")),
        Err(CallError::Refused(Refusal::UnknownAction { .. }))
    ));
    assert_eq!(store.snapshot().unwrap().number(), 0);
}

// --- a run nobody is working on ---------------------------------------------------------------

#[test]
fn a_run_the_store_does_not_hold_is_refused_by_status_and_by_wait() {
    let dir = Scratch::new("watch");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let executor = engine(store, Vec::new());
    let run = RunId::parse("9000-999999").unwrap();
    let refused = CallError::Refused(Refusal::NoSuchRun { run: run.clone() });
    assert_eq!(refused.to_string(), "the store holds no run 9000-999999");
    assert_eq!(executor.status(&run), Err(refused.clone()));
    assert_eq!(executor.wait(&run), Err(refused));
}

#[test]
fn waiting_for_a_run_that_no_process_will_end_returns_and_says_so() {
    let dir = Scratch::new("watch");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    // A run whose end the store refuses: the action has the file system refuse the
    // next call that changes the disk, the first of the publication that ends it.
    let stuck = {
        let fs = fs.clone();
        work(move |_, context| {
            context.events().progress("the last thing it said");
            fs.refuse_before(fs.events().len());
            Ok(Produced::nothing())
        })
    };
    let run = {
        let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
        let executor = engine(store, vec![stuck]);
        assert!(matches!(
            executor.request(&asked("order-4")),
            Err(CallError::Store(_))
        ));
        let run = the_run(&root);
        // In the process that admitted it, the run is still this process's, and was
        // abandoned, with the store's failure: a wait for it does not block.
        let waited = executor.wait(&run).unwrap();
        let abandoned = matches!(waited.state(), RunState::Abandoned { failure: Some(_) });
        assert!(abandoned, "{:?}", waited.state());
        // Its live state, and its events with it, were let go once it was abandoned: a
        // marker stands in its place (plan m6, task S1, item 3).
        assert!(messages(&waited).is_empty(), "{:?}", messages(&waited));
        assert_eq!(executor.status(&run).unwrap(), waited);
        // A poll that had seen its first event is answered from there, never before.
        let polled = executor.status_after(&run, 1).unwrap();
        assert_eq!((polled.next(), polled.skipped()), (1, 0));
        assert!(polled.events().is_empty());
        run
    };
    // That process is gone. The next one finds the run admitted and not ended, made
    // by nobody here: a wait returns at once, and says the run will not end.
    let store = Arc::new(Store::open(&root, Arc::new(ShimFs::new())).unwrap());
    let executor = engine(store, Vec::new());
    let waited = executor.wait(&run).unwrap();
    assert_eq!(waited.state(), &RunState::NotEnded { here: false });
    assert!(waited.events().is_empty());
    assert_eq!(executor.status(&run).unwrap(), waited);
    // An executor that only reads the store cannot tell, and says the same.
    drop(executor);
    let reader = Arc::new(Reader::open(&root, Arc::new(RealFs)).unwrap());
    let reading = Engine::new(
        Registry::new(),
        Arc::new(NoBackends),
        reader,
        Digest::of_bytes(b""),
    );
    assert_eq!(
        reading.wait(&run).unwrap().state(),
        &RunState::NotEnded { here: false }
    );
}

#[test]
fn a_run_this_writer_made_by_other_means_is_open_here_and_is_not_waited_for() {
    // A run that the store's writer made and has open, with nothing of an executor's
    // attached to it: other code of this process is working on it. It is here; there
    // are no events to show; and a wait cannot learn when it is over, so it returns.
    let dir = Scratch::new("watch");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let mut publication = store.begin().unwrap();
    publication.create_run("by-hand").unwrap();
    publication.publish().unwrap();
    let executor = engine(store, Vec::new());
    let run = RunId::parse("by-hand").unwrap();
    let watched = executor.status(&run).unwrap();
    assert_eq!(watched.state(), &RunState::NotEnded { here: true });
    assert!(watched.events().is_empty());
    assert_eq!(executor.wait(&run).unwrap(), watched);
}

// --- the two edges ----------------------------------------------------------------------------

#[test]
fn a_status_asked_the_moment_a_run_becomes_visible_says_it_is_executing_here() {
    // Stopped straight after the rename that makes the admission visible, before the
    // publication returns: the run is in the store, and its status is asked.
    let dir = Scratch::new("watch");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let executor = engine(store.clone(), vec![work(|_, _| Ok(Produced::nothing()))]);
    let seen = Arc::new(Mutex::new(None));
    fs.pause_after_rename_onto("CURRENT", {
        let (root, asking, seen) = (root.clone(), executor.clone(), seen.clone());
        move || {
            let run = the_run(&root);
            *seen.lock().unwrap() = Some(asking.status(&run).unwrap());
        }
    });
    let finished = executor.submit(&Call::new("work")).unwrap();
    let seen = seen.lock().unwrap().take().expect("the hook ran");
    assert_eq!(seen.run(), finished.run());
    assert_eq!(seen.state(), &RunState::NotEnded { here: true });
}

#[test]
fn a_status_asked_the_moment_a_runs_end_becomes_visible_says_it_ended() {
    // Stopped straight after the rename that makes the end visible, before the store
    // has taken the run out of its table of open runs: the status is the record's,
    // though the store still says it made the run and has it open.
    let dir = Scratch::new("watch");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let seen = Arc::new(Mutex::new(None));
    let asking: Arc<Mutex<Option<Engine>>> = Arc::new(Mutex::new(None));
    let arming = {
        let (fs, root, store, asking, seen) = (
            fs.clone(),
            root.clone(),
            store.clone(),
            asking.clone(),
            seen.clone(),
        );
        // The action arms the hook: the next pointer to be replaced is its run's end.
        work(move |_, _| {
            let (root, store, asking, seen) =
                (root.clone(), store.clone(), asking.clone(), seen.clone());
            fs.pause_after_rename_onto("CURRENT", move || {
                let run = the_run(&root);
                let open = store.made_here(run.as_str());
                let asking = asking.lock().unwrap().clone().unwrap();
                *seen.lock().unwrap() = Some((open, asking.status(&run).unwrap()));
            });
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![arming]);
    *asking.lock().unwrap() = Some(executor.clone());
    let finished = executor.submit(&Call::new("work")).unwrap();
    let (open, seen) = seen.lock().unwrap().take().expect("the hook ran");
    assert!(open, "the store had already forgotten the run");
    assert_eq!(seen.run(), finished.run());
    assert_eq!(seen.state(), &completed());
    assert!(!store.made_here(finished.run().as_str()));
}

/// The real file system, which runs a hook once, after the first read of a generation
/// file after it was armed: between a status's look at the store and its look at the
/// table of open runs.
pub(super) struct AfterTheLook {
    pub(super) hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Fs for AfterTheLook {
    fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        RealFs.create_dir(path)
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        RealFs.write_new(path, bytes)
    }
    fn sync_dir(&self, path: &Path) -> std::io::Result<()> {
        RealFs.sync_dir(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        RealFs.rename(from, to)
    }
    fn remove_all(&self, path: &Path) -> std::io::Result<()> {
        RealFs.remove_all(path)
    }
    fn read_at_most(&self, path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
        let read = RealFs.read_at_most(path, limit);
        if path
            .parent()
            .is_some_and(|dir| dir.ends_with("generations"))
        {
            let hook = self.hook.lock().unwrap().take();
            if let Some(hook) = hook {
                hook();
            }
        }
        read
    }
    fn list(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
        RealFs.list(path)
    }
    fn exists(&self, path: &Path) -> std::io::Result<bool> {
        RealFs.exists(path)
    }
    fn hold_lock_file(
        &self,
        path: &Path,
        create: bool,
    ) -> std::io::Result<Option<Box<dyn LockFile>>> {
        RealFs.hold_lock_file(path, create)
    }
    fn share(&self, path: &Path) -> std::io::Result<Box<dyn Held>> {
        RealFs.share(path)
    }
    fn try_alone(&self, path: &Path) -> std::io::Result<Option<Box<dyn Held>>> {
        RealFs.try_alone(path)
    }
}

#[test]
fn a_status_that_races_the_end_of_its_run_is_not_told_the_run_will_never_end() {
    // The status looks at the store while the run executes: admitted, no record.
    // Before it looks at the table of open runs, the run ends and leaves the table.
    // The run is in neither place as an open run of this process, and the answer must
    // not be that no process will end it: the second look at the store is the answer.
    let dir = Scratch::new("watch");
    let root = dir.path().join("store");
    let fs = Arc::new(AfterTheLook {
        hook: Mutex::new(None),
    });
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let barriers = Barriers::new();
    let first = engine(store.clone(), vec![busy(&barriers, "nothing")]);
    let second = engine(store.clone(), Vec::new());
    whizbang_testkit::threads::scope(|scope| {
        let running = scope.spawn(|| first.request(&asked("order-5")));
        barriers.arrived("busy", 1);
        let run = the_run(&root);
        *fs.hook.lock().unwrap() = Some(Box::new({
            let (barriers, first, run) = (barriers.clone(), first.clone(), run.clone());
            move || {
                barriers.release("busy");
                // The run is over, and out of the table, when this returns.
                first.wait(&run).unwrap();
            }
        }));
        let watched = second.status(&run).unwrap();
        assert!(fs.hook.lock().unwrap().is_none(), "the hook never ran");
        assert_eq!(watched.state(), &completed());
        running.join().unwrap().unwrap();
    });
}

#[test]
fn a_second_caller_reads_the_status_of_a_run_while_its_end_is_being_published() {
    // Stopped inside the publication that ends the run, at its last sync: the run's
    // end is visible and not yet confirmed. Another thread asks for the status of this
    // run, and of another run that is executing, and both are answered: nothing the
    // publication holds is needed to answer.
    let dir = Scratch::new("watch");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let barriers = Barriers::new();
    let other = engine(store.clone(), vec![busy(&barriers, "nothing")]);
    let Ok(Answer::Started(executing)) = other.start(&asked("order-6")) else {
        panic!("the request was not started");
    };
    barriers.arrived("busy", 1);

    let seen = Arc::new(Mutex::new(None));
    let asking = engine(store.clone(), Vec::new());
    let ending = {
        let (fs, root, seen, executing) =
            (fs.clone(), root.clone(), seen.clone(), executing.clone());
        work(move |_, _| {
            // The calls of the end publication are the next ones this thread makes.
            // Its last sync is of the store's root, after the pointer was replaced.
            let (root, seen, executing, asking) = (
                root.clone(),
                seen.clone(),
                executing.clone(),
                asking.clone(),
            );
            fs.pause_after_rename_onto("CURRENT", move || {
                let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
                let generation = reader.snapshot().unwrap();
                let this = generation
                    .runs()
                    .keys()
                    .find(|id| **id != executing.as_str());
                let this = RunId::parse(this.unwrap()).unwrap();
                let asked = whizbang_testkit::threads::spawn(move || {
                    (
                        asking.status(&this).unwrap(),
                        asking.status(&executing).unwrap(),
                    )
                });
                *seen.lock().unwrap() = Some(asked.join().unwrap());
            });
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![ending]);
    let finished = executor.submit(&Call::new("work")).unwrap();
    let (this, that) = seen.lock().unwrap().take().expect("the hook ran");
    assert_eq!(this.run(), finished.run());
    assert_eq!(this.state(), &completed());
    assert_eq!(that.state(), &RunState::NotEnded { here: true });
    barriers.release("busy");
    assert_eq!(other.wait(&executing).unwrap().state(), &completed());
}

// --- typed events and polls (plan m5, task K1) -----------------------------------------------

#[test]
fn a_second_caller_sees_a_step_started_before_the_parent_ends() {
    // A recipe says its steps as it goes, through its run's own sink, so another caller
    // reads them while the run executes, and not only from its journal at its end.
    use crate::recipe::{Fake, TWO_MARKS, mask_at, recipe};
    use crate::runner::{World, call, engine as recipe_engine};
    use whizbang_core::artifact::ArtifactType;
    use whizbang_core::contract::{Event, InputPort, OutputPort};

    let world = World::new("watch-steps");
    let seed = world.put(crate::artifact::support::rect_mask(
        crate::artifact::support::on("photo"),
    ));
    let barriers = Barriers::new();
    let hold: Arc<dyn Action> = {
        let barriers = barriers.clone();
        let descriptor = ActionDescriptor::draft("hold", "1.0.0", Kind::Primitive, "Holds.")
            .input(InputPort::required("mask", ArtifactType::Mask, "The mask."))
            .output(OutputPort::one("mask", ArtifactType::Mask, "The mask."))
            .build()
            .unwrap();
        Arc::new(Fake(
            descriptor,
            Box::new(move |call, context: &Context| {
                barriers.stop_at("hold");
                Ok(Produced::nothing().with("mask", mask_at(call, context, "mask").unwrap()))
            }),
        ))
    };
    let steps = format!(
        "{TWO_MARKS}\n[[step]]\nid = \"halt\"\nop = \"hold\"\ninputs = {{ mask = \"second.mask\" }}\n"
    );
    let executor = recipe_engine(&world, &recipe(&steps, "halt.mask"), vec![hold]);
    let Ok(Answer::Started(run)) = executor.start(&Request::new(call(seed))) else {
        panic!("the recipe was not started");
    };
    barriers.arrived("hold", 1);
    let second = Engine::over(
        crate::recipe::fakes(Vec::new()),
        Arc::new(NoBackends),
        world.store.clone(),
        Digest::of_bytes(b""),
    );
    let watched = second.status(&run).unwrap();
    assert_eq!(watched.state(), &RunState::NotEnded { here: true });
    let typed: Vec<&Event> = watched.events().iter().filter_map(|l| l.event()).collect();
    let kinds: Vec<String> = (typed.iter())
        .map(|event| match event {
            Event::StepStarted { step, .. } => format!("started {step}"),
            Event::StepEnded { step, outcome, .. } => format!("ended {step} {outcome}"),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "started first",
            "ended first completed",
            "started second",
            "ended second completed",
            "started halt",
        ]
    );
    assert_eq!(watched.next(), watched.events().len() as u64);
    assert_eq!(watched.skipped(), 0);
    barriers.release("hold");
    let ended = executor.wait(&run).unwrap();
    assert!(
        matches!(ended.state(), RunState::Ended(Outcome::Completed { .. })),
        "{ended:?}"
    );
}

/// `work` that says these progress messages, then this typed event, then stops at the
/// barrier `said`.
fn saying(barriers: &Barriers, messages: usize, typed: bool) -> Arc<dyn Action> {
    let barriers = barriers.clone();
    work(move |_, context| {
        for n in 0..messages {
            context.events().progress(&format!("message {n}"));
        }
        if typed {
            context
                .events()
                .event(&whizbang_core::contract::Event::AttemptStarted {
                    number: 1,
                    changed: None,
                });
        }
        barriers.stop_at("said");
        Ok(Produced::nothing())
    })
}

#[test]
fn status_after_a_sequence_hands_back_only_the_newer_events() {
    let dir = Scratch::new("watch-after");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let barriers = Barriers::new();
    let executor = engine(store, vec![saying(&barriers, 3, false)]);
    let Ok(Answer::Started(run)) = executor.start(&asked("order-after")) else {
        panic!("the request was not started");
    };
    barriers.arrived("said", 1);
    let whole = executor.status(&run).unwrap();
    assert_eq!(messages(&whole), ["message 0", "message 1", "message 2"]);
    assert_eq!((whole.skipped(), whole.next()), (0, 3));
    let newer = executor.status_after(&run, 2).unwrap();
    assert_eq!(messages(&newer), ["message 2"]);
    assert_eq!(newer.events()[0].seq(), 2);
    assert_eq!((newer.skipped(), newer.next()), (0, 3));
    // A caller that has seen everything is handed nothing; one ahead, nothing either.
    for seen in [3, 9] {
        let none = executor.status_after(&run, seen).unwrap();
        assert!(none.events().is_empty());
        assert_eq!((none.skipped(), none.next()), (0, 3));
    }
    // The answer of the trait's own form is the same, and an answer cut at a place
    // is never widened back past it.
    assert_eq!(whole.clone().after(2), newer);
    assert_eq!(newer.clone().after(0), newer);
    barriers.release("said");
    executor.wait(&run).unwrap();
}

#[test]
fn a_poll_across_events_not_kept_is_answered_with_the_count_skipped() {
    // The run says four more messages than it keeps, then a typed event, which is
    // kept: a poll from before the gap is answered with what was kept and how many
    // places in it were not.
    use whizbang_core::contract::{EVENTS_KEPT_LIMIT, MESSAGES_KEPT_MAX_BYTES};
    // Held to eight messages in place of the real bound, so a few make the gap.
    let kept = 8;
    let dir = Scratch::new("watch-skipped");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let barriers = Barriers::new();
    let executor = engine(store, vec![saying(&barriers, kept + 4, true)]).with_journal_bounds(
        kept,
        MESSAGES_KEPT_MAX_BYTES,
        EVENTS_KEPT_LIMIT,
    );
    let Ok(Answer::Started(run)) = executor.start(&asked("order-skipped")) else {
        panic!("the request was not started");
    };
    barriers.arrived("said", 1);
    let limit = kept as u64;
    let polled = executor.status_after(&run, limit - 2).unwrap();
    let places: Vec<u64> = polled.events().iter().map(|line| line.seq()).collect();
    assert_eq!(places, [limit - 2, limit - 1, limit + 4]);
    assert_eq!((polled.skipped(), polled.next()), (4, limit + 5));
    assert!(polled.events()[2].event().is_some());
    // Within the gap alone: nothing kept, and every place skipped.
    let gap = executor.status_after(&run, limit).unwrap();
    assert_eq!(gap.events().len(), 1);
    assert_eq!(gap.skipped(), 4);
    barriers.release("said");
    executor.wait(&run).unwrap();
}
