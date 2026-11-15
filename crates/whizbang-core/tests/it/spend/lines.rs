//! The run's spend lines (plan 1B, task S10): every paid request of a run is an
//! element of `spend.requests` in its record, noted by the meter in the run's place
//! for facts once the request's settlement is durable, in the order the requests
//! were sent, for a run of any outcome. Each test is through an executor built with
//! the real ledger and the meter.

use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use whizbang_core::capability::{
    Bounds, Controls, Declaration, Fault, FillRequest, Filler, JudgeRequest, Message, Painted,
    Part, Price, Query, Role, Roster, SegmentRequest, Stage, StopReason, Turn,
};
use whizbang_core::contract::record::{Paid, Status};
use whizbang_core::contract::{
    Action, Answer, Call, Cancel, Capabilities, Capability, Engine, Executor, Failure, Place,
    Produced, Registry, RunId, RunState,
};
use whizbang_core::identity::Digest;
use whizbang_core::spend::{Meter, PerCall, Record, StoreLedger, Sum};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::{FakeFiller, FakeJudge, FakeSegmenter, roster};

use super::meter::{PRICE, call, fill_request, metered, priced};
use super::support::{Said, World, allow, books, ran};

/// A filler that answers each request as the next of these does, in turn: so one run
/// has requests that end in different ways. All of them declare the same.
struct InTurn(Vec<FakeFiller>, AtomicUsize);

impl InTurn {
    fn of(fillers: Vec<FakeFiller>) -> Arc<InTurn> {
        Arc::new(InTurn(fillers, AtomicUsize::new(0)))
    }
}

impl Filler for InTurn {
    fn declaration(&self) -> &Declaration {
        self.0[0].declaration()
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        let at = self.1.fetch_add(1, Ordering::SeqCst);
        self.0[at % self.0.len()].fill(request, bounds)
    }
}

/// One line of a record's `spend.requests`: the request's place among the requests
/// the run sent, what was held for it, what it was settled at, how much of that is
/// unresolved, the stage it reached, and whether it is over its bound.
type Line = (u64, u64, u64, u64, Stage, bool);

fn line(paid: &Paid) -> Line {
    (
        paid.request(),
        paid.reserved_micros(),
        paid.settled_micros(),
        paid.unresolved_micros(),
        paid.stage(),
        paid.is_over_bound(),
    )
}

/// The spend lines of a run's record, in the order the record lists them.
fn lines(world: &World, run: &RunId) -> Vec<Line> {
    world
        .record(run)
        .spend()
        .requests
        .iter()
        .map(line)
        .collect()
}

/// An action that sends its filler this many requests, whatever each comes to.
fn sends(world: &World, name: &str, times: usize) -> Arc<dyn Action> {
    world.action(name, &[Capability::Fill], move |_, context| {
        let fill = context.capabilities().fill().unwrap();
        for _ in 0..times {
            drop(fill.fill(&fill_request()));
        }
        Ok(Produced::nothing())
    })
}

#[test]
fn the_records_of_a_completed_a_failed_and_a_cancelled_run_each_list_every_paid_request() {
    for how in ["completed", "failed", "cancelled"] {
        let world = World::new(&format!("lines-{how}"));
        allow(&world.store, 1_000_000);
        // Three requests: one answered and charged under its bound, one that no
        // answer settled, and one whose backend says it was not sent.
        let filler = InTurn::of(vec![
            priced().charging(100_000),
            priced().failing_at(Stage::Unresolved),
            priced().failing_at(Stage::NotSent),
        ]);
        let barriers = Barriers::new();
        let action = {
            let barriers = barriers.clone();
            world.action("asks", &[Capability::Fill], move |_, context| {
                let fill = context.capabilities().fill().unwrap();
                for _ in 0..3 {
                    drop(fill.fill(&fill_request()));
                }
                match how {
                    "failed" => Err(Failure::error("fill", "it failed")),
                    "cancelled" => {
                        barriers.stop_at("done");
                        Ok(Produced::nothing())
                    }
                    _ => Ok(Produced::nothing()),
                }
            })
        };
        let engine = metered(&world, filler, 3, vec![action]);
        let Ok(Answer::Started(run)) = engine.start(&call()) else {
            panic!("{how}: the request was not started");
        };
        if how == "cancelled" {
            barriers.arrived("done", 1);
            assert_eq!(engine.cancel(&run).unwrap(), Cancel::Accepted);
            barriers.open("done");
        }
        let state = engine.wait(&run).unwrap().state().clone();
        assert!(matches!(state, RunState::Ended(_)), "{how}: {state:?}");
        let record = world.record(&run);
        let status = match how {
            "completed" => Status::Completed,
            "failed" => Status::Error,
            _ => Status::Cancelled,
        };
        assert_eq!(record.outcome().status, status, "{how}");
        // Every paid request, in the order it was sent, with what was reserved for
        // it, what it was settled at and what of that is unresolved.
        assert_eq!(record.spend().reserved_micros, Some(3 * PRICE), "{how}");
        assert_eq!(record.models().len(), 3, "{how}");
        assert_eq!(
            lines(&world, &run),
            [
                (0, PRICE, 100_000, 0, Stage::Answered, false),
                (1, PRICE, PRICE, PRICE, Stage::Unresolved, false),
                (2, PRICE, 0, 0, Stage::NotSent, false),
            ],
            "{how}"
        );
    }
}

#[test]
fn a_request_settled_over_its_bound_is_listed_at_what_it_was_charged_and_marked_as_over() {
    let world = World::new("lines-over");
    allow(&world.store, 1_000_000);
    // The first answer states no charge, and is settled at its bound; the second is
    // charged 50,000 over its bound.
    let filler = InTurn::of(vec![priced(), priced().charging(300_000)]);
    let engine = metered(&world, filler, 2, vec![sends(&world, "asks", 2)]);
    let run = ran(engine.request(&call())).run().clone();
    assert_eq!(
        lines(&world, &run),
        [
            (0, PRICE, PRICE, 0, Stage::Answered, false),
            (1, PRICE, 300_000, 0, Stage::Answered, true),
        ]
    );
}

/// An executor that offers these backends and holds them, with the real ledger and
/// the meter: a call reserves two requests to each backend it needs.
fn engine_of(
    world: &World,
    offered: Roster,
    capabilities: Capabilities,
    action: Arc<dyn Action>,
) -> Engine {
    let mut registry = Registry::new();
    registry.register(action).unwrap();
    let backends = PerCall::new(Arc::new(offered), NonZeroU64::new(2).unwrap());
    let meter = Meter::new(world.store.clone(), world.clock.clone());
    Engine::over(
        registry,
        Arc::new(backends),
        world.store.clone(),
        Digest::of_bytes(b""),
    )
    .with_clock(world.clock.clone())
    .with_capabilities(capabilities)
    .with_ledger(Arc::new(StoreLedger))
    .with_wrapper(Place::Meter, Arc::new(meter))
}

/// The `request` each dispatch record under this reservation holds, by its place.
fn dispatched_requests(world: &World, run: &RunId) -> Vec<u64> {
    let generation = world.store.snapshot().unwrap();
    let prefix = format!("spend/dispatched/{run}/");
    let under = generation.entries().iter();
    let under = under.filter(|(name, _)| name.starts_with(&prefix));
    under
        .map(|(_, entry)| {
            let record = Record::from_bytes(&world.store.read(entry).unwrap()).unwrap();
            let Record::Dispatch { request, .. } = record else {
                panic!("not a dispatch: {record:?}");
            };
            request
        })
        .collect()
}

#[test]
fn the_sum_of_a_runs_lines_equals_what_the_fold_holds_for_that_run() {
    let world = World::new("lines-sum");
    allow(&world.store, 1_000_000);
    let filler = InTurn::of(vec![
        priced().charging(100_000),
        priced().failing_at(Stage::Unresolved),
    ]);
    // A free request to a local segmenter comes first, then between the two paid
    // ones: it is a request of the run and has no line.
    let action = world.action(
        "asks",
        &[Capability::Segment, Capability::Fill],
        |_, context| {
            let picture = fill_request().image().clone();
            let point = vec![Query::point(1.0, 1.0).unwrap()];
            let segment = SegmentRequest::new(picture, point, 1).unwrap();
            let handles = context.capabilities();
            handles.segment().unwrap().segment(&segment).unwrap();
            drop(handles.fill().unwrap().fill(&fill_request()));
            handles.segment().unwrap().segment(&segment).unwrap();
            drop(handles.fill().unwrap().fill(&fill_request()));
            Ok(Produced::nothing())
        },
    );
    // The segmenter is local and states no price.
    let segmenter = Arc::new(FakeSegmenter::new());
    let offered = roster(Some(segmenter.as_ref()), Some(filler.as_ref()), None);
    let capabilities = Capabilities::none()
        .with_segment(segmenter)
        .with_fill(filler);
    let engine = engine_of(&world, offered, capabilities, action);
    let run = ran(engine.request(&call())).run().clone();
    let record = world.record(&run);
    assert_eq!(record.models().len(), 4);
    // A line names its request by its place among all the run sent, which is where
    // the record lists the model that answered it; and the dispatch's own record
    // names the same place.
    let lines = lines(&world, &run);
    assert_eq!(
        lines,
        [
            (1, PRICE, 100_000, 0, Stage::Answered, false),
            (3, PRICE, PRICE, PRICE, Stage::Unresolved, false),
        ]
    );
    assert_eq!(dispatched_requests(&world, &run), [1, 3]);
    // The sums of the lines are what the books, folded, hold for the run.
    let settled: u64 = lines.iter().map(|line| line.2).sum();
    let unresolved: u64 = lines.iter().map(|line| line.3).sum();
    let folded = books(&world.store);
    let spent = folded.runs()[&run];
    assert_eq!(
        (spent.charged, spent.unresolved, spent.outstanding),
        (
            Sum::new(settled.into()),
            Sum::new(unresolved.into()),
            Sum::ZERO
        )
    );
    assert_eq!((settled, unresolved), (350_000, PRICE));
}

#[test]
fn a_nested_calls_lines_are_in_the_childs_record_and_the_parents_record_names_the_child() {
    let world = World::new("lines-nested");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(priced().charging(100_000));
    let child: Said<RunId> = Arc::default();
    // The parent calls its child, which sends two requests, and then sends one.
    let outer = {
        let child = child.clone();
        world.action("asks", &[Capability::Fill], move |_, context| {
            let finished = context.executor().submit(&Call::new("inner")).unwrap();
            child.lock().unwrap().push(finished.run().clone());
            let fill = context.capabilities().fill().unwrap();
            fill.fill(&fill_request()).unwrap();
            Ok(Produced::nothing())
        })
    };
    let engine = metered(&world, filler, 3, vec![outer, sends(&world, "inner", 2)]);
    let run = ran(engine.request(&call())).run().clone();
    let child = child.lock().unwrap()[0].clone();
    let paid = |request| (request, PRICE, 100_000, 0, Stage::Answered, false);
    // The child's two requests are in the child's record, and only there.
    assert_eq!(lines(&world, &child), [paid(0), paid(1)]);
    assert_eq!(lines(&world, &run), [paid(0)]);
    // The parent's record names the child among its steps.
    let record = world.record(&run);
    let steps: Vec<&str> = record.steps().iter().map(|step| step.run()).collect();
    assert_eq!(steps, [child.as_str()]);
    // Each record's lines come to what the books hold for that run.
    let folded = books(&world.store);
    assert_eq!(folded.runs()[&child].charged, Sum::new(200_000));
    assert_eq!(folded.runs()[&run].charged, Sum::new(100_000));
}

#[test]
fn a_paid_request_to_a_judge_is_a_line_as_a_fillers_is() {
    let world = World::new("lines-judge");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(priced().charging(100_000));
    let said = vec![Part::Text("yes".to_owned())];
    let turn = Turn::new(said, StopReason::EndTurn, None, None).unwrap();
    let judge = FakeJudge::scripted(vec![Ok(turn)])
        .priced(Price::at_most(250_000.0).unwrap())
        .charging(40_000);
    let judge = Arc::new(judge);
    // The filler is asked first, then the judge: a judge's handle counts its own
    // request, and tells the meter the same place.
    let action = world.action(
        "asks",
        &[Capability::Fill, Capability::Judge],
        |_, context| {
            let handles = context.capabilities();
            handles.fill().unwrap().fill(&fill_request()).unwrap();
            let asked = vec![Part::Text("is it red".to_owned())];
            let asked = Message::new(Role::User, asked).unwrap();
            let controls = Controls::new(64).unwrap();
            let question = JudgeRequest::new("You judge.", vec![asked], Vec::new(), controls);
            handles.judge().unwrap().send(&question.unwrap()).unwrap();
            Ok(Produced::nothing())
        },
    );
    let offered = roster(None, Some(filler.as_ref()), Some(judge.as_ref()));
    let capabilities = Capabilities::none().with_fill(filler).with_judge(judge);
    let engine = engine_of(&world, offered, capabilities, action);
    let run = ran(engine.request(&call())).run().clone();
    assert_eq!(
        lines(&world, &run),
        [
            (0, PRICE, 100_000, 0, Stage::Answered, false),
            (1, PRICE, 40_000, 0, Stage::Answered, false),
        ]
    );
    assert_eq!(dispatched_requests(&world, &run), [0, 1]);
}
