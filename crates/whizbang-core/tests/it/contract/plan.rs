//! A call's plan (ADR 0004, "A call's plan"; plan m4, task K1, item 1): an action that
//! plans is asked, while its call is prepared, how many requests the call may send each
//! capability, how many attempts it may make and whether it checks. The plan decides the
//! needs and the estimate, is in the fingerprint, is held at admission, and is handed to
//! the action with the prepared call.
//!
//! The action here is a fake that plans as a loop would: each attempt is one request of
//! a filler and, when it checks, one of a judge. The backends are fakes whose bounds a
//! test states.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::contract::record::RunRecord;
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Attempts, Backends, Call, CallError, Capability, Clock,
    ConfigSource, Context, Engine, Estimate, Executor, Failure, Kind, ModelIdentity, Plan,
    Planning, PreparedCall, Produced, Readiness, Refusal, Registry, Request, Requirement, RunId,
    Snapshot, SourceRefused, Stale, Unavailable, Value, ValueType,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::Store;
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;

use crate::spend::support as real;

const FILL: u64 = 40_000;
const JUDGE: u64 = 100_000;

struct Stopped;

impl Clock for Stopped {
    fn now_ms(&self) -> u64 {
        7000
    }
}

/// A filler that is always ready and a judge whose key a test takes away, each
/// stating the bound of one request.
struct Bench {
    judge: AtomicBool,
}

impl Backends for Bench {
    fn resolve(
        &self,
        capability: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        match capability {
            Capability::Judge if !self.judge.load(Ordering::SeqCst) => {
                Err(Unavailable::new("its key is not in the environment"))
            }
            other => Ok(ModelIdentity::new("fake", other.name(), "1")),
        }
    }

    fn bound(&self, capability: Capability, _: &ModelIdentity) -> Estimate {
        Estimate::of_micros(match capability {
            Capability::Fill => FILL,
            Capability::Judge => JUDGE,
            Capability::Segment => 0,
        })
    }
}

/// A config of one digest whose cap of one call a test changes.
struct Capped(Mutex<Option<u64>>);

impl ConfigSource for Capped {
    fn snapshot(&self) -> Result<Snapshot, SourceRefused> {
        let snapshot = Snapshot::of(Digest::of_bytes(b"config"));
        Ok(match *self.0.lock().unwrap() {
            Some(cap) => snapshot.capped_at(Estimate::of_micros(cap)),
            None => snapshot,
        })
    }
}

/// `tries`: plans as a loop would, when `plans` says so, and notes the plan each run
/// was handed.
struct Tries {
    descriptor: ActionDescriptor,
    plans: bool,
    handed: Arc<Mutex<Vec<Option<Plan>>>>,
}

fn draft() -> whizbang_core::contract::Draft {
    ActionDescriptor::draft("tries", "1.0.0", Kind::Loop, "A test action that plans.")
        .argument(Argument::with_default(
            "attempts",
            "How many attempts, at most.",
            ValueType::integer(Some(1), Some(3)),
            Value::Integer(3),
        ))
        .argument(Argument::with_default(
            "check",
            "Whether to check.",
            ValueType::choice(&["auto", "required", "off"]),
            Value::text("auto"),
        ))
        .needs(Requirement::new(Capability::Fill))
        .needs(Requirement::new(Capability::Judge))
}

impl Action for Tries {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn plan(&self, planning: &Planning<'_>) -> Option<Plan> {
        if !self.plans {
            return None;
        }
        let Some(Value::Integer(asked)) = planning.argument("attempts") else {
            return None;
        };
        let asked = *asked as u32;
        let mode = match planning.argument("check") {
            Some(Value::Text(mode)) => mode.as_str(),
            _ => "auto",
        };
        let judge = planning.capability(Capability::Judge)?;
        let fill = planning.capability(Capability::Fill)?.bound()?.micros();
        let checked = fill + judge.bound().map_or(0, |bound| bound.micros());
        let covers = |one: u64| match planning.cap() {
            Some(cap) => (cap.micros() / one.max(1)).min(u64::from(asked)) as u32,
            None => asked,
        };
        let (allowed, checks, why) = match mode {
            "off" => (covers(fill).max(1), false, None),
            "required" => (covers(checked).max(1), true, None),
            _ if !judge.is_ready() => (1, false, Some("no judge is ready")),
            _ if covers(checked) == 0 => (1, false, Some("the call cap does not cover a check")),
            _ => (covers(checked), true, None),
        };
        let mut plan = Plan::new()
            .sending(Capability::Fill, u64::from(allowed))
            .sending(Capability::Judge, u64::from(allowed) * u64::from(checks))
            .attempts(asked, allowed, checks);
        if let Some(why) = why {
            plan = plan.because(why);
        } else if allowed < asked {
            plan = plan.because("the call cap covers fewer attempts than were asked");
        }
        Some(plan)
    }

    fn run(&self, call: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        self.handed.lock().unwrap().push(call.plan().cloned());
        Ok(Produced::nothing())
    }
}

struct World {
    _dir: Scratch,
    store: Arc<Store>,
    bench: Arc<Bench>,
    cap: Arc<Capped>,
    handed: Arc<Mutex<Vec<Option<Plan>>>>,
    engine: Engine,
}

/// A world whose `tries` plans, or is declared to and plans nothing, or is not declared
/// to: `(declared, plans)`.
fn world_of(declared: bool, plans: bool) -> World {
    let dir = Scratch::new("plan");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    real::allow(&store, u64::MAX - 1);
    let bench = Arc::new(Bench {
        judge: AtomicBool::new(true),
    });
    let cap = Arc::new(Capped(Mutex::new(None)));
    let handed = Arc::new(Mutex::new(Vec::new()));
    let draft = if declared { draft().plans() } else { draft() };
    let mut registry = Registry::new();
    let tries = Tries {
        descriptor: draft.build().unwrap(),
        plans,
        handed: handed.clone(),
    };
    registry.register(Arc::new(tries)).unwrap();
    let engine = Engine::over(
        registry,
        bench.clone(),
        store.clone(),
        Digest::of_bytes(b"unused"),
    )
    .with_config(cap.clone())
    .with_clock(Arc::new(Stopped))
    .with_ledger(real::Counted::new());
    World {
        _dir: dir,
        store,
        bench,
        cap,
        handed,
        engine,
    }
}

fn world() -> World {
    world_of(true, true)
}

impl World {
    fn cap(&self, cap: Option<u64>) {
        *self.cap.0.lock().unwrap() = cap;
    }

    fn record(&self, run: &RunId) -> RunRecord {
        let generation = self.store.snapshot().unwrap();
        let read = |entry: &_| self.store.read(entry);
        whizbang_core::contract::record::stored(&generation, run.as_str(), read)
            .unwrap()
            .unwrap()
    }

    fn runs(&self) -> usize {
        self.store.snapshot().unwrap().runs().len()
    }
}

fn tries() -> Call {
    Call::new("tries")
}

fn stated(prepared: &PreparedCall) -> (u32, u32, bool) {
    let Attempts {
        asked,
        allowed,
        checks,
    } = prepared.plan().unwrap().stated().unwrap();
    (asked, allowed, checks)
}

fn capabilities(prepared: &PreparedCall) -> Vec<Capability> {
    prepared.needs().iter().map(|n| n.capability()).collect()
}

mod prepare {
    use super::*;

    #[test]
    fn an_action_that_plans_is_prepared_with_the_needs_and_the_estimate_of_its_plan() {
        let world = world();
        let call = tries().argument("attempts", Value::Integer(2));
        let prepared = world.engine.prepare(&call).unwrap();
        assert_eq!(stated(&prepared), (2, 2, true));
        assert_eq!(*prepared.readiness(), Readiness::Ready);
        assert_eq!(
            capabilities(&prepared),
            [Capability::Fill, Capability::Judge]
        );
        // Two attempts, each one request of the filler and one of the judge: not the one
        // request of each that the descriptor states.
        assert_eq!(prepared.estimate().micros(), 2 * (FILL + JUDGE));
        let plan = prepared.plan().unwrap();
        assert_eq!(plan.requests(Capability::Fill), Some(2));
        assert_eq!(plan.requests(Capability::Judge), Some(2));
        // Preparing it made nothing.
        assert_eq!(world.runs(), 0);
        assert!(world.handed.lock().unwrap().is_empty());
    }

    #[test]
    fn a_plan_fitted_to_the_call_cap_says_what_was_asked_and_what_it_allows() {
        let world = world();
        world.cap(Some(300_000));
        let prepared = world.engine.prepare(&tries()).unwrap();
        // Three were asked; the cap covers two at 140,000 each.
        assert_eq!(stated(&prepared), (3, 2, true));
        assert_eq!(prepared.estimate().micros(), 280_000);
        let plan = prepared.plan().unwrap();
        assert_eq!(
            plan.why(),
            Some("the call cap covers fewer attempts than were asked")
        );
        assert_eq!(
            plan.line(),
            "plan: 2 of 3 attempts asked, checked; fill 2; judge 2; the call cap covers fewer \
             attempts than were asked"
        );
        // A cap that covers a removal and no check: one attempt, unchecked, and why.
        world.cap(Some(50_000));
        let prepared = world.engine.prepare(&tries()).unwrap();
        assert_eq!(stated(&prepared), (3, 1, false));
        assert_eq!(prepared.estimate().micros(), FILL);
        assert_eq!(
            prepared.plan().unwrap().why(),
            Some("the call cap does not cover a check")
        );
    }

    #[test]
    fn a_capability_the_plan_leaves_out_is_not_resolved_and_nothing_is_reserved_for_it() {
        let world = world();
        world.bench.judge.store(false, Ordering::SeqCst);
        let prepared = world.engine.prepare(&tries()).unwrap();
        // No judge is ready: the plan is one attempt, unchecked. The judge is no need of
        // the call, is not missing, and adds nothing to the estimate.
        assert_eq!(stated(&prepared), (3, 1, false));
        assert_eq!(capabilities(&prepared), [Capability::Fill]);
        assert_eq!(*prepared.readiness(), Readiness::Ready);
        assert_eq!(prepared.estimate().micros(), FILL);
        // And the run reserves for the filler alone.
        let finished = world.engine.submit(&tries()).unwrap();
        let record = world.record(finished.run());
        assert_eq!(record.spend().reserved_micros, Some(FILL));
        // Asked off, the judge is left out though it is ready.
        world.bench.judge.store(true, Ordering::SeqCst);
        let off = tries().argument("check", Value::text("off"));
        let prepared = world.engine.prepare(&off).unwrap();
        assert_eq!(capabilities(&prepared), [Capability::Fill]);
        assert_eq!(prepared.estimate().micros(), 3 * FILL);
        // Required, a judge that is not ready is what is missing.
        world.bench.judge.store(false, Ordering::SeqCst);
        let required = tries().argument("check", Value::text("required"));
        let prepared = world.engine.prepare(&required).unwrap();
        let Readiness::NotReady(missing) = prepared.readiness() else {
            panic!("ready without the judge it requires");
        };
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].capability(), Capability::Judge);
        assert_eq!(missing[0].why(), "its key is not in the environment");
    }

    #[test]
    fn the_plan_is_in_the_fingerprint_of_the_prepared_call() {
        let world = world();
        let whole = world.engine.prepare(&tries()).unwrap();
        let again = world.engine.prepare(&tries()).unwrap();
        assert_eq!(whole.fingerprint(), again.fingerprint());
        // The same call, inputs, config digest and models, and another plan.
        world.cap(Some(300_000));
        let fitted = world.engine.prepare(&tries()).unwrap();
        assert_eq!(whole.config(), fitted.config());
        assert_eq!(whole.needs(), fitted.needs());
        assert_ne!(whole.plan(), fitted.plan());
        assert_ne!(whole.fingerprint(), fitted.fingerprint());
        // And it is not the fingerprint the same call has with no plan.
        let plain = world_of(false, false);
        let unplanned = plain.engine.prepare(&tries()).unwrap();
        assert_eq!(unplanned.needs(), whole.needs());
        assert_ne!(unplanned.fingerprint(), whole.fingerprint());
    }

    #[test]
    fn an_action_that_plans_nothing_is_prepared_as_before() {
        // Not declared to plan: never asked, though it would answer.
        let undeclared = world_of(false, true);
        // Declared, and planning nothing for this call.
        let silent = world_of(true, false);
        let mut fingerprints = Vec::new();
        for world in [&undeclared, &silent] {
            let prepared = world.engine.prepare(&tries()).unwrap();
            assert_eq!(prepared.plan(), None);
            // The descriptor's fixed needs, one request each.
            assert_eq!(
                capabilities(&prepared),
                [Capability::Fill, Capability::Judge]
            );
            assert_eq!(prepared.estimate().micros(), FILL + JUDGE);
            fingerprints.push(prepared.fingerprint());
            // A capability that is missing makes the call not ready, as it always did.
            world.bench.judge.store(false, Ordering::SeqCst);
            let prepared = world.engine.prepare(&tries()).unwrap();
            assert!(matches!(prepared.readiness(), Readiness::NotReady(_)));
            assert_eq!(capabilities(&prepared).len(), 2);
            world.bench.judge.store(true, Ordering::SeqCst);
            // Its run is handed no plan, and its record is of the version before.
            let finished = world.engine.submit(&tries()).unwrap();
            assert_eq!(*world.handed.lock().unwrap(), [None]);
            let record = world.record(finished.run());
            assert_eq!(record.version(), "run/4");
            assert_eq!(record.plan(), None);
        }
        // The fingerprint is of `prepared/1`, whose fixed vectors
        // (`prepare::the_fingerprint_is_these_bytes_hashed`) hold as they did.
        assert_eq!(fingerprints[0], fingerprints[1]);
    }

    #[test]
    fn a_plan_that_is_no_plan_refuses_the_call() {
        struct Bad(ActionDescriptor, Plan);
        impl Action for Bad {
            fn descriptor(&self) -> &ActionDescriptor {
                &self.0
            }
            fn plan(&self, _: &Planning<'_>) -> Option<Plan> {
                Some(self.1.clone())
            }
        }
        let bad = [
            Plan::new().sending(Capability::Segment, 1),
            Plan::new()
                .sending(Capability::Fill, 1)
                .sending(Capability::Fill, 2),
            Plan::new().attempts(2, 3, true),
            Plan::new().attempts(2, 0, true),
            Plan::new().attempts(17, 1, true),
            Plan::new().because(""),
            Plan::new().because("two\nlines"),
            Plan::new().because(&"w".repeat(201)),
        ];
        for plan in bad {
            let mut registry = Registry::new();
            let descriptor = draft().plans().build().unwrap();
            let action = Bad(descriptor, plan.clone());
            registry.register(Arc::new(action)).unwrap();
            let world = world();
            let engine = Engine::new(
                registry,
                world.bench.clone(),
                world.store.clone(),
                Digest::of_bytes(b""),
            );
            let refused = engine.prepare(&tries()).unwrap_err();
            assert!(
                matches!(&refused, CallError::Refused(Refusal::Invalid { why, .. })
                    if why.starts_with("its action planned what is no plan: ")),
                "{plan:?}: {refused:?}"
            );
        }
        // A descriptor that plans names no capability in two needs.
        let twice = draft().needs(Requirement::new(Capability::Fill)).plans();
        assert!(twice.build().unwrap_err().why.contains("the action plans"));
    }
}

mod admit {
    use super::*;

    fn id(executor: &Engine, text: &str) -> whizbang_core::contract::RequestId {
        executor.request_id(text).unwrap()
    }

    #[test]
    fn a_preparation_whose_plan_would_now_differ_is_refused_as_stale() {
        let stale = |world: &World, prepared: &PreparedCall| {
            let request = Request::new(tries()).expecting(prepared);
            let refused = world.engine.request(&request).unwrap_err();
            assert_eq!(
                refused,
                CallError::Refused(Refusal::Stale {
                    action: "tries".to_owned(),
                    why: Stale::Plan,
                })
            );
            // No plan is fitted at admission: nothing was admitted and nothing ran.
            assert_eq!(world.runs(), 0);
            assert!(world.handed.lock().unwrap().is_empty());
        };
        // A judge's key that has gone.
        let world = world();
        let prepared = world.engine.prepare(&tries()).unwrap();
        world.bench.judge.store(false, Ordering::SeqCst);
        stale(&world, &prepared);
        // One that has appeared.
        let unchecked = world.engine.prepare(&tries()).unwrap();
        assert_eq!(stated(&unchecked), (3, 1, false));
        world.bench.judge.store(true, Ordering::SeqCst);
        stale(&world, &unchecked);
        // A call cap that changed, under a config of the same digest.
        let whole = world.engine.prepare(&tries()).unwrap();
        world.cap(Some(300_000));
        stale(&world, &whole);
        assert_eq!(
            Stale::Plan.to_string(),
            "the plan of the call would be another now"
        );
        // The preparation that still holds is admitted, and runs under its plan.
        let fitted = world.engine.prepare(&tries()).unwrap();
        let request = Request::new(tries()).expecting(&fitted);
        world.engine.request(&request).unwrap();
        assert_eq!(*world.handed.lock().unwrap(), [fitted.plan().cloned()]);
    }

    #[test]
    fn a_repeated_request_id_answers_with_the_run_and_the_plan_of_its_first_admission() {
        let world = world();
        let request = Request::new(tries()).with_id(id(&world.engine, "order-1"));
        let first = real::ran(world.engine.request(&request));
        let plan = world.record(first.run()).plan().cloned().unwrap();
        assert_eq!(plan.stated().map(|a| (a.asked, a.allowed)), Some((3, 3)));
        // The cap changes, so the same call would be planned otherwise now.
        world.cap(Some(300_000));
        let now = world.engine.prepare(&tries()).unwrap();
        assert_eq!(stated(&now), (3, 2, true));
        // The same request again is answered with the first run, under the plan it had.
        let again = match world.engine.request(&request) {
            Ok(whizbang_core::contract::Answer::Repeat(earlier)) => earlier,
            other => panic!("not answered as a repeat: {other:?}"),
        };
        assert_eq!(again.run(), first.run());
        assert_eq!(world.record(again.run()).plan(), Some(&plan));
        assert_eq!(world.runs(), 1);
        assert_eq!(world.handed.lock().unwrap().len(), 1);
    }
}

mod executor {
    use super::*;

    #[test]
    fn an_action_is_handed_the_plan_it_was_prepared_with() {
        let world = world();
        world.cap(Some(300_000));
        let prepared = world.engine.prepare(&tries()).unwrap();
        let finished = world.engine.submit(&tries()).unwrap();
        assert_eq!(finished.outcome().name(), "completed");
        // What ran is what was prepared, and what was reserved for.
        let handed = world.handed.lock().unwrap().clone();
        assert_eq!(handed, [prepared.plan().cloned()]);
        let record = world.record(finished.run());
        assert_eq!(record.version(), "run/6");
        assert_eq!(record.plan(), prepared.plan());
        assert_eq!(record.spend().reserved_micros, Some(280_000));
    }
}
