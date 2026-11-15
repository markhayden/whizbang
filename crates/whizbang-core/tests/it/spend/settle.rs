//! Settlement of each attempt (plan 1B, task S5b): when a paid request ends, the meter
//! settles it by the stage it reached, and gives back to its run what the bound held
//! beyond the charge, or takes the excess of a charge over its bound. Each test is
//! through an executor built with the real ledger and the meter.

use std::sync::{Arc, Mutex};

use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillRequest, Filler, Painted, Stage,
};
use whizbang_core::contract::record::Status;
use whizbang_core::contract::{
    Answer, Call, CallError, Cancel, Capability, Executor, Outcome, Produced, RefusalKind, RunId,
    RunState,
};
use whizbang_core::spend::{Record, Sum};
use whizbang_core::store::Store;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::{FakeFiller, Paint};

use super::meter::{
    Asked, PRICE, asks, call, dispatch_names, fill_request, metered, outstanding, priced, sent,
};
use super::support::{Said, World, allow, books, ran, reserved, spend_names};

fn settled_names(run: &RunId, place: u64, stage: &str, micros: u64) -> [String; 2] {
    [
        format!("spend/settled/{run}/{place}/{stage}/{micros}"),
        format!("spend/settling/{run}/{place}"),
    ]
}

/// The names a dispatch at this place by this run and its settlement make, in the
/// order of the names.
fn whole(run: &RunId, place: u64, by: &RunId, stage: &str, micros: u64) -> Vec<String> {
    let [dispatched, guard] = dispatch_names(run, place, by, PRICE);
    let [settled, settling] = settled_names(run, place, stage, micros);
    vec![dispatched, guard, settled, settling]
}

fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names
}

/// The settlement the store holds at this place under this reservation.
fn settlement(world: &World, run: &RunId, place: u64) -> Record {
    let generation = world.store.snapshot().unwrap();
    let prefix = format!("spend/settled/{run}/{place}/");
    let (_, entry) = generation
        .entries()
        .iter()
        .find(|(name, _)| name.starts_with(&prefix))
        .unwrap();
    Record::from_bytes(&world.store.read(entry).unwrap()).unwrap()
}

/// What the books say of a reservation: whether its names are a set the ledger
/// writes, what is charged, what is outstanding and what is unresolved.
fn said(world: &World, run: &RunId) -> (bool, u128, u128, u128) {
    let books = books(&world.store);
    let names = books.reservation(run).unwrap().names().clone();
    (
        names.reachable(),
        names.charged().micros(),
        names.outstanding().micros(),
        names.unresolved().micros(),
    )
}

#[test]
fn a_request_charged_under_its_bound_is_charged_that_and_its_run_spends_the_difference_on_its_next()
{
    let world = World::new("settle-under");
    let epoch = allow(&world.store, 1_000_000);
    let filler = Arc::new(priced().charging(100_000));
    let asked: Asked = Arc::default();
    // The call reserves two bounds. Three requests fit only because each gave back
    // what its bound held beyond its charge.
    let engine = metered(&world, filler.clone(), 2, vec![asks(&world, 3, &asked)]);
    let run = ran(engine.request(&call())).run().clone();
    let asked = asked.lock().unwrap().clone();
    assert!(asked.iter().all(|(answer, _)| answer.is_ok()), "{asked:?}");
    let left: Vec<Option<u64>> = asked.iter().map(|(_, left)| *left).collect();
    assert_eq!(left, [Some(400_000), Some(300_000), Some(200_000)]);
    assert_eq!(filler.requests(), 3);

    let mut names = Vec::new();
    for place in 1..=3 {
        names.extend(whole(&run, place, &run, "answered", 100_000));
    }
    assert_eq!(sent(&world), sorted(names));
    let Record::Settlement {
        stage,
        charged,
        over_bound,
        at_ms,
        ..
    } = settlement(&world, &run, 1)
    else {
        panic!("not a settlement");
    };
    assert_eq!(
        (stage, charged.micros(), over_bound, at_ms),
        (Stage::Answered, 100_000, false, 7000)
    );
    // The books: every dispatch settled, and each settlement took a turn of the
    // reservation's epoch, so the epoch's turns are those its publications took.
    assert_eq!(said(&world, &run), (true, 300_000, 0, 0));
    let books = books(&world.store);
    assert!(!books.needs_owner(epoch));
    assert_eq!(
        books.next_turn(epoch),
        5,
        "an admission and three settlements"
    );
}

#[test]
fn a_request_that_succeeds_with_no_charge_is_charged_its_bound() {
    let world = World::new("settle-none");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(priced());
    let asked: Asked = Arc::default();
    let engine = metered(&world, filler, 1, vec![asks(&world, 1, &asked)]);
    let run = ran(engine.request(&call())).run().clone();
    let asked = asked.lock().unwrap().clone();
    assert!(asked[0].0.is_ok());
    assert_eq!(asked[0].1, Some(0), "nothing is given back");
    assert_eq!(sent(&world), whole(&run, 1, &run, "answered", PRICE));
    assert_eq!(said(&world, &run), (true, u128::from(PRICE), 0, 0));
}

/// What a run could spend, and by how much it was in debt.
type Purse = (Option<u64>, Option<u64>);

#[test]
fn a_charge_over_its_bound_is_settled_at_the_charge_marked_as_over_and_taken_from_its_run_and_its_parent()
 {
    let world = World::new("settle-over");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(priced().charging(300_000));
    let purses: Said<(&'static str, Purse)> = Arc::default();
    let child: Said<RunId> = Arc::default();
    let inner = {
        let purses = purses.clone();
        world.action("inner", &[Capability::Fill], move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            fill.fill(&fill_request()).unwrap();
            let allowance = context.allowance();
            let purse = (allowance.left_micros(), allowance.owes_micros());
            purses.lock().unwrap().push(("inner", purse));
            Ok(Produced::nothing())
        })
    };
    // The outer run reserves one bound and sends nothing itself: its child takes
    // all of it as its share.
    let outer = {
        let (purses, child) = (purses.clone(), child.clone());
        world.action("asks", &[Capability::Fill], move |_, context| {
            let finished = context.executor().submit(&Call::new("inner")).unwrap();
            child.lock().unwrap().push(finished.run().clone());
            let allowance = context.allowance();
            let purse = (allowance.left_micros(), allowance.owes_micros());
            purses.lock().unwrap().push(("outer", purse));
            Ok(Produced::nothing())
        })
    };
    let engine = metered(&world, filler, 1, vec![outer, inner]);
    let run = ran(engine.request(&call())).run().clone();
    let child = child.lock().unwrap()[0].clone();

    // The excess is 50,000: the run that sent the request and its parent each have
    // that much less, below nothing.
    assert_eq!(
        *purses.lock().unwrap(),
        [
            ("inner", (Some(0), Some(50_000))),
            ("outer", (Some(0), Some(50_000)))
        ]
    );
    // Settled at the charge, and marked as over: the settlement took a place.
    let mut names = whole(&run, 1, &child, "answered", 300_000);
    names.push(format!("spend/place/{run}/2"));
    assert_eq!(sent(&world), sorted(names));
    let Record::Settlement {
        charged,
        over_bound,
        ..
    } = settlement(&world, &run, 1)
    else {
        panic!("not a settlement");
    };
    assert_eq!((charged.micros(), over_bound), (300_000, true));
    assert_eq!(said(&world, &run), (true, 300_000, 0, 0));
}

#[test]
fn a_fault_at_each_stage_and_a_timeout_settle_as_not_sent_at_nothing_answered_at_its_charge_and_unresolved_at_its_bound()
 {
    let timed_out = Fault::new(FaultKind::TimedOut, "no answer came in time");
    let table: Vec<(&str, FakeFiller, &str, u64)> = vec![
        (
            "not-sent",
            priced().failing_at(Stage::NotSent),
            "not_sent",
            0,
        ),
        (
            "answered",
            priced().charging(100_000).failing_at(Stage::Answered),
            "answered",
            100_000,
        ),
        (
            "answered-no-charge",
            priced().failing_at(Stage::Answered),
            "answered",
            PRICE,
        ),
        (
            "unresolved",
            priced().failing_at(Stage::Unresolved),
            "unresolved",
            PRICE,
        ),
        (
            "timed-out",
            FakeFiller::new(Paint::Fails(timed_out)).declared_as(priced().declaration().clone()),
            "unresolved",
            PRICE,
        ),
    ];
    for (label, filler, stage, charged) in table {
        let world = World::new(&format!("settle-{label}"));
        allow(&world.store, 1_000_000);
        let filler = Arc::new(filler);
        let asked: Asked = Arc::default();
        let engine = metered(&world, filler.clone(), 1, vec![asks(&world, 1, &asked)]);
        let run = ran(engine.request(&call())).run().clone();
        let (answer, left) = asked.lock().unwrap()[0].clone();
        let fault = answer.unwrap_err();
        if label == "timed-out" {
            assert_eq!(fault.kind(), FaultKind::TimedOut);
        }
        assert_eq!(filler.requests(), 1, "{label}");
        // What the bound held beyond the charge is back with the run.
        assert_eq!(left, Some(PRICE - charged), "{label}");
        assert_eq!(
            sent(&world),
            whole(&run, 1, &run, stage, charged),
            "{label}"
        );
        let unresolved = if stage == "unresolved" { charged } else { 0 };
        assert_eq!(
            said(&world, &run),
            (true, u128::from(charged), 0, u128::from(unresolved)),
            "{label}"
        );
    }
}

#[test]
fn a_run_cancelled_after_dispatch_whose_request_ends_with_no_answer_is_charged_the_bound_as_unresolved()
 {
    let world = World::new("settle-cancelled");
    allow(&world.store, 1_000_000);
    let barriers = Barriers::new();
    let filler = Arc::new(priced().stopping_at(&barriers, "asked"));
    let asked: Asked = Arc::default();
    let engine = metered(&world, filler.clone(), 1, vec![asks(&world, 1, &asked)]);
    let Ok(Answer::Started(run)) = engine.start(&call()) else {
        panic!("the request was not started");
    };
    // The request is with its backend, its dispatch durable, when the run is
    // cancelled.
    barriers.arrived("asked", 1);
    assert_eq!(sent(&world), dispatch_names(&run, 1, &run, PRICE));
    assert_eq!(engine.cancel(&run).unwrap(), Cancel::Accepted);
    barriers.open("asked");
    let state = engine.wait(&run).unwrap().state().clone();
    let RunState::Ended(ended) = state else {
        panic!("{state:?}");
    };
    assert!(matches!(ended, Outcome::Cancelled), "{ended:?}");
    let (answer, left) = asked.lock().unwrap()[0].clone();
    assert_eq!(answer.unwrap_err().kind(), FaultKind::Cancelled);
    assert_eq!(left, Some(0));
    assert_eq!(sent(&world), whole(&run, 1, &run, "unresolved", PRICE));
    let price = u128::from(PRICE);
    assert_eq!(said(&world, &run), (true, price, 0, price));
}

#[test]
fn a_parent_a_child_and_a_grandchild_each_send_one_request_under_one_reservation_and_the_parent_spends_what_the_child_did_not()
 {
    let world = World::new("settle-nested");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(priced().charging(100_000));
    let seen: Said<(&'static str, RunId, Option<u64>)> = Arc::default();
    // Each calls the one below it first, which takes all its caller has as its
    // share, and then sends one request of its own with what came back.
    let level = |name: &'static str, below: Option<&'static str>| {
        let seen = seen.clone();
        world.action(name, &[Capability::Fill], move |_, context| {
            if let Some(below) = below {
                let finished = context.executor().submit(&Call::new(below)).unwrap();
                let left = context.allowance().left_micros();
                seen.lock()
                    .unwrap()
                    .push((below, finished.run().clone(), left));
            }
            let fill = context.capabilities().fill().unwrap();
            fill.fill(&fill_request()).unwrap();
            Ok(Produced::nothing())
        })
    };
    let actions = vec![
        level("asks", Some("child")),
        level("child", Some("grandchild")),
        level("grandchild", None),
    ];
    // A call reserves three bounds: 750,000.
    let engine = metered(&world, filler.clone(), 3, actions);
    let run = ran(engine.request(&call())).run().clone();
    let seen = seen.lock().unwrap().clone();
    let [(_, grandchild, child_left), (_, child, parent_left)] = seen.as_slice() else {
        panic!("{seen:?}");
    };
    // The grandchild used 100,000 of 750,000 and handed the rest up; the child
    // then used 100,000 more. What the child did not use, the parent spent from.
    assert_eq!((*child_left, *parent_left), (Some(650_000), Some(550_000)));
    assert_eq!(filler.requests(), 3);

    // One reservation, and three dispatches under it, each naming it and the run
    // that sent it, each settled.
    let reservations = reserved(&world.store);
    assert_eq!(reservations.len(), 1);
    assert_eq!(
        (reservations[0].run(), reservations[0].micros()),
        (&run, 750_000)
    );
    let mut names = Vec::new();
    for (place, by) in [(1, grandchild), (2, child), (3, &run)] {
        names.extend(whole(&run, place, by, "answered", 100_000));
    }
    assert_eq!(sent(&world), sorted(names));
    assert_eq!(said(&world, &run), (true, 300_000, 0, 0));
    let books = books(&world.store);
    for by in [grandchild, child, &run] {
        assert_eq!(books.runs()[by].charged, Sum::new(100_000), "{by}");
    }
}

#[test]
fn a_child_that_retries_a_failed_request_has_an_attempt_for_each_and_a_retry_that_does_not_fit_is_refused_before_dispatch()
 {
    let world = World::new("settle-retry");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(priced().failing_at(Stage::Unresolved));
    let asked: Asked = Arc::default();
    let child: Said<RunId> = Arc::default();
    // The child's share is two bounds. It asks until a request is not sent.
    let retries = {
        let asked = asked.clone();
        world.action("retries", &[Capability::Fill], move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            for _ in 0..3 {
                let answer = fill.fill(&fill_request()).map(drop);
                let left = context.allowance().left_micros();
                asked.lock().unwrap().push((answer, left));
            }
            Ok(Produced::nothing())
        })
    };
    let outer = {
        let child = child.clone();
        world.action("asks", &[Capability::Fill], move |_, context| {
            let finished = context.executor().submit(&Call::new("retries")).unwrap();
            child.lock().unwrap().push(finished.run().clone());
            Ok(Produced::nothing())
        })
    };
    let engine = metered(&world, filler.clone(), 2, vec![outer, retries]);
    let run = ran(engine.request(&call())).run().clone();
    let child = child.lock().unwrap()[0].clone();

    let asked = asked.lock().unwrap().clone();
    let stages: Vec<(Stage, Option<u64>)> = asked
        .iter()
        .map(|(answer, left)| (answer.clone().unwrap_err().stage(), *left))
        .collect();
    assert_eq!(
        stages,
        [
            (Stage::Unresolved, Some(PRICE)),
            (Stage::Unresolved, Some(0)),
            (Stage::NotSent, Some(0)),
        ]
    );
    // Two attempts reached the backend, each with its own dispatch and settlement;
    // the third was refused before either.
    assert_eq!(filler.requests(), 2);
    let mut names = whole(&run, 1, &child, "unresolved", PRICE);
    names.extend(whole(&run, 2, &child, "unresolved", PRICE));
    assert_eq!(sent(&world), sorted(names));
    let record = world.record(&child);
    assert_eq!(record.outcome().status, Status::Completed);
    let kinds: Vec<RefusalKind> = record
        .outcome()
        .refusals
        .iter()
        .map(|one| one.kind())
        .collect();
    assert_eq!(kinds, [RefusalKind::RequestOverAllowance]);
}

/// A filler that answers as the one it holds does, once it has left the store with
/// no room for a publication that adds a name.
struct Filling(FakeFiller, Arc<Store>);

impl Filler for Filling {
    fn declaration(&self) -> &Declaration {
        self.0.declaration()
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        self.1.limit_generations_to(1);
        self.0.fill(request, bounds)
    }
}

#[test]
fn a_settlement_whose_publication_fails_leaves_the_dispatch_unsettled_counted_at_its_bound_and_the_run_abandoned()
 {
    let world = World::new("settle-fails");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(Filling(priced().charging(100_000), world.store.clone()));
    let seen: Arc<Mutex<Vec<Fault>>> = Arc::default();
    let asks = {
        let seen = seen.clone();
        world.action("asks", &[Capability::Fill], move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            seen.lock()
                .unwrap()
                .push(fill.fill(&fill_request()).unwrap_err());
            assert_eq!(context.allowance().left_micros(), Some(0));
            Ok(Produced::nothing())
        })
    };
    let engine = metered(&world, filler, 1, vec![asks]);
    let failed = engine.request(&call()).unwrap_err();
    let CallError::Abandoned { run, why } = &failed else {
        panic!("{failed:?}");
    };
    assert!(why.contains("the meter failed"), "{why}");
    // The action was handed no answer of the request.
    assert_eq!(seen.lock().unwrap()[0].stage(), Stage::Unresolved);
    let state = engine.status(run).unwrap().state().clone();
    assert!(matches!(state, RunState::Abandoned { .. }), "{state:?}");
    // The dispatch is durable and has no settlement: counted at its bound.
    assert_eq!(sent(&world), dispatch_names(run, 1, run, PRICE));
    assert_eq!(outstanding(&world, run), Sum::new(PRICE.into()));
    assert!(
        !spend_names(&world.store)
            .iter()
            .any(|name| name.contains("/settl"))
    );
}
