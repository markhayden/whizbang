//! The cap of one call (plan 1B, task S3): the part that refuses, at admission, a call
//! whose estimate is over the most one call may reserve. The cap is the config's, from
//! the snapshot the call is admitted under, lowered by the request's own.

use std::sync::Arc;

use whizbang_core::contract::{
    Answer, Call, CallError, Capability, Engine, Estimate, Executor, Outcome, Produced, Refusal,
    RefusalKind, Request, RequestId, Secrets,
};
use whizbang_core::spend::{
    CAP_PART, CallCap, OVER_ITS_OWN_CAP, OVER_THE_CAP, OWN_CAP_ABOVE, OWN_CAP_ZERO,
    SNAPSHOT_CAP_ZERO,
};
use whizbang_testkit::ledger::FakeLedger;

use super::support::{Bounds, QUARTER, Said, World, by_part, ran};

fn id(text: &str) -> RequestId {
    RequestId::parse(text).unwrap()
}

/// A request for `paint`, which may cost a quarter of a dollar, capped by itself at
/// this.
fn capped(micros: u64) -> Request {
    Request::new(Call::new("paint")).capped_at(Estimate::of_micros(micros))
}

/// An executor with `paint`, a ledger and the cap's part.
fn painting(world: &World, ledger: &Arc<FakeLedger>) -> Engine {
    world
        .engine(Bounds::quarter(), vec![world.paints("paint")])
        .with_ledger(ledger.clone())
        .with_part(Arc::new(CallCap))
}

fn over_the_cap(action: &str, why: &str) -> CallError {
    CallError::Refused(Refusal::ByPart {
        action: action.to_owned(),
        part: "cap",
        kind: RefusalKind::OverAllowance,
        why: why.to_owned(),
    })
}

#[test]
fn a_call_whose_estimate_is_over_the_cap_is_refused_with_no_run_no_reservation_and_its_id_free() {
    let world = World::new("cap");
    let ledger = Arc::new(FakeLedger::new());
    let engine = painting(&world, &ledger);
    let call = Call::new("paint");
    assert_eq!(
        engine.prepare(&call).unwrap().estimate(),
        Estimate::of_micros(QUARTER)
    );
    assert_eq!(CAP_PART, "cap");

    // One millionth under the estimate: every way a call comes in is refused, before
    // a run exists, and the store is as it was.
    world.cap(Some(QUARTER - 1));
    let before = world.held();
    let with_id = Request::new(call.clone()).with_id(id("order-17"));
    for request in [with_id.clone(), Request::new(call.clone())] {
        let refused = engine.request(&request).unwrap_err();
        assert_eq!(refused, over_the_cap("paint", OVER_THE_CAP));
        assert_eq!(
            refused.to_string(),
            "paint: refused at admission, by cap: its estimate is over the cap of one call, \
             which the config sets as spend.call_cap_usd"
        );
        assert_eq!(world.held(), before);
        assert_eq!(ledger.asked(), 0);
        assert_eq!(ledger.reservations(world.store.as_ref()).unwrap(), []);
    }
    assert_eq!(
        engine.submit(&call).unwrap_err(),
        over_the_cap("paint", OVER_THE_CAP)
    );
    assert_eq!(
        engine.start(&with_id).unwrap_err(),
        over_the_cap("paint", OVER_THE_CAP)
    );
    assert_eq!((world.held(), world.ran()), (before, 0));

    // A cap the estimate is not over admits it: an estimate equal to the cap is not
    // over it. The id the refused request came with was not taken.
    world.cap(Some(QUARTER));
    let finished = ran(engine.request(&with_id));
    assert_eq!(finished.outcome(), &Outcome::Completed { outputs: vec![] });
    assert_eq!(ledger.reservations(world.store.as_ref()).unwrap().len(), 1);
    // And a config that caps nothing admits any call.
    world.cap(None);
    engine.submit(&call).unwrap();
    assert_eq!(world.ran(), 2);
}

#[test]
fn a_cost_with_no_bound_is_over_every_cap() {
    // No ledger: an executor that reserves refuses such a call itself, before its
    // parts are asked.
    let world = World::new("cap-unbounded");
    let engine = world
        .engine(Bounds(Estimate::unbounded()), vec![world.paints("paint")])
        .with_part(Arc::new(CallCap));
    let call = Call::new("paint");
    assert!(engine.prepare(&call).unwrap().estimate().is_unbounded());
    world.cap(Some(4_000_000_000_000));
    assert_eq!(
        engine.submit(&call).unwrap_err(),
        over_the_cap("paint", OVER_THE_CAP)
    );
    let largest = Request::new(call.clone()).capped_at(Estimate::of_micros(u64::MAX - 1));
    world.cap(None);
    assert_eq!(
        engine.request(&largest).unwrap_err(),
        over_the_cap("paint", OVER_ITS_OWN_CAP)
    );
    assert_eq!(world.ran(), 0);
    // The largest amount there is, as a cap, is no bound either: no cap admits such a
    // cost, the configured one or the request's own.
    world.cap(Some(u64::MAX));
    assert_eq!(
        engine.submit(&call).unwrap_err(),
        over_the_cap("paint", OVER_THE_CAP)
    );
    world.cap(None);
    let no_bound = Request::new(call.clone()).capped_at(Estimate::unbounded());
    assert_eq!(
        engine.request(&no_bound).unwrap_err(),
        over_the_cap("paint", OVER_ITS_OWN_CAP)
    );
    assert_eq!(world.ran(), 0);
    // Only a call that nothing caps is let through by the part.
    engine.submit(&call).unwrap();
}

#[test]
fn a_call_from_inside_an_action_that_the_cap_refuses_gives_its_callers_share_back() {
    // Under a ledger a call from inside an action takes its estimate from what its
    // caller reserved before the parts are asked. `outer` reserves half a dollar, and
    // each call of `paint` takes a quarter.
    //
    // A call that ends having used nothing gives its share back to its caller (ADR
    // 0016, "An allowance holds a signed balance"; this test held the rule before
    // it, under which a share never came back). So that the caller's share is still
    // seen to be a limit, each `paint` here spends its quarter: it sends a request
    // of that bound, which is never settled and stays counted in full.
    let world = World::new("cap-nested-ledger");
    type Step = (Option<u64>, Result<(), CallError>, Option<u64>);
    let said: Said<Step> = Said::default();
    let outer = {
        let said = said.clone();
        world.action(
            "outer",
            &[Capability::Fill, Capability::Segment],
            move |_, context| {
                let low =
                    Request::new(Call::new("paint")).capped_at(Estimate::of_micros(QUARTER - 1));
                let plain = Request::new(Call::new("paint"));
                // Two the cap refuses, then two that run, then one more.
                for request in [&low, &low, &plain, &plain, &plain] {
                    let before = context.allowance().left_micros();
                    let answer = context.executor().request(request).map(drop);
                    let after = context.allowance().left_micros();
                    said.lock().unwrap().push((before, answer, after));
                }
                Ok(Produced::nothing())
            },
        )
    };
    let paint = world.action("paint", &[Capability::Fill], |_, context| {
        let bound = whizbang_core::spend::Amount::new(QUARTER).unwrap();
        let sent = context.allowance().dispatch(bound, || Ok(()));
        assert!(sent.is_ok(), "{:?}", sent.err());
        Ok(Produced::nothing())
    });
    let ledger = Arc::new(FakeLedger::new());
    let engine = world
        .engine(Bounds::quarter(), vec![outer, paint])
        .with_ledger(ledger.clone())
        .with_part(Arc::new(CallCap));
    world.cap(Some(2 * QUARTER));
    engine.submit(&Call::new("outer")).unwrap();
    let (half, quarter) = (Some(2 * QUARTER), Some(QUARTER));
    let refused = || Err(over_the_cap("paint", OVER_ITS_OWN_CAP));
    // What a refused call took is given back whole, so the two that follow run, and
    // each spends its quarter; the last is refused only because nothing is left, and
    // not by the cap.
    let nothing_left = CallError::Refused(Refusal::OverCaller {
        action: "paint".to_owned(),
        asked_micros: QUARTER,
        left_micros: 0,
    });
    assert_eq!(
        *said.lock().unwrap(),
        [
            (half, refused(), half),
            (half, refused(), half),
            (half, Ok(()), quarter),
            (quarter, Ok(()), Some(0)),
            (Some(0), Err(nothing_left), Some(0)),
        ]
    );
    assert_eq!(world.ran(), 3);
}

#[test]
fn a_calls_own_cap_lowers_the_cap_for_that_call() {
    let world = World::new("cap-own");
    let ledger = Arc::new(FakeLedger::new());
    let engine = painting(&world, &ledger);
    world.cap(Some(1_000_000));
    let before = world.held();

    // Under the configured cap and over its own: refused, by its own.
    let refused = engine.request(&capped(QUARTER - 1)).unwrap_err();
    assert_eq!(refused, over_the_cap("paint", OVER_ITS_OWN_CAP));
    assert_eq!(
        refused.to_string(),
        "paint: refused at admission, by cap: its estimate is over the cap the request came with"
    );
    assert_eq!((world.held(), ledger.asked(), world.ran()), (before, 0, 0));
    // Its own cap is of that call alone: the same call without one is admitted, and
    // so is one whose own cap it is not over.
    engine.submit(&Call::new("paint")).unwrap();
    ran(engine.request(&capped(QUARTER)));
    assert_eq!(world.ran(), 2);

    // A request's cap holds its call where the config caps nothing.
    world.cap(None);
    assert_eq!(
        engine.request(&capped(QUARTER - 1)).unwrap_err(),
        over_the_cap("paint", OVER_ITS_OWN_CAP)
    );
    ran(engine.request(&capped(QUARTER)));
    assert_eq!(world.ran(), 3);
}

#[test]
fn a_calls_own_cap_above_the_configured_one_is_refused_not_lowered() {
    let world = World::new("cap-above");
    let ledger = Arc::new(FakeLedger::new());
    let engine = painting(&world, &ledger);
    // The call may cost a quarter, which is under both caps: were its own cap lowered
    // to the configured one, it would be admitted.
    world.cap(Some(300_000));
    let before = world.held();
    let request = capped(300_001).with_id(id("order-17"));
    let refused = engine.request(&request).unwrap_err();
    assert_eq!(
        refused,
        CallError::Refused(Refusal::ByPart {
            action: "paint".to_owned(),
            part: "cap",
            kind: RefusalKind::Invalid,
            why: OWN_CAP_ABOVE.to_owned(),
        })
    );
    assert_eq!(
        refused.to_string(),
        "paint: refused at admission, by cap: the cap the request came with is above the cap \
         of one call, which the config sets as spend.call_cap_usd: a request lowers that cap, \
         and never raises it"
    );
    let (_, kind, _) = by_part(engine.request(&capped(u64::MAX)));
    assert_eq!(kind, RefusalKind::Invalid);
    assert_eq!((world.held(), ledger.asked(), world.ran()), (before, 0, 0));
    // A cap of its own that is the configured one raises nothing, and the id is free.
    ran(engine.request(&capped(300_000).with_id(id("order-17"))));
    assert_eq!(world.ran(), 1);
}

#[test]
fn a_cap_of_zero_is_refused_as_not_a_cap() {
    let world = World::new("cap-zero");
    let ledger = Arc::new(FakeLedger::new());
    // `free` needs no model, so a call of it may cost nothing: it is over no cap, zero
    // among them, and is refused all the same.
    let free = world.action("free", &[], |_, _| Ok(Produced::nothing()));
    let engine = world
        .engine(Bounds::quarter(), vec![free])
        .with_ledger(ledger.clone())
        .with_part(Arc::new(CallCap));
    let call = Call::new("free");
    assert_eq!(
        engine.prepare(&call).unwrap().estimate(),
        Estimate::of_micros(0)
    );
    let zero = Request::new(call.clone()).capped_at(Estimate::of_micros(0));
    let before = world.held();
    for cap in [None, Some(1_000_000)] {
        world.cap(cap);
        let refused = engine.request(&zero).unwrap_err();
        assert_eq!(
            refused,
            CallError::Refused(Refusal::ByPart {
                action: "free".to_owned(),
                part: "cap",
                kind: RefusalKind::Invalid,
                why: OWN_CAP_ZERO.to_owned(),
            })
        );
        assert_eq!(
            refused.to_string(),
            "free: refused at admission, by cap: the cap the request came with is zero, which \
             is not a cap: a cap is an amount above zero"
        );
        assert_eq!((world.held(), ledger.asked()), (before.clone(), 0));
    }
    // The smallest cap there is admits it.
    ran(engine.request(&Request::new(call.clone()).capped_at(Estimate::of_micros(1))));
    assert_eq!(world.ran(), 1);

    // A snapshot whose cap is zero: the config's reader gives none (it refuses the
    // file), so this is no judgement of the call. The part fails, and no run is made.
    world.cap(Some(0));
    let before_failing = world.held();
    assert_ne!(before_failing, before);
    for request in [Request::new(call.clone()), zero] {
        let failed = engine.request(&request).unwrap_err();
        assert_eq!(
            failed,
            CallError::Part {
                part: "cap",
                why: SNAPSHOT_CAP_ZERO.to_owned(),
            }
        );
        assert!(failed.refusal().is_none());
    }
    assert_eq!((world.held(), world.ran()), (before_failing, 1));
    assert_eq!(
        SNAPSHOT_CAP_ZERO,
        "the cap of one call in the snapshot of the config is zero, which is not a cap"
    );
}

#[test]
fn a_call_from_inside_an_action_is_held_to_the_cap_too_and_its_refusal_is_in_its_callers_record() {
    let world = World::new("cap-nested");
    type Answered = Result<(), CallError>;
    let said: Said<Answered> = Said::default();
    let outer = {
        let said = said.clone();
        world.action("outer", &[Capability::Fill], move |_, context| {
            let inside = context.executor();
            let mut said = said.lock().unwrap();
            // Two models: half a dollar, which is over the cap its caller was under.
            said.push(inside.submit(&Call::new("both")).map(drop));
            // One model, and a cap of its own that it is over, then one that is above
            // the configured cap, then one that is neither.
            let one =
                |micros| Request::new(Call::new("paint")).capped_at(Estimate::of_micros(micros));
            said.push(inside.request(&one(QUARTER - 1)).map(drop));
            said.push(inside.request(&one(300_001)).map(drop));
            said.push(inside.request(&one(QUARTER)).map(drop));
            Ok(Produced::nothing())
        })
    };
    let both = world.action("both", &[Capability::Fill, Capability::Segment], |_, _| {
        Ok(Produced::nothing())
    });
    // No ledger, so that nothing but the cap holds the calls: with one, a call that
    // may cost more than its caller reserved is refused before any part is asked.
    let engine = world
        .engine(Bounds::quarter(), vec![outer, both, world.paints("paint")])
        .with_part(Arc::new(CallCap));
    assert_eq!(
        engine.prepare(&Call::new("both")).unwrap().estimate(),
        Estimate::of_micros(2 * QUARTER)
    );
    world.cap(Some(300_000));
    let finished = engine.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome(), &Outcome::Completed { outputs: vec![] });
    let invalid = CallError::Refused(Refusal::ByPart {
        action: "paint".to_owned(),
        part: "cap",
        kind: RefusalKind::Invalid,
        why: OWN_CAP_ABOVE.to_owned(),
    });
    assert_eq!(
        *said.lock().unwrap(),
        [
            Err(over_the_cap("both", OVER_THE_CAP)),
            Err(over_the_cap("paint", OVER_ITS_OWN_CAP)),
            Err(invalid),
            Ok(()),
        ]
    );
    // `outer` and the one call that was admitted ran; no refused call made a run.
    assert_eq!(world.ran(), 2);
    assert_eq!(world.store.snapshot().unwrap().runs().len(), 2);
    // The caller's record lists each refusal under the kind the part chose, with the
    // part's name in the sentence.
    let record = world.record(finished.run());
    let listed: Vec<(RefusalKind, Option<&str>, &str)> = record
        .outcome()
        .refusals
        .iter()
        .map(|refusal| (refusal.kind(), refusal.action(), refusal.why()))
        .collect();
    let over = "both: refused at admission, by cap: its estimate is over the cap of one call, \
                which the config sets as spend.call_cap_usd";
    let own = "paint: refused at admission, by cap: its estimate is over the cap the request \
               came with";
    let above = format!("paint: refused at admission, by cap: {OWN_CAP_ABOVE}");
    assert_eq!(
        listed,
        [
            (RefusalKind::OverAllowance, Some("both"), over),
            (RefusalKind::OverAllowance, Some("paint"), own),
            (RefusalKind::Invalid, Some("paint"), above.as_str()),
        ]
    );
}

#[test]
fn a_call_from_inside_an_action_is_held_to_the_cap_its_caller_was_admitted_under() {
    // The config caps nothing when `outer` is admitted, and caps a call at a millionth
    // while it runs: the call it makes is held to the cap of its caller's snapshot.
    let world = World::new("cap-nested-snapshot");
    let said: Said<bool> = Said::default();
    let outer = {
        let (said, source) = (said.clone(), world.source.clone());
        world.action("outer", &[], move |_, context| {
            source.swap(World::snapshot(Some(1)));
            let answer = context.executor().submit(&Call::new("paint"));
            said.lock().unwrap().push(answer.is_ok());
            Ok(Produced::nothing())
        })
    };
    let engine = world
        .engine(Bounds::quarter(), vec![outer, world.paints("paint")])
        .with_part(Arc::new(CallCap));
    engine.submit(&Call::new("outer")).unwrap();
    assert_eq!(*said.lock().unwrap(), [true]);
    // The next call that nothing called is held to the cap the config has by then.
    assert_eq!(
        engine.submit(&Call::new("paint")).unwrap_err(),
        over_the_cap("paint", OVER_THE_CAP)
    );
}

#[test]
fn no_refusal_by_the_cap_quotes_an_amount_so_a_cap_that_spells_a_secret_is_not_shown() {
    // A cap whose millionths spell a secret of digits, which the config's reader does
    // not find in a number (ADR 0011, "What the reader does not do"). The executor
    // here is told of no secret, so nothing but the part's own words keeps it out.
    let spelled: u64 = 73_190_482_655;
    let world = World::new("cap-secret");
    let engine = world
        .engine(Bounds::quarter(), vec![world.paints("paint")])
        .with_part(Arc::new(CallCap));
    let shown = |answer: Result<Answer, CallError>| {
        let refused = answer.unwrap_err();
        format!("{refused:?} {refused}")
    };
    let mut all = String::new();
    // The configured cap spells it: a request's own cap above it, and a snapshot the
    // call is over.
    world.cap(Some(spelled));
    all += &shown(engine.request(&capped(spelled + 1)));
    let dear = world
        .engine(
            Bounds(Estimate::of_micros(spelled + 1)),
            vec![world.paints("paint")],
        )
        .with_part(Arc::new(CallCap));
    all += &shown(dear.request(&Request::new(Call::new("paint"))));
    // The request's own cap spells it, and the call is over it; and the estimate
    // spells it, over a cap that does not.
    world.cap(None);
    all += &shown(dear.request(&capped(spelled)));
    let exact = world
        .engine(
            Bounds(Estimate::of_micros(spelled)),
            vec![world.paints("paint")],
        )
        .with_part(Arc::new(CallCap));
    all += &shown(exact.request(&capped(1)));
    world.cap(Some(1));
    all += &shown(exact.request(&Request::new(Call::new("paint"))));
    assert_eq!(all.matches("by cap").count(), 5, "{all}");
    assert_eq!(
        all.matches(|c: char| c.is_ascii_digit()).count(),
        0,
        "{all}"
    );
    assert_eq!(world.ran(), 0);

    // Told of the secret, the executor finds it in no sentence either: each is as the
    // part wrote it.
    let secret = spelled.to_string();
    let told = world
        .engine(
            Bounds(Estimate::of_micros(spelled + 1)),
            vec![world.paints("paint")],
        )
        .with_secrets(Secrets::new([secret.clone()]).unwrap())
        .with_part(Arc::new(CallCap));
    world.cap(Some(spelled));
    let (_, _, why) = by_part(told.submit(&Call::new("paint")));
    assert_eq!(why, OVER_THE_CAP);
}
