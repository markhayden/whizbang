//! The reservation settled when its run ends (plan 1B, task S11): the real ledger's
//! `settle`, in the publication that ends a run nothing called. What the requests of
//! its tree were charged is spent, what is unresolved stays counted, and the rest is
//! released; where a dispatch has no settlement the reservation is closed as
//! interrupted and nothing is released. Each test is through an executor built with
//! the real ledger and the meter.

use std::sync::Arc;

use whizbang_core::capability::Stage;
use whizbang_core::contract::record::Status;
use whizbang_core::contract::{
    Answer, Call, CallError, Cancel, Capability, Closing, Declined, Executor, Failure, Ledger,
    MadeName, NotJudged, Produced, Refusal, Reread, Reservation, RunId, RunState, Settled, Step,
};
use whizbang_core::spend::fold::State;
use whizbang_core::spend::{ClosedBy, Record, StoreLedger, Sum, TurnBy};
use whizbang_core::store::GENERATION_MAX_BYTES;
use whizbang_testkit::barrier::Barriers;

use super::meter::{
    Asked, PRICE, asks, call, dispatch_names, fill_request, metered, metered_with, priced,
};
use super::settle::Filling;
use super::support::{Counted, Said, World, allow, books, ran, spend_names};

/// The closings the store holds, by their names.
fn closed(world: &World) -> Vec<String> {
    let names = spend_names(&world.store).into_iter();
    names
        .filter(|name| name.starts_with("spend/closed/"))
        .collect()
}

/// The closing the store holds of this run's reservation: whether it released, what
/// closed it, what it says was charged and is unresolved, and when.
fn closing(world: &World, run: &RunId) -> (bool, ClosedBy, u128, u128, u64) {
    let generation = world.store.snapshot().unwrap();
    let prefix = format!("spend/closed/{run}/");
    let (_, entry) = generation
        .entries()
        .iter()
        .find(|(name, _)| name.starts_with(&prefix))
        .unwrap();
    let record = Record::from_bytes(&world.store.read(entry).unwrap()).unwrap();
    let Record::Closing {
        released,
        by,
        charged,
        unresolved,
        at_ms,
        ..
    } = record
    else {
        panic!("not a closing: {record:?}");
    };
    (released, by, charged.micros(), unresolved.micros(), at_ms)
}

/// What the books say of a reservation and of its epoch: its state, what it counts
/// for, and what the epoch has left.
fn stands(world: &World, run: &RunId) -> (State, u128, u64) {
    let books = books(&world.store);
    let one = books.reservation(run).unwrap();
    (
        one.state(),
        one.counts().micros(),
        books.balance().left.micros(),
    )
}

/// What is left for a call that is over it, as its refusal says.
fn left_for_the_next(answer: Result<Answer, CallError>) -> u64 {
    match answer {
        Err(CallError::Refused(Refusal::OverAllowance { left_micros, .. })) => left_micros,
        other => panic!("not refused as over the allowance: {other:?}"),
    }
}

#[test]
fn a_run_that_reserved_ten_and_spent_three_leaves_seven_for_the_next_call_however_it_ended() {
    for how in ["completed", "failed", "panicked", "cancelled"] {
        let world = World::new(&format!("close-{how}"));
        let epoch = allow(&world.store, 1_000_000);
        let filler = Arc::new(priced().charging(75_000));
        let barriers = Barriers::new();
        // Four requests, each charged 75,000: 300,000 of the 1,000,000 reserved.
        let action = {
            let barriers = barriers.clone();
            world.action("asks", &[Capability::Fill], move |_, context| {
                let fill = context.capabilities().fill().unwrap();
                for _ in 0..4 {
                    fill.fill(&fill_request()).unwrap();
                }
                match how {
                    "failed" => Err(Failure::error("fill", "it failed")),
                    "panicked" => panic!("the action panicked"),
                    "cancelled" => {
                        barriers.stop_at("done");
                        Ok(Produced::nothing())
                    }
                    _ => Ok(Produced::nothing()),
                }
            })
        };
        let engine = metered(&world, filler.clone(), 4, vec![action]);
        let Ok(Answer::Started(run)) = engine.start(&call()) else {
            panic!("{how}: the request was not started");
        };
        if how == "cancelled" {
            barriers.arrived("done", 1);
            assert_eq!(engine.cancel(&run).unwrap(), Cancel::Accepted);
            barriers.open("done");
        }
        let state = engine.wait(&run).unwrap().state().clone();
        let RunState::Ended(ended) = state else {
            panic!("{how}: {state:?}");
        };
        let name = match how {
            "completed" | "cancelled" => how,
            _ => "error",
        };
        assert_eq!(ended.name(), name, "{how}");
        assert_eq!(filler.requests(), 4, "{how}");

        // The closing is in the store with the run's end: released, by the end, with
        // what was charged, at the time the run ended.
        assert_eq!(
            closed(&world),
            [format!("spend/closed/{run}/released")],
            "{how}"
        );
        assert_eq!(
            closing(&world, &run),
            (true, ClosedBy::End, 300_000, 0, 7000),
            "{how}"
        );
        // Three are spent and seven are left, and the epoch's turns are those its
        // publications took: an admission, four settlements and the closing.
        assert_eq!(stands(&world, &run), (State::Released, 300_000, 700_000));
        let folded = books(&world.store);
        assert!(!folded.needs_owner(epoch), "{how}");
        assert_eq!(folded.next_turn(epoch), 7, "{how}");
        assert!(folded.reservation(&run).unwrap().names().reachable());
        // The next call: one of 750,000 is refused with 700,000 left, and one of
        // 500,000 is admitted and runs.
        let over = metered(&world, filler.clone(), 3, vec![world.paints("asks")]);
        assert_eq!(left_for_the_next(over.request(&call())), 700_000, "{how}");
        let within = metered(&world, filler.clone(), 2, vec![world.paints("asks")]);
        ran(within.request(&call()));
    }
}

#[test]
fn a_run_that_spent_nothing_releases_all_of_it() {
    let world = World::new("close-nothing");
    let epoch = allow(&world.store, 1_000_000);
    let filler = Arc::new(priced());
    let engine = metered(&world, filler.clone(), 4, vec![world.paints("asks")]);
    let run = ran(engine.request(&call())).run().clone();
    assert_eq!(filler.requests(), 0);
    assert_eq!(closing(&world, &run), (true, ClosedBy::End, 0, 0, 7000));
    assert_eq!(stands(&world, &run), (State::Released, 0, 1_000_000));
    // The closing took the reservation's first place and its epoch's next turn.
    let names = spend_names(&world.store);
    for name in [
        format!("spend/place/{run}/1"),
        format!("spend/turn/{epoch}/2"),
    ] {
        assert!(names.contains(&name), "{name} is not in {names:?}");
    }
    // All of it is there for the next call, which reserves all of it.
    let run = ran(engine.request(&call())).run().clone();
    assert_eq!(stands(&world, &run), (State::Released, 0, 1_000_000));
}

#[test]
fn a_run_with_an_unresolved_request_releases_only_what_is_not_unresolved() {
    let world = World::new("close-unresolved");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(priced().failing_at(Stage::Unresolved));
    let asked: Asked = Arc::default();
    let engine = metered(&world, filler, 4, vec![asks(&world, 1, &asked)]);
    let run = ran(engine.request(&call())).run().clone();
    assert_eq!(
        asked.lock().unwrap()[0].clone().0.unwrap_err().stage(),
        Stage::Unresolved
    );
    // Its settlement is durable, at its bound: the closing releases the rest, and
    // the bound stays counted as unresolved.
    let price = u128::from(PRICE);
    assert_eq!(
        closing(&world, &run),
        (true, ClosedBy::End, price, price, 7000)
    );
    assert_eq!(stands(&world, &run), (State::Released, price, 750_000));
    let balance = books(&world.store).balance();
    assert_eq!(
        (balance.charged, balance.unresolved, balance.held),
        (Sum::new(price), Sum::new(price), Sum::ZERO)
    );
}

#[test]
fn a_tree_charged_more_than_was_reserved_is_closed_at_what_it_was_charged() {
    let world = World::new("close-over");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(priced().charging(300_000));
    let asked: Asked = Arc::default();
    let engine = metered(&world, filler, 1, vec![asks(&world, 1, &asked)]);
    let run = ran(engine.request(&call())).run().clone();
    // It reserved 250,000 and was charged 300,000: it counts for the charge, and
    // nothing of the reservation is given back.
    assert_eq!(
        closing(&world, &run),
        (true, ClosedBy::End, 300_000, 0, 7000)
    );
    assert_eq!(stands(&world, &run), (State::Released, 300_000, 700_000));
    // The settlement over its bound took the second place, and the closing the third.
    let names = spend_names(&world.store);
    assert!(names.contains(&format!("spend/place/{run}/3")), "{names:?}");
    assert!(
        !names.contains(&format!("spend/place/{run}/4")),
        "{names:?}"
    );
}

#[test]
fn a_root_with_a_dispatch_that_has_no_settlement_ends_closed_as_interrupted_with_nothing_released()
{
    let world = World::new("close-interrupted");
    let epoch = allow(&world.store, 1_000_000);
    // The child's request is dispatched, and its settlement cannot be published:
    // the store has no room left once the backend was called.
    let filler = Arc::new(Filling(priced().charging(100_000), world.store.clone()));
    let inner = world.action("inner", &[Capability::Fill], |_, context| {
        let fill = context.capabilities().fill().unwrap();
        fill.fill(&fill_request()).unwrap_err();
        Ok(Produced::nothing())
    });
    let child: Said<RunId> = Arc::default();
    let outer = {
        let (child, store) = (child.clone(), world.store.clone());
        world.action("asks", &[Capability::Fill], move |_, context| {
            let failed = context.executor().submit(&Call::new("inner")).unwrap_err();
            let CallError::Abandoned { run, .. } = failed else {
                panic!("{failed:?}");
            };
            child.lock().unwrap().push(run);
            // The store has room again: the root's own end can be published.
            store.limit_generations_to(GENERATION_MAX_BYTES);
            Ok(Produced::nothing())
        })
    };
    let engine = metered(&world, filler, 2, vec![outer, inner]);
    let run = ran(engine.request(&call())).run().clone();
    let child = child.lock().unwrap()[0].clone();

    // The dispatch is under the root's reservation with no settlement.
    let names = spend_names(&world.store);
    for name in dispatch_names(&run, 1, &child, PRICE) {
        assert!(names.contains(&name), "{name} is not in {names:?}");
    }
    assert!(!names.iter().any(|name| name.contains("/settl")));
    // Closed as interrupted: it counts for all that was reserved, which is more
    // than the one bound that is known, and nothing is released.
    let price = u128::from(PRICE);
    assert_eq!(closed(&world), [format!("spend/closed/{run}/held")]);
    assert_eq!(
        closing(&world, &run),
        (false, ClosedBy::End, 0, price, 7000)
    );
    assert_eq!(stands(&world, &run), (State::Interrupted, 500_000, 500_000));
    let folded = books(&world.store);
    assert!(!folded.needs_owner(epoch));
    assert_eq!(folded.balance().held, Sum::new(500_000));
    // The next call finds the 500,000 that was never reserved, and no more.
    let over = metered(
        &world,
        Arc::new(priced()),
        3,
        vec![world.paints("asks"), world.paints("inner")],
    );
    assert_eq!(left_for_the_next(over.request(&call())), 500_000);
}

#[test]
fn a_nested_calls_end_settles_nothing_in_the_ledger() {
    let world = World::new("close-nested");
    allow(&world.store, 1_000_000);
    let ledger = Counted::new();
    let filler = Arc::new(priced().charging(100_000));
    let inner = world.action("inner", &[Capability::Fill], |_, context| {
        let fill = context.capabilities().fill().unwrap();
        fill.fill(&fill_request()).unwrap();
        Ok(Produced::nothing())
    });
    // What the store held of closings, and how often the ledger had been asked to
    // close, once the child had ended.
    let seen: Said<(RunId, Vec<String>, u64)> = Arc::default();
    let outer = {
        let (seen, store, ledger) = (seen.clone(), world.store.clone(), ledger.clone());
        world.action("asks", &[Capability::Fill], move |_, context| {
            let finished = context.executor().submit(&Call::new("inner")).unwrap();
            let names = spend_names(&store).into_iter();
            let closings = names.filter(|name| name.starts_with("spend/closed/"));
            seen.lock().unwrap().push((
                finished.run().clone(),
                closings.collect(),
                ledger.settled(),
            ));
            Ok(Produced::nothing())
        })
    };
    let engine = metered_with(&world, filler, 2, vec![outer, inner], ledger.clone());
    let run = ran(engine.request(&call())).run().clone();
    let (child, closings, asked) = seen.lock().unwrap()[0].clone();
    // The child's end closed nothing and asked the ledger nothing.
    assert_eq!((closings, asked), (Vec::new(), 0));
    assert_eq!(world.record(&child).outcome().status, Status::Completed);
    // The root's end asked once, and closed the one reservation there is.
    assert_eq!(ledger.settled(), 1);
    assert_eq!(closed(&world), [format!("spend/closed/{run}/released")]);
    assert_eq!(stands(&world, &run), (State::Released, 100_000, 900_000));
}

/// A ledger that admits with no hold: it makes the epoch's next turn and nothing
/// else, as if the hold were lost, and closes as the real ledger does.
struct NoHold;

impl Ledger for NoHold {
    fn reserve(&self, of: &Reservation, step: &mut Step<'_, '_>) -> Result<MadeName, Declined> {
        let turn = Record::Turn {
            epoch: 1,
            turn: super::n(1),
            by: TurnBy::Admission,
            run: Some(of.run().clone()),
        };
        Ok(step.record(&turn)?)
    }

    fn confirm(&self, _: &Reservation, _: &Reread<'_>) -> Result<bool, NotJudged> {
        Ok(true)
    }

    fn settle(&self, closing: &Closing<'_>, step: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
        StoreLedger.settle(closing, step)
    }
}

#[test]
fn a_reservation_with_no_hold_is_closed_as_interrupted_and_takes_no_turn() {
    let world = World::new("close-no-hold");
    allow(&world.store, 1_000_000);
    let actions = vec![world.paints("asks")];
    let engine = metered_with(&world, Arc::new(priced()), 1, actions, Arc::new(NoHold));
    let run = ran(engine.request(&call())).run().clone();
    // Its names are not a set the ledger writes: it is closed with nothing
    // released. No hold names an epoch for it, so the fold owes no turn for its
    // closing, and none is taken: the one turn of the epoch is the admission's.
    assert_eq!(closed(&world), [format!("spend/closed/{run}/held")]);
    let names = spend_names(&world.store);
    let turns = names
        .iter()
        .filter(|name| name.starts_with("spend/turn/1/"));
    assert_eq!(turns.collect::<Vec<_>>(), ["spend/turn/1/1"]);
    assert!(names.contains(&format!("spend/place/{run}/1")), "{names:?}");
}
