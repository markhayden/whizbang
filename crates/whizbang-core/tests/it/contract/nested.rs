//! Nested calls (ADR 0008, "Nested calls"): an action calls another through the
//! executor in its context, and the call is a run of its own under the run that made
//! it, with an allowance taken from that run's. A fifth level and a call back are
//! refused, and the record says all of it.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};
use whizbang_core::contract::RecordError;
use whizbang_core::contract::record::{RunRecord, Status, StepKind};
use whizbang_core::contract::request::admitted;
use whizbang_core::contract::{
    Action, ActionDescriptor, Admission, Argument, Backends, Call, CallError, Capability, Clock,
    Context, DESCENDANTS_LIMIT, Engine, Estimate, Executor, Failure, Kind, LEVELS_LIMIT,
    ModelIdentity, Outcome, PreparedCall, Produced, RECORD_MAX_BYTES, REFUSAL_WHY_LIMIT,
    REFUSALS_LIMIT, Refusal, RefusalKind, Registry, Request, RequestId, Requirement, RunId,
    RunState, STARTED_LIMIT, STEPS_LIMIT, Secrets, Unavailable, Value, ValueType,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::fs_shim::Event;
use whizbang_testkit::ledger::FakeLedger;
use whizbang_testkit::schema::validate;

use super::commit::{World, held};
use super::record::schema;
use super::redact::made_up_key;

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

#[test]
fn a_call_whose_admission_is_visible_and_not_confirmed_keeps_what_was_taken_for_it() {
    // The store shows the first call's run, with its admission naming the run that
    // made the call and the amount, though the caller was told of the store's
    // failure. What was taken for it stays taken: the admissions that name a run
    // never add up to more than that run was given.
    let world = World::new();
    let said: Noted<String> = noted();
    let outer = {
        let (fs, root, said) = (world.fs.clone(), world.root(), said.clone());
        action(
            "outer",
            &[Capability::Fill, Capability::Judge],
            move |_, context| {
                let root = root.clone();
                fs.refuse_matching(
                    0,
                    move |event| matches!(event, Event::SyncDir(dir) if *dir == root),
                );
                let note = |what: String| said.lock().unwrap().push(what);
                match context.executor().submit(&Call::new("inner")) {
                    Err(CallError::Store(_)) => note("unconfirmed".to_owned()),
                    other => note(format!("{other:?}")),
                }
                note(format!("{:?} left", context.allowance().left_micros()));
                for _ in 0..2 {
                    match context.executor().submit(&Call::new("inner")) {
                        Ok(_) => note("ran".to_owned()),
                        Err(CallError::Refused(refusal)) => note(format!("{:?}", refusal.kind())),
                        other => note(format!("{other:?}")),
                    }
                }
                Ok(Produced::nothing())
            },
        )
    };
    let inner = action("inner", &[Capability::Fill], |_, _| Ok(Produced::nothing()));
    let bound = Arc::new(Priced(AtomicU64::new(100)));
    let executor = engine_on(world.store(), bound, vec![outer, inner])
        .with_ledger(Arc::new(FakeLedger::new()));
    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(
        *said.lock().unwrap(),
        ["unconfirmed", "Some(100) left", "ran", "OverCaller"]
    );
    let all = records(&world);
    let taken: u64 = all
        .keys()
        .map(|id| admission(&world, id))
        .filter(|admitted| admitted.parent() == Some(finished.run()))
        .map(|admitted| admitted.reserved_micros().unwrap())
        .sum();
    assert_eq!((all.len(), taken), (3, 200));
}

#[test]
fn the_runs_under_one_run_that_nothing_called_are_bounded_at_every_level_together() {
    assert_eq!(DESCENDANTS_LIMIT, 1024);
    let world = World::new();
    let said: Noted<(String, Refusal)> = noted();
    let outer = {
        let said = said.clone();
        act("outer", move |_, context| {
            for _ in 0..4 {
                let called = context.executor().submit(&Call::new("middle"));
                if let Err(CallError::Refused(refusal)) = called {
                    said.lock().unwrap().push(("outer".to_owned(), refusal));
                }
            }
            Ok(Produced::nothing())
        })
    };
    let leaf = act("leaf", |_, _| Ok(Produced::nothing()));
    let actions = vec![outer, calls("middle", "leaf", &said), leaf];
    let executor = engine(world.store(), actions).with_descendants_limit(5);
    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    // Five runs under the first: three of `middle` and two of `leaf`. The third
    // `middle` is refused its `leaf`, and the first run its fourth `middle`.
    let full = |action: &str| Refusal::TreeFull {
        action: action.to_owned(),
        limit: 5,
    };
    assert_eq!(
        *said.lock().unwrap(),
        [
            ("middle".to_owned(), full("leaf")),
            ("outer".to_owned(), full("middle"))
        ]
    );
    let all = records(&world);
    assert_eq!(all.len(), 6);
    let kinds = |stored: &RunRecord| -> Vec<RefusalKind> {
        stored.outcome().refusals.iter().map(|r| r.kind()).collect()
    };
    assert_eq!(
        kinds(&record(&world, finished.run())),
        [RefusalKind::TreeFull]
    );
    let refusing = all.values().flatten().filter(|r| !kinds(r).is_empty());
    assert_eq!(refusing.count(), 2);
    // Another run that nothing called has a count of its own.
    let again = executor.submit(&Call::new("middle")).unwrap();
    assert_eq!(record(&world, again.run()).steps().len(), 1);
}

#[test]
fn a_calls_deadline_is_the_earlier_of_its_callers_and_the_one_its_request_came_with() {
    let world = World::new();
    let now = Instant::now();
    let at = move |hours: u64| now + Duration::from_secs(3600 * hours);
    let seen: Noted<Option<Instant>> = noted();
    let outer = act("outer", move |_, context| {
        let executor = context.executor();
        let inner = || Request::new(Call::new("inner"));
        executor.request(&inner()).unwrap();
        executor.request(&inner().until(at(1))).unwrap();
        executor.request(&inner().until(at(3))).unwrap();
        Ok(Produced::nothing())
    });
    let inner = {
        let seen = seen.clone();
        act("inner", move |_, context| {
            seen.lock().unwrap().push(context.deadline());
            Ok(Produced::nothing())
        })
    };
    let executor = engine(world.store(), vec![outer, inner]);
    let outer = || Request::new(Call::new("outer"));
    executor.request(&outer().until(at(2))).unwrap();
    executor.request(&outer()).unwrap();
    let hours = |hours: u64| Some(at(hours));
    assert_eq!(
        *seen.lock().unwrap(),
        [hours(2), hours(1), hours(2), None, hours(1), hours(3)]
    );
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
