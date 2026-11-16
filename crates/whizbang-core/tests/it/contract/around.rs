//! A wrapper around each request (ADR 0015; plan 1B, task I3): the three places
//! between a handle and its backend, their order, what a refusal and a failure of one
//! come to, and the gate that stands in the first.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::capability::{
    Bounds, Controls, Declaration, Fault, FaultKind, FillRequest, Filler, JudgeRequest, Message,
    Painted, Part, Picture, Price, Query, Role, SegmentRequest, Stage, StopReason, Support, Turn,
};
use whizbang_core::contract::Secrets;
use whizbang_core::contract::record::{Handed, RunRecord, Status};
use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Call, CallError, Capabilities, Capability, Clock, Context,
    Engine, Executor, Failure, Kind, Place, PreparedCall, Produced, Refusal, RefusalKind, Registry,
    Request, RequestId, Requirement, RunId, RunState,
};
use whizbang_core::egress::{Classes, DataClass, Origin, Recipient};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::store::Store;
use whizbang_testkit::around::{FakeWrapper, Saw};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::{FakeFiller, FakeJudge, FakeSegmenter, Paint, roster};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::ledger::FakeLedger;
use whizbang_testkit::scratch::Scratch;

// --- what the tests are made of ----------------------------------------------------------------

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

fn action(
    name: &str,
    needs: &[Capability],
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let mut draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.");
    for need in needs {
        draft = draft.needs(Requirement::new(*need));
    }
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

fn request(prompt: Option<&str>) -> FillRequest {
    let rgba = (0..24u8).flat_map(|at| [at, at, at, 255]).collect();
    let picture = Picture::new(CanonicalPixels::from_rgba(6, 4, rgba).unwrap()).unwrap();
    let mut plane = vec![0; 24];
    plane[..6].fill(255);
    FillRequest::new(picture, Support::from_plane(6, 4, plane).unwrap(), prompt).unwrap()
}

struct World {
    _dir: Scratch,
    store: Arc<Store>,
}

impl World {
    fn new() -> World {
        let dir = Scratch::new("around");
        let store = Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap();
        World {
            _dir: dir,
            store: Arc::new(store),
        }
    }

    /// An executor that holds this filler, with these actions.
    fn engine(&self, filler: Arc<dyn Filler>, actions: Vec<Arc<dyn Action>>) -> Engine {
        let mut registry = Registry::new();
        for action in actions {
            registry.register(action).unwrap();
        }
        let backends = roster(None, Some(filler.as_ref()), None);
        Engine::over(
            registry,
            Arc::new(backends),
            self.store.clone(),
            Digest::of_bytes(b""),
        )
        .with_clock(Arc::new(Stopped(7000)))
        .with_capabilities(Capabilities::none().with_fill(filler))
    }

    /// An executor that holds this segmenter and this judge, with these actions.
    fn engine_of_two(
        &self,
        segmenter: Arc<FakeSegmenter>,
        judge: Arc<FakeJudge>,
        actions: Vec<Arc<dyn Action>>,
    ) -> Engine {
        let mut registry = Registry::new();
        for action in actions {
            registry.register(action).unwrap();
        }
        let backends = roster(Some(segmenter.as_ref()), None, Some(judge.as_ref()));
        let held = Capabilities::none()
            .with_segment(segmenter)
            .with_judge(judge);
        Engine::over(
            registry,
            Arc::new(backends),
            self.store.clone(),
            Digest::of_bytes(b""),
        )
        .with_clock(Arc::new(Stopped(7000)))
        .with_capabilities(held)
    }

    fn record(&self, run: &RunId) -> Option<RunRecord> {
        let generation = self.store.snapshot().unwrap();
        let entries = generation.run(run.as_str()).unwrap().entries();
        let entry = entries.get("record")?;
        Some(RunRecord::from_bytes(&self.store.read(entry).unwrap()).unwrap())
    }

    fn runs(&self) -> Vec<RunId> {
        let generation = self.store.snapshot().unwrap();
        let runs = generation.runs().keys();
        runs.map(|id| RunId::parse(id).unwrap()).collect()
    }
}

/// `asks`: an action that asks its filler once, with no prompt, and keeps what it
/// was answered with.
type Answered = Arc<Mutex<Vec<Result<(), Fault>>>>;

fn asks(answered: &Answered) -> Arc<dyn Action> {
    let answered = answered.clone();
    action("asks", &[Capability::Fill], move |_, context| {
        let fill = context.capabilities().fill().unwrap();
        let answer = fill.fill(&request(None)).map(drop);
        answered.lock().unwrap().push(answer);
        Ok(Produced::nothing())
    })
}

/// The fault its one request was answered with.
fn the_fault(answered: &Answered) -> Fault {
    let all = answered.lock().unwrap().clone();
    assert_eq!(all.len(), 1, "{all:?}");
    all.into_iter().next().unwrap().unwrap_err()
}

/// Three fake wrappers, one for each place, that write in one journal.
fn three() -> (FakeWrapper, FakeWrapper, FakeWrapper) {
    let gate = FakeWrapper::new("gate");
    let (lease, meter) = (gate.beside("lease"), gate.beside("meter"));
    (gate, lease, meter)
}

fn wrapped(engine: Engine, three: &(FakeWrapper, FakeWrapper, FakeWrapper)) -> Engine {
    engine
        .with_wrapper(Place::Gate, Arc::new(three.0.clone()))
        .with_wrapper(Place::Lease, Arc::new(three.1.clone()))
        .with_wrapper(Place::Meter, Arc::new(three.2.clone()))
}

fn ran(answer: Result<Answer, CallError>) -> RunId {
    match answer {
        Ok(Answer::Ran(finished)) => finished.run().clone(),
        other => panic!("the request did not run: {other:?}"),
    }
}

fn of_kind(fault: &Fault) -> (FaultKind, Stage) {
    (fault.kind(), fault.stage())
}

// --- the order ---------------------------------------------------------------------------------

#[test]
fn three_wrappers_see_a_request_in_the_stated_order_and_none_sees_one_the_handles_own_checks_refused()
 {
    let world = World::new();
    // The filler declares no prompt: a request with one is refused by the handle's
    // own checks, before anything else sees it.
    let filler = Arc::new(FakeFiller::new(Paint::Inverse).declaring(&[]).charging(7));
    let wrappers = three();
    let seen: Arc<Mutex<Vec<(FaultKind, Stage)>>> = Arc::default();
    let inner = {
        let seen = seen.clone();
        action("inner", &[Capability::Fill], move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            fill.fill(&request(None)).unwrap();
            let unsupported = fill.fill(&request(Some("a red door"))).unwrap_err();
            seen.lock().unwrap().push(of_kind(&unsupported));
            Ok(Produced::nothing())
        })
    };
    let outer = action("outer", &[], |_, context| {
        context.executor().submit(&Call::new("inner")).unwrap();
        Ok(Produced::nothing())
    });
    let engine = wrapped(world.engine(filler.clone(), vec![outer, inner]), &wrappers);
    let root = ran(engine.request(&Request::new(Call::new("outer"))));
    let runs = world.runs();
    let inner = runs.iter().find(|run| **run != root).unwrap();

    // In: the gate, the lease, the meter. Out: the meter, the lease, the gate. The
    // request the handle refused is in nobody's journal.
    assert_eq!(
        *seen.lock().unwrap(),
        [(FaultKind::Unsupported, Stage::NotSent)]
    );
    assert_eq!(filler.requests(), 1);
    let journal = wrappers.0.journal();
    let order: Vec<(&str, &str)> = journal
        .iter()
        .map(|(name, saw)| match saw {
            Saw::Before(_) => (*name, "before"),
            Saw::Answered(Some(7)) => (*name, "answered at 7"),
            other => panic!("{name}: {other:?}"),
        })
        .collect();
    assert_eq!(
        order,
        [
            ("gate", "before"),
            ("lease", "before"),
            ("meter", "before"),
            ("meter", "answered at 7"),
            ("lease", "answered at 7"),
            ("gate", "answered at 7"),
        ]
    );
    // Each was told the same of the request: the run that sent it, the run at the
    // root of its tree, the capability, what the backend declares, and what the
    // request is known to hold.
    for wrapper in [&wrappers.0, &wrappers.1, &wrappers.2] {
        let [told] = wrapper.requests().try_into().unwrap();
        assert_eq!((&told.run, &told.root), (inner, &root));
        assert_eq!(told.capability, Capability::Fill);
        let model = ("fake", "fake-filler", "1");
        assert_eq!(
            (
                told.model.0.as_str(),
                told.model.1.as_str(),
                told.model.2.as_str()
            ),
            model
        );
        assert_eq!((told.quote_micros, told.classes), (None, Classes::none()));
        assert_eq!(told.left_micros, None);
    }
}

#[test]
fn a_wrapper_is_told_the_quote_the_classes_and_the_allowance_of_the_run() {
    use whizbang_core::capability::Price;
    let world = World::new();
    let price = Price::at_most(40.0).unwrap();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse).priced(price));
    let meter = FakeWrapper::new("meter");
    let with_a_prompt = action("asks", &[Capability::Fill], |_, context| {
        let fill = context.capabilities().fill().unwrap();
        fill.fill(&request(Some("a red door"))).unwrap();
        Ok(Produced::nothing())
    });
    let engine = world
        .engine(filler, vec![with_a_prompt])
        .with_ledger(Arc::new(FakeLedger::new()))
        .with_wrapper(Place::Meter, Arc::new(meter.clone()));
    let run = ran(engine.request(&Request::new(Call::new("asks"))));
    let [told] = meter.requests().try_into().unwrap();
    assert_eq!((&told.run, &told.root), (&run, &run));
    // The quote of one request, the prompt the request is known to hold, and what
    // the run reserved, of which nothing is taken: no wrapper here debits it.
    assert_eq!(told.quote_micros, Some(40));
    assert_eq!(told.classes, Classes::of(&[DataClass::Prompts]));
    assert_eq!(told.left_micros, Some(40));
}

// --- a refusal ---------------------------------------------------------------------------------

#[test]
fn a_wrapper_that_refuses_stops_the_request_before_the_next_sees_it_and_before_the_backend() {
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let wrappers = three();
    wrappers
        .1
        .refuse(RefusalKind::RequestNotReady, "the model is being removed");
    let answered: Answered = Arc::default();
    let engine = wrapped(
        world.engine(filler.clone(), vec![asks(&answered)]),
        &wrappers,
    );
    let run = ran(engine.request(&Request::new(Call::new("asks"))));

    // The fault reaches the action, and says the request did not leave.
    let fault = the_fault(&answered);
    assert_eq!(of_kind(&fault), (FaultKind::Rejected, Stage::NotSent));
    assert_eq!(fault.sentence(), "the model is being removed");
    // The gate saw it and is told it did not leave; the lease refused it; the meter
    // and the backend never saw it.
    assert_eq!(
        wrappers
            .0
            .journal()
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>(),
        ["gate", "lease", "gate"]
    );
    assert_eq!(
        wrappers.0.saw().last(),
        Some(&Saw::Faulted(FaultKind::Rejected, Stage::NotSent))
    );
    assert_eq!(wrappers.2.saw(), []);
    assert_eq!(filler.requests(), 0);
    // The refusal is in the run's record, among its refusals, and names no action.
    let record = world.record(&run).unwrap();
    assert_eq!(record.outcome().status, Status::Completed);
    let refusals = &record.outcome().refusals;
    assert_eq!(refusals.len(), 1);
    assert_eq!(
        (refusals[0].kind(), refusals[0].why(), refusals[0].action()),
        (
            RefusalKind::RequestNotReady,
            "the model is being removed",
            None
        )
    );
    // A kind that is not one of a request makes no refusal: the request goes on.
    wrappers
        .1
        .refuse(RefusalKind::NoBound, "not a kind of a request");
    ran(engine.request(&Request::new(Call::new("asks"))));
    assert_eq!(filler.requests(), 1);
}

// --- outstanding until the last wrapper has returned -------------------------------------------

#[test]
fn with_a_wrapper_stopped_after_the_backend_answered_the_run_and_its_parent_are_not_terminal() {
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let barriers = Barriers::new();
    let meter = FakeWrapper::new("meter");
    meter.stop_after(&barriers, "settling");
    let answered: Answered = Arc::default();
    let outer = action("outer", &[], |_, context| {
        context.executor().submit(&Call::new("asks")).unwrap();
        Ok(Produced::nothing())
    });
    let engine = world
        .engine(filler.clone(), vec![outer, asks(&answered)])
        .with_wrapper(Place::Meter, Arc::new(meter.clone()));
    let Ok(Answer::Started(parent)) = engine.start(&Request::new(Call::new("outer"))) else {
        panic!("the request was not started");
    };
    // The backend has answered, and the meter has not returned.
    barriers.arrived("settling", 1);
    assert_eq!(filler.requests(), 1);
    assert_eq!(meter.saw().last(), Some(&Saw::Answered(None)));
    let child = world.runs().into_iter().find(|run| *run != parent).unwrap();
    for run in [&parent, &child] {
        let state = engine.status(run).unwrap().state().clone();
        assert_eq!(state, RunState::NotEnded { here: true }, "{run}");
        assert!(world.record(run).is_none(), "{run}");
    }
    // The action has not been answered either: its request is still outstanding.
    assert_eq!(answered.lock().unwrap().len(), 0);
    barriers.open("settling");
    for run in [&child, &parent] {
        let state = engine.wait(run).unwrap().state().clone();
        assert!(matches!(state, RunState::Ended(_)), "{run}: {state:?}");
    }
    assert_eq!(answered.lock().unwrap().len(), 1);
}

// --- a failure ---------------------------------------------------------------------------------

/// What a store holds of a run that was abandoned, and what its caller was told.
fn abandoned(world: &World, engine: &Engine, failed: CallError, says: &str) -> RunId {
    let CallError::Abandoned { run, why } = &failed else {
        panic!("{failed:?}");
    };
    assert!(why.contains(says), "{why}");
    assert!(failed.refusal().is_none());
    assert!(
        failed
            .to_string()
            .contains("was abandoned and is not ended"),
        "{failed}"
    );
    // The run is not ended: it has no record, and whoever asks is told it was
    // abandoned.
    assert!(world.record(run).is_none());
    let state = engine.status(run).unwrap().state().clone();
    assert!(matches!(state, RunState::Abandoned { .. }), "{state:?}");
    run.clone()
}

#[test]
fn a_wrapper_that_fails_after_the_backend_answered_leaves_the_run_abandoned_and_not_ended() {
    let world = World::new();
    // The filler states a price, so its call reserves something: a call that may
    // cost nothing has no reservation.
    let filler = Arc::new(priced_filler().charging(7));
    let meter = FakeWrapper::new("meter");
    meter.fail_after("its settlement was not published");
    let seen: Arc<Mutex<Vec<(FaultKind, Stage)>>> = Arc::default();
    let twice = {
        let seen = seen.clone();
        action("asks", &[Capability::Fill], move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            for _ in 0..2 {
                let fault = fill.fill(&request(None)).unwrap_err();
                seen.lock().unwrap().push(of_kind(&fault));
            }
            // The action goes on and completes: its run is abandoned all the same.
            Ok(Produced::nothing())
        })
    };
    let ledger = Arc::new(FakeLedger::new());
    let engine = world
        .engine(filler.clone(), vec![twice])
        .with_ledger(ledger.clone())
        .with_wrapper(Place::Meter, Arc::new(meter.clone()));
    let failed = engine
        .request(&Request::new(Call::new("asks")))
        .unwrap_err();
    let run = abandoned(&world, &engine, failed, "the meter failed: its settlement");
    // The backend was called once. The action is not handed its answer, which the
    // run can no longer account for; and its next request is not sent.
    assert_eq!(filler.requests(), 1);
    assert_eq!(
        *seen.lock().unwrap(),
        [
            (FaultKind::Unavailable, Stage::Unresolved),
            (FaultKind::Cancelled, Stage::NotSent)
        ]
    );
    assert_eq!(meter.requests().len(), 1);
    // Nothing of it is released: its reservation is not closed, and counts in full.
    let generation = world.store.snapshot().unwrap();
    let names: Vec<&String> = generation.entries().keys().collect();
    assert!(
        !names.iter().any(|name| name.starts_with("spend/closed/")),
        "{names:?}"
    );
    assert!(names.contains(&&format!("spend/reservations/{run}")));
}

/// A filler that states a bound of ten millionths for a request.
fn priced_filler() -> FakeFiller {
    FakeFiller::new(Paint::Inverse).priced(Price::at_most(10.0).unwrap())
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
fn a_backend_that_panics_under_a_wrapper_leaves_the_run_abandoned_and_not_ended() {
    let declaration = FakeFiller::new(Paint::Inverse).declaration().clone();
    let panicking = || Arc::new(Panicking(declaration.clone(), AtomicU64::new(0)));
    // The action catches nothing: the panic unwinds through it.
    let asks = || {
        action("asks", &[Capability::Fill], |_, context| {
            let fill = context.capabilities().fill().unwrap();
            fill.fill(&request(None))
                .map(drop)
                .map_err(|fault| Failure::error("fill", fault.sentence()))?;
            Ok(Produced::nothing())
        })
    };
    // Under a wrapper that holds something for the request: abandoned. What the
    // wrapper held is dropped, and never told how the request ended.
    let world = World::new();
    let (filler, meter) = (panicking(), FakeWrapper::new("meter"));
    let engine = world
        .engine(filler.clone(), vec![asks()])
        .with_wrapper(Place::Meter, Arc::new(meter.clone()));
    let failed = engine
        .request(&Request::new(Call::new("asks")))
        .unwrap_err();
    abandoned(
        &world,
        &engine,
        failed,
        "a backend panicked under the meter",
    );
    assert_eq!(filler.1.load(Ordering::SeqCst), 1);
    assert!(matches!(
        meter.saw().as_slice(),
        [Saw::Before(_), Saw::Dropped]
    ));

    // With no wrapper but the gate, which holds nothing for a request, it is what it
    // was: the run ends, as an error that says its action panicked.
    let world = World::new();
    let filler = panicking();
    let engine = world.engine(filler.clone(), vec![asks()]);
    let run = ran(engine.request(&Request::new(Call::new("asks"))));
    let record = world.record(&run).unwrap();
    assert_eq!(record.outcome().status, Status::Error);
    assert_eq!(record.outcome().error.as_ref().unwrap().kind, "panic");
}

// --- the gate's place --------------------------------------------------------------------------

#[test]
fn with_the_default_policy_a_backend_that_names_a_recipient_is_refused_at_the_gates_place_and_one_that_names_none_is_sent()
 {
    let origin = Origin::of("https://api.example.com", &Secrets::none()).unwrap();
    let recipient = Recipient::new(origin);
    // An executor that was given no policy holds the one that approves nothing, and
    // puts its gate in the gate's place.
    let world = World::new();
    let remote = Arc::new(FakeFiller::new(Paint::Inverse).sending_to(recipient));
    let meter = FakeWrapper::new("meter");
    let answered: Answered = Arc::default();
    let engine = world
        .engine(remote.clone(), vec![asks(&answered)])
        .with_wrapper(Place::Meter, Arc::new(meter.clone()));
    let run = ran(engine.request(&Request::new(Call::new("asks"))));
    // Refused before the backend was called, and before the meter's place saw it.
    let fault = the_fault(&answered);
    assert_eq!(of_kind(&fault), (FaultKind::Rejected, Stage::NotSent));
    assert_eq!(
        fault.sentence(),
        "the recipient is not approved for the classes of data the request holds"
    );
    assert_eq!(fault.charge(), None);
    assert_eq!((remote.requests(), meter.saw()), (0, vec![]));
    // It is among the run's refusals, and in where its data did not go.
    let record = world.record(&run).unwrap();
    let refusals = &record.outcome().refusals;
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].kind(), RefusalKind::RequestNotApproved);
    assert_eq!(
        refusals[0].why(),
        "nothing was sent to https://api.example.com: the recipient is not approved for the \
         classes of data the request holds (the classes of data the request holds: none \
         known before a piece of it is made)"
    );
    let went = record.egress();
    assert_eq!(went.len(), 1);
    assert_eq!(
        (went[0].origin(), went[0].outcome()),
        ("https://api.example.com", Handed::Refused)
    );

    // A backend that names no recipient is local: it is called, and the meter's
    // place sees its request.
    let world = World::new();
    let local = Arc::new(FakeFiller::new(Paint::Inverse));
    let meter = FakeWrapper::new("meter");
    let answered: Answered = Arc::default();
    let engine = world
        .engine(local.clone(), vec![asks(&answered)])
        .with_wrapper(Place::Meter, Arc::new(meter.clone()));
    let run = ran(engine.request(&Request::new(Call::new("asks"))));
    assert_eq!(*answered.lock().unwrap(), [Ok(())]);
    assert_eq!((local.requests(), meter.requests().len()), (1, 1));
    assert_eq!(world.record(&run).unwrap().outcome().refusals.len(), 0);
}

// --- the bound of a request --------------------------------------------------------------------

#[test]
fn the_bound_a_request_is_held_to_is_its_own_backends_and_an_admission_reserves_the_largest() {
    use whizbang_core::capability::{Price, Roster};
    let world = World::new();
    // Two backends of one capability, offered under two names, that declare one
    // model and two prices.
    let priced = |micros: f64| {
        Arc::new(FakeFiller::new(Paint::Inverse).priced(Price::at_most(micros).unwrap()))
    };
    let (cheap, dear) = (priced(40.0), priced(90.0));
    let offered = Roster::new()
        .offer(Capability::Fill, "cheap", cheap.declaration().clone())
        .and_then(|roster| roster.offer(Capability::Fill, "dear", dear.declaration().clone()))
        .and_then(|roster| roster.default_for(Capability::Fill, "cheap"))
        .unwrap();
    let answered: Answered = Arc::default();
    let mut registry = Registry::new();
    registry.register(asks(&answered)).unwrap();
    let meter = FakeWrapper::new("meter");
    let held = Capabilities::none()
        .with_fill(cheap.clone())
        .with_fill(dear.clone());
    let engine = Engine::over(
        registry,
        Arc::new(offered),
        world.store.clone(),
        Digest::of_bytes(b""),
    )
    .with_clock(Arc::new(Stopped(7000)))
    .with_capabilities(held)
    .with_ledger(Arc::new(FakeLedger::new()))
    .with_wrapper(Place::Meter, Arc::new(meter.clone()));
    ran(engine.request(&Request::new(Call::new("asks"))));
    // The request went to the backend the call resolved, and the meter's place was
    // told that backend's quote. The admission asked by the model, and reserved the
    // largest that a backend of that model states.
    assert_eq!((cheap.requests(), dear.requests()), (1, 0));
    let [told] = meter.requests().try_into().unwrap();
    assert_eq!(told.quote_micros, Some(40));
    assert_eq!(told.left_micros, Some(90));
}

// --- a judge and a segmenter go through the same places ----------------------------------------

fn a_point() -> SegmentRequest {
    let picture = request(None).image().clone();
    SegmentRequest::new(picture, vec![Query::point(1.0, 1.0).unwrap()], 1).unwrap()
}

fn a_question() -> JudgeRequest {
    let asked = Message::new(Role::User, vec![Part::Text("is it red".to_owned())]).unwrap();
    JudgeRequest::new(
        "You judge.",
        vec![asked],
        Vec::new(),
        Controls::new(64).unwrap(),
    )
    .unwrap()
}

fn a_turn() -> Turn {
    let parts = vec![Part::Text("yes".to_owned())];
    Turn::new(parts, StopReason::EndTurn, None, None).unwrap()
}

/// `asks`: an action that asks its segmenter once and then its judge once, and keeps
/// what each was answered with.
fn asks_both(answered: &Answered) -> Arc<dyn Action> {
    let answered = answered.clone();
    let needs = [Capability::Segment, Capability::Judge];
    action("asks", &needs, move |_, context| {
        let handles = context.capabilities();
        let segmented = handles.segment().unwrap().segment(&a_point()).map(drop);
        answered.lock().unwrap().push(segmented);
        let judged = handles.judge().unwrap().send(&a_question()).map(drop);
        answered.lock().unwrap().push(judged);
        Ok(Produced::nothing())
    })
}

#[test]
fn a_segmentation_and_a_judge_request_each_pass_the_three_places_in_order() {
    let world = World::new();
    let segmenter = Arc::new(FakeSegmenter::new().charging(3));
    let judge = Arc::new(FakeJudge::scripted(vec![Ok(a_turn())]).charging(5));
    let wrappers = three();
    let answered: Answered = Arc::default();
    let engine = wrapped(
        world.engine_of_two(segmenter.clone(), judge.clone(), vec![asks_both(&answered)]),
        &wrappers,
    );
    let run = ran(engine.request(&Request::new(Call::new("asks"))));
    assert_eq!(*answered.lock().unwrap(), [Ok(()), Ok(())]);
    assert_eq!((segmenter.requests(), judge.requests()), (1, 1));

    // Each request went in by the gate, the lease and the meter, and came back by
    // the meter, the lease and the gate, with the charge its answer stated: the
    // segmentation first, then the judge's, which goes by a path of its own.
    let journal = wrappers.0.journal();
    let order: Vec<(&str, String)> = journal
        .iter()
        .map(|(name, saw)| match saw {
            Saw::Before(told) => (*name, format!("before {:?}", told.capability)),
            Saw::Answered(Some(micros)) => (*name, format!("answered at {micros}")),
            other => panic!("{name}: {other:?}"),
        })
        .collect();
    let around_one = |capability: Capability, micros: u64| {
        let before = format!("before {capability:?}");
        let after = format!("answered at {micros}");
        vec![
            ("gate", before.clone()),
            ("lease", before.clone()),
            ("meter", before),
            ("meter", after.clone()),
            ("lease", after.clone()),
            ("gate", after),
        ]
    };
    let mut expected = around_one(Capability::Segment, 3);
    expected.extend(around_one(Capability::Judge, 5));
    assert_eq!(order, expected);
    // Each place was told of both: the run, the backend's model, and what the
    // request is known to hold, which for a point is nothing and for a conversation
    // is a prompt.
    for wrapper in [&wrappers.0, &wrappers.1, &wrappers.2] {
        let [segmented, judged] = wrapper.requests().try_into().unwrap();
        assert_eq!((&segmented.run, &judged.run), (&run, &run));
        assert_eq!(segmented.model.1, "fake-segmenter");
        assert_eq!(judged.model.1, "fake-judge");
        assert_eq!(segmented.classes, Classes::none());
        assert_eq!(judged.classes, Classes::of(&[DataClass::Prompts]));
    }
}

#[test]
fn a_gate_that_refuses_stops_a_segmentation_and_a_judge_request_before_its_backend() {
    // Under the policy that approves nothing, the gate of the executor stands in the
    // gate's place: each backend names a recipient, and neither is called.
    let origin = Origin::of("https://api.example.com", &Secrets::none()).unwrap();
    let recipient = Recipient::new(origin);
    let world = World::new();
    let segmenter = Arc::new(FakeSegmenter::new().sending_to(recipient.clone()));
    let judge = Arc::new(FakeJudge::scripted(vec![Ok(a_turn())]).sending_to(recipient));
    let meter = FakeWrapper::new("meter");
    let lease = meter.beside("lease");
    let answered: Answered = Arc::default();
    let engine = world
        .engine_of_two(segmenter.clone(), judge.clone(), vec![asks_both(&answered)])
        .with_wrapper(Place::Lease, Arc::new(lease.clone()))
        .with_wrapper(Place::Meter, Arc::new(meter.clone()));
    let run = ran(engine.request(&Request::new(Call::new("asks"))));
    let faults = answered.lock().unwrap().clone();
    assert_eq!(faults.len(), 2);
    for fault in faults {
        let fault = fault.unwrap_err();
        assert_eq!(of_kind(&fault), (FaultKind::Rejected, Stage::NotSent));
        assert_eq!(fault.charge(), None);
    }
    // Neither backend was called, the judge's script is whole, and neither the
    // lease's place nor the meter's saw a request.
    assert_eq!((segmenter.requests(), judge.requests()), (0, 0));
    assert_eq!(judge.remaining(), 1);
    assert_eq!(meter.journal(), []);
    // Both refusals are in the run's record, each as a request that was not approved.
    let record = world.record(&run).unwrap();
    let refusals = &record.outcome().refusals;
    let kinds: Vec<RefusalKind> = refusals.iter().map(|refused| refused.kind()).collect();
    assert_eq!(kinds, [RefusalKind::RequestNotApproved; 2]);
    // The same with a wrapper a test put in the gate's place, that refuses.
    let world = World::new();
    let segmenter = Arc::new(FakeSegmenter::new());
    let judge = Arc::new(FakeJudge::scripted(vec![Ok(a_turn())]));
    let wrappers = three();
    wrappers
        .0
        .refuse(RefusalKind::RequestNotApproved, "nothing goes by");
    let answered: Answered = Arc::default();
    let engine = wrapped(
        world.engine_of_two(segmenter.clone(), judge.clone(), vec![asks_both(&answered)]),
        &wrappers,
    );
    ran(engine.request(&Request::new(Call::new("asks"))));
    assert_eq!((segmenter.requests(), judge.requests()), (0, 0));
    let names: Vec<&str> = wrappers.0.journal().iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["gate", "gate"]);
}

// --- a wrapper that panics once the request is over ---------------------------------------------

#[test]
fn a_wrapper_that_panics_after_the_backend_answered_leaves_the_run_abandoned_and_not_ended() {
    let world = World::new();
    // The filler states a price, so its call reserves something: a call that may
    // cost nothing has no reservation.
    let filler = Arc::new(priced_filler().charging(7));
    let wrappers = three();
    wrappers.2.panic_after("the meter went wrong");
    // The panic reaches the action, which catches it, keeps what it carried and
    // completes: its run is abandoned all the same.
    let reached: Arc<Mutex<Vec<String>>> = Arc::default();
    let asks = {
        let reached = reached.clone();
        action("asks", &[Capability::Fill], move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            let asked = std::panic::AssertUnwindSafe(|| fill.fill(&request(None)).map(drop));
            let carried = std::panic::catch_unwind(asked).unwrap_err();
            let text = carried
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_default();
            reached.lock().unwrap().push(text);
            Ok(Produced::nothing())
        })
    };
    let ledger = Arc::new(FakeLedger::new());
    let engine = wrapped(
        world
            .engine(filler.clone(), vec![asks])
            .with_ledger(ledger.clone()),
        &wrappers,
    );
    let failed = match engine.request(&Request::new(Call::new("asks"))) {
        Err(failed) => failed,
        Ok(answer) => panic!("the run was not abandoned: {answer:?}"),
    };
    let run = abandoned(
        &world,
        &engine,
        failed,
        "the meter panicked once its request was over",
    );
    // The backend was called once, and had answered: the money may be spent.
    assert_eq!(filler.requests(), 1);
    // The panic was resumed, and not swallowed: the action was handed no answer
    // and no fault, but the panic itself, with what the wrapper's carried.
    assert_eq!(*reached.lock().unwrap(), ["the meter went wrong"]);
    // The places before the meter's are told how the request ended all the same: a
    // panic of one does not keep the others from being told.
    let journal = wrappers.0.journal();
    let told: Vec<(&str, bool)> = journal
        .iter()
        .map(|(name, saw)| (*name, matches!(saw, Saw::Answered(Some(7)))))
        .collect();
    assert_eq!(
        told[3..],
        [("meter", true), ("lease", true), ("gate", true)],
        "{journal:?}"
    );
    // Nothing of it is released: its reservation is not closed, and counts in full.
    let generation = world.store.snapshot().unwrap();
    let names: Vec<&String> = generation.entries().keys().collect();
    assert!(
        !names.iter().any(|name| name.starts_with("spend/closed/")),
        "{names:?}"
    );
    assert!(names.contains(&&format!("spend/reservations/{run}")));
}

#[test]
fn a_wrapper_that_panics_when_told_a_later_place_refused_leaves_the_run_abandoned() {
    // The lease let the request through and holds something for it; the meter
    // refuses; the lease panics when it is told. What it held is not accounted
    // for, as when it fails.
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let wrappers = three();
    wrappers.1.panic_after("the lease went wrong");
    wrappers
        .2
        .refuse(RefusalKind::RequestNotReady, "nothing is left");
    let reached: Arc<Mutex<Vec<String>>> = Arc::default();
    let asks = {
        let reached = reached.clone();
        action("asks", &[Capability::Fill], move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            let asked = std::panic::AssertUnwindSafe(|| fill.fill(&request(None)).map(drop));
            let carried = std::panic::catch_unwind(asked).unwrap_err();
            let text = carried
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_default();
            reached.lock().unwrap().push(text);
            Ok(Produced::nothing())
        })
    };
    let engine = wrapped(world.engine(filler.clone(), vec![asks]), &wrappers);
    let failed = match engine.request(&Request::new(Call::new("asks"))) {
        Err(failed) => failed,
        Ok(answer) => panic!("the run was not abandoned: {answer:?}"),
    };
    abandoned(
        &world,
        &engine,
        failed,
        "the lease panicked once its request was over",
    );
    assert_eq!(filler.requests(), 0);
    // The panic reached the action, in place of the meter's refusal.
    assert_eq!(*reached.lock().unwrap(), ["the lease went wrong"]);
    // The gate, before it, is told all the same.
    assert_eq!(
        wrappers.0.saw().last(),
        Some(&Saw::Faulted(FaultKind::Rejected, Stage::NotSent))
    );
}

// --- the request id of an abandoned run ---------------------------------------------------------

#[test]
fn a_repeat_of_the_request_of_an_abandoned_run_is_answered_with_that_run_and_never_runs_again() {
    let world = World::new();
    // The filler states a price, so its call reserves something: a call that may
    // cost nothing has no reservation.
    let filler = Arc::new(priced_filler().charging(7));
    let meter = FakeWrapper::new("meter");
    meter.fail_after("its settlement was not published");
    let answered: Answered = Arc::default();
    let other = action("other", &[], |_, _| Ok(Produced::nothing()));
    let engine = world
        .engine(filler.clone(), vec![asks(&answered), other])
        .with_ledger(Arc::new(FakeLedger::new()))
        .with_wrapper(Place::Meter, Arc::new(meter.clone()));
    let asked = || Request::new(Call::new("asks")).with_id(RequestId::parse("order-17").unwrap());
    let failed = engine.request(&asked()).unwrap_err();
    let run = abandoned(&world, &engine, failed, "the meter failed");
    // The same request again, by each way of asking: answered with the run that is
    // not ended, as abandoned. Its action does not run again, and nothing more is
    // sent or reserved.
    for again in [engine.request(&asked()), engine.start(&asked())] {
        let Ok(Answer::Repeat(earlier)) = again else {
            panic!("not answered as a repeat: {again:?}");
        };
        assert_eq!(earlier.run(), &run);
        let state = earlier.state();
        assert!(matches!(state, RunState::Abandoned { .. }), "{state:?}");
    }
    assert_eq!(filler.requests(), 1);
    assert_eq!(answered.lock().unwrap().len(), 1);
    assert_eq!(meter.requests().len(), 1);
    assert_eq!(world.runs(), std::slice::from_ref(&run));
    assert!(world.record(&run).is_none());
    let generation = world.store.snapshot().unwrap();
    let reservations = generation.entries().keys();
    let reservations: Vec<&String> = reservations
        .filter(|name| name.starts_with("spend/reservations/"))
        .collect();
    assert_eq!(reservations, [&format!("spend/reservations/{run}")]);
    // A different request under the same id is a conflict, as for any id that is
    // taken, and runs nothing.
    let other = Request::new(Call::new("other")).with_id(RequestId::parse("order-17").unwrap());
    let conflict = engine.request(&other).unwrap_err();
    let Some(Refusal::Conflict { run: taken_by, .. }) = conflict.refusal() else {
        panic!("{conflict:?}");
    };
    assert_eq!(taken_by, &run);
    assert_eq!((filler.requests(), world.runs().len()), (1, 1));
}

// --- what the gate's refusal says of the list of where data went --------------------------------

#[test]
fn the_gates_refusal_says_so_when_it_could_not_be_listed_in_where_the_runs_data_went() {
    let origin = Origin::of("https://api.example.com", &Secrets::none()).unwrap();
    let remote = Arc::new(FakeFiller::new(Paint::Inverse).sending_to(Recipient::new(origin)));
    // The list of where a run's data went keeps half its room for what was sent:
    // once 64 refused sends are listed, one to a new origin has no entry.
    let run_with = |refused_before: usize| {
        let world = World::new();
        let asks = action("asks", &[Capability::Fill], move |_, context| {
            for at in 0..refused_before {
                let to = format!("https://host-{at}.example.org");
                let to = Origin::of(&to, &Secrets::none()).unwrap();
                let none = Classes::none();
                assert!(context.facts().went(&to, None, none, Handed::Refused));
            }
            let fill = context.capabilities().fill().unwrap();
            fill.fill(&request(None)).unwrap_err();
            Ok(Produced::nothing())
        });
        let engine = world.engine(remote.clone(), vec![asks]);
        let run = ran(engine.request(&Request::new(Call::new("asks"))));
        world.record(&run).unwrap()
    };
    let unlisted = "; not listed in where the run's data went)";
    let listed = run_with(63);
    let [refused] = listed.outcome().refusals.as_slice() else {
        panic!("{:?}", listed.outcome().refusals);
    };
    assert!(!refused.why().contains("not listed"), "{}", refused.why());
    assert_eq!(listed.egress().len(), 64);
    let full = run_with(64);
    let [refused] = full.outcome().refusals.as_slice() else {
        panic!("{:?}", full.outcome().refusals);
    };
    assert_eq!(refused.kind(), RefusalKind::RequestNotApproved);
    assert!(refused.why().ends_with(unlisted), "{}", refused.why());
    assert!(
        refused
            .why()
            .starts_with("nothing was sent to https://api.example.com: ")
    );
    // The refusal stands for the entry: the list holds the 64 and not the gate's.
    assert_eq!(full.egress().len(), 64);
    let origins = full.egress().iter();
    assert!(
        origins
            .map(|went| went.origin())
            .all(|at| at != "https://api.example.com")
    );
    assert_eq!(remote.requests(), 0);
}
