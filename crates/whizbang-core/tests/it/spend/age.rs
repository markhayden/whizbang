//! Estimates expire (plan 1B, task S8): the part that refuses, as stale, a request
//! that came with a preparation an hour old or older, or one made at a time later than
//! now. Every test moves the executor's clock by hand; none sleeps.

use std::sync::Arc;

use whizbang_core::contract::{
    Answer, Call, CallError, Capability, Engine, Executor, Outcome, Produced, Refusal, RefusalKind,
    Request, RequestId,
};
use whizbang_core::spend::{
    AGE_PART, MADE_AFTER_NOW, PREPARATION_HOLDS_MS, PreparationAge, TOO_OLD,
};
use whizbang_testkit::ledger::FakeLedger;
use whizbang_testkit::part::FakePart;

use super::support::{Bounds, Said, World, ran};

/// When each test's first preparation is made, by the executor's clock.
const MADE: u64 = 1_700_000_000_000;
const HOUR: u64 = 3_600_000;

fn id(text: &str) -> RequestId {
    RequestId::parse(text).unwrap()
}

/// A world whose clock says [`MADE`], and an executor over it with `paint`, a ledger
/// and the part.
fn painting(label: &str) -> (World, Arc<FakeLedger>, Engine) {
    let world = World::new(label);
    world.clock.set(MADE);
    let ledger = Arc::new(FakeLedger::new());
    let engine = world
        .engine(Bounds::quarter(), vec![world.paints("paint")])
        .with_ledger(ledger.clone())
        .with_part(Arc::new(PreparationAge));
    (world, ledger, engine)
}

fn stale(why: &str) -> CallError {
    CallError::Refused(Refusal::ByPart {
        action: "paint".to_owned(),
        part: "age",
        kind: RefusalKind::Stale,
        why: why.to_owned(),
    })
}

#[test]
fn the_hour_is_a_constant_and_the_part_holds_nothing_that_could_shorten_it() {
    assert_eq!(PREPARATION_HOLDS_MS, HOUR);
    assert_eq!(AGE_PART, "age");
    // The part is made of nothing: no length, no variable's name and no switch is in
    // it for a test or an environment to set.
    assert_eq!(std::mem::size_of::<PreparationAge>(), 0);
}

#[test]
fn a_preparation_is_admitted_one_millisecond_before_the_hour_and_refused_at_the_hour() {
    let (world, ledger, engine) = painting("age");
    let call = Call::new("paint");
    let prepared = engine.prepare(&call).unwrap();
    assert_eq!(prepared.made_ms(), MADE);
    let expecting = |request: &str| {
        Request::new(call.clone())
            .expecting(&prepared)
            .with_id(id(request))
    };

    // One millisecond before the hour it is admitted, and runs.
    world.clock.set(MADE + HOUR - 1);
    let finished = ran(engine.request(&expecting("order-1")));
    assert_eq!(finished.outcome(), &Outcome::Completed { outputs: vec![] });
    assert_eq!(ledger.reservations(world.store.as_ref()).unwrap().len(), 1);

    // At the hour, and at any time after it, it is refused: no run, no reservation,
    // and the store as it was.
    let before = (world.held(), ledger.asked());
    for now in [MADE + HOUR, MADE + HOUR + 1, MADE + 40 * HOUR, u64::MAX] {
        world.clock.set(now);
        let refused = engine.request(&expecting("order-2")).unwrap_err();
        assert_eq!(refused, stale(TOO_OLD), "{now}");
        assert_eq!(refused.refusal().unwrap().kind(), RefusalKind::Stale);
        assert_eq!(
            refused.to_string(),
            "paint: refused at admission, by age: the preparation the request came with is an \
             hour old or older: prepare the call again"
        );
        assert_eq!((world.held(), ledger.asked()), before, "{now}");
        assert_eq!(ledger.reservations(world.store.as_ref()).unwrap().len(), 1);
    }
    // The other ways in are held the same.
    world.clock.set(MADE + HOUR);
    assert_eq!(
        engine.start(&expecting("order-2")).unwrap_err(),
        stale(TOO_OLD)
    );
    let without_id = Request::new(call.clone()).expecting(&prepared);
    assert_eq!(engine.request(&without_id).unwrap_err(), stale(TOO_OLD));
    assert_eq!(
        (world.held(), ledger.asked(), world.ran()),
        (before.0, 1, 1)
    );
}

#[test]
fn a_preparation_made_after_now_as_a_clock_set_back_gives_is_refused() {
    let (world, ledger, engine) = painting("age-set-back");
    let call = Call::new("paint");
    let prepared = engine.prepare(&call).unwrap();
    let request = Request::new(call.clone())
        .expecting(&prepared)
        .with_id(id("order-1"));
    let before = world.held();
    // The clock is set back, by a millisecond and by a long way.
    for now in [MADE - 1, MADE - 2 * HOUR, 0] {
        world.clock.set(now);
        let refused = engine.request(&request).unwrap_err();
        assert_eq!(refused, stale(MADE_AFTER_NOW), "{now}");
        assert_eq!(
            refused.to_string(),
            "paint: refused at admission, by age: the preparation the request came with was \
             made at a time later than now, so its age cannot be told: prepare the call again"
        );
        assert_eq!(
            (world.held(), ledger.asked(), world.ran()),
            (before.clone(), 0, 0)
        );
    }
    // Made at the very time of the admission, it is no time old, and admitted: the
    // id the refused request came with is free.
    world.clock.set(MADE);
    ran(engine.request(&request));
    assert_eq!(world.ran(), 1);
}

#[test]
fn the_same_request_id_sent_again_after_the_hour_returns_the_first_run_and_is_not_checked_for_age()
{
    let world = World::new("age-repeat");
    world.clock.set(MADE);
    // A part before the age's counts how often the parts are asked.
    let counting = Arc::new(FakePart::new("counting"));
    let engine = world
        .engine(Bounds::quarter(), vec![world.paints("paint")])
        .with_part(counting.clone())
        .with_part(Arc::new(PreparationAge));
    let call = Call::new("paint");
    let prepared = engine.prepare(&call).unwrap();
    let request = Request::new(call.clone())
        .expecting(&prepared)
        .with_id(id("order-17"));
    let finished = ran(engine.request(&request));
    assert_eq!(counting.asked(), 1);

    // After the hour, and with the clock set back to before the preparation, the same
    // request under its id is answered with the run it made: the id is looked up
    // first, and no part is asked.
    for now in [MADE + HOUR, MADE + 30 * HOUR, MADE - 1] {
        world.clock.set(now);
        match engine.request(&request) {
            Ok(Answer::Repeat(earlier)) => assert_eq!(earlier.run(), finished.run(), "{now}"),
            other => panic!("{now}: not a repeat: {other:?}"),
        }
    }
    assert_eq!((counting.asked(), world.ran()), (1, 1));
    // The same preparation under another id is a new request, and is checked.
    world.clock.set(MADE + HOUR);
    let other = Request::new(call)
        .expecting(&prepared)
        .with_id(id("order-18"));
    assert_eq!(engine.request(&other).unwrap_err(), stale(TOO_OLD));
    assert_eq!(world.ran(), 1);
}

#[test]
fn preparing_again_after_a_refusal_gives_a_preparation_that_is_admitted() {
    let (world, ledger, engine) = painting("age-again");
    let call = Call::new("paint");
    let old = engine.prepare(&call).unwrap();
    world.clock.set(MADE + HOUR);
    let request = |prepared| {
        Request::new(call.clone())
            .expecting(prepared)
            .with_id(id("order-1"))
    };
    assert_eq!(engine.request(&request(&old)).unwrap_err(), stale(TOO_OLD));
    assert_eq!(ledger.asked(), 0);
    // Prepared again, at the time it now is: the same call, with the same fingerprint,
    // and a time of its own. Under the same id, which the refusal left free, it runs.
    let fresh = engine.prepare(&call).unwrap();
    assert_eq!(fresh.fingerprint(), old.fingerprint());
    assert_eq!(fresh.made_ms(), MADE + HOUR);
    world.clock.set(MADE + 2 * HOUR - 1);
    ran(engine.request(&request(&fresh)));
    assert_eq!(world.ran(), 1);
    // And the old one is still refused.
    let again = Request::new(call.clone()).expecting(&old);
    assert_eq!(engine.request(&again).unwrap_err(), stale(TOO_OLD));
}

#[test]
fn a_request_that_came_with_no_preparation_is_not_checked_for_age() {
    let (world, _, engine) = painting("age-none");
    let call = Call::new("paint");
    // A preparation was made, and is not handed over.
    let prepared = engine.prepare(&call).unwrap();
    for (runs, now) in [MADE + HOUR, MADE + 90 * HOUR, MADE - 1, 0]
        .into_iter()
        .enumerate()
    {
        world.clock.set(now);
        // With the preparation the request is refused; without it, it is admitted,
        // whatever the clock says, and so is a plain call.
        let with = Request::new(call.clone()).expecting(&prepared);
        assert!(matches!(
            engine.request(&with),
            Err(CallError::Refused(Refusal::ByPart { part: "age", .. }))
        ));
        ran(engine.request(&Request::new(call.clone())));
        engine.submit(&call).unwrap();
        assert_eq!(world.ran(), 2 * (runs as u64 + 1), "{now}");
    }
}

#[test]
fn a_call_from_inside_an_action_is_held_to_the_hour_too_and_its_refusal_is_in_its_callers_record() {
    let world = World::new("age-nested");
    world.clock.set(MADE);
    type Answered = Result<(), CallError>;
    let said: Said<Answered> = Said::default();
    let outer = {
        let (said, clock) = (said.clone(), world.clock.clone());
        world.action("outer", &[Capability::Fill], move |_, context| {
            let inside = context.executor();
            let call = Call::new("paint");
            let prepared = inside.prepare(&call).unwrap();
            let request = Request::new(call).expecting(&prepared);
            let mut said = said.lock().unwrap();
            clock.set(MADE + HOUR - 1);
            said.push(inside.request(&request).map(drop));
            clock.set(MADE + HOUR);
            said.push(inside.request(&request).map(drop));
            Ok(Produced::nothing())
        })
    };
    let engine = world
        .engine(Bounds::quarter(), vec![outer, world.paints("paint")])
        .with_part(Arc::new(PreparationAge));
    let finished = engine.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome(), &Outcome::Completed { outputs: vec![] });
    assert_eq!(*said.lock().unwrap(), [Ok(()), Err(stale(TOO_OLD))]);
    assert_eq!(world.ran(), 2);
    // The caller's record lists the refusal as stale, with the part's name in the
    // sentence.
    let record = world.record(finished.run());
    let listed: Vec<(RefusalKind, Option<&str>, &str)> = record
        .outcome()
        .refusals
        .iter()
        .map(|refusal| (refusal.kind(), refusal.action(), refusal.why()))
        .collect();
    let why = "paint: refused at admission, by age: the preparation the request came with is \
               an hour old or older: prepare the call again";
    assert_eq!(listed, [(RefusalKind::Stale, Some("paint"), why)]);
}
