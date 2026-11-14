//! A part in the admission (ADR 0015): the executor asks each of its parts before
//! each attempt at an admission's publication. A part may refuse the call, and it
//! may add records among the run's names, which are published with the run or not
//! at all. And the two things a part reads that are the executor's: the cap a
//! request came with, and the time a preparation was made.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::contract::record::{self, Status};
use whizbang_core::contract::{
    ADMITTED_NAME, Action, ActionDescriptor, AdmissionPart, Answer, Argument, Asking, Backends,
    Call, CallError, Capability, Clock, Context, Engine, Estimate, Executor, Failure, Kind,
    ModelIdentity, Outcome, PART_RECORD_MAX_BYTES, PART_RECORDS_LIMIT, PART_RECORDS_PREFIX,
    PartDeclined, PartRefusal, PartStep, PreparedCall, Produced, REFUSAL_WHY_LIMIT, Refusal,
    RefusalKind, Registry, Request, RequestId, Requirement, RunState, Secrets, Unavailable, Value,
    ValueType, part_record_name, request_fingerprint,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::Fs;
use whizbang_core::store::{self, Reader, Store};
use whizbang_testkit::fs_shim::{Lost, ShimFs};
use whizbang_testkit::ledger::FakeLedger;
use whizbang_testkit::part::{FakePart, Seen};
use whizbang_testkit::scratch::Scratch;

use super::execute::Taker;
use super::redact::made_up_key;

// --- what the tests are made of ----------------------------------------------------------------

/// A clock a test moves by hand.
struct Hand(AtomicU64);

impl Hand {
    fn at(ms: u64) -> Arc<Hand> {
        Arc::new(Hand(AtomicU64::new(ms)))
    }

    fn set(&self, ms: u64) {
        self.0.store(ms, Ordering::SeqCst);
    }
}

impl Clock for Hand {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// A clock that says one millisecond more each time it is read.
struct Ticking(AtomicU64);

impl Clock for Ticking {
    fn now_ms(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

/// Backends with one model, whose every request may cost a quarter of a dollar.
struct Priced;

impl Backends for Priced {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Ok(ModelIdentity::new("fake", "fake-filler", "1"))
    }

    fn bound(&self, _: Capability, _: &ModelIdentity) -> Estimate {
        Estimate::of_micros(250_000)
    }
}

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

struct Scripted {
    descriptor: ActionDescriptor,
    runs: Arc<AtomicU64>,
    body: Body,
}

impl Action for Scripted {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        (self.body)(call, context)
    }
}

/// An action of this name that takes a text and needs a filler, counts its runs, and
/// does what the test says.
fn action(
    name: &str,
    runs: &Arc<AtomicU64>,
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let descriptor = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::optional("text", "A text.", ValueType::Text))
        .needs(Requirement::new(Capability::Fill))
        .build()
        .unwrap();
    Arc::new(Scripted {
        descriptor,
        runs: runs.clone(),
        body: Box::new(body),
    })
}

fn call(text: &str) -> Call {
    Call::new("paint").argument("text", Value::text(text))
}

fn id(text: &str) -> RequestId {
    RequestId::parse(text).unwrap()
}

fn asked(text: &str, request: &str) -> Request {
    Request::new(call(text)).with_id(id(request))
}

struct World {
    dir: Scratch,
    fs: Arc<ShimFs>,
    runs: Arc<AtomicU64>,
}

impl World {
    fn new() -> World {
        World::on(ShimFs::new())
    }

    fn on(fs: ShimFs) -> World {
        World {
            dir: Scratch::new("part"),
            fs: Arc::new(fs),
            runs: Arc::new(AtomicU64::new(0)),
        }
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("store")
    }

    fn store(&self) -> Arc<Store> {
        Arc::new(Store::open(&self.root(), self.fs.clone()).unwrap())
    }

    fn ran(&self) -> u64 {
        self.runs.load(Ordering::SeqCst)
    }

    /// An executor over this store with `paint`, which does nothing, one model at a
    /// quarter of a dollar, and a clock that says 7000.
    fn engine(&self, store: Arc<Store>) -> Engine {
        engine(
            store,
            vec![action("paint", &self.runs, |_, _| Ok(Produced::nothing()))],
        )
    }
}

fn engine(store: Arc<Store>, actions: Vec<Arc<dyn Action>>) -> Engine {
    let mut registry = Registry::new();
    for action in actions {
        registry.register(action).unwrap();
    }
    Engine::over(registry, Arc::new(Priced), store, Digest::of_bytes(b""))
        .with_clock(Hand::at(7000))
}

/// What a store holds: its generation's number, each name outside runs, and each run
/// with its names and their content.
type Held = (u64, Vec<String>, Vec<(String, Vec<(String, Vec<u8>)>)>);

fn held(root: &Path, fs: Arc<dyn Fs>) -> Held {
    let reader = Reader::open(root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    let runs = generation.runs().iter().map(|(id, run)| {
        let names = run.entries().iter().map(|(name, entry)| {
            assert_eq!(entry.kind, store::Kind::Diagnostic, "{name}");
            (name.clone(), reader.read(entry).unwrap())
        });
        (id.clone(), names.collect())
    });
    (
        generation.number(),
        generation.entries().keys().cloned().collect(),
        runs.collect(),
    )
}

fn files(root: &Path) -> usize {
    let mut found = 0;
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                found += 1;
            }
        }
    }
    found
}

fn ran(answer: Result<Answer, CallError>) -> whizbang_core::contract::Finished {
    match answer {
        Ok(Answer::Ran(finished)) => finished,
        other => panic!("the request did not run: {other:?}"),
    }
}

fn by_part(answer: Result<impl std::fmt::Debug, CallError>) -> (&'static str, RefusalKind, String) {
    match answer {
        Err(CallError::Refused(Refusal::ByPart {
            part, kind, why, ..
        })) => (part, kind, why),
        other => panic!("not refused by a part: {other:?}"),
    }
}

const NOTE: &str = "admission/note";

/// A part that adds the record `note` to every admission.
fn noting() -> Arc<FakePart> {
    let part = Arc::new(FakePart::new("notes"));
    part.add("note", b"a note\n");
    part
}

// --- a part's record -----------------------------------------------------------------------------

/// The number of calls that change the disk during one admission with a part that
/// adds a record: from the request to the moment the action starts.
fn steps_of_an_admission() -> usize {
    let world = World::new();
    let store = world.store();
    let opened = world.fs.events().len();
    let started = Arc::new(AtomicU64::new(0));
    let noticing = {
        let (fs, started) = (world.fs.clone(), started.clone());
        action("paint", &world.runs, move |_, _| {
            started.store(fs.events().len() as u64, Ordering::SeqCst);
            Ok(Produced::nothing())
        })
    };
    let engine = engine(store, vec![noticing]).with_part(noting());
    ran(engine.request(&asked("hello", "order-17")));
    started.load(Ordering::SeqCst) as usize - opened
}

#[test]
fn a_parts_record_is_in_the_store_with_the_run_or_not_at_all_whichever_call_of_the_publication_is_refused()
 {
    let steps = steps_of_an_admission();
    assert!(steps > 8, "{steps}");
    let mut seen = BTreeSet::new();
    for step in 0..steps {
        let world = World::new();
        let store = world.store();
        let part = noting();
        let engine = world.engine(store).with_part(part.clone());
        world.fs.refuse_before(world.fs.events().len() + step);
        let answer = engine.request(&asked("hello", "order-17"));
        // A step the store fails at before the publication is begun stops the
        // admission before any part is asked.
        assert!(part.asked() <= 1, "step {step}");
        drop(engine);
        let (_, names, runs) = held(&world.root(), world.fs.clone());
        match runs.as_slice() {
            [] => {
                // Nothing of the admission: no run, no id's record, no record of
                // the part, and the action did not run.
                assert!(matches!(answer, Err(CallError::Store(_))), "step {step}");
                assert_eq!(names, [] as [&str; 0], "step {step}");
                assert_eq!(world.ran(), 0, "step {step}");
                seen.insert("nothing");
            }
            [(_, of_run)] => {
                // The run, and with it the part's record, whole, whatever became
                // of the run since.
                assert_eq!(names[0], "requests/order-17/admitted", "step {step}");
                let get = |name: &str| of_run.iter().find(|(held, _)| held == name);
                assert!(get(ADMITTED_NAME).is_some(), "step {step}");
                let note = get(NOTE).unwrap_or_else(|| panic!("step {step}: {of_run:?}"));
                assert_eq!(note.1, b"a note\n", "step {step}");
                seen.insert("all of it");
            }
            other => panic!("step {step}: {other:?}"),
        }
    }
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        ["all of it", "nothing"]
    );
}

/// Power is lost before each step of an admission with a part's record, with every way
/// the test kit's disk can lose what was not yet durable. What the next open finds is
/// the run with the part's record, or neither.
fn crashes_inside_an_admission(half: usize) {
    let steps = steps_of_an_admission();
    let opened = {
        let world = World::new();
        let _store = world.store();
        world.fs.events().len()
    };
    let mut outcomes = BTreeSet::new();
    let mut crashes = 0;
    for step in (0..=steps).filter(|step| step % 2 == half) {
        let mut ways = vec![Lost::Unsynced, Lost::ContentOnly, Lost::Subset(0)];
        let mut next = 0;
        while next < ways.len() {
            let lost = ways[next];
            next += 1;
            crashes += 1;
            let world = World::on(ShimFs::new().crash_before(opened + step, lost));
            let engine = world.engine(world.store()).with_part(noting());
            let answer = engine.request(&asked("hello", "order-17"));
            assert!(
                matches!(answer, Err(CallError::Store(_))),
                "step {step}: {answer:?}"
            );
            if lost == Lost::Subset(0) {
                let (dirs, names) = (world.fs.crash_choices(), world.fs.crash_names());
                assert!(dirs <= 10 && names <= 10, "step {step}: {dirs} {names}");
                ways.extend((1..1u64 << dirs).map(Lost::Subset));
                ways.extend((0..1u64 << names).map(Lost::Names));
            }
            drop(engine);
            let fs: Arc<dyn Fs> = Arc::new(ShimFs::new());
            let _store = Store::open(&world.root(), fs.clone()).unwrap();
            let (_, names, runs) = held(&world.root(), fs);
            let at = format!("step {step}, {lost:?}");
            match (names.as_slice(), runs.as_slice()) {
                ([], []) => {
                    outcomes.insert("nothing");
                }
                ([name], [(_, of_run)]) => {
                    assert_eq!(name, "requests/order-17/admitted", "{at}");
                    let names: Vec<&str> = of_run.iter().map(|(name, _)| name.as_str()).collect();
                    assert_eq!(names, [NOTE, ADMITTED_NAME], "{at}");
                    assert_eq!(of_run[0].1, b"a note\n", "{at}");
                    outcomes.insert("all of it");
                }
                other => panic!("{at}: a part of an admission: {other:?}"),
            }
        }
    }
    assert!(outcomes.contains("nothing"), "{outcomes:?}");
    assert!(
        steps % 2 != half || outcomes.contains("all of it"),
        "{outcomes:?}"
    );
    assert!(crashes > 100, "{crashes}");
}

#[test]
fn admitted_with_a_part_a_crash_at_any_event_leaves_the_parts_record_with_its_run_or_neither_even_steps()
 {
    crashes_inside_an_admission(0);
}

#[test]
fn admitted_with_a_part_a_crash_at_any_event_leaves_the_parts_record_with_its_run_or_neither_odd_steps()
 {
    crashes_inside_an_admission(1);
}

#[test]
fn a_parts_record_is_among_the_names_of_the_run_that_was_made_and_goes_with_it() {
    let world = World::new();
    let store = world.store();
    let engine = world.engine(store.clone()).with_part(noting());
    let finished = ran(engine.request(&asked("hello", "order-17")));
    let generation = store.snapshot().unwrap();
    let run = generation.run(finished.run().as_str()).unwrap();
    assert_eq!(part_record_name("note").as_deref(), Some(NOTE));
    assert!(NOTE.starts_with(PART_RECORDS_PREFIX));
    let entry = run.get(NOTE).unwrap();
    // It is readable, and never an output.
    assert_eq!(entry.kind, store::Kind::Diagnostic);
    assert_eq!(store.read(entry).unwrap(), b"a note\n");
    // It is no name outside runs, and it stays when the run ends.
    assert!(generation.get(NOTE).is_none());
    assert!(run.get("record").is_some());
    // The run's record lists it nowhere: it is the part's, not a diagnostic.
    let stored = record::stored(&generation, finished.run().as_str(), |e| store.read(e));
    let stored = stored.unwrap().unwrap();
    assert_eq!(stored.outcome().status, Status::Completed);
    assert!(
        !String::from_utf8(stored.to_bytes())
            .unwrap()
            .contains("note")
    );
    // Deleting the run removes it with the run's other names.
    store.delete(&[finished.run().as_str()]).unwrap();
    assert!(
        store
            .snapshot()
            .unwrap()
            .run(finished.run().as_str())
            .is_none()
    );
}

#[test]
fn a_part_is_asked_before_each_attempt_and_its_record_is_of_the_run_that_was_made() {
    // The id the executor chose for the run is taken three times, each time as its
    // admission is about to be written: four attempts, and the part is asked for
    // each, with the run that attempt would make.
    let dir = Scratch::new("part-renamed");
    let root = dir.path().join("store");
    let (fs, store) = Taker::store(&root, 3);
    let runs = Arc::new(AtomicU64::new(0));
    let part = noting();
    let ledger = Arc::new(FakeLedger::new());
    let paint = action("paint", &runs, |_, _| Ok(Produced::nothing()));
    let engine = engine(store.clone(), vec![paint])
        .with_clock(Arc::new(Ticking(AtomicU64::new(7000))))
        .with_ledger(ledger.clone())
        .with_part(part.clone());
    let finished = ran(engine.request(&asked("hello", "order-17")));
    let taken = fs.taken();
    assert_eq!(taken.len(), 3);
    // The time it is shown is the admission's, read once: the same for each attempt,
    // though the clock moves at every reading.
    let times: Vec<(u64, u64)> = part
        .seen()
        .iter()
        .map(|seen| (seen.now_ms, seen.prepared_made_ms))
        .collect();
    assert_eq!(times, [(7000, 7000); 4]);
    let seen: Vec<String> = part
        .seen()
        .iter()
        .map(|seen| seen.run.to_string())
        .collect();
    assert_eq!(seen.len(), 4, "{seen:?}");
    assert_eq!(seen[..3], taken[..]);
    assert_eq!(seen[3], finished.run().as_str());
    assert_eq!(ledger.asked(), 4);
    // The record is among the names of the run that was made, and of no other run.
    let generation = store.snapshot().unwrap();
    for (run, names) in generation.runs() {
        let has = names.get(NOTE).is_some();
        assert_eq!(has, run == finished.run().as_str(), "{run}");
    }
}

// --- a part that refuses -------------------------------------------------------------------------

#[test]
fn a_part_that_refuses_leaves_no_run_no_reservation_and_the_requests_id_free() {
    let world = World::new();
    let store = world.store();
    let ledger = Arc::new(FakeLedger::new());
    let part = noting();
    part.refuse(RefusalKind::OverAllowance, "it may cost more than its cap");
    let engine = world
        .engine(store.clone())
        .with_ledger(ledger.clone())
        .with_part(part.clone());
    let before = (held(&world.root(), world.fs.clone()), files(&world.root()));
    for request in [asked("hello", "order-17"), Request::new(call("hello"))] {
        let refused = engine.request(&request).unwrap_err();
        assert_eq!(
            refused,
            CallError::Refused(Refusal::ByPart {
                action: "paint".to_owned(),
                part: "notes",
                kind: RefusalKind::OverAllowance,
                why: "it may cost more than its cap".to_owned(),
            })
        );
        // Its kind is the one the part chose, and its sentence names the part.
        assert_eq!(
            refused.refusal().unwrap().kind(),
            RefusalKind::OverAllowance
        );
        assert_eq!(
            refused.to_string(),
            "paint: refused at admission, by notes: it may cost more than its cap"
        );
        // No run, no name, no file of the attempt, no reservation: the ledger was
        // not asked, and the action did not run.
        let after = (held(&world.root(), world.fs.clone()), files(&world.root()));
        assert_eq!(after, before);
        assert_eq!(ledger.asked(), 0);
        assert_eq!(ledger.reservations(store.as_ref()).unwrap(), []);
        assert_eq!(world.ran(), 0);
    }
    assert!(matches!(
        engine.submit(&call("hello")),
        Err(CallError::Refused(Refusal::ByPart { .. }))
    ));
    assert!(matches!(
        engine.start(&asked("hello", "order-17")),
        Err(CallError::Refused(Refusal::ByPart { .. }))
    ));
    assert_eq!(part.asked(), 4);
    // The id is free: once the part lets the request through, it is admitted under
    // the same id, and runs.
    part.allow();
    let finished = ran(engine.request(&asked("hello", "order-17")));
    assert_eq!(finished.outcome(), &Outcome::Completed { outputs: vec![] });
    assert_eq!(world.ran(), 1);
    assert_eq!(ledger.reservations(store.as_ref()).unwrap().len(), 1);
}

#[test]
fn parts_are_asked_in_the_order_they_were_given_and_the_first_refusal_is_the_answer() {
    let world = World::new();
    let store = world.store();
    let parts = ["first", "second", "third"].map(|name| Arc::new(FakePart::new(name)));
    let mut engine = world.engine(store);
    for part in &parts {
        engine = engine.with_part(part.clone());
    }
    let counts = || parts.clone().map(|part| part.asked());
    parts[1].refuse(RefusalKind::Stale, "the second says no");
    parts[2].refuse(RefusalKind::Invalid, "the third says no");
    let refused = by_part(engine.request(&asked("hello", "order-17")));
    let second = "the second says no".to_owned();
    assert_eq!(refused, ("second", RefusalKind::Stale, second));
    // The part after the one that refused was not asked.
    assert_eq!(counts(), [1, 1, 0]);
    parts[1].allow();
    let refused = by_part(engine.request(&asked("hello", "order-17")));
    let third = "the third says no".to_owned();
    assert_eq!(refused, ("third", RefusalKind::Invalid, third));
    assert_eq!(counts(), [2, 2, 1]);
    // All three let it through: each is asked, in the order they were given.
    parts[2].allow();
    let order = Arc::new(Mutex::new(Vec::new()));
    for part in &parts {
        let (order, name) = (order.clone(), part.name());
        part.when_asked(move || order.lock().unwrap().push(name));
    }
    ran(engine.request(&asked("hello", "order-17")));
    assert_eq!(*order.lock().unwrap(), ["first", "second", "third"]);
    assert_eq!(counts(), [3, 3, 2]);
    assert_eq!(world.ran(), 1);
}

#[test]
fn a_refusal_by_a_part_is_a_refusal_after_the_look_so_the_id_is_looked_up_once_more() {
    // A request looks its id up and finds nothing. While a part looks at it, a
    // request under the same id is admitted by another executor over the store.
    // The part then refuses. What the caller is told is what the id says now.
    let race = |text: &'static str| {
        let world = World::new();
        let store = world.store();
        let winner = world.engine(store.clone());
        let part = Arc::new(FakePart::new("gate"));
        part.refuse(RefusalKind::Stale, "no");
        let first = Arc::new(Mutex::new(None));
        part.when_asked({
            let first = first.clone();
            move || *first.lock().unwrap() = Some(winner.request(&asked("hello", "order-17")))
        });
        let loser = world.engine(store).with_part(part);
        let answer = loser.request(&asked(text, "order-17"));
        let first = ran(first.lock().unwrap().take().unwrap());
        assert_eq!(world.ran(), 1);
        // An id that nobody took leaves the refusal as it is.
        assert_eq!(by_part(loser.request(&asked(text, "order-18"))).0, "gate");
        (answer, first)
    };
    let (answer, first) = race("hello");
    let Ok(Answer::Repeat(earlier)) = answer else {
        panic!("not answered as a repeat: {answer:?}");
    };
    assert_eq!(earlier.run(), first.run());
    let completed = Outcome::Completed { outputs: vec![] };
    assert_eq!(earlier.state(), &RunState::Ended(completed));
    let (answer, first) = race("goodbye");
    assert_eq!(
        answer.unwrap_err(),
        CallError::Refused(Refusal::Conflict {
            request: id("order-17"),
            run: first.run().clone(),
        })
    );
}

#[test]
fn a_call_from_inside_an_action_is_shown_to_the_parts_and_its_refusal_is_in_its_callers_record() {
    let world = World::new();
    let store = world.store();
    let part = Arc::new(FakePart::new("cap"));
    type Said = (Option<u64>, Result<(), CallError>, Option<u64>);
    let said: Arc<Mutex<Vec<Said>>> = Arc::default();
    let outer = {
        let (part, said) = (part.clone(), said.clone());
        action("outer", &world.runs, move |_, context| {
            // From here on the part refuses: the next call it sees is the one below.
            part.refuse(RefusalKind::OverAllowance, "it may cost more than its cap");
            let before = context.allowance().left_micros();
            let answer = context.executor().submit(&Call::new("inner")).map(drop);
            let after = context.allowance().left_micros();
            said.lock().unwrap().push((before, answer, after));
            Ok(Produced::nothing())
        })
    };
    let ran_inside = Arc::new(AtomicU64::new(0));
    let inner = action("inner", &ran_inside, |_, _| Ok(Produced::nothing()));
    let engine = engine(store.clone(), vec![outer, inner])
        .with_ledger(Arc::new(FakeLedger::new()))
        .with_part(part.clone());
    let finished = engine.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome(), &Outcome::Completed { outputs: vec![] });
    let refusal = Refusal::ByPart {
        action: "inner".to_owned(),
        part: "cap",
        kind: RefusalKind::OverAllowance,
        why: "it may cost more than its cap".to_owned(),
    };
    // The call was refused, and what was taken from its caller's allowance for it
    // was given back.
    let quarter = Some(250_000);
    assert_eq!(
        *said.lock().unwrap(),
        [(quarter, Err(CallError::Refused(refusal)), quarter)]
    );
    assert_eq!(ran_inside.load(Ordering::SeqCst), 0);
    // The part saw both calls: the one nothing called, and the one its run made.
    let seen = part.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!((seen[0].action.as_str(), &seen[0].parent), ("outer", &None));
    let parent = Some(finished.run().clone());
    assert_eq!(
        (seen[1].action.as_str(), &seen[1].parent),
        ("inner", &parent)
    );
    // One run, and its record lists the refusal under the kind the part chose.
    let generation = store.snapshot().unwrap();
    assert_eq!(generation.runs().len(), 1);
    let stored = record::stored(&generation, finished.run().as_str(), |e| store.read(e));
    let stored = stored.unwrap().unwrap();
    let refusals = &stored.outcome().refusals;
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].kind(), RefusalKind::OverAllowance);
    assert_eq!(refusals[0].action(), Some("inner"));
    assert_eq!(
        refusals[0].why(),
        "inner: refused at admission, by cap: it may cost more than its cap"
    );
    assert!(stored.steps().is_empty());
}

#[test]
fn a_parts_refusal_is_of_a_kind_of_a_call_and_its_sentence_is_redacted_and_then_cut() {
    // A part refuses as a kind a record has for a call, and as no other.
    for kind in RefusalKind::ALL {
        let made = PartRefusal::new(kind, "no");
        assert_eq!(made.is_some(), !kind.is_of_a_request(), "{kind:?}");
    }
    assert_eq!(PartRefusal::new(RefusalKind::Stale, ""), None);
    let refusal = PartRefusal::new(RefusalKind::Stale, "too old").unwrap();
    assert_eq!(
        (refusal.kind(), refusal.why()),
        (RefusalKind::Stale, "too old")
    );

    // Its sentence has the executor's secrets taken out while it is whole, and is
    // then cut: a secret that straddles the cut is not left in part.
    let key = made_up_key("");
    let world = World::new();
    let part = Arc::new(FakePart::new("gate"));
    let engine = world
        .engine(world.store())
        .with_secrets(Secrets::new([key.clone()]).unwrap())
        .with_part(part.clone());
    part.refuse(RefusalKind::Invalid, &format!("the key {key} is not one"));
    let refused = engine.request(&asked("hello", "order-17"));
    let shown = format!("{refused:?} {}", refused.as_ref().unwrap_err());
    assert!(!shown.contains(&key), "{shown}");
    let (_, _, why) = by_part(refused);
    assert_eq!(why, "the key [redacted] is not one");
    let straddling = format!("{}{key}", "x".repeat(REFUSAL_WHY_LIMIT - 4));
    part.refuse(RefusalKind::Invalid, &straddling);
    let (_, _, why) = by_part(engine.request(&asked("hello", "order-17")));
    assert!(why.chars().count() <= REFUSAL_WHY_LIMIT, "{}", why.len());
    assert!(!why.contains(&key[..8]), "{why}");
    assert_eq!(world.ran(), 0);
}

// --- what a part may not add ---------------------------------------------------------------------

/// Records for a part to add: each a name and a content.
type Records = Vec<(String, Vec<u8>)>;

/// A part that adds what the test says, each time it is asked.
struct Adding(Records);

impl AdmissionPart for Adding {
    fn name(&self) -> &'static str {
        "adding"
    }

    fn look(&self, _: &Asking<'_>, step: &mut PartStep<'_, '_>) -> Result<(), PartDeclined> {
        for (name, bytes) in &self.0 {
            step.put(name, bytes)?;
        }
        Ok(())
    }
}

#[test]
fn a_parts_record_that_holds_a_secret_is_refused_and_not_shown() {
    let key = made_up_key("");
    let one = |name: &str, bytes: &[u8]| vec![(name.to_owned(), bytes.to_vec())];
    let inside = format!("{{\"token\":\"{key}\"}}\n");
    let many = |count: usize| -> Records {
        (0..count)
            .map(|n| (format!("n{n}"), b"x".to_vec()))
            .collect()
    };
    let cases: Vec<(&str, Records, &str)> = vec![
        (
            "the content is a secret",
            one("note", key.as_bytes()),
            "must not be written",
        ),
        (
            "the content holds one",
            one("note", inside.as_bytes()),
            "must not be written",
        ),
        (
            "the name holds one",
            one(&key.to_lowercase(), b"x"),
            "must not be written",
        ),
        // A name that is none and holds a secret is refused for the secret: nothing
        // of it is shown.
        (
            "a name that is none holds one",
            one(&format!("A {key}"), b"x"),
            "must not be written",
        ),
        ("a capital", one("Note", b"x"), "is no name for"),
        ("a path", one("a/b", b"x"), "is no name for"),
        ("no name", one("", b"x"), "is no name for"),
        ("an escape", one("\u{1b}[31m", b"x"), "is no name for"),
        (
            "too long a name",
            one(&"n".repeat(65), b"x"),
            "is no name for",
        ),
        (
            "too large",
            one("note", &vec![b'x'; PART_RECORD_MAX_BYTES + 1]),
            "bytes",
        ),
        (
            "twice",
            [one("note", b"x"), one("note", b"y")].concat(),
            "added twice",
        ),
        ("too many", many(PART_RECORDS_LIMIT + 1), "one more than"),
    ];
    for (case, adds, says) in cases {
        let world = World::new();
        let store = world.store();
        let secrets = Secrets::new([key.clone(), key.to_lowercase()]).unwrap();
        let engine = world
            .engine(store.clone())
            .with_secrets(secrets)
            .with_part(Arc::new(Adding(adds)));
        let refused = engine.request(&asked("hello", "order-17")).unwrap_err();
        let CallError::Part { part, why } = &refused else {
            panic!("{case}: {refused:?}");
        };
        assert_eq!(*part, "adding", "{case}");
        assert!(why.contains(says), "{case}: {why}");
        assert_eq!(refused.refusal(), None, "{case}");
        let said = format!("{refused} {refused:?}");
        assert!(
            said.starts_with("a part of the admission failed (adding): "),
            "{said}"
        );
        assert!(!said.to_lowercase().contains(&key.to_lowercase()), "{case}");
        assert!(!said.chars().any(char::is_control), "{case}: {said:?}");
        assert!(said.len() < 1200, "{case}: {}", said.len());
        // Nothing was published, and the action did not run.
        assert_eq!(
            held(&world.root(), world.fs.clone()),
            (0, vec![], vec![]),
            "{case}"
        );
        assert_eq!(world.ran(), 0, "{case}");
        // No byte the attempt wrote holds the secret.
        let mut dirs = vec![world.root()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                let bytes = std::fs::read(&path).unwrap();
                let found = bytes
                    .windows(key.len())
                    .any(|window| window == key.as_bytes());
                assert!(!found, "{case}");
            }
        }
    }
    // As many as a part may add, each as large as one may be, are published.
    let world = World::new();
    let store = world.store();
    let full: Records = (0..PART_RECORDS_LIMIT)
        .map(|n| (format!("n{n}"), vec![b'x'; PART_RECORD_MAX_BYTES]))
        .collect();
    let engine = world
        .engine(store.clone())
        .with_part(Arc::new(Adding(full)));
    let finished = ran(engine.request(&asked("hello", "order-17")));
    let generation = store.snapshot().unwrap();
    let names = generation.run(finished.run().as_str()).unwrap().entries();
    let added = names
        .keys()
        .filter(|name| name.starts_with(PART_RECORDS_PREFIX));
    assert_eq!(added.count(), PART_RECORDS_LIMIT);
}

#[test]
fn two_parts_share_the_bound_on_records_and_cannot_give_one_name_twice() {
    let world = World::new();
    let store = world.store();
    let (first, second) = (noting(), noting());
    let engine = world
        .engine(store.clone())
        .with_part(first.clone())
        .with_part(second.clone());
    let refused = engine.request(&asked("hello", "order-17")).unwrap_err();
    let CallError::Part { part, why } = &refused else {
        panic!("{refused:?}");
    };
    assert_eq!(*part, "notes");
    assert!(why.contains("added twice"), "{why}");
    assert_eq!((first.asked(), second.asked()), (1, 1));
    assert_eq!(store.snapshot().unwrap().number(), 0);
    assert_eq!(world.ran(), 0);
}

#[test]
fn a_part_that_fails_is_not_a_refusal_of_the_call_and_nothing_is_published() {
    let key = made_up_key("");
    let world = World::new();
    let store = world.store();
    let part = noting();
    part.fail(&format!("its books are closed, said {key}"));
    let engine = world
        .engine(store.clone())
        .with_secrets(Secrets::new([key.clone()]).unwrap())
        .with_ledger(Arc::new(FakeLedger::new()))
        .with_part(part.clone());
    let failed = engine.request(&asked("hello", "order-17")).unwrap_err();
    assert_eq!(
        failed,
        CallError::Part {
            part: "notes",
            why: "its books are closed, said [redacted]".to_owned(),
        }
    );
    assert_eq!(failed.refusal(), None);
    assert_eq!(
        failed.to_string(),
        "a part of the admission failed (notes): its books are closed, said [redacted]"
    );
    assert_eq!(held(&world.root(), world.fs.clone()), (0, vec![], vec![]));
    assert_eq!(world.ran(), 0);
    // Put right, the same request is admitted under its id.
    part.allow();
    ran(engine.request(&asked("hello", "order-17")));
}

// --- what a part reads that is the executor's ----------------------------------------------------

#[test]
fn a_requests_cap_and_a_preparations_time_reach_a_part_as_they_were_given() {
    let world = World::new();
    let store = world.store();
    let clock = Hand::at(1_000);
    let part = Arc::new(FakePart::new("watch"));
    let engine = world
        .engine(store.clone())
        .with_clock(clock.clone())
        .with_part(part.clone());

    // A preparation carries the time it was made, by the executor's clock, and the
    // time is no part of what it is: the same call prepared later has the same
    // fingerprint.
    let prepared = engine.prepare(&call("hello")).unwrap();
    assert_eq!(prepared.made_ms(), 1_000);
    clock.set(5_000);
    let later = engine.prepare(&call("hello")).unwrap();
    assert_eq!(later.made_ms(), 5_000);
    assert_eq!(later.fingerprint(), prepared.fingerprint());
    // Nor of whether two preparations are the same: they saw the same.
    assert_eq!(later, prepared);
    assert_eq!(part.asked(), 0);

    // A request carries a cap of its own, which is no part of its fingerprint.
    clock.set(9_000);
    let cap = Estimate::of_micros(123);
    let request = asked("hello", "order-17")
        .expecting(&prepared)
        .capped_at(cap);
    assert_eq!(request.cap(), Some(cap));
    assert_eq!(request.fingerprint(), request_fingerprint(&call("hello")));
    assert_eq!(asked("hello", "order-17").cap(), None);

    // A preparation made at another time is not stale for that: the request is
    // admitted, and the part was shown the cap, both times and the time of the
    // admission, as they were given.
    let finished = ran(engine.request(&request));
    assert_eq!(
        part.seen(),
        [Seen {
            action: "paint".to_owned(),
            request: Some(id("order-17")),
            cap: Some(cap),
            expected_made_ms: Some(1_000),
            prepared_made_ms: 9_000,
            prepared: prepared.fingerprint(),
            now_ms: 9_000,
            run: finished.run().clone(),
            parent: None,
        }]
    );
    // The time a part is shown is the one the admission says the run started at.
    let generation = store.snapshot().unwrap();
    let stored = record::stored(&generation, finished.run().as_str(), |e| store.read(e));
    assert_eq!(stored.unwrap().unwrap().time().start_ms, Some(9_000));

    // The executor holds no call to a cap, whatever it is: nothing, and the largest
    // amount, reach a part as they were given. A call that came with neither a cap
    // nor a preparation is shown with neither.
    for cap in [Estimate::of_micros(0), Estimate::unbounded()] {
        ran(engine.request(&Request::new(call("hello")).capped_at(cap)));
        assert_eq!(part.seen().last().unwrap().cap, Some(cap));
    }
    engine.submit(&call("hello")).unwrap();
    let last = part.seen().last().unwrap().clone();
    assert_eq!(
        (last.cap, last.expected_made_ms, last.request),
        (None, None, None)
    );

    // The same call under the same id with another cap is the same request: it is
    // answered with its run, and no part is asked.
    let asked_before = part.asked();
    let again = asked("hello", "order-17").capped_at(Estimate::of_micros(7));
    let Ok(Answer::Repeat(earlier)) = engine.request(&again) else {
        panic!("not answered as a repeat");
    };
    assert_eq!(earlier.run(), finished.run());
    assert_eq!(part.asked(), asked_before);
}

#[test]
fn a_cap_on_a_call_from_inside_an_action_reaches_a_part_too() {
    let world = World::new();
    let part = Arc::new(FakePart::new("watch"));
    let outer = action("outer", &world.runs, |_, context| {
        let request = Request::new(Call::new("inner")).capped_at(Estimate::of_micros(55));
        context.executor().request(&request).unwrap();
        Ok(Produced::nothing())
    });
    let inner = action("inner", &world.runs, |_, _| Ok(Produced::nothing()));
    let engine = engine(world.store(), vec![outer, inner]).with_part(part.clone());
    engine.submit(&Call::new("outer")).unwrap();
    let caps: Vec<Option<Estimate>> = part.seen().iter().map(|seen| seen.cap).collect();
    assert_eq!(caps, [None, Some(Estimate::of_micros(55))]);
}

#[test]
fn a_part_that_adds_nothing_and_refuses_nothing_leaves_the_store_as_it_is_without_one() {
    let names_of = |with_part: bool| {
        let world = World::new();
        let store = world.store();
        let mut engine = world.engine(store).with_ledger(Arc::new(FakeLedger::new()));
        if with_part {
            engine = engine.with_part(Arc::new(FakePart::new("quiet")));
        }
        ran(engine.request(&asked("hello", "order-17")));
        let (number, names, runs) = held(&world.root(), world.fs.clone());
        let of_run: Vec<String> = runs[0].1.iter().map(|(name, _)| name.clone()).collect();
        let outside: Vec<String> = names
            .into_iter()
            .filter(|name| !name.starts_with("spend/reservations/"))
            .collect();
        (number, outside, of_run, world.fs.events().len())
    };
    assert_eq!(names_of(true), names_of(false));
}
