//! The meter (plan 1B, task S5): before a paid request leaves it is quoted, its bound
//! is debited from its run's allowance, and its dispatch is made durable; only then
//! is the backend called. Each test is through an executor built with the real
//! ledger and the meter in the meter's place.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::capability::{
    Bounds, Declaration, Fault, FillRequest, Filler, Painted, Picture, Price, Stage, Support,
};
use whizbang_core::config::Config;
use whizbang_core::contract::record::Status;
use whizbang_core::contract::{
    Action, Answer, Call, CallError, Capabilities, Capability, Engine, Executor, Failure, Place,
    Produced, RefusalKind, Registry, Request, RunId, RunState, Secrets,
};
use whizbang_core::egress::{DispatchPolicy, Origin, Recipient, SendCheck, Ways};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::spend::{ClosedBy, Meter, PerCall, Record, StoreLedger, Sum};
use whizbang_core::store::GENERATION_MAX_BYTES;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::{FakeFiller, Paint, roster};
use whizbang_testkit::sending::FakeSender;

use super::support::{Said, World, allow, books, plant, plant_bytes, ran, reserved, spend_names};

/// What one request to the priced filler may cost, in millionths.
const PRICE: u64 = 250_000;

fn priced() -> FakeFiller {
    FakeFiller::new(Paint::Inverse).priced(Price::at_most(250_000.0).unwrap())
}

fn fill_request() -> FillRequest {
    let rgba = (0..24u8).flat_map(|at| [at, at, at, 255]).collect();
    let picture = Picture::new(CanonicalPixels::from_rgba(6, 4, rgba).unwrap()).unwrap();
    let mut plane = vec![0; 24];
    plane[..6].fill(255);
    FillRequest::new(picture, Support::from_plane(6, 4, plane).unwrap(), None).unwrap()
}

/// An executor that holds this filler, with the real ledger and the meter: a call
/// reserves the price of one request times `requests`.
fn metered(
    world: &World,
    filler: Arc<dyn Filler>,
    requests: u64,
    actions: Vec<Arc<dyn Action>>,
) -> Engine {
    let mut registry = Registry::new();
    for action in actions {
        registry.register(action).unwrap();
    }
    let offered = Arc::new(roster(None, Some(filler.as_ref()), None));
    let backends = PerCall::new(offered, NonZeroU64::new(requests).unwrap());
    let meter = Meter::new(world.store.clone(), world.clock.clone());
    Engine::over(
        registry,
        Arc::new(backends),
        world.store.clone(),
        Digest::of_bytes(b""),
    )
    .with_clock(world.clock.clone())
    .with_capabilities(Capabilities::none().with_fill(filler))
    .with_ledger(Arc::new(StoreLedger))
    .with_wrapper(Place::Meter, Arc::new(meter))
}

/// What each request of an action came to, and what its run could spend after it.
type Asked = Said<(Result<(), Fault>, Option<u64>)>;

/// `asks`: an action that asks its filler this many times, and keeps what each
/// request came to and what its run had left afterwards.
fn asks(world: &World, times: usize, asked: &Asked) -> Arc<dyn Action> {
    let asked = asked.clone();
    world.action("asks", &[Capability::Fill], move |_, context| {
        let fill = context.capabilities().fill().unwrap();
        for _ in 0..times {
            let answer = fill.fill(&fill_request()).map(drop);
            let left = context.allowance().left_micros();
            asked.lock().unwrap().push((answer, left));
        }
        Ok(Produced::nothing())
    })
}

fn call() -> Request {
    Request::new(Call::new("asks"))
}

/// The names of the ledger's that a dispatch and a settlement make, of those the
/// store holds.
fn sent(world: &World) -> Vec<String> {
    let made = |name: &String| {
        ["place", "dispatched", "settling", "settled"]
            .iter()
            .any(|word| name.starts_with(&format!("spend/{word}/")))
    };
    spend_names(&world.store).into_iter().filter(made).collect()
}

fn dispatch_names(run: &RunId, place: u64, by: &RunId, micros: u64) -> [String; 2] {
    [
        format!("spend/dispatched/{run}/{place}/{by}/{micros}"),
        format!("spend/place/{run}/{place}"),
    ]
}

/// What the books say is outstanding under a run's reservation.
fn outstanding(world: &World, run: &RunId) -> Sum {
    let books = books(&world.store);
    books.reservation(run).unwrap().names().outstanding()
}

#[test]
fn the_backend_stopped_at_its_barrier_finds_its_own_dispatch_durable_before_it_was_called() {
    let world = World::new("meter-dispatch-first");
    allow(&world.store, 1_000_000);
    let barriers = Barriers::new();
    let filler = Arc::new(priced().stopping_at(&barriers, "asked"));
    let asked: Asked = Arc::default();
    let engine = metered(&world, filler.clone(), 1, vec![asks(&world, 1, &asked)]);
    let Ok(Answer::Started(run)) = engine.start(&call()) else {
        panic!("the request was not started");
    };
    // The backend holds the request and has not answered. What the store's folder
    // holds, read by a reader of its own, names the dispatch already.
    barriers.arrived("asked", 1);
    let ((_, names, _), _) = world.held();
    for name in dispatch_names(&run, 1, &run, PRICE) {
        assert!(names.contains(&name), "{name} is not in {names:?}");
    }
    assert_eq!(asked.lock().unwrap().len(), 0, "the request is not over");
    // The record under the dispatch's name says what the name says, and when.
    let generation = world.store.snapshot().unwrap();
    let name = &dispatch_names(&run, 1, &run, PRICE)[0];
    let bytes = world.store.read(generation.get(name).unwrap()).unwrap();
    let Record::Dispatch {
        by, bound, at_ms, ..
    } = Record::from_bytes(&bytes).unwrap()
    else {
        panic!("not a dispatch");
    };
    assert_eq!((by, bound.micros(), at_ms), (run.clone(), PRICE, 7000));

    barriers.open("asked");
    let state = engine.wait(&run).unwrap().state().clone();
    assert!(matches!(state, RunState::Ended(_)), "{state:?}");
    assert_eq!(filler.requests(), 1);
    assert!(asked.lock().unwrap()[0].0.is_ok());
}

#[test]
fn a_request_over_what_its_run_has_left_is_refused_unsent_with_no_dispatch_and_is_in_its_runs_record()
 {
    let world = World::new("meter-over-run");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(priced());
    let asked: Asked = Arc::default();
    // The call reserves the price of one request, and its action asks twice and
    // catches what the second comes to.
    let engine = metered(&world, filler.clone(), 1, vec![asks(&world, 2, &asked)]);
    let run = ran(engine.request(&call())).run().clone();

    let asked = asked.lock().unwrap().clone();
    assert!(asked[0].0.is_ok());
    assert_eq!(asked[0].1, Some(0), "the first was debited its bound");
    let fault = asked[1].0.clone().unwrap_err();
    assert_eq!(fault.stage(), Stage::NotSent);
    assert_eq!(fault.charge(), None);
    // The backend counted one request, and the store names one dispatch.
    assert_eq!(filler.requests(), 1);
    assert_eq!(sent(&world), dispatch_names(&run, 1, &run, PRICE));
    // The run completed, and its record has the refusal: what, and why.
    let record = world.record(&run);
    assert_eq!(record.outcome().status, Status::Completed);
    let [refused] = record.outcome().refusals.as_slice() else {
        panic!("{:?}", record.outcome().refusals);
    };
    assert_eq!(refused.kind(), RefusalKind::RequestOverAllowance);
    assert_eq!(refused.action(), None);
    assert_eq!(
        refused.why(),
        "the request was not sent: it may cost 250000 millionths of a US dollar, and its \
         run has 0 left"
    );
}

#[test]
fn a_request_to_a_backend_with_no_bound_is_refused_by_the_meter_whether_or_not_its_run_reserved() {
    // An executor with no ledger admits the call and reserves nothing: the request
    // is refused all the same.
    let world = World::new("meter-no-bound");
    let filler = Arc::new(FakeFiller::new(Paint::Inverse).priced(Price::about(9.0).unwrap()));
    let asked: Asked = Arc::default();
    let mut registry = Registry::new();
    registry.register(asks(&world, 1, &asked)).unwrap();
    let backends = roster(None, Some(filler.as_ref()), None);
    let digest = Digest::of_bytes(b"");
    let meter = Meter::new(world.store.clone(), world.clock.clone());
    let engine = Engine::over(registry, Arc::new(backends), world.store.clone(), digest)
        .with_capabilities(Capabilities::none().with_fill(filler.clone()))
        .with_wrapper(Place::Meter, Arc::new(meter));
    let run = ran(engine.request(&call())).run().clone();
    let fault = asked.lock().unwrap()[0].0.clone().unwrap_err();
    assert_eq!(fault.stage(), Stage::NotSent);
    assert_eq!((filler.requests(), sent(&world)), (0, vec![]));
    let record = world.record(&run);
    let [refused] = record.outcome().refusals.as_slice() else {
        panic!("{:?}", record.outcome().refusals);
    };
    assert_eq!(refused.kind(), RefusalKind::RequestNoBound);
    assert!(refused.why().contains("an estimate only"), "{refused:?}");

    // With a bound, a run nothing was reserved for sends its request, and nothing
    // is recorded: there is no reservation to record it under.
    let filler = Arc::new(priced());
    let asked: Asked = Arc::default();
    let mut registry = Registry::new();
    registry.register(asks(&world, 1, &asked)).unwrap();
    let backends = roster(None, Some(filler.as_ref()), None);
    let meter = Meter::new(world.store.clone(), world.clock.clone());
    let engine = Engine::over(registry, Arc::new(backends), world.store.clone(), digest)
        .with_capabilities(Capabilities::none().with_fill(filler.clone()))
        .with_wrapper(Place::Meter, Arc::new(meter));
    ran(engine.request(&call()));
    assert!(asked.lock().unwrap()[0].0.is_ok());
    assert_eq!((filler.requests(), spend_names(&world.store)), (1, vec![]));
}

#[test]
fn a_request_the_gate_refused_has_no_dispatch_record() {
    let world = World::new("meter-gate");
    allow(&world.store, 1_000_000);
    // The policy a config that names no recipient makes approves nothing.
    let config = Config::parse(b"", &Secrets::none()).unwrap();
    let approvals = config.recipients().map(|(_, approval)| approval);
    let policy = DispatchPolicy::from_approvals(approvals).unwrap();
    let ways = Ways::through(Arc::new(FakeSender::scripted(vec![])))
        .checked_by(Arc::new(SendCheck::new()))
        .under(policy);
    let origin = Origin::of("https://api.example.com", &Secrets::none()).unwrap();
    let filler = Arc::new(priced().sending_to(Recipient::new(origin)));
    let asked: Asked = Arc::default();
    let engine = metered(&world, filler.clone(), 1, vec![asks(&world, 1, &asked)]).with_ways(ways);
    let run = ran(engine.request(&call())).run().clone();

    let (answer, left) = asked.lock().unwrap()[0].clone();
    assert_eq!(answer.unwrap_err().stage(), Stage::NotSent);
    assert_eq!(left, Some(PRICE), "nothing was debited");
    assert_eq!((filler.requests(), sent(&world)), (0, vec![]));
    let record = world.record(&run);
    let kinds: Vec<RefusalKind> = record
        .outcome()
        .refusals
        .iter()
        .map(|one| one.kind())
        .collect();
    assert_eq!(kinds, [RefusalKind::RequestNotApproved]);
}

#[test]
fn a_free_request_to_a_local_backend_makes_no_record() {
    let world = World::new("meter-free");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let asked: Asked = Arc::default();
    let engine = metered(&world, filler.clone(), 1, vec![asks(&world, 2, &asked)]);
    ran(engine.request(&call()));
    let asked = asked.lock().unwrap().clone();
    assert!(asked.iter().all(|(answer, _)| answer.is_ok()), "{asked:?}");
    // The run reserved nothing and was debited nothing, and the store names no
    // dispatch.
    assert_eq!(asked[1].1, Some(0));
    assert_eq!((filler.requests(), sent(&world)), (2, vec![]));
}

#[test]
fn a_dispatch_the_store_refuses_at_its_generation_limit_sends_nothing_and_its_debit_is_undone() {
    let world = World::new("meter-limit");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(priced());
    let seen: Asked = Arc::default();
    let asks = {
        let (seen, store) = (seen.clone(), world.store.clone());
        world.action("asks", &[Capability::Fill], move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            // From here the store takes no publication that adds a name.
            store.limit_generations_to(1);
            let answer = fill.fill(&fill_request()).map(drop);
            seen.lock()
                .unwrap()
                .push((answer, context.allowance().left_micros()));
            // There is room again: the request is sent.
            store.limit_generations_to(GENERATION_MAX_BYTES);
            let answer = fill.fill(&fill_request()).map(drop);
            seen.lock()
                .unwrap()
                .push((answer, context.allowance().left_micros()));
            Ok(Produced::nothing())
        })
    };
    let engine = metered(&world, filler.clone(), 1, vec![asks]);
    let run = ran(engine.request(&call())).run().clone();
    let seen = seen.lock().unwrap().clone();
    let fault = seen[0].0.clone().unwrap_err();
    assert_eq!(fault.stage(), Stage::NotSent);
    assert_eq!(seen[0].1, Some(PRICE), "the debit was undone");
    assert!(seen[1].0.is_ok(), "{:?}", seen[1]);
    assert_eq!(seen[1].1, Some(0));
    // One request reached the backend, and the store names its dispatch alone, at
    // the first place: the refused one took none.
    assert_eq!(filler.requests(), 1);
    assert_eq!(sent(&world), dispatch_names(&run, 1, &run, PRICE));
    let record = world.record(&run);
    let [refused] = record.outcome().refusals.as_slice() else {
        panic!("{:?}", record.outcome().refusals);
    };
    assert!(
        refused.why().contains("could not be judged or recorded"),
        "{refused:?}"
    );
}

/// A filler that panics when it is asked, and counts.
struct Panicking(Declaration, AtomicU64);

impl Filler for Panicking {
    fn declaration(&self) -> &Declaration {
        &self.0
    }

    fn fill(&self, _: &FillRequest, _: &Bounds) -> Result<Painted, Fault> {
        self.1.fetch_add(1, Ordering::SeqCst);
        panic!("the backend went wrong")
    }
}

#[test]
fn a_backend_that_panics_under_the_meter_leaves_the_run_abandoned_and_its_request_counted_at_its_bound()
 {
    let world = World::new("meter-panic");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(Panicking(priced().declaration().clone(), AtomicU64::new(0)));
    // The action catches nothing: the panic unwinds through it.
    let asks = world.action("asks", &[Capability::Fill], |_, context| {
        let fill = context.capabilities().fill().unwrap();
        fill.fill(&fill_request())
            .map(drop)
            .map_err(|fault| Failure::error("fill", fault.sentence()))?;
        Ok(Produced::nothing())
    });
    let engine = metered(&world, filler.clone(), 1, vec![asks]);
    let failed = engine.request(&call()).unwrap_err();
    let CallError::Abandoned { run, why } = &failed else {
        panic!("{failed:?}");
    };
    assert!(why.contains("a backend panicked under the meter"), "{why}");
    assert_eq!(filler.1.load(Ordering::SeqCst), 1);
    let state = engine.status(run).unwrap().state().clone();
    assert!(matches!(state, RunState::Abandoned { .. }), "{state:?}");
    // Its dispatch is durable and nothing settles it: the books count the request
    // at its bound, under a reservation that is not closed.
    assert_eq!(sent(&world), dispatch_names(run, 1, run, PRICE));
    assert_eq!(outstanding(&world, run), Sum::new(PRICE.into()));
    let books = books(&world.store);
    let reservation = books.reservation(run).unwrap();
    assert!(!reservation.closed());
    assert_eq!(reservation.counts(), Sum::new(PRICE.into()));
}

#[test]
fn a_reservation_whose_names_read_as_frozen_or_closed_takes_no_dispatch() {
    type Hands = Box<dyn Fn(&World, &RunId) + Send + Sync>;
    let closes: Hands = Box::new(|world, run| {
        let place = NonZeroU64::new(1).unwrap();
        plant(
            &world.store,
            &[
                Record::Closing {
                    run: run.clone(),
                    released: false,
                    by: ClosedBy::End,
                    charged: Sum::ZERO,
                    unresolved: Sum::ZERO,
                    at_ms: 0,
                },
                Record::Place {
                    run: run.clone(),
                    place,
                    by: whizbang_core::spend::PlaceBy::Closing,
                },
            ],
        );
    });
    // A name of no form under the reservation: one the ledger could not have written.
    let strays: Hands = Box::new(|world, run| {
        plant_bytes(&world.store, &[(format!("spend/place/{run}/x"), vec![])]);
    });
    for (label, hands, says) in [
        ("meter-closed", closes, "its reservation is closed"),
        ("meter-frozen", strays, "its reservation is frozen"),
    ] {
        let world = Arc::new(World::new(label));
        allow(&world.store, 1_000_000);
        let filler = Arc::new(priced());
        let seen: Arc<Mutex<Vec<Fault>>> = Arc::default();
        let asks = {
            let (seen, world) = (seen.clone(), world.clone());
            let hands = Mutex::new(Some(hands));
            world
                .clone()
                .action("asks", &[Capability::Fill], move |_, context| {
                    // The one run of this store is the action's own.
                    let run = reserved(&world.store)[0].run().clone();
                    if let Some(hands) = hands.lock().unwrap().take() {
                        hands(&world, &run);
                    }
                    let fill = context.capabilities().fill().unwrap();
                    seen.lock()
                        .unwrap()
                        .push(fill.fill(&fill_request()).unwrap_err());
                    assert_eq!(context.allowance().left_micros(), Some(PRICE));
                    Ok(Produced::nothing())
                })
        };
        let engine = metered(&world, filler.clone(), 1, vec![asks]);
        let run = ran(engine.request(&call())).run().clone();
        assert_eq!(seen.lock().unwrap()[0].stage(), Stage::NotSent, "{label}");
        assert_eq!(filler.requests(), 0, "{label}");
        let dispatched = format!("spend/dispatched/{run}/");
        assert!(
            !spend_names(&world.store)
                .iter()
                .any(|name| name.starts_with(&dispatched)),
            "{label}"
        );
        let record = world.record(&run);
        let why = record.outcome().refusals[0].why().to_owned();
        assert!(why.contains(says), "{label}: {why}");
    }
}
