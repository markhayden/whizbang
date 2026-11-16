//! Reserving at admission (plan 1B, task S2; ADR 0016): the real ledger,
//! `spend::StoreLedger`, in an executor a test builds. It folds the books of the
//! generation its step shows, declines what the allowance of the newest epoch does not
//! cover, and makes its hold and the epoch's next turn in the admission's publication;
//! and it answers whether a published admission stands, on the names read after it.
//! Then its proofs: two executors over one store, held inside the ledger; and a crash
//! before every step of an admission, with every choice of what the disk loses.
//!
//! An allowance is the owner's reset, which a test makes as the command does
//! (`support::allow`, which is `spend::reset`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::contract::{
    ADMITTED_NAME, Action, Admission, Answer, Call, CallError, Capability, Closing, Declined,
    Engine, Estimate, Executor, Ledger, MadeName, NotJudged, Produced, Refusal, RefusalKind,
    Registry, Request, RequestId, Reread, Reservation, RunId, RunState, Settled, Step,
};
use whizbang_core::identity::Digest;
use whizbang_core::spend::fold::State;
use whizbang_core::spend::{Counted as Count, Record, StoreLedger, Sum, TurnBy};
use whizbang_core::store::Store;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::fs_shim::{Lost, ShimFs};
use whizbang_testkit::ledger::FakeLedger;
use whizbang_testkit::scratch::Scratch;

use super::support::{
    Bounds, Counted, Hand, World, allow, books, counting, plant, plant_bytes, ran,
    reservation_bytes, reserved, spend_names,
};
use super::{amount, n, run};

// --- what the tests are made of ----------------------------------------------------------------

fn id(text: &str) -> RequestId {
    RequestId::parse(text).unwrap()
}

fn asked(request: &str) -> Request {
    Request::new(Call::new("paint")).with_id(id(request))
}

/// An executor with `paint`, a call of which may cost this, and this ledger.
fn painting(world: &World, price: u64, ledger: Arc<dyn Ledger>) -> Engine {
    world
        .engine(
            Bounds(Estimate::of_micros(price)),
            vec![world.paints("paint")],
        )
        .with_ledger(ledger)
}

fn over(asked_micros: u64, left_micros: u64) -> CallError {
    CallError::Refused(Refusal::OverAllowance {
        action: "paint".to_owned(),
        asked_micros,
        left_micros,
    })
}

fn needs_owner(epoch: u64) -> CallError {
    CallError::Refused(Refusal::NeedsOwner {
        action: "paint".to_owned(),
        epoch,
    })
}

/// What an epoch's balance says: allowed, counted, left.
fn balance(store: &Store, epoch: u64) -> (u64, Count, u64) {
    let balance = books(store).balance_of(epoch);
    (
        balance.allowed.micros(),
        balance.counted,
        balance.left.micros(),
    )
}

fn exactly(micros: u64) -> Count {
    Count::Exactly(Sum::new(micros.into()))
}

/// The content the store holds under a name outside runs.
fn content(store: &Store, name: &str) -> Vec<u8> {
    let generation = store.snapshot().unwrap();
    store.read(generation.get(name).unwrap()).unwrap()
}

type Hands = Box<dyn Fn(&Reservation) + Send + Sync>;

/// The real ledger, with other hands at work in one of two places: when it is asked
/// for a reservation for the first time, before it judges; or each time it has
/// judged and staged an admission, which is before that admission is published, and
/// so before the reading that confirms it. What those hands publish there and what
/// the admission makes are two publications that share no name, so the store holds
/// both when the reading is made: the names are those of hands that came between
/// the admission's publication and its reading.
struct Meddled {
    inner: StoreLedger,
    at_first_reserve: Mutex<Option<Hands>>,
    after_reserve: Option<Hands>,
}

impl Meddled {
    fn at_first_reserve(hands: impl Fn(&Reservation) + Send + Sync + 'static) -> Arc<Meddled> {
        Arc::new(Meddled {
            inner: StoreLedger,
            at_first_reserve: Mutex::new(Some(Box::new(hands))),
            after_reserve: None,
        })
    }

    fn after_reserve(hands: impl Fn(&Reservation) + Send + Sync + 'static) -> Arc<Meddled> {
        Arc::new(Meddled {
            inner: StoreLedger,
            at_first_reserve: Mutex::new(None),
            after_reserve: Some(Box::new(hands)),
        })
    }
}

impl Ledger for Meddled {
    fn reserve(
        &self,
        reservation: &Reservation,
        step: &mut Step<'_, '_>,
    ) -> Result<MadeName, Declined> {
        if let Some(hands) = self.at_first_reserve.lock().unwrap().take() {
            hands(reservation);
        }
        let made = self.inner.reserve(reservation, step)?;
        if let Some(hands) = &self.after_reserve {
            hands(reservation);
        }
        Ok(made)
    }

    fn confirm(&self, reservation: &Reservation, reading: &Reread<'_>) -> Result<bool, NotJudged> {
        self.inner.confirm(reservation, reading)
    }

    fn settle(&self, closing: &Closing<'_>, step: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
        self.inner.settle(closing, step)
    }
}

/// The owner's reset as other hands would leave it had it come after an admission
/// that is not yet published: the epoch after the first, with this allowance, and
/// this turn of the first.
fn reset_after(store: &Store, micros: u64, turn: u64) {
    plant(
        store,
        &[
            Record::Reset {
                epoch: n(2),
                allowance: amount(micros),
                at_ms: 0,
            },
            Record::Epoch { epoch: n(2) },
            Record::Turn {
                epoch: 1,
                turn: n(turn),
                by: TurnBy::Reset,
                run: None,
            },
        ],
    );
}

// --- reserve -----------------------------------------------------------------------------------

#[test]
fn the_ledger_folds_the_steps_generation_declines_over_the_allowance_and_makes_its_names() {
    let world = World::new("reserve");
    let ledger = Counted::left_open();
    // The owner allows a dollar and ten cents; a call may cost a quarter.
    assert_eq!(allow(&world.store, 1_100_000), 1);
    let engine = painting(&world, 250_000, ledger.clone());
    let first = ran(engine.request(&asked("order-1")));
    let one = first.run().clone();
    // The hold, in the newest epoch, and that epoch's next turn, made with the
    // reservation in the admission's publication; each record says what its name says
    // and nothing more.
    assert_eq!(
        spend_names(&world.store),
        [
            "spend/epoch/1".to_owned(),
            format!("spend/held/1/{one}/250000"),
            format!("spend/reservations/{one}"),
            "spend/reset/1/1100000".to_owned(),
            "spend/turn/0/1".to_owned(),
            "spend/turn/1/1".to_owned(),
        ]
    );
    assert_eq!(
        content(&world.store, &format!("spend/held/1/{one}/250000")),
        Record::Hold {
            epoch: 1,
            run: one.clone(),
            reserved: amount(250_000),
        }
        .to_bytes()
    );
    assert_eq!(
        content(&world.store, "spend/turn/1/1"),
        Record::Turn {
            epoch: 1,
            turn: n(1),
            by: TurnBy::Admission,
            run: Some(one.clone()),
        }
        .to_bytes()
    );
    // Asked once, confirmed once, and asked to close it when the run ended. This
    // test's ledger leaves it open, as that of a run that still executes
    // (`Counted::left_open`): it counts in full.
    assert_eq!(
        (ledger.asked(), ledger.confirmed(), ledger.settled()),
        (1, 1, 1)
    );
    let folded = books(&world.store);
    let standing = folded.reservation(&one).unwrap();
    assert_eq!(
        (standing.state(), standing.counts()),
        (State::Open, Sum::new(250_000))
    );
    assert_eq!(
        balance(&world.store, 1),
        (1_100_000, exactly(250_000), 850_000)
    );

    // Each admission is judged on the books as they are then, and takes the next
    // turn. The fifth is over what is left, and changes nothing: no generation, no
    // run, no name, and its id stays free.
    for (request, turn) in [("order-2", 2), ("order-3", 3), ("order-4", 4)] {
        ran(engine.request(&asked(request)));
        assert!(spend_names(&world.store).contains(&format!("spend/turn/1/{turn}")));
    }
    assert_eq!(
        balance(&world.store, 1),
        (1_100_000, exactly(1_000_000), 100_000)
    );
    let before = world.held();
    let refused = engine.request(&asked("order-5")).unwrap_err();
    assert_eq!(refused, over(250_000, 100_000));
    assert_eq!(
        engine.submit(&Call::new("paint")).unwrap_err(),
        over(250_000, 100_000)
    );
    assert_eq!(world.held(), before);
    assert_eq!((world.ran(), ledger.asked()), (4, 6));
    assert_eq!(reserved(&world.store).len(), 4);
    // An exact fit is taken, under the id that was refused, and then nothing is left.
    let exact = painting(&world, 100_000, ledger.clone());
    ran(exact.request(&asked("order-5")));
    assert_eq!(balance(&world.store, 1), (1_100_000, exactly(1_100_000), 0));
    assert_eq!(
        painting(&world, 1, ledger.clone())
            .request(&asked("order-6"))
            .unwrap_err(),
        over(1, 0)
    );
}

#[test]
fn a_store_with_no_allowance_admits_a_call_of_nothing_and_refuses_one_over_nothing() {
    let world = World::new("reserve-none");
    let ledger = Counted::new();
    // No reset was ever made: the store is in the epoch before the first, which
    // allows nothing. A call that may cost one millionth is refused, and nothing is
    // written.
    let before = world.held();
    let paid = painting(&world, 1, ledger.clone());
    let refused = paid.request(&asked("order-1")).unwrap_err();
    assert_eq!(refused, over(1, 0));
    assert_eq!(
        refused.to_string(),
        "paint was not admitted: it may cost up to 1 millionths of a US dollar, and 0 are \
         left of the ledger's allowance"
    );
    assert_eq!(
        refused.refusal().unwrap().kind(),
        RefusalKind::OverAllowance
    );
    assert_eq!(paid.submit(&Call::new("paint")).unwrap_err(), over(1, 0));
    assert_eq!(world.held(), before);
    assert_eq!((world.ran(), ledger.asked()), (0, 2));

    // A call of nothing is admitted there and runs, and the ledger is not asked:
    // not to reserve, not to confirm, and not to close at the run's end. No name
    // is made under `spend/`. (Restated after the review of task W1, 2026-11-15: it
    // was admitted under a hold of nothing, with six names.)
    let free = painting(&world, 0, ledger.clone());
    let finished = ran(free.request(&asked("order-1")));
    assert_eq!(spend_names(&world.store), Vec::<String>::new());
    assert_eq!(
        (ledger.asked(), ledger.confirmed(), ledger.settled()),
        (2, 0, 0)
    );
    assert_eq!(world.ran(), 1);
    assert_eq!(balance(&world.store, 0), (0, exactly(0), 0));
    // Its record says that nothing was reserved, as an amount: it ran under a
    // ledger, with nothing to spend.
    let record = world.record(finished.run());
    assert_eq!(record.spend().reserved_micros, Some(0));
    assert_eq!(
        paid.request(&asked("order-2")).unwrap_err(),
        over(1, 0),
        "a call of nothing leaves nothing to spend"
    );

    // The same in an epoch whose reset allows nothing.
    assert_eq!(allow(&world.store, 0), 1);
    assert_eq!(paid.request(&asked("order-2")).unwrap_err(), over(1, 0));
    ran(free.request(&asked("order-2")));
    assert_eq!(world.ran(), 2);
    assert_eq!(balance(&world.store, 1), (0, exactly(0), 0));
}

#[test]
fn a_call_with_no_bound_is_refused_and_the_ledger_is_not_asked() {
    let world = World::new("reserve-no-bound");
    let ledger = Counted::new();
    allow(&world.store, 1_000_000);
    let engine = world
        .engine(Bounds(Estimate::unbounded()), vec![world.paints("paint")])
        .with_ledger(ledger.clone());
    let before = world.held();
    let no_bound = CallError::Refused(Refusal::NoBound {
        action: "paint".to_owned(),
    });
    assert_eq!(engine.request(&asked("order-1")).unwrap_err(), no_bound);
    assert_eq!(engine.submit(&Call::new("paint")).unwrap_err(), no_bound);
    assert_eq!(world.held(), before);
    assert_eq!((world.ran(), ledger.asked()), (0, 0));
    // The id is free: with a bound, the same request is admitted under it.
    ran(painting(&world, 250_000, ledger.clone()).request(&asked("order-1")));
    assert_eq!(ledger.asked(), 1);
}

#[test]
fn prepare_makes_no_reservation_and_no_record() {
    let world = World::new("reserve-prepare");
    let ledger = Counted::new();
    allow(&world.store, 1_000_000);
    let engine = painting(&world, 250_000, ledger.clone());
    let before = world.held();
    let prepared = engine.prepare(&Call::new("paint")).unwrap();
    assert_eq!(prepared.estimate(), Estimate::of_micros(250_000));
    // Nothing: no generation, no name, no file, and the ledger was not asked.
    assert_eq!(world.held(), before);
    assert_eq!(
        (ledger.asked(), ledger.confirmed(), ledger.settled()),
        (0, 0, 0)
    );
    assert_eq!(reserved(&world.store), []);
    assert_eq!(balance(&world.store, 1), (1_000_000, exactly(0), 1_000_000));
}

#[test]
fn a_nested_call_asks_the_ledger_nothing() {
    let world = World::new("reserve-nested");
    let ledger = Counted::new();
    allow(&world.store, 1_000);
    // `outer` needs two models, so it may cost 200; `inner` needs one, and takes its
    // 100 from the allowance of the run that calls it.
    let outer = world.action(
        "outer",
        &[Capability::Fill, Capability::Judge],
        |_, context| {
            assert_eq!(context.allowance().total_micros(), Some(200));
            for _ in 0..2 {
                context.executor().submit(&Call::new("inner")).unwrap();
            }
            Ok(Produced::nothing())
        },
    );
    let inner = world.action("inner", &[Capability::Fill], |_, context| {
        assert_eq!(context.allowance().total_micros(), Some(100));
        Ok(Produced::nothing())
    });
    let engine = world
        .engine(Bounds(Estimate::of_micros(100)), vec![outer, inner])
        .with_ledger(ledger.clone());
    let finished = engine.submit(&Call::new("outer")).unwrap();
    assert_eq!(world.ran(), 3);
    // Asked once, for the run nothing called; and one reservation, one hold and one
    // turn, of that run, with the closing its end made. The two runs it called have
    // no name under `spend/`, and nothing was asked at their ends.
    assert_eq!(
        (ledger.asked(), ledger.confirmed(), ledger.settled()),
        (1, 1, 1)
    );
    let root = finished.run();
    assert_eq!(
        spend_names(&world.store),
        [
            format!("spend/closed/{root}/released"),
            "spend/epoch/1".to_owned(),
            format!("spend/held/1/{root}/200"),
            format!("spend/place/{root}/1"),
            format!("spend/reservations/{root}"),
            "spend/reset/1/1000".to_owned(),
            "spend/turn/0/1".to_owned(),
            "spend/turn/1/1".to_owned(),
            "spend/turn/1/2".to_owned(),
        ]
    );
    assert_eq!(world.store.snapshot().unwrap().runs().len(), 3);
    // Nothing was sent under it: all of it is released.
    assert_eq!(balance(&world.store, 1), (1_000, exactly(0), 1_000));
}

#[test]
fn an_admission_under_a_run_id_the_books_already_name_is_not_made() {
    // Other hands make a hold for the run the executor has just named, and take the
    // turn the ledger is about to choose. The ledger judged before them: its
    // publication is refused for the turn, and it is asked again for the same run.
    // The books now name that run. A second hold beside the one that is there would
    // leave a reservation whose amount no name can be trusted for, and a store that
    // needs its owner: the ledger makes nothing, and says why.
    let world = World::new("reserve-taken");
    allow(&world.store, 1_000_000);
    let store = world.store.clone();
    let ledger = Meddled::at_first_reserve(move |reservation| {
        plant(
            &store,
            &[
                Record::Hold {
                    epoch: 1,
                    run: reservation.run().clone(),
                    reserved: amount(7),
                },
                Record::Turn {
                    epoch: 1,
                    turn: n(1),
                    by: TurnBy::Admission,
                    run: Some(reservation.run().clone()),
                },
            ],
        );
    });
    let engine = painting(&world, 250_000, ledger);
    let before = world.held().0.1.len();
    let failed = engine.request(&asked("order-1")).unwrap_err();
    let CallError::Ledger { why } = &failed else {
        panic!("{failed:?}");
    };
    assert!(why.contains("already hold a name of the run"), "{why}");
    // Nothing of the admission is there: the two names of other hands, and no more.
    assert_eq!(world.held().0.1.len(), before + 2);
    assert_eq!(world.store.snapshot().unwrap().runs().len(), 0);
    assert_eq!((world.ran(), reserved(&world.store).len()), (0, 0));
    // The store goes on: the frozen reservation's one hold states its amount, and
    // a call beside it is admitted, with its reservation left open.
    let again = painting(&world, 250_000, Counted::left_open());
    ran(again.request(&asked("order-1")));
    assert_eq!(
        balance(&world.store, 1),
        (1_000_000, exactly(250_007), 749_993)
    );
}

// --- where the books need their owner, and where they do not ------------------------------------

#[test]
fn an_epoch_whose_turns_are_not_its_publications_admits_no_paid_call_until_a_reset_and_a_free_one_all_the_same()
 {
    let world = World::new("reserve-turns");
    let ledger = Counted::left_open();
    allow(&world.store, 1_000_000);
    let engine = painting(&world, 250_000, ledger.clone());
    ran(engine.request(&asked("order-1")));
    // Other hands make a turn that no publication took.
    plant(
        &world.store,
        &[Record::Turn {
            epoch: 1,
            turn: n(5),
            by: TurnBy::Settlement,
            run: None,
        }],
    );
    let before = world.held();
    let refused = engine.request(&asked("order-2")).unwrap_err();
    assert_eq!(refused, needs_owner(1));
    assert_eq!(
        refused.to_string(),
        "paint was not admitted: the ledger's books of epoch 1 need their owner, and nothing \
         is admitted until the owner resets the allowance (whizbang spend reset)"
    );
    // The record's kind for it is the allowance's: `run/3` has none of its own.
    assert_eq!(
        refused.refusal().unwrap().kind(),
        RefusalKind::OverAllowance
    );
    assert_eq!(world.held(), before);
    assert_eq!(world.ran(), 1);
    // A call of nothing is admitted all the same, whatever state the books are in:
    // the ledger is not asked, and nothing is made under `spend/`. (Restated after
    // the review of task W1, 2026-11-15: it was refused as every other call was.)
    let names = spend_names(&world.store);
    let asked_before = ledger.asked();
    let free = painting(&world, 0, ledger.clone());
    ran(free.request(&asked("order-free")));
    assert_eq!(world.ran(), 2);
    assert_eq!(spend_names(&world.store), names);
    assert_eq!(ledger.asked(), asked_before);
    let before = world.held();
    // The test kit's fake says the same of the same books.
    let fake = painting(&world, 250_000, Arc::new(FakeLedger::new()));
    assert_eq!(fake.request(&asked("order-2")).unwrap_err(), needs_owner(1));
    assert_eq!(world.held(), before);

    // The owner's reset opens the next epoch, whose turns start again.
    assert_eq!(allow(&world.store, 300_000), 2);
    let second = ran(engine.request(&asked("order-2")));
    assert!(spend_names(&world.store).contains(&format!("spend/held/2/{}/250000", second.run())));
    assert_eq!(
        balance(&world.store, 2),
        (300_000, exactly(250_000), 50_000)
    );
}

#[test]
fn a_frozen_reservation_whose_amount_no_name_states_counts_nowhere_and_stops_no_admission() {
    // Restated with the spend strand cut down (2026-11-15; ADR 0016, "The strand
    // cut down"). It was `..._stops_every_admission_and_a_reset_too`: every call
    // was told that the books need their owner, before a reset and after it.
    let world = World::new("reserve-unknown");
    let ledger = Counted::left_open();
    allow(&world.store, 1_000_000);
    // A name of a run that has no hold: a frozen reservation with no amount.
    plant(
        &world.store,
        &[Record::Place {
            run: run("9-9"),
            place: n(1),
            by: whizbang_core::spend::PlaceBy::Dispatch,
        }],
    );
    let stray = |store: &Store| {
        let folded = books(store);
        let one = folded.reservation(&run("9-9")).unwrap();
        let nowhere = !folded.resets().keys().any(|epoch| one.counts_in(*epoch));
        (one.state(), one.counted(), nowhere)
    };
    assert_eq!(
        stray(&world.store),
        (State::Frozen, Count::AtLeast(Sum::ZERO), true)
    );
    // It counts against no allowance and stops nothing: the call is admitted, and
    // is all that the epoch counts.
    let engine = painting(&world, 250_000, ledger.clone());
    ran(engine.request(&asked("order-1")));
    assert_eq!(
        balance(&world.store, 1),
        (1_000_000, exactly(250_000), 750_000)
    );
    // After a reset too.
    assert_eq!(allow(&world.store, 1_000_000), 2);
    ran(engine.request(&asked("order-2")));
    assert_eq!((world.ran(), reserved(&world.store).len()), (2, 2));
    assert_eq!(
        balance(&world.store, 2),
        (1_000_000, exactly(250_000), 750_000)
    );
    assert_eq!(
        stray(&world.store),
        (State::Frozen, Count::AtLeast(Sum::ZERO), true)
    );
}

/// A reservation of 10 that other hands, or an earlier process, left in epoch 1, as
/// history 81 of ADR 0016 has it: its request of 5 was dispatched and never settled,
/// and recovery closed it with nothing released. `dispatch` is the content of the
/// dispatch's record.
fn interrupted_reservation_of_ten(store: &Store, dispatch: Vec<u8>) {
    let nine = run("9-9");
    let sent = Record::Dispatch {
        run: nine.clone(),
        place: n(1),
        by: nine.clone(),
        request: 0,
        bound: amount(5),
        at_ms: 0,
    };
    let records = [
        Record::Hold {
            epoch: 1,
            run: nine.clone(),
            reserved: amount(10),
        },
        Record::Turn {
            epoch: 1,
            turn: n(1),
            by: TurnBy::Admission,
            run: Some(nine.clone()),
        },
        Record::Place {
            run: nine.clone(),
            place: n(1),
            by: whizbang_core::spend::PlaceBy::Dispatch,
        },
        Record::Closing {
            run: nine.clone(),
            released: false,
            by: whizbang_core::spend::ClosedBy::Recovery,
            charged: Sum::ZERO,
            unresolved: Sum::ZERO,
            at_ms: 0,
        },
        Record::Place {
            run: nine.clone(),
            place: n(2),
            by: whizbang_core::spend::PlaceBy::Closing,
        },
        Record::Turn {
            epoch: 1,
            turn: n(2),
            by: TurnBy::Closing,
            run: Some(nine),
        },
    ];
    let mut named: Vec<(String, Vec<u8>)> = records
        .iter()
        .map(|record| (record.name().to_string(), record.to_bytes()))
        .collect();
    named.push(reservation_bytes("9-9", 10));
    named.push((sent.name().to_string(), dispatch));
    plant_bytes(store, &named);
}

#[test]
fn one_record_that_cannot_be_read_freezes_its_own_reservation_and_stops_no_other_call() {
    // History 81, through an executor with the real ledger. The owner allows 30. A
    // reservation of 10 has one record that cannot be read: it is frozen at the most
    // its names can mean, which is 25 (its amount, the bound of 5, and its amount
    // again for the place its closing took), and that is all it does.
    let world = World::new("reserve-frozen");
    let ledger = Counted::left_open();
    allow(&world.store, 30);
    interrupted_reservation_of_ten(&world.store, b"not a record\n".to_vec());
    let folded = books(&world.store);
    let frozen = folded.reservation(&run("9-9")).unwrap();
    assert_eq!(
        (frozen.state(), frozen.counted()),
        (State::Frozen, exactly(25))
    );
    assert_eq!(balance(&world.store, 1), (30, exactly(25), 5));

    // A call of 5, which the rest of the allowance covers, is admitted, confirmed
    // and runs.
    let five = painting(&world, 5, ledger.clone());
    let finished = ran(five.request(&asked("order-1")));
    assert_eq!(world.ran(), 1);
    assert_eq!(ledger.confirmed(), 1);
    assert!(spend_names(&world.store).contains(&format!("spend/held/1/{}/5", finished.run())));
    assert_eq!(balance(&world.store, 1), (30, exactly(30), 0));
    // And one that it does not cover is refused for what is left, which is nothing.
    let one = painting(&world, 1, ledger.clone());
    assert_eq!(one.request(&asked("order-2")).unwrap_err(), over(1, 0));

    // With that record readable the same reservation is interrupted and counts its
    // amount, 10: the damage raised what it counts for, and lowered nothing.
    let sound = World::new("reserve-frozen-sound");
    allow(&sound.store, 30);
    let sent = Record::Dispatch {
        run: run("9-9"),
        place: n(1),
        by: run("9-9"),
        request: 0,
        bound: amount(5),
        at_ms: 0,
    };
    interrupted_reservation_of_ten(&sound.store, sent.to_bytes());
    let folded = books(&sound.store);
    let whole = folded.reservation(&run("9-9")).unwrap();
    assert_eq!(
        (whole.state(), whole.counted()),
        (State::Interrupted, exactly(10))
    );
}

#[test]
fn a_reset_whose_record_cannot_be_read_allows_nothing() {
    // Stored bytes are not trusted: a reset whose content does not say what its name
    // says is no allowance, whatever its name states.
    let world = World::new("reserve-reset-damaged");
    let reset = Record::Reset {
        epoch: n(1),
        allowance: amount(1_000_000),
        at_ms: 0,
    };
    let other = Record::Reset {
        epoch: n(1),
        allowance: amount(2_000_000),
        at_ms: 0,
    };
    let guard = Record::Epoch { epoch: n(1) };
    let turn = Record::Turn {
        epoch: 0,
        turn: n(1),
        by: TurnBy::Reset,
        run: None,
    };
    plant_bytes(
        &world.store,
        &[
            (reset.name().to_string(), other.to_bytes()),
            (guard.name().to_string(), guard.to_bytes()),
            (turn.name().to_string(), turn.to_bytes()),
        ],
    );
    let engine = painting(&world, 1, Counted::new());
    assert_eq!(engine.request(&asked("order-1")).unwrap_err(), over(1, 0));
    assert_eq!(world.ran(), 0);
}

// --- the confirmation ----------------------------------------------------------------------------

/// A whole reservation of this much that other hands make in epoch 1: the record, a
/// hold and this turn of the epoch.
fn admitted_elsewhere(store: &Store, of: &str, micros: u64, turn: u64) {
    let records = [
        Record::Hold {
            epoch: 1,
            run: run(of),
            reserved: amount(micros),
        },
        Record::Turn {
            epoch: 1,
            turn: n(turn),
            by: TurnBy::Admission,
            run: Some(run(of)),
        },
    ];
    let mut named: Vec<(String, Vec<u8>)> = records
        .iter()
        .map(|record| (record.name().to_string(), record.to_bytes()))
        .collect();
    named.push(reservation_bytes(of, micros));
    plant_bytes(store, &named);
}

/// An admission of a quarter of a dollar, which is all the owner allowed, with these
/// hands at work between its publication and the reading that confirms it. What
/// comes back is the world, the run, and whether the admission stood.
fn confirmed_after(
    label: &str,
    hands: impl Fn(&Store, &Reservation) + Send + Sync + 'static,
) -> (World, RunId, bool) {
    let world = World::new(label);
    assert_eq!(allow(&world.store, 250_000), 1);
    let store = world.store.clone();
    let ledger = Meddled::after_reserve(move |reservation| hands(&store, reservation));
    let engine = painting(&world, 250_000, ledger);
    let answer = engine.request(&asked("order-1"));
    let generation = world.store.snapshot().unwrap();
    let mut runs = generation.runs().keys();
    let made = run(runs.next().unwrap());
    assert_eq!(runs.next(), None);
    let stood = match answer {
        Ok(_) => {
            assert_eq!(world.ran(), 1, "{label}");
            true
        }
        Err(CallError::NotConfirmed { run, action, ended }) => {
            // The run was made, ran nothing, and was ended at once; a repeat of the
            // request is answered with it.
            assert_eq!((&run, action.as_str(), ended), (&made, "paint", true));
            assert_eq!(world.ran(), 0, "{label}");
            let repeat = engine.request(&asked("order-1")).unwrap();
            assert_eq!(repeat.run(), &made, "{label}");
            assert!(matches!(
                repeat,
                Answer::Repeat(ref earlier)
                    if matches!(earlier.state(), RunState::Ended(_))
            ));
            assert_eq!(world.ran(), 0, "{label}");
            false
        }
        Err(other) => panic!("{label}: {other:?}"),
    };
    (world, made, stood)
}

#[test]
fn an_admission_is_held_to_the_epoch_it_fell_under_when_a_reset_came_before_its_reading() {
    // The third condition of a confirmation is of the epoch the reservation's hold
    // names, which after a reset is not the newest.
    //
    // The owner resets to nothing between the publication and the reading. The new
    // epoch allows nothing, and the admission's own epoch is exactly full: it stands,
    // and its run goes on drawing on its own reservation.
    let (world, made, stood) = confirmed_after("confirm-reset", |store, _| {
        reset_after(store, 0, 2);
    });
    assert!(stood);
    assert_eq!(books(&world.store).epoch(), 2);
    // Its run has ended since, having sent nothing: its end closed the reservation
    // and released all of it, in the epoch it fell under.
    assert_eq!(balance(&world.store, 1), (250_000, exactly(0), 250_000));
    assert_eq!(balance(&world.store, 2), (0, exactly(0), 0));
    assert_eq!(
        books(&world.store).reservation(&made).unwrap().epoch(),
        Some(1)
    );

    // Its own epoch is over its allowance, by a reservation other hands made there,
    // and the newest epoch has a dollar left: it does not stand.
    let (world, made, stood) = confirmed_after("confirm-reset-over", |store, _| {
        admitted_elsewhere(store, "9-9", 1, 2);
        reset_after(store, 1_000_000, 3);
    });
    assert!(!stood);
    // The run was ended at once, and its end closed the reservation (task S11):
    // nothing was sent under it, so all of it is released, in the epoch it fell
    // under, and what other hands reserved there is all that epoch counts.
    assert_eq!(balance(&world.store, 1), (250_000, exactly(1), 249_999));
    assert_eq!(balance(&world.store, 2), (1_000_000, exactly(0), 1_000_000));
    let folded = books(&world.store);
    let own = folded.reservation(&made).unwrap();
    assert_eq!(
        (own.state(), own.counts(), own.epoch()),
        (State::Released, Sum::ZERO, Some(1))
    );
}

#[test]
fn an_admission_does_not_stand_where_its_epoch_or_the_newest_needs_its_owner() {
    // The newest epoch, which a reset opened before the reading, has a turn that no
    // publication took.
    let (world, _, stood) = confirmed_after("confirm-newest-owner", |store, _| {
        reset_after(store, 1_000_000, 2);
        plant(
            store,
            &[Record::Turn {
                epoch: 2,
                turn: n(7),
                by: TurnBy::Settlement,
                run: None,
            }],
        );
    });
    assert!(!stood);
    let folded = books(&world.store);
    assert!(!folded.needs_owner(1) && folded.needs_owner(2));

    // Its own epoch has one, and the newest is sound.
    let (world, _, stood) = confirmed_after("confirm-own-owner", |store, _| {
        plant(
            store,
            &[Record::Turn {
                epoch: 1,
                turn: n(9),
                by: TurnBy::Settlement,
                run: None,
            }],
        );
        reset_after(store, 1_000_000, 2);
    });
    assert!(!stood);
    let folded = books(&world.store);
    assert!(folded.needs_owner(1) && !folded.needs_owner(2));
}

#[test]
fn an_admission_stands_only_as_the_record_and_one_hold_and_within_its_allowance() {
    // Nothing between the publication and the reading: it stands.
    let (_, _, stood) = confirmed_after("confirm-sound", |_, _| {});
    assert!(stood);

    // A name under its reservation that the ledger did not make there: a place.
    let (_, _, stood) = confirmed_after("confirm-place", |store, reservation| {
        plant(
            store,
            &[Record::Place {
                run: reservation.run().clone(),
                place: n(1),
                by: whizbang_core::spend::PlaceBy::Dispatch,
            }],
        );
    });
    assert!(!stood);

    // Its epoch over its allowance, by another admission that other hands made.
    let (world, _, stood) = confirmed_after("confirm-over", |store, _| {
        admitted_elsewhere(store, "9-9", 1, 2);
    });
    assert!(!stood);
    // Its own reservation was closed with the end that the answer made, and all of
    // it released: what other hands reserved is what is counted.
    assert_eq!(balance(&world.store, 1), (250_000, exactly(1), 249_999));

    // A frozen reservation beside it whose one hold states its amount, and that the
    // allowance covers, stops nothing: the admission stands.
    let world = World::new("confirm-beside-frozen");
    allow(&world.store, 250_010);
    let store = world.store.clone();
    let ledger = Meddled::after_reserve(move |_| {
        plant(
            &store,
            &[
                Record::Hold {
                    epoch: 1,
                    run: run("9-9"),
                    reserved: amount(10),
                },
                Record::Turn {
                    epoch: 1,
                    turn: n(2),
                    by: TurnBy::Admission,
                    run: Some(run("9-9")),
                },
            ],
        );
    });
    ran(painting(&world, 250_000, ledger).request(&asked("order-1")));
    assert_eq!(world.ran(), 1);
    assert_eq!(
        books(&world.store)
            .reservation(&run("9-9"))
            .unwrap()
            .state(),
        State::Frozen
    );
}

#[test]
fn a_call_of_nothing_is_admitted_in_a_store_in_debt_and_the_ledger_is_not_asked() {
    // A reservation other hands made puts the epoch over its allowance. A call of
    // nothing adds nothing to what is counted, and is not held to that debt: the
    // ledger is asked nothing of it. (Restated after the review of task W1,
    // 2026-11-15. It was `a_call_of_nothing_is_confirmed_in_a_store_in_debt`: the
    // ledger was asked, and confirmed it. What the fold says of a hold of nothing
    // in an epoch in debt is still held, in `spend::fold::`.)
    let world = World::new("confirm-nothing");
    let ledger = Counted::new();
    allow(&world.store, 10);
    admitted_elsewhere(&world.store, "9-9", 11, 1);
    assert_eq!(balance(&world.store, 1), (10, exactly(11), 0));
    let before = spend_names(&world.store);
    ran(painting(&world, 0, ledger.clone()).request(&asked("order-1")));
    assert_eq!(world.ran(), 1);
    assert_eq!(
        (ledger.asked(), ledger.confirmed(), ledger.settled()),
        (0, 0, 0)
    );
    assert_eq!(spend_names(&world.store), before);
    // A call over nothing is refused there.
    let paid = painting(&world, 1, ledger.clone());
    assert_eq!(paid.request(&asked("order-2")).unwrap_err(), over(1, 0));
}

// --- two executors over one store ----------------------------------------------------------------

/// The real ledger, stopped at a barrier each time it has judged and staged an
/// admission: after the read a wrong ledger would decide from, and before the
/// publication. It counts how many times it answered yes.
struct Held {
    inner: Arc<Counted>,
    barriers: Barriers,
    yes: AtomicU64,
}

impl Held {
    fn over(inner: &Arc<Counted>, barriers: &Barriers) -> Arc<Held> {
        Arc::new(Held {
            inner: inner.clone(),
            barriers: barriers.clone(),
            yes: AtomicU64::new(0),
        })
    }
}

impl Ledger for Held {
    fn reserve(
        &self,
        reservation: &Reservation,
        step: &mut Step<'_, '_>,
    ) -> Result<MadeName, Declined> {
        let answer = self.inner.reserve(reservation, step);
        if answer.is_ok() {
            self.yes.fetch_add(1, Ordering::SeqCst);
        }
        self.barriers.stop_at("judged");
        answer
    }

    fn confirm(&self, reservation: &Reservation, reading: &Reread<'_>) -> Result<bool, NotJudged> {
        self.inner.confirm(reservation, reading)
    }

    fn settle(&self, closing: &Closing<'_>, step: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
        self.inner.settle(closing, step)
    }
}

#[test]
fn two_executors_each_told_yes_for_the_last_of_the_allowance_end_with_one_admitted() {
    // Rule 1. Two executors over one store, and forty cents allowed. Each asks the
    // ledger for a quarter on books in which nothing is reserved, each is told yes,
    // and each is held there, with its hold and its turn staged. They are let go
    // together. Both chose the epoch's next turn, which is one name: the store
    // refuses the later publication whole, that executor asks again on the books as
    // they are then, and is declined for what is left.
    let world = World::new("reserve-race");
    assert_eq!(allow(&world.store, 400_000), 1);
    let ledger = Counted::left_open();
    let barriers = Barriers::new();
    let held = Held::over(&ledger, &barriers);
    let engines: Vec<Engine> = (0..2)
        .map(|_| painting(&world, 250_000, held.clone()))
        .collect();
    let answers: Vec<_> = whizbang_testkit::threads::scope(|scope| {
        // Two show a race.
        let threads: Vec<_> = engines
            .iter()
            .zip(["order-1", "order-2"])
            .map(|(engine, id)| scope.spawn(move || engine.request(&asked(id))))
            .collect();
        // Both have judged, and both were told yes, before either publishes.
        barriers.arrived("judged", 2);
        assert_eq!((ledger.asked(), held.yes.load(Ordering::SeqCst)), (2, 2));
        assert_eq!(reserved(&world.store), []);
        barriers.open("judged");
        threads.into_iter().map(|t| t.join().unwrap()).collect()
    });
    let lost = answers.iter().position(|answer| answer.is_err()).unwrap();
    let won = ran(answers
        .into_iter()
        .enumerate()
        .fold(None, |made, (at, answer)| match (at == lost, answer) {
            (true, Err(refused)) => {
                assert_eq!(refused, over(250_000, 150_000));
                made
            }
            (false, answer) => Some(answer),
            (true, Ok(answer)) => panic!("both were admitted: {answer:?}"),
        })
        .unwrap());
    // Asked three times: the loser again, once, on the books with the winner's
    // names. One reservation, one hold, and the epoch's first turn and no second.
    assert_eq!((ledger.asked(), held.yes.load(Ordering::SeqCst)), (3, 2));
    assert_eq!((world.ran(), ledger.confirmed()), (1, 1));
    let one = won.run();
    assert_eq!(
        spend_names(&world.store),
        [
            "spend/epoch/1".to_owned(),
            format!("spend/held/1/{one}/250000"),
            format!("spend/reservations/{one}"),
            "spend/reset/1/400000".to_owned(),
            "spend/turn/0/1".to_owned(),
            "spend/turn/1/1".to_owned(),
        ]
    );
    let folded = books(&world.store);
    assert_eq!(folded.turn_epochs(one), [1]);
    assert_eq!((folded.next_turn(1), folded.due(1)), (2, 1));
    assert_eq!(
        balance(&world.store, 1),
        (400_000, exactly(250_000), 150_000)
    );
    // The refused request left no name and no run, and its id is free: under an
    // allowance that covers it, it is admitted, and takes the next turn.
    let free = ["order-1", "order-2"][lost];
    let generation = world.store.snapshot().unwrap();
    assert!(generation.entries().keys().all(|name| !name.contains(free)));
    assert_eq!(generation.runs().len(), 1);
    let small = painting(&world, 150_000, ledger.clone());
    ran(small.request(&asked(free)));
    assert!(spend_names(&world.store).contains(&"spend/turn/1/2".to_owned()));
    assert_eq!(balance(&world.store, 1), (400_000, exactly(400_000), 0));
}

#[test]
fn two_executors_that_judge_together_under_an_allowance_that_covers_both_are_both_admitted() {
    // The same race where nothing has to be refused: the later publication is
    // refused for the turn, asked again, and admitted at the next. Each of the two
    // reservations has its own turn, and the epoch's turns are those its
    // publications took.
    let world = World::new("reserve-race-both");
    allow(&world.store, 500_000);
    let ledger = Counted::left_open();
    let barriers = Barriers::new();
    let held = Held::over(&ledger, &barriers);
    let engines: Vec<Engine> = (0..2)
        .map(|_| painting(&world, 250_000, held.clone()))
        .collect();
    whizbang_testkit::threads::scope(|scope| {
        let threads: Vec<_> = engines
            .iter()
            .zip(["order-1", "order-2"])
            .map(|(engine, id)| scope.spawn(move || ran(engine.request(&asked(id)))))
            .collect();
        barriers.arrived("judged", 2);
        barriers.open("judged");
        for thread in threads {
            thread.join().unwrap();
        }
    });
    assert_eq!((ledger.asked(), ledger.confirmed(), world.ran()), (3, 2, 2));
    let folded = books(&world.store);
    assert_eq!(folded.turns()[&1], [1, 2].into());
    assert_eq!((folded.next_turn(1), folded.due(1)), (3, 2));
    assert!(!folded.needs_owner(1));
    assert_eq!(balance(&world.store, 1), (500_000, exactly(500_000), 0));
}

#[test]
fn an_admission_that_judged_before_the_owners_reset_is_asked_again_under_the_new_epoch() {
    // A reset takes the next turn of the epoch it closes, which is the turn an
    // admission that judged before it chose. The admission's publication is refused,
    // and it is judged again: on the new epoch, whose allowance does not cover it.
    let world = World::new("reserve-race-reset");
    allow(&world.store, 1_000_000);
    let ledger = Counted::new();
    let barriers = Barriers::new();
    let held = Held::over(&ledger, &barriers);
    let engine = painting(&world, 250_000, held.clone());
    let refused = whizbang_testkit::threads::scope(|scope| {
        let thread = scope.spawn(|| engine.request(&asked("order-1")));
        barriers.arrived("judged", 1);
        assert_eq!(allow(&world.store, 100_000), 2);
        barriers.open("judged");
        thread.join().unwrap().unwrap_err()
    });
    assert_eq!(refused, over(250_000, 100_000));
    assert_eq!((ledger.asked(), held.yes.load(Ordering::SeqCst)), (2, 1));
    assert_eq!((world.ran(), reserved(&world.store).len()), (0, 0));
    // One that the new epoch covers falls under it, with that epoch's first turn.
    let small = painting(&world, 100_000, ledger.clone());
    let finished = ran(small.request(&asked("order-1")));
    let names = spend_names(&world.store);
    assert!(names.contains(&format!("spend/held/2/{}/100000", finished.run())));
    assert!(names.contains(&"spend/turn/2/1".to_owned()));
    let folded = books(&world.store);
    assert_eq!(folded.turn_epochs(finished.run()), [2]);
    assert!(!folded.needs_owner(1) && !folded.needs_owner(2));
}

// --- a crash at each step of an admission --------------------------------------------------------

/// How many tests the sweep below is cut into.
const PARTS: usize = 6;

/// What the owner allowed in the store the sweep admits in, and what its call may
/// cost.
const ALLOWED: u64 = 1_000_000;
const PRICE: u64 = 250_000;

/// Copies a folder and all under it: a store that nothing has open.
fn copy_all(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if source.is_dir() {
            copy_all(&source, &target);
        } else {
            std::fs::copy(&source, &target).unwrap();
        }
    }
}

/// Power is lost before each call that changes the disk during an admission through
/// the real ledger, from the request to the moment the action starts. Each is tried
/// with every way a disk can lose what was not yet durable: every name undone, every
/// name kept, and every choice there is of which directories, and of which names,
/// lose theirs. Whatever is left, the next writer finds the run, the id's record, the
/// reservation, the ledger's hold and its turn there together, or none of them
/// (rule 2); and the same request is then admitted and runs once, or is answered
/// with the run that is there.
///
/// **It is cut by the ways, not by the steps.** Half the crashes there are belong to
/// a few of the steps, where nine names are undecided at once. So each part goes
/// through every step, and takes every `PARTS`th of the ways a crash there can end
/// from `part`; between them the parts try every way of every step. How many ways a
/// step has is learnt from one crash at it, which every part makes.
///
/// The call may cost a quarter of a dollar, under an allowance of a dollar that the
/// owner's reset made durable before any of it. That store is made once and copied
/// for each crash, so that no reset is published before each of them.
fn admissions_crashed(part: usize) {
    // The store every crash starts from: the owner allowed a dollar, and nothing has
    // it open.
    let template = Scratch::new("reserve-crash-template");
    {
        let fs = Arc::new(ShimFs::new());
        let store = Store::open(&template.path().join("store"), fs).unwrap();
        assert_eq!(allow(&store, ALLOWED), 1);
    }
    let reset = [
        "spend/epoch/1".to_owned(),
        format!("spend/reset/1/{ALLOWED}"),
        "spend/turn/0/1".to_owned(),
    ];
    let scratch = |fs: ShimFs| {
        let dir = Scratch::new("reserve-crash");
        copy_all(&template.path().join("store"), &dir.path().join("store"));
        let fs = Arc::new(fs);
        let store = Arc::new(Store::open(&dir.path().join("store"), fs.clone()).unwrap());
        (dir, fs, store)
    };
    let over = |store: Arc<Store>, action: Arc<dyn Action>, ledger: &Arc<Counted>| {
        let mut registry = Registry::new();
        registry.register(action).unwrap();
        let backends = Arc::new(Bounds(Estimate::of_micros(PRICE)));
        Engine::over(registry, backends, store, Digest::of_bytes(b""))
            .with_clock(Hand::at(7000))
            .with_ledger(ledger.clone())
    };
    let admitting = |store: Arc<Store>, runs: &Arc<AtomicU64>, ledger: &Arc<Counted>| {
        let paint = counting(runs, "paint", &[Capability::Fill], |_, _| {
            Ok(Produced::nothing())
        });
        over(store, paint, ledger)
    };
    // The calls that change the disk during one admission are found by running one.
    let (opened, steps) = {
        let (_dir, fs, store) = scratch(ShimFs::new());
        let opened = fs.events().len();
        let started = Arc::new(AtomicU64::new(0));
        let noting = {
            let (fs, started) = (fs.clone(), started.clone());
            let runs = Arc::new(AtomicU64::new(0));
            counting(&runs, "paint", &[Capability::Fill], move |_, _| {
                started.store(fs.events().len() as u64, Ordering::SeqCst);
                Ok(Produced::nothing())
            })
        };
        ran(over(store, noting, &Counted::new()).request(&asked("order-17")));
        (opened, started.load(Ordering::SeqCst) as usize - opened)
    };
    assert!(steps > 8, "{steps}");

    let mut outcomes = std::collections::BTreeSet::new();
    let (mut most_dirs, mut most_names, mut crashes) = (0, 0, 0);
    for step in 0..=steps {
        let mut ways = vec![Lost::Unsynced, Lost::ContentOnly, Lost::Subset(0)];
        let mut next = 0;
        while next < ways.len() {
            let (index, lost) = (next, ways[next]);
            next += 1;
            // The third way is the one the rest are learnt from: every part makes it.
            if index % PARTS != part && index != 2 {
                continue;
            }
            crashes += 1;
            let at = format!("step {step}, {lost:?}");
            let (dir, fs, store) = scratch(ShimFs::new().crash_before(opened + step, lost));
            let (runs, ledger) = (Arc::new(AtomicU64::new(0)), Counted::new());
            let engine = admitting(store, &runs, &ledger);
            let answer = engine.request(&asked("order-17"));
            assert!(
                matches!(answer, Err(CallError::Store(_))),
                "{at}: {answer:?}"
            );
            if index == 2 {
                let (dirs, names) = (fs.crash_choices(), fs.crash_names());
                assert!(dirs <= 10 && names <= 10, "{at}: {dirs} {names}");
                ways.extend((1..1u64 << dirs).map(Lost::Subset));
                ways.extend((0..1u64 << names).map(Lost::Names));
                most_dirs = most_dirs.max(dirs);
                most_names = most_names.max(names);
            }
            // The action started only where the admission was whole and durable, and
            // was confirmed: the last call of the step clears its staging away.
            let ran_here = runs.load(Ordering::SeqCst);
            assert!(
                ran_here == 0 || (ran_here == 1 && step >= steps - 1),
                "{at}"
            );
            drop(engine);

            // The next writer recovers the store, and reads what survived: every
            // name outside runs with its content, and every run with its names.
            let root = dir.path().join("store");
            let store = Arc::new(Store::open(&root, Arc::new(ShimFs::new())).unwrap());
            let generation = store.snapshot().unwrap();
            let read = |name: &str| {
                let entry = generation.get(name).unwrap();
                assert_eq!(entry.kind, whizbang_core::store::Kind::Diagnostic, "{at}");
                store.read(entry).unwrap()
            };
            let names: Vec<&str> = generation.entries().keys().map(String::as_str).collect();
            for name in &names {
                read(name);
            }
            let made: Vec<&String> = generation.runs().keys().collect();
            let folded = books(&store);
            match made.as_slice() {
                [] => {
                    // Nothing: no id's record, no reservation, and neither the hold
                    // nor its turn. No action had started.
                    assert_eq!(names, reset, "{at}");
                    assert_eq!(reserved(&store), [], "{at}");
                    assert_eq!(ran_here, 0, "{at}: an action ran unadmitted");
                    assert_eq!(folded.next_turn(1), 1, "{at}");
                    assert_eq!(balance(&store, 1), (ALLOWED, exactly(0), ALLOWED), "{at}");
                    // The same request is admitted, and runs, once.
                    let again = admitting(store.clone(), &runs, &ledger);
                    ran(again.request(&asked("order-17")));
                    assert_eq!(runs.load(Ordering::SeqCst), 1, "{at}");
                    assert_eq!(reserved(&store).len(), 1, "{at}");
                    assert!(spend_names(&store).contains(&"spend/turn/1/1".to_owned()));
                    outcomes.insert("nothing");
                }
                [made_id] => {
                    // All of it: the id's record, the ledger's hold and its turn,
                    // and the reservation, each readable and saying what its name
                    // says; and the run with its admission.
                    assert_eq!(
                        names,
                        [
                            "requests/order-17/admitted",
                            reset[0].as_str(),
                            format!("spend/held/1/{made_id}/{PRICE}").as_str(),
                            format!("spend/reservations/{made_id}").as_str(),
                            reset[1].as_str(),
                            reset[2].as_str(),
                            "spend/turn/1/1",
                        ],
                        "{at}"
                    );
                    let of = run(made_id);
                    // The id's record names the run and says what was reserved; the
                    // run holds the same admission and nothing else.
                    let admission = Admission::from_bytes(&read(names[0])).unwrap();
                    assert_eq!(admission.run(), &of, "{at}");
                    assert_eq!(admission.reserved_micros(), Some(PRICE), "{at}");
                    let own = generation.run(made_id).unwrap();
                    let held: Vec<&String> = own.entries().keys().collect();
                    assert_eq!(held, [ADMITTED_NAME], "{at}");
                    let inside = own.get(ADMITTED_NAME).unwrap();
                    assert_eq!(inside.digest, generation.get(names[0]).unwrap().digest);
                    // The reservation is of that run and that request, for the
                    // amount the admission says, and holds the admission's digest.
                    let stored = Reservation::from_bytes(&read(names[3])).unwrap();
                    assert_eq!(reserved(&store), std::slice::from_ref(&stored), "{at}");
                    assert_eq!(stored.run(), &of, "{at}");
                    assert_eq!(stored.request(), Some(&id("order-17")), "{at}");
                    assert_eq!((stored.micros(), stored.start_ms()), (PRICE, 7000));
                    assert_eq!(stored.admission(), inside.digest, "{at}");
                    // The ledger's names: the hold and its turn, both there, each
                    // the record its name says; the reservation is a set the ledger
                    // could have written, open, and the record and one hold alone.
                    let hold = Record::Hold {
                        epoch: 1,
                        run: of.clone(),
                        reserved: amount(PRICE),
                    };
                    let turn = Record::Turn {
                        epoch: 1,
                        turn: n(1),
                        by: TurnBy::Admission,
                        run: Some(of.clone()),
                    };
                    assert_eq!(read(names[2]), hold.to_bytes(), "{at}");
                    assert_eq!(read(names[6]), turn.to_bytes(), "{at}");
                    let standing = folded.reservation(&of).unwrap();
                    assert_eq!(standing.state(), State::Open, "{at}");
                    assert!(standing.names().reachable(), "{at}");
                    assert!(!standing.names().damaged(), "{at}");
                    assert_eq!(standing.names().count(), 2, "{at}");
                    assert!(!folded.needs_owner(1), "{at}");
                    assert_eq!((folded.next_turn(1), folded.due(1)), (2, 1), "{at}");
                    assert!(folded.stands(&of, amount(PRICE)), "{at}");
                    let left = ALLOWED - PRICE;
                    assert_eq!(balance(&store, 1), (ALLOWED, exactly(PRICE), left), "{at}");
                    // The same request is answered with that run, which did not end
                    // and is not this process's: nothing runs, the ledger is not
                    // asked, and nothing more is reserved.
                    let (before, asked_before) = (runs.load(Ordering::SeqCst), ledger.asked());
                    let again = admitting(store.clone(), &runs, &ledger);
                    let earlier = match again.request(&asked("order-17")) {
                        Ok(Answer::Repeat(earlier)) => earlier,
                        other => panic!("{at}: not answered as a repeat: {other:?}"),
                    };
                    assert_eq!(earlier.run(), &of, "{at}");
                    assert_eq!(earlier.state(), &RunState::NotEnded { here: false });
                    assert_eq!(runs.load(Ordering::SeqCst), before, "{at}");
                    assert_eq!(ledger.asked(), asked_before, "{at}");
                    assert_eq!(reserved(&store), [stored], "{at}");
                    assert_eq!(spend_names(&store).len(), 6, "{at}");
                    outcomes.insert("all of it");
                }
                more => panic!("{at}: {} runs", more.len()),
            }
        }
    }
    // Before the last step there is nothing, and at it there is all of it.
    let both: Vec<&str> = outcomes.into_iter().collect();
    assert_eq!(both, ["all of it", "nothing"]);
    // The masks were not a formality: some step left several directories and nine
    // names undecided, and each choice was a crash, in one part or another.
    assert!(
        most_dirs >= 2 && most_names >= 9,
        "{most_dirs} {most_names}"
    );
    assert!(crashes > 350, "{crashes}");
}

#[test]
fn a_crash_at_any_event_of_an_admission_through_the_real_ledger_leaves_all_of_it_or_none_1_of_6() {
    admissions_crashed(0);
}

#[test]
fn a_crash_at_any_event_of_an_admission_through_the_real_ledger_leaves_all_of_it_or_none_2_of_6() {
    admissions_crashed(1);
}

#[test]
fn a_crash_at_any_event_of_an_admission_through_the_real_ledger_leaves_all_of_it_or_none_3_of_6() {
    admissions_crashed(2);
}

#[test]
fn a_crash_at_any_event_of_an_admission_through_the_real_ledger_leaves_all_of_it_or_none_4_of_6() {
    admissions_crashed(3);
}

#[test]
fn a_crash_at_any_event_of_an_admission_through_the_real_ledger_leaves_all_of_it_or_none_5_of_6() {
    admissions_crashed(4);
}

#[test]
fn a_crash_at_any_event_of_an_admission_through_the_real_ledger_leaves_all_of_it_or_none_6_of_6() {
    admissions_crashed(5);
}
