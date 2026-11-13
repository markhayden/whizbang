//! Nested calls (ADR 0008, "Nested calls"): an action calls another through the
//! executor in its context, and the call is a run of its own under the run that made
//! it, with an allowance taken from that run's. A fifth level and a call back are
//! refused; a cancellation reaches every run under the one it cancels and refuses new
//! ones; a run is terminal only when the runs it called are and every request its
//! capability handles sent is accounted for; what a run that completed committed
//! stays when the run that called it does not complete; and the record says all of it.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};
use whizbang_core::artifact::{self, ArtifactError, ArtifactType};
use whizbang_core::capability::{FaultKind, FillRequest, Picture, Support};
use whizbang_core::contract::RecordError;
use whizbang_core::contract::record::{RunRecord, Status, StepKind};
use whizbang_core::contract::request::admitted;
use whizbang_core::contract::{
    Action, ActionDescriptor, Admission, Answer, Argument, Backends, Call, CallError, Cancel,
    Capabilities, Capability, Clock, Context, Engine, Estimate, Executor, Failure, HeldOpen,
    InputPort, Kind, LEVELS_LIMIT, ModelIdentity, Outcome, OutputPort, PreparedCall, Produced,
    RECORD_MAX_BYTES, REFUSAL_WHY_LIMIT, REFUSALS_LIMIT, Refusal, RefusalKind, Registry, Request,
    RequestId, Requirement, RunId, RunState, STARTED_LIMIT, STEPS_LIMIT, Secrets, Unavailable,
    Value, ValueType,
};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::{FakeFiller, Paint};
use whizbang_testkit::ledger::FakeLedger;
use whizbang_testkit::schema::validate;

use super::commit::{World, both, committed, held, pair};
use super::record::schema;
use super::redact::made_up_key;
use crate::artifact::support::{limits, on, rect_mask};

/// How long a wait for another thread may last before it is taken for a test that
/// cannot pass: a limit, not a pause.
const LIMIT: Duration = Duration::from_secs(20);

struct Stopped(u64);

impl Clock for Stopped {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

/// Backends whose one model states a bound the test sets.
struct Priced(AtomicU64);

impl Backends for Priced {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Ok(ModelIdentity::new("fake", "fake-model", "1"))
    }

    fn bound(&self, _: Capability, _: &ModelIdentity) -> Estimate {
        Estimate::of_micros(self.0.load(Ordering::SeqCst))
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

/// An action of this name that needs these capabilities, and whose run is whatever the
/// test says. It takes an optional text.
fn action(
    name: &str,
    needs: &[Capability],
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let mut draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::optional("text", "A text.", ValueType::Text));
    for need in needs {
        draft = draft.needs(Requirement::new(*need));
    }
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

fn act(
    name: &str,
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    action(name, &[], body)
}

fn engine_on(
    store: Arc<Store>,
    backends: Arc<dyn Backends>,
    actions: Vec<Arc<dyn Action>>,
) -> Engine {
    let mut registry = Registry::new();
    for action in actions {
        registry.register(action).unwrap();
    }
    Engine::over(registry, backends, store, Digest::of_bytes(b""))
        .with_clock(Arc::new(Stopped(5000)))
}

fn engine(store: Arc<Store>, actions: Vec<Arc<dyn Action>>) -> Engine {
    engine_on(store, Arc::new(Priced(AtomicU64::new(0))), actions)
}

/// Every run the store holds, with its record when it has one.
fn records(world: &World) -> BTreeMap<String, Option<RunRecord>> {
    let runs = held(&world.root(), world.fs.clone()).runs;
    runs.into_iter()
        .map(|(id, record, _)| (id, record))
        .collect()
}

fn record(world: &World, run: &RunId) -> RunRecord {
    let record = records(world)
        .remove(run.as_str())
        .expect("the run is there");
    let record = record.expect("the run has its record");
    let document: Json = serde_json::from_slice(&record.to_bytes()).unwrap();
    validate(&schema(), &document).unwrap();
    record
}

fn admission(world: &World, run: &str) -> Admission {
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    admitted(&generation, run, |entry| reader.read(entry))
        .unwrap()
        .unwrap()
}

fn refused(result: Result<impl std::fmt::Debug, CallError>) -> Refusal {
    match result {
        Err(CallError::Refused(refusal)) => refusal,
        other => panic!("not refused: {other:?}"),
    }
}

fn until(what: &str, done: impl Fn() -> bool) {
    let limit = Instant::now() + LIMIT;
    while !done() {
        assert!(Instant::now() < limit, "{what}");
        std::thread::yield_now();
    }
}

/// What each call of a test's action came to, noted by the action.
type Noted<T> = Arc<Mutex<Vec<T>>>;

fn noted<T>() -> Noted<T> {
    Arc::new(Mutex::new(Vec::new()))
}

// --- a call is a run of its own ---------------------------------------------------------------

#[test]
fn a_call_from_inside_an_action_is_a_run_of_its_own_under_the_run_that_made_it() {
    let world = World::new();
    let made: Noted<RunId> = noted();
    let outer = {
        let made = made.clone();
        act("outer", move |_, context| {
            for text in ["one", "two"] {
                let call = Call::new("inner").argument("text", Value::text(text));
                let finished = context.executor().submit(&call).unwrap();
                assert_eq!(finished.outcome().name(), "completed");
                made.lock().unwrap().push(finished.run().clone());
            }
            Ok(Produced::nothing())
        })
    };
    let inner = act("inner", |_, context| {
        context.diagnostics().write("notes.txt", b"inside").unwrap();
        // Nothing was reserved for the run that made the call, so nothing is counted.
        assert_eq!(context.allowance().total_micros(), None);
        Ok(Produced::nothing())
    });
    let executor = engine(world.store(), vec![outer, inner]);
    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let parent = finished.run().clone();
    let made = made.lock().unwrap().clone();

    // Three runs, each with an id of its own, its own admission and its own record.
    let all = records(&world);
    assert_eq!(all.len(), 3);
    assert!(made[0] != made[1] && !made.contains(&parent));
    for (child, text) in made.iter().zip(["one", "two"]) {
        let stored = record(&world, child);
        assert_eq!(stored.identity().parent.as_deref(), Some(parent.as_str()));
        assert_eq!(stored.identity().request, None);
        assert_eq!(stored.asked().action, "inner");
        assert_eq!(stored.outcome().status, Status::Completed);
        assert_eq!(stored.spend().reserved_micros, None);
        assert_eq!(stored.diagnostics().len(), 2);
        assert!(stored.steps().is_empty());
        let bytes = String::from_utf8(stored.to_bytes()).unwrap();
        assert!(
            bytes.contains(&format!(r#"{{"text":"{text}"}}"#)),
            "{bytes}"
        );
        let admitted = admission(&world, child.as_str());
        assert_eq!(admitted.parent(), Some(&parent));
        assert_eq!(
            (admitted.request(), admitted.reserved_micros()),
            (None, None)
        );
    }

    // The run that made the calls names no parent, and lists the two in order, each
    // by its run, its action and how it ended.
    let stored = record(&world, &parent);
    assert_eq!(stored.identity().parent, None);
    assert_eq!(admission(&world, parent.as_str()).parent(), None);
    let steps: Vec<_> = stored
        .steps()
        .iter()
        .map(|step| (step.kind(), step.run(), step.action(), step.status()))
        .collect();
    let completed = Some(Status::Completed);
    assert_eq!(
        steps,
        [
            (StepKind::Call, made[0].as_str(), "inner", completed),
            (StepKind::Call, made[1].as_str(), "inner", completed),
        ]
    );
    assert!(stored.outcome().refusals.is_empty());
    assert_eq!(stored.outcome().refusals_omitted, 0);

    // A run that another run called is asked after by its id, as any run is.
    assert_eq!(
        executor.status(&made[0]).unwrap().state(),
        &RunState::Ended(Outcome::Completed {
            outputs: Vec::new()
        })
    );
}

#[test]
fn a_call_from_inside_an_action_carries_no_request_id() {
    let world = World::new();
    let said: Noted<Refusal> = noted();
    let outer = {
        let said = said.clone();
        act("outer", move |_, context| {
            let id = RequestId::parse("order-9").unwrap();
            let request = Request::new(Call::new("inner")).with_id(id);
            said.lock()
                .unwrap()
                .push(refused(context.executor().request(&request)));
            said.lock()
                .unwrap()
                .push(refused(context.executor().start(&request)));
            Ok(Produced::nothing())
        })
    };
    let inner = act("inner", |_, _| panic!("the call ran"));
    let executor = engine(world.store(), vec![outer, inner]);
    let finished = executor.submit(&Call::new("outer")).unwrap();
    let inside = Refusal::IdInside {
        action: "inner".to_owned(),
    };
    assert_eq!(*said.lock().unwrap(), [inside.clone(), inside]);

    // No run was made, the id was not taken, and the refusals are in the record.
    assert_eq!(records(&world).len(), 1);
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let id = RequestId::parse("order-9").unwrap();
    assert!(
        reader
            .snapshot()
            .unwrap()
            .get(&id.admission_name())
            .is_none()
    );
    let stored = record(&world, finished.run());
    let kinds: Vec<_> = stored.outcome().refusals.iter().map(|r| r.kind()).collect();
    assert_eq!(kinds, [RefusalKind::IdInside, RefusalKind::IdInside]);
}

// --- depth, and the stack ---------------------------------------------------------------------

/// An action of this name that calls `next`, notes a refusal, and completes.
fn calls(name: &str, next: &'static str, said: &Noted<(String, Refusal)>) -> Arc<dyn Action> {
    let (said, name_) = (said.clone(), name.to_owned());
    act(name, move |_, context| {
        if let Err(CallError::Refused(refusal)) = context.executor().submit(&Call::new(next)) {
            said.lock().unwrap().push((name_.clone(), refusal));
        }
        Ok(Produced::nothing())
    })
}

#[test]
fn a_fifth_level_is_refused_and_the_refusal_is_in_the_record_of_the_run_that_made_the_call() {
    assert_eq!(LEVELS_LIMIT, 4);
    let world = World::new();
    let said = noted();
    let actions = vec![
        calls("one", "two", &said),
        calls("two", "three", &said),
        calls("three", "four", &said),
        calls("four", "five", &said),
        act("five", |_, _| panic!("a fifth level ran")),
    ];
    let executor = engine(world.store(), actions);
    let finished = executor.submit(&Call::new("one")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let too_deep = Refusal::TooDeep {
        action: "five".to_owned(),
        levels: 4,
    };
    assert_eq!(*said.lock().unwrap(), [("four".to_owned(), too_deep)]);

    // Four runs, one under the other; the fourth has the refusal, and no step.
    let all = records(&world);
    assert_eq!(all.len(), 4);
    let by_action = |name: &str| {
        let found = all.values().flatten().find(|r| r.asked().action == name);
        found.unwrap_or_else(|| panic!("no run of {name}")).clone()
    };
    let mut above: Option<String> = None;
    for name in ["one", "two", "three", "four"] {
        let stored = by_action(name);
        assert_eq!(stored.identity().parent, above, "{name}");
        assert_eq!(stored.outcome().status, Status::Completed);
        above = Some(stored.identity().run.clone());
        let refusals = &stored.outcome().refusals;
        if name != "four" {
            assert!(refusals.is_empty());
            assert_eq!(stored.steps().len(), 1);
            continue;
        }
        assert!(stored.steps().is_empty());
        assert_eq!(refusals.len(), 1);
        assert_eq!(refusals[0].kind(), RefusalKind::TooDeep);
        assert_eq!(refusals[0].action(), Some("five"));
        assert_eq!(
            refusals[0].why(),
            "five was not called: its caller runs at level 4, and calls nest 4 levels deep \
             at most"
        );
    }
}

#[test]
fn a_call_back_into_an_action_on_the_stack_is_refused_by_the_actions_name() {
    let world = World::new();
    let said: Noted<Refusal> = noted();
    let outer = act("outer", |_, context| {
        // The same action twice, one run after the other, is no call back: the first
        // is over when the second is made.
        for _ in 0..2 {
            let finished = context.executor().submit(&Call::new("inner")).unwrap();
            assert_eq!(finished.outcome().name(), "completed");
        }
        Ok(Produced::nothing())
    });
    let inner = {
        let said = said.clone();
        act("inner", move |_, context| {
            // Back into the action that called this one, and into this one itself.
            for back in ["outer", "inner"] {
                let refusal = refused(context.executor().submit(&Call::new(back)));
                said.lock().unwrap().push(refusal);
            }
            Ok(Produced::nothing())
        })
    };
    let executor = engine(world.store(), vec![outer, inner]);
    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let back = |action: &str| Refusal::CallBack {
        action: action.to_owned(),
    };
    assert_eq!(
        *said.lock().unwrap(),
        [back("outer"), back("inner"), back("outer"), back("inner")]
    );
    // Three runs: neither call back made one. Each run of `inner` has its two.
    let all = records(&world);
    assert_eq!(all.len(), 3);
    for stored in all.values().flatten() {
        let kinds: Vec<_> = stored.outcome().refusals.iter().map(|r| r.kind()).collect();
        if stored.asked().action == "inner" {
            assert_eq!(kinds, [RefusalKind::CallBack, RefusalKind::CallBack]);
        } else {
            assert!(kinds.is_empty());
        }
    }
}

#[test]
fn a_run_starts_only_so_many_runs_at_once_and_one_more_is_refused() {
    // Each run that is started is a thread. Four are executing, held inside their
    // actions; a fifth is refused, and is run on the caller's own thread instead.
    // When one of the four is over, another is started.
    assert_eq!(STARTED_LIMIT, 4);
    let world = World::new();
    let barriers = Barriers::new();
    let said: Noted<Refusal> = noted();
    let outer = {
        let (barriers, said) = (barriers.clone(), said.clone());
        act("outer", move |_, context| {
            let executor = context.executor();
            let held = Request::new(Call::new("held"));
            let started: Vec<RunId> = (0..STARTED_LIMIT)
                .map(|_| executor.start(&held).unwrap().run().clone())
                .collect();
            barriers.arrived("held", STARTED_LIMIT as u64);
            said.lock().unwrap().push(refused(executor.start(&held)));
            executor.submit(&Call::new("free")).unwrap();
            barriers.open("held");
            for run in &started {
                executor.wait(run).unwrap();
            }
            let again = executor.start(&Request::new(Call::new("free"))).unwrap();
            executor.wait(again.run()).unwrap();
            Ok(Produced::nothing())
        })
    };
    let held = {
        let barriers = barriers.clone();
        act("held", move |_, _| {
            barriers.stop_at("held");
            Ok(Produced::nothing())
        })
    };
    let free = act("free", |_, _| Ok(Produced::nothing()));
    let executor = engine(world.store(), vec![outer, held, free]);
    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let many = Refusal::TooManyStarted {
        action: "held".to_owned(),
        limit: 4,
    };
    assert_eq!(*said.lock().unwrap(), [many]);
    let stored = record(&world, finished.run());
    assert_eq!(stored.steps().len(), 6);
    let kinds: Vec<_> = stored.outcome().refusals.iter().map(|r| r.kind()).collect();
    assert_eq!(kinds, [RefusalKind::TooManyStarted]);
}

// --- allowances -------------------------------------------------------------------------------

#[test]
fn a_nested_call_draws_on_the_allowance_of_the_run_that_made_it_and_the_ledger_is_asked_once() {
    let world = World::new();
    let store = world.store();
    let bound = Arc::new(Priced(AtomicU64::new(100)));
    let ledger = Arc::new(FakeLedger::with_allowance(1000));
    let said: Noted<Refusal> = noted();
    let left: Noted<Option<u64>> = noted();
    // It needs two capabilities, so its estimate is twice the bound: 200.
    let outer = {
        let (said, left, bound) = (said.clone(), left.clone(), bound.clone());
        action(
            "outer",
            &[Capability::Fill, Capability::Judge],
            move |_, context| {
                let see = || left.lock().unwrap().push(context.allowance().left_micros());
                assert_eq!(context.allowance().total_micros(), Some(200));
                see();
                for _ in 0..2 {
                    context.executor().submit(&Call::new("inner")).unwrap();
                    see();
                }
                // Nothing is left: a third is refused, and nothing is taken.
                let over = refused(context.executor().submit(&Call::new("inner")));
                said.lock().unwrap().push(over);
                see();
                // A cost without a bound is not an amount to take.
                bound.0.store(u64::MAX, Ordering::SeqCst);
                let unbounded = refused(context.executor().submit(&Call::new("inner")));
                said.lock().unwrap().push(unbounded);
                bound.0.store(100, Ordering::SeqCst);
                Ok(Produced::nothing())
            },
        )
    };
    let inner = {
        let said = said.clone();
        action("inner", &[Capability::Fill], move |_, context| {
            // What was taken for this run is its own allowance, and a run it calls
            // draws on that.
            assert_eq!(context.allowance().total_micros(), Some(100));
            context.executor().submit(&Call::new("leaf")).unwrap();
            assert_eq!(context.allowance().left_micros(), Some(0));
            let over = refused(context.executor().submit(&Call::new("leaf")));
            said.lock().unwrap().push(over);
            Ok(Produced::nothing())
        })
    };
    let leaf = action("leaf", &[Capability::Fill], |_, _| Ok(Produced::nothing()));
    let executor = engine_on(store.clone(), bound.clone(), vec![outer, inner, leaf])
        .with_ledger(ledger.clone());
    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let parent = finished.run().clone();

    let over = |action: &str| Refusal::OverCaller {
        action: action.to_owned(),
        asked_micros: 100,
        left_micros: 0,
    };
    let no_bound = Refusal::NoBound {
        action: "inner".to_owned(),
    };
    assert_eq!(
        *said.lock().unwrap(),
        [over("leaf"), over("leaf"), over("inner"), no_bound]
    );
    assert_eq!(
        *left.lock().unwrap(),
        [Some(200), Some(100), Some(0), Some(0)]
    );

    // The ledger was asked once, for the run nothing called, and one reservation is
    // in the store: the calls reserved nothing more.
    assert_eq!(ledger.asked(), 1);
    let reserved = ledger.reservations(store.as_ref()).unwrap();
    assert_eq!(reserved.len(), 1);
    assert_eq!((reserved[0].run(), reserved[0].micros()), (&parent, 200));
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    let spend: Vec<&String> = generation
        .entries()
        .keys()
        .filter(|name| name.starts_with("spend/"))
        .collect();
    assert_eq!(spend.len(), 2, "{spend:?}");

    // What was taken for each call is in its admission, which names the run it was
    // taken from, and in its record.
    let all = records(&world);
    assert_eq!(all.len(), 5);
    for (id, stored) in &all {
        let stored = stored.as_ref().unwrap();
        let admitted = admission(&world, id);
        let (taken, above) = match stored.asked().action.as_str() {
            "outer" => (200, None),
            "inner" => (100, Some(parent.as_str())),
            _ => (100, stored.identity().parent.as_deref()),
        };
        assert_eq!(stored.spend().reserved_micros, Some(taken));
        assert_eq!(admitted.reserved_micros(), Some(taken));
        assert_eq!(stored.identity().parent.as_deref(), above);
        assert_eq!(admitted.parent().map(RunId::as_str), above);
        assert_eq!(above.is_some(), stored.asked().action != "outer");
    }
    let stored = record(&world, &parent);
    let refusals = &stored.outcome().refusals;
    let kinds: Vec<_> = refusals.iter().map(|r| r.kind()).collect();
    assert_eq!(kinds, [RefusalKind::OverCaller, RefusalKind::NoBound]);
    assert_eq!(
        refusals[0].why(),
        "inner was not called: it may cost up to 100 millionths of a US dollar, and 0 are \
         left of its caller's allowance"
    );
}

#[test]
fn two_calls_at_once_cannot_both_take_the_last_of_the_allowance() {
    let world = World::new();
    let barriers = Barriers::new();
    let said: Noted<Refusal> = noted();
    let admitted = Arc::new(AtomicU64::new(0));
    let outer = {
        let (barriers, said) = (barriers.clone(), said.clone());
        action("outer", &[Capability::Fill], move |_, context| {
            // The first call takes all there is, and its run is held inside its
            // action. A second, made while the first runs, is refused; and so is one
            // made after it ended: nothing is given back before settlement.
            let executor = context.executor();
            let started = executor.start(&Request::new(Call::new("inner"))).unwrap();
            barriers.arrived("inner", 1);
            let over = refused(executor.submit(&Call::new("inner")));
            said.lock().unwrap().push(over);
            barriers.open("inner");
            executor.wait(started.run()).unwrap();
            let over = refused(executor.submit(&Call::new("inner")));
            said.lock().unwrap().push(over);
            Ok(Produced::nothing())
        })
    };
    // Eight calls made at once, of which the allowance covers one.
    let crowd = {
        let admitted = admitted.clone();
        action("crowd", &[Capability::Fill], move |_, context| {
            let gate = std::sync::Barrier::new(8);
            std::thread::scope(|scope| {
                for _ in 0..8 {
                    scope.spawn(|| {
                        gate.wait();
                        if context.executor().submit(&Call::new("inner")).is_ok() {
                            admitted.fetch_add(1, Ordering::SeqCst);
                        }
                    });
                }
            });
            assert_eq!(context.allowance().left_micros(), Some(0));
            Ok(Produced::nothing())
        })
    };
    let inner = {
        let barriers = barriers.clone();
        action("inner", &[Capability::Fill], move |_, _| {
            barriers.stop_at("inner");
            Ok(Produced::nothing())
        })
    };
    let bound = Arc::new(Priced(AtomicU64::new(100)));
    let executor = engine_on(world.store(), bound, vec![outer, crowd, inner])
        .with_ledger(Arc::new(FakeLedger::new()));
    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let over = Refusal::OverCaller {
        action: "inner".to_owned(),
        asked_micros: 100,
        left_micros: 0,
    };
    assert_eq!(*said.lock().unwrap(), [over.clone(), over]);
    assert_eq!(records(&world).len(), 2);

    let finished = executor.submit(&Call::new("crowd")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(admitted.load(Ordering::SeqCst), 1);
    let stored = record(&world, finished.run());
    assert_eq!(stored.steps().len(), 1);
    assert_eq!(stored.outcome().refusals.len(), 7);
    assert_eq!(records(&world).len(), 4);
}

// --- cancellation -----------------------------------------------------------------------------

#[test]
fn cancelling_a_run_cancels_every_run_under_it_and_a_call_made_after_is_refused() {
    // Three levels. The lowest is held inside its action when the highest is
    // cancelled. Its signal is set, and it ends as cancelled, with its own record. The
    // run between them then tries to call again: it is refused, and no run is made.
    let world = World::new();
    let store = world.store();
    let barriers = Barriers::new();
    let saw: Noted<bool> = noted();
    let said: Noted<Refusal> = noted();
    let outer = act("outer", |_, context| {
        let finished = context.executor().submit(&Call::new("middle")).unwrap();
        assert_eq!(finished.outcome(), &Outcome::Cancelled);
        Ok(Produced::nothing())
    });
    let middle = {
        let said = said.clone();
        act("middle", move |_, context| {
            let finished = context.executor().submit(&Call::new("leaf")).unwrap();
            assert_eq!(finished.outcome(), &Outcome::Cancelled);
            assert!(context.cancellation().is_cancelled());
            let after = refused(context.executor().submit(&Call::new("leaf")));
            said.lock().unwrap().push(after);
            let after = refused(context.executor().start(&Request::new(Call::new("leaf"))));
            said.lock().unwrap().push(after);
            Ok(Produced::nothing())
        })
    };
    let leaf = {
        let (barriers, saw) = (barriers.clone(), saw.clone());
        act("leaf", move |_, context| {
            barriers.stop_at("leaf");
            saw.lock()
                .unwrap()
                .push(context.cancellation().is_cancelled());
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![outer, middle, leaf]);
    let asking = engine(store.clone(), Vec::new());
    let Ok(Answer::Started(root)) = executor.start(&Request::new(Call::new("outer"))) else {
        panic!("the request was not started");
    };
    barriers.arrived("leaf", 1);
    let before = records(&world);
    assert_eq!(before.len(), 3);
    for id in before.keys() {
        let run = RunId::parse(id).unwrap();
        let state = asking.status(&run).unwrap();
        assert_eq!(state.state(), &RunState::NotEnded { here: true });
    }
    assert_eq!(asking.cancel(&root), Ok(Cancel::Accepted));
    barriers.release("leaf");
    let cancelled = RunState::Ended(Outcome::Cancelled);
    assert_eq!(asking.wait(&root).unwrap().state(), &cancelled);
    assert_eq!(*saw.lock().unwrap(), [true]);
    assert_eq!(barriers.arrivals("leaf"), 1);
    let after = Refusal::Cancelled {
        action: "leaf".to_owned(),
    };
    assert_eq!(*said.lock().unwrap(), [after.clone(), after]);

    // Three runs, all cancelled, each with its record; the refused calls made none.
    let all = records(&world);
    assert_eq!(all.len(), 3);
    for (id, stored) in &all {
        let stored = stored.as_ref().unwrap();
        assert_eq!(stored.outcome().status, Status::Cancelled, "{id}");
        let kinds: Vec<_> = stored.outcome().refusals.iter().map(|r| r.kind()).collect();
        let steps: Vec<_> = stored.steps().iter().map(|s| s.status()).collect();
        match stored.asked().action.as_str() {
            "leaf" => assert!(kinds.is_empty() && steps.is_empty()),
            "middle" => {
                assert_eq!(kinds, [RefusalKind::Cancelled, RefusalKind::Cancelled]);
                assert_eq!(steps, [Some(Status::Cancelled)]);
            }
            _ => {
                assert!(kinds.is_empty());
                assert_eq!(steps, [Some(Status::Cancelled)]);
            }
        }
    }
}

#[test]
fn cancelling_a_run_that_another_run_called_does_not_cancel_the_run_that_called_it() {
    let world = World::new();
    let store = world.store();
    let barriers = Barriers::new();
    let outer = act("outer", |_, context| {
        let finished = context.executor().submit(&Call::new("inner")).unwrap();
        assert_eq!(finished.outcome(), &Outcome::Cancelled);
        assert!(!context.cancellation().is_cancelled());
        // The run goes on, and calls again.
        let finished = context.executor().submit(&Call::new("inner")).unwrap();
        assert_eq!(finished.outcome().name(), "completed");
        Ok(Produced::nothing())
    });
    let inner = {
        let barriers = barriers.clone();
        act("inner", move |_, _| {
            barriers.stop_at("inner");
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![outer, inner]);
    let Ok(Answer::Started(root)) = executor.start(&Request::new(Call::new("outer"))) else {
        panic!("the request was not started");
    };
    barriers.arrived("inner", 1);
    let all = records(&world);
    let child = all.keys().find(|id| **id != root.as_str()).unwrap();
    let child = RunId::parse(child).unwrap();
    assert_eq!(executor.cancel(&child), Ok(Cancel::Accepted));
    barriers.open("inner");
    let waited = executor.wait(&root).unwrap();
    assert_eq!(
        waited.state(),
        &RunState::Ended(Outcome::Completed {
            outputs: Vec::new()
        })
    );
    let stored = record(&world, &root);
    let steps: Vec<_> = stored.steps().iter().map(|s| s.status()).collect();
    assert_eq!(steps, [Some(Status::Cancelled), Some(Status::Completed)]);
    assert_eq!(record(&world, &child).outcome().status, Status::Cancelled);
}

// --- when a run is terminal -------------------------------------------------------------------

/// A request to paint the top row of a small picture.
fn fill_request() -> FillRequest {
    let rgba = (0..24u8).flat_map(|at| [at, at, at, 255]).collect();
    let picture = Picture::new(CanonicalPixels::from_rgba(6, 4, rgba).unwrap()).unwrap();
    let mut plane = vec![0; 24];
    plane[..6].fill(255);
    let support = Support::from_plane(6, 4, plane).unwrap();
    FillRequest::new(picture, support, None).unwrap()
}

#[test]
fn a_run_stays_open_while_a_capability_request_is_outstanding_and_ends_when_it_is_accounted_for() {
    // The action sends a request from a thread of its own and comes back while the
    // fake filler still holds the request. The run does not end: its status says it
    // is executing, a wait for it does not return, and the store holds no record.
    // When the request is let go, the run ends.
    let world = World::new();
    let store = world.store();
    let barriers = Barriers::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse).stopping_at(&barriers, "fill"));
    let later: Noted<Option<FaultKind>> = noted();
    let asks = {
        let (barriers, later) = (barriers.clone(), later.clone());
        act("asks", move |_, context| {
            let fill = context.capabilities().fill().unwrap().clone();
            let (barriers_, later) = (barriers.clone(), later.clone());
            std::thread::spawn(move || {
                fill.fill(&fill_request()).unwrap();
                // The action has come back by now: nothing more is sent for its run.
                barriers_.stop_at("after");
                let fault = fill.fill(&fill_request()).err().map(|fault| fault.kind());
                later.lock().unwrap().push(fault);
                barriers_.stop_at("told");
            });
            barriers.arrived("fill", 1);
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![asks])
        .with_capabilities(Capabilities::none().with_fill(filler.clone()));
    let Ok(Answer::Started(run)) = executor.start(&Request::new(Call::new("asks"))) else {
        panic!("the request was not started");
    };
    let outstanding = HeldOpen {
        came_back: true,
        children: 0,
        requests: 1,
        waits_for_requests: true,
    };
    until("the action does not come back", || {
        executor.held_open(&run) == Some(outstanding)
    });
    let waiting = {
        let (executor, run) = (executor.clone(), run.clone());
        std::thread::spawn(move || executor.wait(&run))
    };
    until("nobody waits for the run", || {
        executor.waiting_for(&run) > 0
    });
    let watched = executor.status(&run).unwrap();
    assert_eq!(watched.state(), &RunState::NotEnded { here: true });
    assert!(!waiting.is_finished());
    assert!(records(&world)[run.as_str()].is_none());

    barriers.release("fill");
    let completed = RunState::Ended(Outcome::Completed {
        outputs: Vec::new(),
    });
    assert_eq!(waiting.join().unwrap().unwrap().state(), &completed);
    assert_eq!(record(&world, &run).outcome().status, Status::Completed);

    // A request on a handle the action left behind is not sent.
    barriers.release("after");
    barriers.arrived("told", 1);
    assert_eq!(*later.lock().unwrap(), [Some(FaultKind::Cancelled)]);
    assert_eq!(filler.requests(), 1);
    barriers.open("told");
}

#[test]
fn a_run_stays_open_while_a_run_it_started_is_not_over_and_a_cancel_still_reaches_both() {
    // The action starts another and comes back at once. Its run does not end while
    // the run it started is executing; it is cancelled meanwhile, which is accepted,
    // and reaches the run it started; and it ends once that run has.
    let world = World::new();
    let store = world.store();
    let barriers = Barriers::new();
    let late: Noted<Refusal> = noted();
    let outer = {
        let (barriers, late) = (barriers.clone(), late.clone());
        act("outer", move |_, context| {
            let started = context.executor().start(&Request::new(Call::new("inner")));
            assert!(matches!(started, Ok(Answer::Started(_))));
            // Something the action leaves behind calls again after it came back.
            let (context, barriers, late) = (context.clone(), barriers.clone(), late.clone());
            std::thread::spawn(move || {
                barriers.stop_at("late");
                let refusal = refused(context.executor().submit(&Call::new("inner")));
                late.lock().unwrap().push(refusal);
                drop(context);
                barriers.stop_at("told");
            });
            Ok(Produced::nothing())
        })
    };
    let inner = {
        let barriers = barriers.clone();
        act("inner", move |_, _| {
            barriers.stop_at("inner");
            Ok(Produced::nothing())
        })
    };
    let executor = engine(store.clone(), vec![outer, inner]);
    let Ok(Answer::Started(root)) = executor.start(&Request::new(Call::new("outer"))) else {
        panic!("the request was not started");
    };
    barriers.arrived("inner", 1);
    let open = HeldOpen {
        came_back: true,
        children: 1,
        requests: 0,
        waits_for_requests: false,
    };
    until("the action does not come back", || {
        executor.held_open(&root) == Some(open)
    });
    // The run's end is waiting for the run it started: that wait is counted there.
    let all = records(&world);
    let child = all.keys().find(|id| **id != root.as_str()).unwrap();
    let child = RunId::parse(child).unwrap();
    until(
        "the end of the run does not wait for the run it started",
        || executor.waiting_for(&child) > 0,
    );
    let waiting = {
        let (executor, root) = (executor.clone(), root.clone());
        std::thread::spawn(move || executor.wait(&root))
    };
    until("nobody waits for the run", || {
        executor.waiting_for(&root) > 0
    });
    let state = executor.status(&root).unwrap();
    assert_eq!(state.state(), &RunState::NotEnded { here: true });
    assert!(!waiting.is_finished());
    assert!(records(&world)[root.as_str()].is_none());

    // A call made now is refused: the action had come back.
    barriers.release("late");
    barriers.arrived("told", 1);
    barriers.open("told");
    let over = Refusal::CallerOver {
        action: "inner".to_owned(),
    };
    assert_eq!(*late.lock().unwrap(), [over]);

    assert_eq!(executor.cancel(&root), Ok(Cancel::Accepted));
    assert!(!waiting.is_finished());
    barriers.release("inner");
    let cancelled = RunState::Ended(Outcome::Cancelled);
    assert_eq!(waiting.join().unwrap().unwrap().state(), &cancelled);
    let stored = record(&world, &root);
    assert_eq!(stored.outcome().status, Status::Cancelled);
    let steps: Vec<_> = stored.steps().iter().map(|s| s.status()).collect();
    assert_eq!(steps, [Some(Status::Cancelled)]);
    let kinds: Vec<_> = stored.outcome().refusals.iter().map(|r| r.kind()).collect();
    assert_eq!(kinds, [RefusalKind::CallerOver]);
    assert_eq!(records(&world).len(), 2);
}

// --- what a run that completed committed ------------------------------------------------------

#[test]
fn a_completed_runs_outputs_remain_when_the_run_that_called_it_does_not_complete() {
    // The run that made the call then fails, is cancelled, panics, or has its end
    // refused by the store. Each time the run it called stays completed, with both of
    // its outputs in the store, and says which run it was produced under.
    for end in ["fails", "cancelled", "panics", "abandoned"] {
        let world = World::new();
        let fs = world.fs.clone();
        let outer = act("outer", move |_, context| {
            let finished = context.executor().submit(&Call::new("pair")).unwrap();
            assert_eq!(finished.outcome().outputs(), committed());
            match end {
                "fails" => return Err(Failure::error("outer", "it broke")),
                "cancelled" => context.cancellation().cancel(),
                "panics" => panic!("it broke"),
                _ => fs.refuse_before(fs.events().len()),
            }
            Ok(Produced::nothing())
        });
        let executor = engine(world.store(), vec![outer, pair(|_, _| Ok(both()))]);
        let finished = executor.submit(&Call::new("outer"));

        let after = held(&world.root(), world.fs.clone());
        assert!(after.mask && after.place, "{end}");
        assert_eq!(after.outputs.len(), 2, "{end}");
        assert_eq!(after.runs.len(), 2);
        let of = |action: &str| {
            let found = after
                .runs
                .iter()
                .find(|(id, _, _)| admission(&world, id).action() == action);
            found.unwrap().clone()
        };
        let (parent, parents, _) = of("outer");
        let (_, child, _) = of("pair");
        let child = child.expect("the run that completed has its record");
        assert_eq!(child.outcome().status, Status::Completed, "{end}");
        assert_eq!(child.outputs().len(), 2);
        assert_eq!(child.identity().parent.as_deref(), Some(parent.as_str()));

        let status = match end {
            "abandoned" => {
                assert!(matches!(finished, Err(CallError::Store(_))));
                assert!(parents.is_none());
                continue;
            }
            "cancelled" => Status::Cancelled,
            _ => Status::Error,
        };
        assert_eq!(finished.unwrap().run().as_str(), parent);
        let parents = parents.expect("the run has its record");
        assert_eq!(parents.outcome().status, status, "{end}");
        assert!(parents.outputs().is_empty());
        let steps: Vec<_> = parents.steps().iter().map(|s| s.status()).collect();
        assert_eq!(steps, [Some(Status::Completed)], "{end}");
    }
}

#[test]
fn a_run_reads_what_a_run_it_called_committed_as_its_own_output_and_as_the_input_of_a_second() {
    let world = World::new();
    let store = world.store();
    let mask = committed()[0].id();
    let other = rect_mask(on("another photo"));
    let other_id = other.id();
    // A run that nothing here called, which commits another mask.
    let outside = {
        let other = other.clone();
        let draft = ActionDescriptor::draft("other", "1.0.0", Kind::Primitive, "A test action.")
            .output(OutputPort::one("mask", ArtifactType::Mask, "A mask."));
        let body: Body = Box::new(move |_, _| Ok(Produced::nothing().with("mask", other.clone())));
        engine(
            store.clone(),
            vec![Arc::new(Scripted(draft.build().unwrap(), body))],
        )
    };
    let keep = {
        let draft = ActionDescriptor::draft("keep", "1.0.0", Kind::Primitive, "A test action.")
            .output(OutputPort::one("mask", ArtifactType::Mask, "A mask."));
        let body: Body = Box::new(move |_, context| {
            let read = |id| artifact::get(context.store(), ArtifactType::Mask, id, &limits());
            let admitted_against = context.store().snapshot().unwrap().number();
            assert!(matches!(read(mask), Err(ArtifactError::Missing { .. })));
            let finished = context.executor().submit(&Call::new("pair")).unwrap();
            let made = finished.outcome().outputs()[0].clone();
            assert_eq!(made.id(), mask);
            // Another run commits meanwhile, and nothing called it from here.
            outside.submit(&Call::new("other")).unwrap();

            // The store of this run's context is still the generation the run was
            // admitted against, with what the run it called committed, and no more.
            let view = context.store().snapshot().unwrap();
            assert_eq!(view.number(), admitted_against);
            assert!(view.run(finished.run().as_str()).is_none());
            assert!(matches!(read(other_id), Err(ArtifactError::Missing { .. })));
            let artifact = read(made.id()).unwrap();

            // It is the input of a second call, and this run's own output.
            let second = Call::new("take").input("mask", made.id());
            let finished = context.executor().submit(&second).unwrap();
            assert_eq!(finished.outcome().name(), "completed");
            Ok(Produced::nothing().with("mask", artifact))
        });
        Arc::new(Scripted(draft.build().unwrap(), body)) as Arc<dyn Action>
    };
    let take = {
        let draft = ActionDescriptor::draft("take", "1.0.0", Kind::Primitive, "A test action.")
            .input(InputPort::required("mask", ArtifactType::Mask, "A mask."));
        let body: Body = Box::new(move |call, context| {
            let id = call.inputs()[0].id();
            artifact::get(context.store(), ArtifactType::Mask, id, &limits()).unwrap();
            Ok(Produced::nothing())
        });
        Arc::new(Scripted(draft.build().unwrap(), body)) as Arc<dyn Action>
    };
    let executor = engine(store.clone(), vec![keep, take, pair(|_, _| Ok(both()))]);
    let finished = executor.submit(&Call::new("keep")).unwrap();
    let outputs = finished.outcome().outputs();
    assert_eq!(outputs.len(), 1, "{finished:?}");
    assert_eq!((outputs[0].port(), outputs[0].id()), ("mask", mask));
    let stored = record(&world, finished.run());
    assert_eq!(stored.steps().len(), 2);
    let taken = stored.steps()[1].run();
    let taken = record(&world, &RunId::parse(taken).unwrap());
    assert_eq!(taken.inputs()[0].artifact, mask.0);
}

// --- the record -------------------------------------------------------------------------------

#[test]
fn what_is_refused_inside_a_run_is_recorded_with_the_secrets_taken_out_and_within_a_bound() {
    let world = World::new();
    let key = made_up_key("nested");
    let outer = {
        let key = key.clone();
        act("outer", move |_, context| {
            let submit = |call: Call| refused(context.executor().submit(&call));
            // The action's name is one no action has, and holds the secret where the
            // cut of a shown name, and the cut of the sentence, go through it.
            for before in [50, 60, REFUSAL_WHY_LIMIT - 30] {
                submit(Call::new(&format!("{}{key}", "x".repeat(before))));
            }
            // The secret in an argument's name, and in an argument's text.
            submit(Call::new("inner").argument(&key, Value::Boolean(true)));
            submit(Call::new("outer").argument("text", Value::text(&key)));
            // A name that holds no secret and is far too long.
            submit(Call::new(&"y".repeat(5000)));
            // And then more refusals than a record lists.
            for _ in 0..REFUSALS_LIMIT {
                submit(Call::new("outer"));
            }
            Ok(Produced::nothing())
        })
    };
    let inner = act("inner", |_, _| Ok(Produced::nothing()));
    let executor = engine(world.store(), vec![outer, inner])
        .with_secrets(Secrets::new([key.clone()]).unwrap());
    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(records(&world).len(), 1);

    let stored = record(&world, finished.run());
    let bytes = stored.to_bytes();
    assert!((bytes.len() as u64) < RECORD_MAX_BYTES / 8);
    let text = String::from_utf8(bytes).unwrap();
    // Neither the secret nor a part of it that a cut left.
    assert!(!text.contains(&key));
    assert!(
        !text.contains(&key[..12]),
        "a part of the secret is written"
    );
    let refusals = &stored.outcome().refusals;
    assert_eq!(refusals.len(), REFUSALS_LIMIT);
    assert_eq!(stored.outcome().refusals_omitted, 6);
    let unquoted = "the call held something that must not be written, so its refusal is not quoted";
    for refused in &refusals[..5] {
        assert_eq!((refused.action(), refused.why()), (None, unquoted));
    }
    let kinds: Vec<_> = refusals[..7].iter().map(|r| r.kind()).collect();
    let unknown = RefusalKind::UnknownAction;
    assert_eq!(
        kinds,
        [
            unknown,
            unknown,
            unknown,
            RefusalKind::Invalid,
            RefusalKind::CallBack,
            unknown,
            RefusalKind::CallBack
        ]
    );
    assert_eq!(refusals[5].action(), None);
    assert_eq!(refusals[5].why().chars().count(), REFUSAL_WHY_LIMIT);
    assert!(refusals[5].why().starts_with("no action named yyyy"));
    assert_eq!(refusals[6].action(), Some("outer"));
}

/// A record as the executor writes one for a run that another run called, that called
/// one itself and had one call refused: by hand.
fn nested() -> String {
    concat!(
        r#"{"record":"run/2","#,
        r#""identity":{"run":"1000-2","request":null,"parent":"1000-1","whizbang":"0.0.1"},"#,
        r#""asked":{"action":"inner","version":"1.0.0","arguments":[],"settings":[],"#,
        r#""config":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"},"#,
        r#""inputs":[],"outputs":[],"#,
        r#""outcome":{"status":"completed","error":null,"invalid":null,"#,
        r#""refusals":[{"action":"outer","kind":"call_back","why":"outer was not called"}],"#,
        r#""refusals_omitted":0},"#,
        r#""time":{"start_ms":1000,"end_ms":1003,"durations":[]},"#,
        r#""models":[],"spend":{"reserved_micros":100,"requests":[]},"egress":[],"#,
        r#""steps":[{"step":"call","run":"1000-3","action":"leaf","status":"completed"},"#,
        r#"{"step":"call","run":"1000-4","action":"leaf","status":null}],"#,
        r#""judge":[],"diagnostics":[]}"#,
        "\n"
    )
    .to_owned()
}

/// The fields of a record, in the order a record is written in.
const ORDER: [&str; 37] = [
    "record",
    "identity",
    "step",
    "run",
    "request",
    "parent",
    "whizbang",
    "asked",
    "action",
    "version",
    "arguments",
    "settings",
    "config",
    "inputs",
    "outputs",
    "outcome",
    "status",
    "error",
    "invalid",
    "refusals",
    "kind",
    "why",
    "refusals_omitted",
    "time",
    "start_ms",
    "end_ms",
    "durations",
    "models",
    "spend",
    "reserved_micros",
    "requests",
    "egress",
    "steps",
    "judge",
    "diagnostics",
    "name",
    "extra",
];

/// JSON on one line with every object's fields in the record's order, so that what a
/// test changed is the one thing that differs from what the executor writes.
fn written(value: &Json, out: &mut String) {
    match value {
        Json::Array(items) => {
            out.push('[');
            for (at, item) in items.iter().enumerate() {
                out.push_str(if at == 0 { "" } else { "," });
                written(item, out);
            }
            out.push(']');
        }
        Json::Object(fields) => {
            out.push('{');
            let present = ORDER
                .iter()
                .filter_map(|name| Some((name, fields.get(*name)?)));
            for (at, (name, field)) in present.enumerate() {
                assert!(fields.keys().all(|key| ORDER.contains(&key.as_str())));
                out.push_str(if at == 0 { "" } else { "," });
                out.push_str(&format!("\"{name}\":"));
                written(field, out);
            }
            out.push('}');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// The record above with one change made to its JSON.
fn changed(change: impl Fn(&mut Json)) -> (Vec<u8>, Json) {
    let mut document: Json = serde_json::from_str(&nested()).unwrap();
    change(&mut document);
    let mut bytes = String::new();
    written(&document, &mut bytes);
    bytes.push('\n');
    (bytes.into_bytes(), document)
}

#[test]
fn a_record_with_a_parent_steps_and_refusals_is_read_and_written_back_as_the_same_bytes() {
    let record = RunRecord::from_bytes(nested().as_bytes()).unwrap();
    assert_eq!(record.to_bytes(), nested().as_bytes());
    assert_eq!(record.identity().parent.as_deref(), Some("1000-1"));
    assert_eq!(record.steps().len(), 2);
    assert_eq!(record.steps()[1].status(), None);
    assert_eq!(record.outcome().refusals[0].kind(), RefusalKind::CallBack);
    let document: Json = serde_json::from_str(&nested()).unwrap();
    validate(&schema(), &document).unwrap();

    // The kinds of refusal the schema lists are the kinds there are, in order.
    let listed = &schema()["properties"]["outcome"]["properties"]["refusals"]["items"]["properties"]
        ["kind"]["enum"];
    let kinds: Vec<Json> = RefusalKind::ALL
        .iter()
        .map(|kind| serde_json::to_value(kind).unwrap())
        .collect();
    assert_eq!(listed, &Json::Array(kinds));
}

/// Sets what is at this place in the document, or takes it out when given nothing.
fn set(document: &mut Json, at: &str, value: Option<Json>) {
    let (object, field) = at.rsplit_once('/').unwrap();
    let object = document.pointer_mut(object).unwrap();
    match value {
        Some(value) => object[field] = value,
        None => drop(object.as_object_mut().unwrap().remove(field)),
    }
}

#[test]
fn what_is_no_parent_no_step_and_no_refusal_is_refused_by_the_types_and_by_the_schema() {
    let long = "x".repeat(65);
    let step = |at: usize| json!({"step": "call", "run": format!("2000-{at}"), "action": "leaf", "status": null});
    let steps = |count: usize| Json::Array((0..count).map(step).collect());
    let refusal = |why: &str| json!({"action": null, "kind": "invalid", "why": why});
    let (first, said) = ("/steps/0", "/outcome/refusals/0");
    let both = [
        (
            "an empty parent",
            "/identity/parent".to_owned(),
            Some(json!("")),
        ),
        (
            "a long parent",
            "/identity/parent".to_owned(),
            Some(json!(long)),
        ),
        (
            "a parent that is a number",
            "/identity/parent".to_owned(),
            Some(json!(7)),
        ),
        (
            "a step of another kind",
            format!("{first}/step"),
            Some(json!("recipe")),
        ),
        (
            "a step with one field more",
            format!("{first}/extra"),
            Some(json!(1)),
        ),
        ("a step without its status", format!("{first}/status"), None),
        (
            "a step with a status that is none",
            format!("{first}/status"),
            Some(json!("done")),
        ),
        (
            "a step with an empty run",
            format!("{first}/run"),
            Some(json!("")),
        ),
        (
            "a step with a long run",
            format!("{first}/run"),
            Some(json!(long)),
        ),
        (
            "a step with an empty action",
            format!("{first}/action"),
            Some(json!("")),
        ),
        (
            "more steps than a run may have",
            "/steps".to_owned(),
            Some(steps(STEPS_LIMIT + 1)),
        ),
        (
            "a refusal of a kind that is none",
            format!("{said}/kind"),
            Some(json!("no_nested_calls")),
        ),
        (
            "a refusal with one field more",
            format!("{said}/extra"),
            Some(json!(1)),
        ),
        (
            "a refusal without its action",
            format!("{said}/action"),
            None,
        ),
        (
            "a refusal with an empty action",
            format!("{said}/action"),
            Some(json!("")),
        ),
        (
            "a refusal with no sentence",
            format!("{said}/why"),
            Some(json!("")),
        ),
        (
            "a refusal with a sentence too long",
            format!("{said}/why"),
            Some(json!("y".repeat(REFUSAL_WHY_LIMIT + 1))),
        ),
        (
            "more refusals than a record lists",
            "/outcome/refusals".to_owned(),
            Some(Json::Array(vec![refusal("no"); REFUSALS_LIMIT + 1])),
        ),
        (
            "a count of refusals below none",
            "/outcome/refusals_omitted".to_owned(),
            Some(json!(-1)),
        ),
    ];
    // What the schema cannot say, the types still refuse.
    let only_types = [
        (
            "the run as its own parent",
            "/identity/parent".to_owned(),
            json!("1000-2"),
        ),
        (
            "a parent that is no run id",
            "/identity/parent".to_owned(),
            json!("Run 1"),
        ),
        (
            "a step that names the run itself",
            format!("{first}/run"),
            json!("1000-2"),
        ),
        (
            "a step that names the parent",
            format!("{first}/run"),
            json!("1000-1"),
        ),
        (
            "a run named by two steps",
            "/steps/1/run".to_owned(),
            json!("1000-3"),
        ),
        (
            "a step whose run is no run id",
            format!("{first}/run"),
            json!("a b"),
        ),
        (
            "a step whose action is no name",
            format!("{first}/action"),
            json!("Leaf"),
        ),
        (
            "a refusal whose action is no name",
            format!("{said}/action"),
            json!("not a name"),
        ),
        (
            "refusals left out of a list that is not full",
            "/outcome/refusals_omitted".to_owned(),
            json!(1),
        ),
    ];
    let both = both
        .iter()
        .map(|(what, at, value)| (*what, at, value.clone(), true));
    let only_types = only_types
        .iter()
        .map(|(what, at, value)| (*what, at, Some(value.clone()), false));
    for (what, at, value, by_schema) in both.chain(only_types) {
        let (bytes, document) = changed(|d| set(d, at, value.clone()));
        // Refused for what was changed, and not for how the test wrote it.
        let refusal = RunRecord::from_bytes(&bytes).expect_err(what);
        assert_ne!(refusal, RecordError::NotCanonical, "{what}");
        assert_eq!(validate(&schema(), &document).is_err(), by_schema, "{what}");
    }

    // At the bounds, both take it.
    let (bytes, document) = changed(|d| {
        d["steps"] = steps(STEPS_LIMIT);
        let full = refusal(&"y".repeat(REFUSAL_WHY_LIMIT));
        d["outcome"]["refusals"] = Json::Array(vec![full; REFUSALS_LIMIT]);
        d["outcome"]["refusals_omitted"] = json!(3);
    });
    RunRecord::from_bytes(&bytes).unwrap();
    validate(&schema(), &document).unwrap();
}

#[test]
fn an_admission_names_the_run_that_called_its_run_and_is_read_strictly() {
    let admission = |run: &str, request: &str, parent: &str| {
        format!(
            concat!(
                r#"{{"record":"admission/1","run":"{}","request":{},"parent":{},"#,
                r#""fingerprint":"sha256:f93fc011cfbdeda221670f83fa218719c8133d99d92b95492d70211584063c40","#,
                r#""prepared":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
                r#""reserved_micros":100,"action":"inner","version":"1.0.0","start_ms":7000}}"#,
                "\n"
            ),
            run, request, parent
        )
    };
    let bytes = admission("7000-2", "null", r#""7000-1""#);
    let read = Admission::from_bytes(bytes.as_bytes()).unwrap();
    assert_eq!(read.parent().map(RunId::as_str), Some("7000-1"));
    assert_eq!(read.to_bytes(), bytes.as_bytes());
    for (what, bytes) in [
        (
            "the run as its own parent",
            admission("7000-2", "null", r#""7000-2""#),
        ),
        (
            "a parent that is no run id",
            admission("7000-2", "null", r#""Run 1""#),
        ),
        (
            "a request id on a run with a parent",
            admission("7000-2", r#""order-1""#, r#""7000-1""#),
        ),
        (
            "no parent at all",
            admission("7000-2", "null", "null").replace(r#""parent":null,"#, ""),
        ),
    ] {
        assert!(Admission::from_bytes(bytes.as_bytes()).is_err(), "{what}");
    }
}

#[test]
fn a_run_calls_as_many_actions_as_its_record_lists_and_one_more_is_refused() {
    let world = World::new();
    let said: Noted<Refusal> = noted();
    let outer = {
        let said = said.clone();
        act("outer", move |_, context| {
            for _ in 0..STEPS_LIMIT {
                context.executor().submit(&Call::new("inner")).unwrap();
            }
            let more = refused(context.executor().submit(&Call::new("inner")));
            said.lock().unwrap().push(more);
            Ok(Produced::nothing())
        })
    };
    let inner = act("inner", |_, _| Ok(Produced::nothing()));
    let executor = engine(world.store(), vec![outer, inner]);
    let finished = executor.submit(&Call::new("outer")).unwrap();
    let many = Refusal::TooManyCalls {
        action: "inner".to_owned(),
        limit: STEPS_LIMIT,
    };
    assert_eq!(*said.lock().unwrap(), [many]);
    // The record lists every one of them, and is one that is read back.
    let stored = record(&world, finished.run());
    assert_eq!(stored.steps().len(), STEPS_LIMIT);
    assert!((stored.to_bytes().len() as u64) < RECORD_MAX_BYTES / 8);
}
