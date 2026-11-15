//! The ledger's interface (ADR 0016, "What the second part builds"; plan 1B, task I2):
//! what `reserve` hands back, the confirmation of an admission, the closing of a
//! reservation in the publication that ends its run, and a dispatch's refusals. The
//! ledger here is the test kit's fake, which keeps the ledger's names in the store and
//! judges by the model's fold.

use std::io;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::capability::Stage;
use whizbang_core::contract::record::{self, Status};
use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Argument, Backends, Call, CallError, Cancel, Capability,
    Clock, Closing, Context, Declined, DispatchRefused, Engine, Estimate, Executor, Failure, Kind,
    Ledger, MadeName, ModelIdentity, NOT_CONFIRMED_KIND, NotJudged, Outcome, PreparedCall,
    Produced, Registry, Request, RequestId, Requirement, Reread, Reservation, RunId, RunState,
    Settled, Step, Unavailable, Value, ValueType,
};
use whizbang_core::identity::Digest;
use whizbang_core::spend::{Amount, ClosedBy, PlaceBy, Reading, Record, Sum, TurnBy};
use whizbang_core::store::fs::{Fs, Held, LockFile, NewFile, OpenFile};
use whizbang_core::store::{self, Store};
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

/// One model, whose bound the test sets: what a call of `spend` reserves.
struct Priced(u64);

/// What a call of `inner`, which needs a segmenter, takes of its caller's allowance.
const INNER: u64 = 5;

impl Backends for Priced {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Ok(ModelIdentity::new("fake", "fake-filler", "1"))
    }

    fn bound(&self, capability: Capability, _: &ModelIdentity) -> Estimate {
        match capability {
            Capability::Fill => Estimate::of_micros(self.0),
            _ => Estimate::of_micros(INNER),
        }
    }
}

type Body = Box<dyn Fn(&Context) -> Result<Produced, Failure> + Send + Sync>;

/// `spend`, which needs a filler, and `inner`, which needs a segmenter: each does
/// what the test says, and their runs are counted together.
struct Spend {
    descriptor: ActionDescriptor,
    runs: Arc<AtomicU64>,
    body: Arc<Body>,
}

impl Action for Spend {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        (self.body)(context)
    }
}

struct World {
    _dir: Scratch,
    store: Arc<Store>,
    ledger: Arc<FakeLedger>,
    runs: Arc<AtomicU64>,
}

impl World {
    fn new(allowance: u64) -> World {
        let dir = Scratch::new("ledger");
        let store =
            Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
        World {
            _dir: dir,
            store,
            ledger: Arc::new(FakeLedger::with_allowance(allowance)),
            runs: Arc::new(AtomicU64::new(0)),
        }
    }

    /// An executor whose one action reserves `price` and runs `body`, with this
    /// ledger.
    fn engine_with(
        &self,
        price: u64,
        ledger: Arc<dyn Ledger>,
        body: impl Fn(&Context) -> Result<Produced, Failure> + Send + Sync + 'static,
    ) -> Engine {
        let body: Arc<Body> = Arc::new(Box::new(body));
        let mut registry = Registry::new();
        for (name, needs) in [("spend", Capability::Fill), ("inner", Capability::Segment)] {
            let descriptor =
                ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.")
                    .argument(Argument::required("text", "A text.", ValueType::Text))
                    .needs(Requirement::new(needs))
                    .build()
                    .unwrap();
            registry
                .register(Arc::new(Spend {
                    descriptor,
                    runs: self.runs.clone(),
                    body: body.clone(),
                }))
                .unwrap();
        }
        let backends = Arc::new(Priced(price));
        Engine::over(
            registry,
            backends,
            self.store.clone(),
            Digest::of_bytes(b""),
        )
        .with_clock(Arc::new(Stopped(7000)))
        .with_ledger(ledger)
    }

    fn engine(&self, price: u64) -> Engine {
        self.engine_with(price, self.ledger.clone(), |_| Ok(Produced::nothing()))
    }

    fn ran(&self) -> u64 {
        self.runs.load(Ordering::SeqCst)
    }

    /// Every name the store holds under `spend/`.
    fn names(&self) -> Vec<String> {
        let generation = self.store.snapshot().unwrap();
        let spend = generation.entries().keys();
        spend
            .filter(|name| name.starts_with("spend/"))
            .cloned()
            .collect()
    }

    /// Other hands make these records, in one publication.
    fn plant(&self, records: &[Record]) {
        plant(&self.store, records);
    }

    /// The one run the store holds.
    fn only_run(&self) -> RunId {
        let generation = self.store.snapshot().unwrap();
        let mut runs = generation.runs().keys();
        let run = RunId::parse(runs.next().unwrap()).unwrap();
        assert_eq!(runs.next(), None);
        run
    }
}

fn plant(store: &Store, records: &[Record]) {
    let mut publication = store.begin().unwrap();
    for record in records {
        publication
            .create(
                &record.name().to_string(),
                store::Kind::Diagnostic,
                &record.to_bytes(),
            )
            .unwrap();
    }
    publication.publish().unwrap();
}

fn call() -> Call {
    Call::new("spend").argument("text", Value::text("hello"))
}

fn asked(request: &str) -> Request {
    Request::new(call()).with_id(RequestId::parse(request).unwrap())
}

fn run(text: &str) -> RunId {
    RunId::parse(text).unwrap()
}

fn n(number: u64) -> NonZeroU64 {
    NonZeroU64::new(number).unwrap()
}

fn amount(micros: u64) -> Amount {
    Amount::new(micros).unwrap()
}

/// The hold and the turn of a reservation other hands made whole but for its record:
/// a frozen reservation whose one hold states its amount.
fn hold_and_turn(of: &str, micros: u64, turn: u64) -> Vec<Record> {
    vec![
        Record::Hold {
            epoch: 0,
            run: run(of),
            reserved: amount(micros),
        },
        Record::Turn {
            epoch: 0,
            turn: n(turn),
            by: TurnBy::Admission,
            run: Some(run(of)),
        },
    ]
}

fn ran(answer: Result<Answer, CallError>) -> whizbang_core::contract::Finished {
    match answer {
        Ok(Answer::Ran(finished)) => finished,
        other => panic!("the request did not run: {other:?}"),
    }
}

type AfterReserve = Box<dyn Fn(&Reservation) + Send + Sync>;
type InSettle = Box<dyn Fn(&mut Step<'_, '_>) -> Result<Settled, NotJudged> + Send + Sync>;

/// A ledger that is the fake, with the test's hands in two places: after the fake
/// judged and staged an admission, which is before that admission is published and
/// so before the reading that confirms it; and in the fake's place at a run's end.
struct Meddled {
    inner: Arc<FakeLedger>,
    after_reserve: Option<AfterReserve>,
    settle: Option<InSettle>,
}

impl Meddled {
    fn after_reserve(
        world: &World,
        then: impl Fn(&Reservation) + Send + Sync + 'static,
    ) -> Arc<Meddled> {
        Arc::new(Meddled {
            inner: world.ledger.clone(),
            after_reserve: Some(Box::new(then)),
            settle: None,
        })
    }

    fn settling(
        world: &World,
        settle: impl Fn(&mut Step<'_, '_>) -> Result<Settled, NotJudged> + Send + Sync + 'static,
    ) -> Arc<Meddled> {
        Arc::new(Meddled {
            inner: world.ledger.clone(),
            after_reserve: None,
            settle: Some(Box::new(settle)),
        })
    }
}

impl Ledger for Meddled {
    fn reserve(
        &self,
        reservation: &Reservation,
        step: &mut Step<'_, '_>,
    ) -> Result<MadeName, Declined> {
        let made = self.inner.reserve(reservation, step)?;
        if let Some(then) = &self.after_reserve {
            then(reservation);
        }
        Ok(made)
    }

    fn confirm(&self, reservation: &Reservation, reading: &Reread<'_>) -> Result<bool, NotJudged> {
        self.inner.confirm(reservation, reading)
    }

    fn settle(&self, closing: &Closing<'_>, step: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
        match &self.settle {
            Some(settle) => settle(step),
            None => self.inner.settle(closing, step),
        }
    }
}

// --- reserve -----------------------------------------------------------------------------------

#[test]
fn an_admission_through_the_ledger_makes_its_hold_and_the_next_turn_with_the_reservation() {
    let world = World::new(1000);
    let finished = ran(world.engine(250).request(&asked("order-1")));
    let id = finished.run();
    assert_eq!(world.ran(), 1);
    // The reservation, its hold and a turn were published with the admission; the
    // run's end closed it, at its first place and the epoch's second turn, and
    // released all of it: nothing was dispatched under it.
    assert_eq!(
        world.names(),
        [
            format!("spend/closed/{id}/released"),
            format!("spend/held/0/{id}/250"),
            format!("spend/place/{id}/1"),
            format!("spend/reservations/{id}"),
            "spend/turn/0/1".to_owned(),
            "spend/turn/0/2".to_owned(),
        ]
    );
    assert_eq!(
        (
            world.ledger.asked(),
            world.ledger.confirmed(),
            world.ledger.settled()
        ),
        (1, 1, 1)
    );
    // Every record says what its name says, and the books fold to nothing counted.
    let generation = world.store.snapshot().unwrap();
    let reading = Reading::of(&generation, |entry| world.store.read(entry));
    let names = &reading.reservations[id];
    assert!(names.reachable() && names.released() && !names.damaged());
    assert!(reading.epochs.iter().all(|(_, hurt)| !hurt));
    let closing = generation
        .get(&format!("spend/closed/{id}/released"))
        .unwrap();
    assert_eq!(
        Record::from_bytes(&world.store.read(closing).unwrap()).unwrap(),
        Record::Closing {
            run: id.clone(),
            released: true,
            by: ClosedBy::End,
            charged: Sum::ZERO,
            unresolved: Sum::ZERO,
            at_ms: 7000,
        }
    );
    let books = world.ledger.books_of(&world.store).unwrap();
    assert_eq!(books.folded.balance().counted, 0);
    // A second call takes the next turn.
    ran(world.engine(250).request(&asked("order-2")));
    assert!(world.names().contains(&"spend/turn/0/4".to_owned()));
}

#[test]
fn a_ledger_that_hands_back_a_name_it_did_not_make_in_this_attempt_is_refused() {
    // What `reserve` returns can only be had from `Step::make`, so a ledger that
    // makes no name does not compile (the example on `Ledger`). One that keeps a name
    // from an earlier attempt and hands it back from a later one, in which it made
    // none, is refused by the executor: nothing is published.
    let world = World::new(1000);
    let kept: Arc<Mutex<Option<MadeName>>> = Arc::default();
    struct Keeping(Arc<Mutex<Option<MadeName>>>);
    impl Ledger for Keeping {
        fn reserve(&self, _: &Reservation, step: &mut Step<'_, '_>) -> Result<MadeName, Declined> {
            let mut kept = self.0.lock().unwrap();
            if let Some(made) = kept.take() {
                return Ok(made);
            }
            let turn = |turn: u64| Record::Turn {
                epoch: 0,
                turn: n(turn),
                by: TurnBy::Admission,
                run: None,
            };
            *kept = Some(step.record(&turn(1))?);
            Ok(step.record(&turn(2))?)
        }

        fn confirm(&self, _: &Reservation, _: &Reread<'_>) -> Result<bool, NotJudged> {
            Ok(true)
        }

        fn settle(&self, _: &Closing<'_>, _: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
            Ok(Settled::Nothing)
        }
    }
    let engine = world.engine_with(250, Arc::new(Keeping(kept)), |_| Ok(Produced::nothing()));
    ran(engine.request(&asked("order-1")));
    let before = world.store.snapshot().unwrap().number();
    let refused = engine.request(&asked("order-2")).unwrap_err();
    let CallError::Ledger { why } = &refused else {
        panic!("{refused:?}");
    };
    assert!(why.contains("did not make in this attempt"), "{why}");
    assert_eq!(world.store.snapshot().unwrap().number(), before);
    assert_eq!(world.ran(), 1);
}

// --- settle ------------------------------------------------------------------------------------

#[test]
fn a_settle_that_fails_leaves_the_run_admitted_and_its_caller_told() {
    let world = World::new(1000);
    let failing = Meddled::settling(&world, |_| {
        Err(NotJudged::saying("its books do not balance"))
    });
    let engine = world.engine_with(250, failing, |_| Ok(Produced::nothing()));
    let failed = engine.request(&asked("order-1")).unwrap_err();
    assert_eq!(
        failed.to_string(),
        "the ledger failed: its books do not balance"
    );
    // The action ran; its end was not published: the run is admitted and has no
    // record, its reservation is open and counts in full, and the id has no ending.
    assert_eq!(world.ran(), 1);
    let id = world.only_run();
    let generation = world.store.snapshot().unwrap();
    assert!(
        generation
            .run(id.as_str())
            .unwrap()
            .entries()
            .get(record::RECORD_NAME)
            .is_none()
    );
    assert_eq!(
        world.names(),
        [
            format!("spend/held/0/{id}/250"),
            format!("spend/reservations/{id}"),
            "spend/turn/0/1".to_owned(),
        ]
    );
    assert!(generation.get("requests/order-1/ended").is_none());
    assert_eq!(
        world
            .ledger
            .books_of(&world.store)
            .unwrap()
            .folded
            .balance()
            .counted,
        250
    );
    // Whoever asks after the run is told that it was abandoned.
    let watched = engine.status(&id).unwrap();
    assert!(
        matches!(watched.state(), RunState::Abandoned { .. }),
        "{watched:?}"
    );

    // A closing whose turn another publication took meanwhile is made again.
    let world = World::new(1000);
    let raced = Arc::new(AtomicU64::new(0));
    let racing = {
        let (inner, store, raced) = (world.ledger.clone(), world.store.clone(), raced.clone());
        Meddled::settling(&world, move |step| {
            // The first time, an admission of another call takes the turn first.
            if raced.fetch_add(1, Ordering::SeqCst) == 0 {
                plant(&store, &hold_and_turn("9-9", 1, 2));
            }
            let run = RunId::parse(step.generation().runs().keys().next().unwrap()).unwrap();
            let closing = Closing {
                run: &run,
                reserved_micros: 250,
                at_ms: 7000,
            };
            inner.settle(&closing, step)
        })
    };
    let engine = world.engine_with(250, racing, |_| Ok(Produced::nothing()));
    let finished = ran(engine.request(&asked("order-1")));
    assert_eq!(raced.load(Ordering::SeqCst), 2);
    assert!(
        world
            .names()
            .contains(&format!("spend/closed/{}/released", finished.run()))
    );
    assert!(world.names().contains(&"spend/turn/0/3".to_owned()));
}

// --- the confirmation --------------------------------------------------------------------------

/// What a store holds of a request whose admission was not confirmed, and what its
/// caller and a repeat of it are told.
fn not_confirmed(world: &World, engine: &Engine, refused: CallError, closed: &str) {
    let id = world.only_run();
    let CallError::NotConfirmed { run, action, ended } = &refused else {
        panic!("{refused:?}");
    };
    assert_eq!((run, action.as_str(), *ended), (&id, "spend", true));
    assert!(refused.refusal().is_none());
    assert!(
        refused.to_string().contains("was not confirmed"),
        "{refused}"
    );
    // It ran nothing.
    assert_eq!(world.ran(), 0);
    // The run is ended, as an error of the kind for it, in a record that is read.
    let generation = world.store.snapshot().unwrap();
    let stored = record::stored(&generation, id.as_str(), |entry| world.store.read(entry));
    let stored = stored.unwrap().unwrap();
    assert_eq!(stored.outcome().status, Status::Error);
    let error = stored.outcome().error.as_ref().unwrap();
    assert_eq!(error.kind, NOT_CONFIRMED_KIND);
    assert!(
        error.message.contains("was not confirmed"),
        "{}",
        error.message
    );
    // Its reservation is closed in the publication that ended it.
    assert!(
        world
            .names()
            .contains(&format!("spend/closed/{id}/{closed}")),
        "{:?}",
        world.names()
    );
    // Its id's ending is made: a repeat is answered with the run, which ended, and
    // nothing runs later under the id.
    assert!(generation.get("requests/order-1/ended").is_some());
    match engine.request(&asked("order-1")) {
        Ok(Answer::Repeat(earlier)) => {
            assert_eq!(earlier.run(), &id);
            assert!(matches!(
                earlier.state(),
                RunState::Ended(Outcome::Error { .. })
            ));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(world.ran(), 0);
}

#[test]
fn an_admission_whose_reservation_has_any_name_beside_its_record_and_its_one_hold_is_not_confirmed()
{
    // The model's history 80: before the reading, other hands make a place and a
    // dispatch under the reservation. The set is one the ledger could have written
    // later, and nothing runs under an admission before it is confirmed, so it is
    // not the ledger's.
    let world = World::new(1000);
    let store = world.store.clone();
    let ledger = Meddled::after_reserve(&world, move |reservation| {
        let run = reservation.run().clone();
        plant(
            &store,
            &[
                Record::Place {
                    run: run.clone(),
                    place: n(1),
                    by: PlaceBy::Dispatch,
                },
                Record::Dispatch {
                    run: run.clone(),
                    place: n(1),
                    by: run,
                    request: 0,
                    bound: amount(5),
                    at_ms: 1,
                },
            ],
        );
    });
    let engine = world.engine_with(250, ledger, |_| Ok(Produced::nothing()));
    let refused = engine.request(&asked("order-1")).unwrap_err();
    // A dispatch that is not settled is under it: closed as held, in full.
    not_confirmed(&world, &engine, refused, "held");
    assert_eq!(
        world
            .ledger
            .books_of(&world.store)
            .unwrap()
            .folded
            .balance()
            .counted,
        250
    );
}

#[test]
fn an_admission_whose_epoch_needs_its_owner_is_not_confirmed() {
    // Other hands make a turn that no publication took, before the reading.
    let world = World::new(1000);
    let store = world.store.clone();
    let ledger = Meddled::after_reserve(&world, move |_| {
        let turn = Record::Turn {
            epoch: 0,
            turn: n(9),
            by: TurnBy::Settlement,
            run: None,
        };
        plant(&store, &[turn]);
    });
    let engine = world.engine_with(250, ledger, |_| Ok(Produced::nothing()));
    let refused = engine.request(&asked("order-1")).unwrap_err();
    // Its own names are whole and nothing was dispatched: all of it is released.
    not_confirmed(&world, &engine, refused, "released");
    let books = world.ledger.books_of(&world.store).unwrap();
    assert!(books.folded.balance().needs_owner);
    assert_eq!(books.folded.balance().counted, 0);
}

#[test]
fn an_admission_of_more_than_nothing_whose_epoch_is_over_its_allowance_is_not_confirmed() {
    // 250 of an allowance of 300 were judged to fit. Before the reading another
    // reservation of 100 is there, with its turn: 350 are counted of 300.
    let world = World::new(300);
    let store = world.store.clone();
    let ledger = Meddled::after_reserve(&world, move |_| {
        plant(&store, &hold_and_turn("9-9", 100, 2));
    });
    let engine = world.engine_with(250, ledger, |_| Ok(Produced::nothing()));
    let refused = engine.request(&asked("order-1")).unwrap_err();
    not_confirmed(&world, &engine, refused, "released");
    let balance = world
        .ledger
        .books_of(&world.store)
        .unwrap()
        .folded
        .balance();
    assert!(!balance.needs_owner);
    assert_eq!((balance.allowed, balance.counted), (300, 100));
}

#[test]
fn a_call_of_nothing_in_a_store_in_debt_is_confirmed_and_runs() {
    // 100 are counted of an allowance of 30: the store is in debt by 70. A call
    // that reserves nothing adds nothing to what is counted, and is not held to it.
    let world = World::new(30);
    world.plant(&hold_and_turn("9-9", 100, 1));
    assert_eq!(
        world
            .ledger
            .books_of(&world.store)
            .unwrap()
            .folded
            .balance()
            .debt,
        70
    );
    let finished = ran(world.engine(0).request(&asked("order-1")));
    assert_eq!(world.ran(), 1);
    assert_eq!(
        finished.outcome(),
        &Outcome::Completed {
            outputs: Vec::new()
        }
    );
    // A call of more than nothing is refused there before anything is published.
    let refused = world.engine(1).request(&asked("order-2")).unwrap_err();
    assert!(matches!(
        refused.refusal(),
        Some(whizbang_core::contract::Refusal::OverAllowance { left_micros: 0, .. })
    ));
}

#[test]
fn an_admission_made_beside_a_frozen_reservation_whose_hold_states_its_amount_is_confirmed() {
    // A reservation of 100 with no record: frozen, and its one hold states its
    // amount. It counts 100 and stops nothing else (the plan's invariant 4).
    let world = World::new(1000);
    world.plant(&hold_and_turn("9-9", 100, 1));
    let books = world.ledger.books_of(&world.store).unwrap();
    let frozen = books.reservation(&run("9-9")).unwrap();
    assert_eq!(frozen.state, whizbang_testkit::spend::State::Frozen);
    assert!(!books.folded.balance().needs_owner);
    let finished = ran(world.engine(250).request(&asked("order-1")));
    assert_eq!(world.ran(), 1);
    assert!(
        world
            .names()
            .contains(&format!("spend/closed/{}/released", finished.run()))
    );
    assert_eq!(
        world
            .ledger
            .books_of(&world.store)
            .unwrap()
            .folded
            .balance()
            .counted,
        100
    );
}

#[test]
fn a_call_from_inside_an_action_has_no_reservation_and_is_not_confirmed() {
    let world = World::new(1000);
    let engine = world.engine_with(250, world.ledger.clone(), |context| {
        // The first run calls another action, from inside: a run under it.
        if context.allowance().total_micros() == Some(250) {
            let call = Call::new("inner").argument("text", Value::text("inside"));
            context.executor().submit(&call).unwrap();
        }
        Ok(Produced::nothing())
    });
    ran(engine.request(&asked("order-1")));
    assert_eq!(world.ran(), 2);
    assert_eq!((world.ledger.asked(), world.ledger.confirmed()), (1, 1));
    // One reservation, and one closing: the call from inside made no name.
    assert_eq!(world.names().len(), 6, "{:?}", world.names());
}

// --- a dispatch --------------------------------------------------------------------------------

/// Runs one call that reserves 10, whose action does `inside` with its allowance, the
/// store and its own run's id, and hands back what `inside` answered.
fn inside<T: Send + 'static>(
    world: &World,
    inside: impl Fn(&Context, &Store, &FakeLedger, &RunId) -> T + Send + Sync + 'static,
) -> T {
    let answer: Arc<Mutex<Option<T>>> = Arc::default();
    let body = {
        let (answer, store, ledger) = (answer.clone(), world.store.clone(), world.ledger.clone());
        move |context: &Context| {
            let generation = store.snapshot().unwrap();
            let run = RunId::parse(generation.runs().keys().next().unwrap()).unwrap();
            *answer.lock().unwrap() = Some(inside(context, &store, &ledger, &run));
            Ok(Produced::nothing())
        }
    };
    let engine = world.engine_with(10, world.ledger.clone(), body);
    ran(engine.request(&asked("order-1")));
    let answer = answer.lock().unwrap().take();
    answer.unwrap()
}

/// A dispatch of this bound by the run itself, as the meter is to make it: the
/// allowance is debited, then the fake judges by the names and publishes.
fn dispatch(
    context: &Context,
    store: &Store,
    ledger: &FakeLedger,
    run: &RunId,
    bound: u64,
) -> Result<(whizbang_core::contract::Dispatched, u64), DispatchRefused> {
    let bound = amount(bound);
    context
        .allowance()
        .dispatch(bound, || ledger.place(store, run, run, bound))
}

#[test]
fn a_dispatch_is_debited_and_placed_and_the_end_of_its_run_closes_the_reservation_by_its_settlements()
 {
    let world = World::new(1000);
    let seen = inside(&world, |context, store, ledger, run| {
        let allowance = context.allowance();
        let (first, place) = dispatch(context, store, ledger, run, 6).unwrap();
        assert_eq!(place, 1);
        let after = (allowance.left_micros(), allowance.tree_left_micros());
        // Settled at 2: the names say so, and what the bound held beyond the charge
        // can be spent again.
        ledger
            .settle_at(store, run, 1, amount(6), Stage::Answered, amount(2))
            .unwrap();
        first.settled(amount(2));
        (after, allowance.left_micros(), allowance.tree_left_micros())
    });
    assert_eq!(seen, ((Some(4), Some(4)), Some(8), Some(8)));
    // Every dispatch under it was settled: its closing released, at what was charged.
    let id = world.only_run();
    assert!(
        world
            .names()
            .contains(&format!("spend/closed/{id}/released"))
    );
    assert_eq!(
        world
            .ledger
            .books_of(&world.store)
            .unwrap()
            .folded
            .balance()
            .counted,
        2
    );

    // A request that is never settled: the reservation is closed as held, in full.
    let world = World::new(1000);
    inside(&world, |context, store, ledger, run| {
        drop(dispatch(context, store, ledger, run, 6).unwrap());
    });
    let id = world.only_run();
    assert!(world.names().contains(&format!("spend/closed/{id}/held")));
    assert_eq!(
        world
            .ledger
            .books_of(&world.store)
            .unwrap()
            .folded
            .balance()
            .counted,
        10
    );
}

#[test]
fn what_is_given_back_once_a_run_has_entered_its_commit_step_is_dropped() {
    // The action hands its request, of a bound of 6, out of the run; the run ends,
    // and its reservation is closed as held, by its own names. The request is
    // settled at 2 afterwards: the 4 that would have come back are dropped, because
    // the run entered its commit step before.
    let world = World::new(1000);
    let (request, allowance) = inside(&world, |context, store, ledger, run| {
        let (request, _) = dispatch(context, store, ledger, run, 6).unwrap();
        (request, context.allowance().clone())
    });
    assert_eq!(allowance.left_micros(), Some(4));
    request.settled(amount(2));
    assert_eq!(allowance.left_micros(), Some(4));
    // The same inside the run, before that step, is given back: 8 are left.
    let world = World::new(1000);
    let left = inside(&world, |context, store, ledger, run| {
        let (request, _) = dispatch(context, store, ledger, run, 6).unwrap();
        request.settled(amount(2));
        context.allowance().left_micros()
    });
    assert_eq!(left, Some(8));
}

#[test]
fn a_dispatch_is_refused_with_a_reason_of_its_own_by_each_thing_that_holds_it() {
    // The run's allowance.
    let world = World::new(1000);
    let refused = inside(&world, |context, store, ledger, run| {
        dispatch(context, store, ledger, run, 11).unwrap_err()
    });
    assert_eq!(refused, DispatchRefused::OverRun { left_micros: 10 });
    assert_eq!(
        world.names().len(),
        6,
        "nothing was placed: {:?}",
        world.names()
    );

    // The reservation by its names: other hands made a dispatch of 7 with its place
    // under it, which is a set the ledger could have written. The process holds
    // nothing of that request, and its memory would let 5 by.
    let world = World::new(1000);
    let (refused, left) = inside(&world, |context, store, ledger, run| {
        let other = RunId::parse("9-9").unwrap();
        ledger.place(store, run, &other, amount(7)).unwrap();
        let refused = dispatch(context, store, ledger, run, 5).unwrap_err();
        // The debit was undone.
        let allowance = context.allowance();
        (
            refused,
            (allowance.left_micros(), allowance.tree_left_micros()),
        )
    });
    assert_eq!(refused, DispatchRefused::OverReservation { left_micros: 3 });
    assert_eq!(left, (Some(10), Some(10)));

    // A frozen reservation: a name of no form under it.
    let world = World::new(1000);
    let refused = inside(&world, |context, store, ledger, run| {
        let mut publication = store.begin().unwrap();
        let stray = format!("spend/place/{run}/01");
        publication
            .create(&stray, store::Kind::Diagnostic, b"x")
            .unwrap();
        publication.publish().unwrap();
        dispatch(context, store, ledger, run, 5).unwrap_err()
    });
    assert_eq!(refused, DispatchRefused::Frozen);

    // A closed reservation: a closing names it, whatever else it is.
    let world = World::new(1000);
    let refused = inside(&world, |context, store, ledger, run| {
        let closing = Record::Closing {
            run: run.clone(),
            released: false,
            by: ClosedBy::Recovery,
            charged: Sum::ZERO,
            unresolved: Sum::ZERO,
            at_ms: 1,
        };
        plant(store, &[closing]);
        dispatch(context, store, ledger, run, 5).unwrap_err()
    });
    assert_eq!(refused, DispatchRefused::Closed);
}

#[test]
fn a_dispatch_is_refused_where_its_epoch_needs_its_owner() {
    let world = World::new(1000);
    let (refused, left) = inside(&world, |context, store, ledger, run| {
        // A turn that no publication took: the epoch needs its owner.
        let turn = Record::Turn {
            epoch: 0,
            turn: n(9),
            by: TurnBy::Settlement,
            run: None,
        };
        plant(store, &[turn]);
        // A request of nothing is refused as every other is.
        let nothing = dispatch(context, store, ledger, run, 0).unwrap_err();
        assert_eq!(nothing, DispatchRefused::NeedsOwner { epoch: 0 });
        let refused = dispatch(context, store, ledger, run, 5).unwrap_err();
        let allowance = context.allowance();
        (
            refused,
            (allowance.left_micros(), allowance.tree_left_micros()),
        )
    });
    assert_eq!(refused, DispatchRefused::NeedsOwner { epoch: 0 });
    assert_eq!(left, (Some(10), Some(10)));
    assert_eq!(
        refused.to_string(),
        "the request was not sent: the books of epoch 0 need their owner"
    );
}

#[test]
fn a_dispatch_is_refused_where_the_trees_tally_in_memory_does_not_cover_it_though_the_names_do() {
    // An answer counted in memory before its settlement (the model's history 75).
    // A request of 5 is placed under a reservation of 10; the answer says 12, and
    // the process holds it; no name says so yet. By the names 5 are left. A call
    // from inside the run holds a share of 5, and its request of 5 is refused by
    // what the process holds of the tree, with its own reason.
    let world = World::new(1000);
    // The refusal, what the run that was refused can spend, and what the names have
    // left.
    type Seen = (DispatchRefused, Option<u64>, u64);
    let seen: Arc<Mutex<Option<Seen>>> = Arc::default();
    let body = {
        let (store, ledger, seen) = (world.store.clone(), world.ledger.clone(), seen.clone());
        move |context: &Context| {
            let generation = store.snapshot().unwrap();
            let root = RunId::parse(generation.runs().keys().next().unwrap()).unwrap();
            let allowance = context.allowance();
            let bound = amount(5);
            if generation.runs().len() == 1 {
                // The root: its own request of 5, answered at 12 and not settled.
                let (mut request, _) = allowance
                    .dispatch(bound, || ledger.place(&store, &root, &root, bound))
                    .unwrap();
                request.answered(amount(12));
                // The call from inside takes the 5 the root's balance still has.
                let call = Call::new("inner").argument("text", Value::text("inside"));
                context.executor().submit(&call).unwrap();
                return Ok(Produced::nothing());
            }
            // The call from inside: its share is 5, and the names have 5 left.
            assert_eq!(allowance.total_micros(), Some(5));
            let by_names = ledger
                .books_of(&store)
                .unwrap()
                .reservation(&root)
                .unwrap()
                .left();
            let never = || -> Result<u64, DispatchRefused> { panic!("the names were asked") };
            let refused = allowance.dispatch(bound, never).unwrap_err();
            *seen.lock().unwrap() = Some((refused, allowance.left_micros(), by_names));
            Ok(Produced::nothing())
        }
    };
    let engine = world.engine_with(10, world.ledger.clone(), body);
    ran(engine.request(&asked("order-1")));
    assert_eq!(world.ran(), 2);
    let seen = seen.lock().unwrap().take().unwrap();
    assert_eq!(
        seen,
        (DispatchRefused::OverTree { left_micros: 0 }, Some(5), 5)
    );
    assert_eq!(
        seen.0.to_string(),
        "the request was not sent: the tree it is sent under has, as its process holds \
         it, 0 millionths of a US dollar left"
    );
}

// --- the confirmation, where the reading fails or the run cannot be ended -----------------------

type Confirm = Box<dyn Fn(&Reservation, &Reread<'_>) -> Result<bool, NotJudged> + Send + Sync>;

/// A ledger that is the fake, with the test's answer in place of its confirmation,
/// and the test's hands in its place at a run's end when the test gives them.
struct Answering {
    inner: Arc<FakeLedger>,
    confirm: Confirm,
    settle: Option<InSettle>,
}

impl Answering {
    fn confirming(
        world: &World,
        confirm: impl Fn(&Reservation, &Reread<'_>) -> Result<bool, NotJudged> + Send + Sync + 'static,
    ) -> Answering {
        Answering {
            inner: world.ledger.clone(),
            confirm: Box::new(confirm),
            settle: None,
        }
    }

    /// The same, with a run's end that fails as the ledger's own.
    fn and_no_end(mut self) -> Answering {
        self.settle = Some(Box::new(|_| {
            Err(NotJudged::saying("its books do not balance"))
        }));
        self
    }
}

impl Ledger for Answering {
    fn reserve(
        &self,
        reservation: &Reservation,
        step: &mut Step<'_, '_>,
    ) -> Result<MadeName, Declined> {
        self.inner.reserve(reservation, step)
    }

    fn confirm(&self, reservation: &Reservation, reading: &Reread<'_>) -> Result<bool, NotJudged> {
        (self.confirm)(reservation, reading)
    }

    fn settle(&self, closing: &Closing<'_>, step: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
        match &self.settle {
            Some(settle) => settle(step),
            None => self.inner.settle(closing, step),
        }
    }
}

/// The sentence of the error a run ended with.
fn ended_saying(world: &World, id: &RunId) -> String {
    let generation = world.store.snapshot().unwrap();
    let stored = record::stored(&generation, id.as_str(), |entry| world.store.read(entry));
    let stored = stored.unwrap().unwrap();
    stored.outcome().error.as_ref().unwrap().message.clone()
}

/// What the books count in the newest epoch.
fn counted(world: &World) -> u128 {
    let books = world.ledger.books_of(&world.store).unwrap();
    books.folded.balance().counted
}

#[test]
fn an_admission_whose_confirmation_fails_is_not_confirmed_and_nothing_runs() {
    // The ledger cannot answer: that is no answer, and is taken as one that says no.
    let world = World::new(1000);
    let failing = Answering::confirming(&world, |_, _| {
        Err(NotJudged::saying("its books could not be folded"))
    });
    let engine = world.engine_with(250, Arc::new(failing), |_| Ok(Produced::nothing()));
    let refused = engine.request(&asked("order-1")).unwrap_err();
    not_confirmed(&world, &engine, refused, "released");
    assert_eq!(
        ended_saying(&world, &world.only_run()),
        "the run was admitted and its admission was not confirmed, so its action was not \
         run: the books could not be read after its admission was published: the ledger \
         failed: its books could not be folded"
    );
    assert_eq!(counted(&world), 0);
}

/// The test kit's file system, on which one reading of a generation's file fails
/// when a test asks for it: the first after the pointer is next moved, which is the
/// reading that follows a publication.
struct Unread {
    inner: ShimFs,
    /// The next move of the pointer arms one failure.
    after_the_next_publication: AtomicBool,
    armed: AtomicBool,
    failed: AtomicU64,
}

impl Fs for Unread {
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.create_dir(path)
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.inner.write_new(path, bytes)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.sync_dir(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)?;
        let pointer = to.ends_with("CURRENT");
        if pointer
            && self
                .after_the_next_publication
                .swap(false, Ordering::SeqCst)
        {
            self.armed.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
    fn remove_all(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_all(path)
    }
    fn read_at_most(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        let generation = path
            .parent()
            .is_some_and(|dir| dir.ends_with("generations"));
        if generation && self.armed.swap(false, Ordering::SeqCst) {
            self.failed.fetch_add(1, Ordering::SeqCst);
            return Err(io::Error::other("the disk did not answer"));
        }
        self.inner.read_at_most(path, limit)
    }
    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.inner.list(path)
    }
    fn exists(&self, path: &Path) -> io::Result<bool> {
        self.inner.exists(path)
    }
    fn hold_lock_file(&self, path: &Path, create: bool) -> io::Result<Option<Box<dyn LockFile>>> {
        self.inner.hold_lock_file(path, create)
    }
    fn share(&self, path: &Path) -> io::Result<Box<dyn Held>> {
        self.inner.share(path)
    }
    fn try_alone(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        self.inner.try_alone(path)
    }
    fn begin_new(&self, path: &Path, most: u64) -> io::Result<Box<dyn NewFile>> {
        self.inner.begin_new(path, most)
    }
    fn open_file(&self, path: &Path) -> io::Result<Box<dyn OpenFile>> {
        self.inner.open_file(path)
    }
    fn try_share(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        self.inner.try_share(path)
    }
}

#[test]
fn an_admission_whose_store_cannot_be_read_again_is_not_confirmed_and_nothing_runs() {
    // The admission is published, and the reading that follows it fails: the ledger
    // is not asked, and that is taken as an answer that says no. The store reads
    // again afterwards, so the run is ended.
    let fs = Arc::new(Unread {
        inner: ShimFs::new(),
        after_the_next_publication: AtomicBool::new(false),
        armed: AtomicBool::new(false),
        failed: AtomicU64::new(0),
    });
    let dir = Scratch::new("ledger-unread");
    let store = Arc::new(Store::open(&dir.path().join("store"), fs.clone()).unwrap());
    let world = World {
        _dir: dir,
        store,
        ledger: Arc::new(FakeLedger::with_allowance(1000)),
        runs: Arc::new(AtomicU64::new(0)),
    };
    let ledger = {
        let fs = fs.clone();
        Meddled::after_reserve(&world, move |_| {
            fs.after_the_next_publication.store(true, Ordering::SeqCst);
        })
    };
    let engine = world.engine_with(250, ledger, |_| Ok(Produced::nothing()));
    let refused = engine.request(&asked("order-1")).unwrap_err();
    assert_eq!(fs.failed.load(Ordering::SeqCst), 1);
    assert_eq!(world.ledger.confirmed(), 0);
    not_confirmed(&world, &engine, refused, "released");
    let said = ended_saying(&world, &world.only_run());
    assert!(
        said.contains("the books could not be read after its admission was published"),
        "{said}"
    );
    assert_eq!(counted(&world), 0);
}

/// What a store holds of a run whose admission was not confirmed and whose end could
/// not be written: the run as it was admitted, its reservation open and counted in
/// full, and no ending of its id. Nothing ran, and the run is abandoned.
fn left_admitted(world: &World, engine: &Engine) {
    assert_eq!(world.ran(), 0);
    let id = world.only_run();
    let generation = world.store.snapshot().unwrap();
    let run = generation.run(id.as_str()).unwrap();
    assert!(run.entries().get(record::RECORD_NAME).is_none());
    assert_eq!(
        world.names(),
        [
            format!("spend/held/0/{id}/250"),
            format!("spend/reservations/{id}"),
            "spend/turn/0/1".to_owned(),
        ]
    );
    assert!(generation.get("requests/order-1/ended").is_none());
    assert_eq!(counted(world), 250);
    let watched = engine.status(&id).unwrap();
    assert!(
        matches!(watched.state(), RunState::Abandoned { .. }),
        "{watched:?}"
    );
}

#[test]
fn an_admission_that_is_not_confirmed_and_cannot_be_ended_stays_admitted_and_its_caller_is_told() {
    // The answer is no, and the publication that would end the run fails.
    let world = World::new(1000);
    let ledger = Answering::confirming(&world, |_, _| Ok(false)).and_no_end();
    let engine = world.engine_with(250, Arc::new(ledger), |_| Ok(Produced::nothing()));
    let refused = engine.request(&asked("order-1")).unwrap_err();
    assert_eq!(
        refused,
        CallError::NotConfirmed {
            run: world.only_run(),
            action: "spend".to_owned(),
            ended: false,
        }
    );
    assert!(refused.refusal().is_none());
    let said = refused.to_string();
    assert!(
        said.ends_with("the run could not be ended and is abandoned"),
        "{said}"
    );
    left_admitted(&world, &engine);

    // The reading itself failed, and the run could not be ended either: the caller
    // is handed the failure of the reading.
    let world = World::new(1000);
    let ledger = Answering::confirming(&world, |_, _| {
        Err(NotJudged::saying("its books could not be folded"))
    })
    .and_no_end();
    let engine = world.engine_with(250, Arc::new(ledger), |_| Ok(Produced::nothing()));
    let failed = engine.request(&asked("order-1")).unwrap_err();
    assert_eq!(
        failed.to_string(),
        "the ledger failed: its books could not be folded"
    );
    left_admitted(&world, &engine);
}

#[test]
fn a_run_cancelled_before_its_admission_is_answered_as_not_standing_ends_as_not_confirmed() {
    // A cancellation is accepted between the admission's publication and the
    // ledger's answer, which is no. The run never could execute: its record and its
    // caller both say that its admission was not confirmed, and neither says
    // cancelled (ADR 0013, "A run whose admission was not confirmed").
    let world = World::new(1000);
    let executor: Arc<Mutex<Option<Engine>>> = Arc::default();
    let answered: Arc<Mutex<Vec<Result<Cancel, CallError>>>> = Arc::default();
    let ledger = {
        let (executor, answered) = (executor.clone(), answered.clone());
        Answering::confirming(&world, move |reservation, _| {
            let engine = executor.lock().unwrap().clone().unwrap();
            let cancelled = engine.cancel(reservation.run());
            answered.lock().unwrap().push(cancelled);
            Ok(false)
        })
    };
    let engine = world.engine_with(250, Arc::new(ledger), |_| Ok(Produced::nothing()));
    *executor.lock().unwrap() = Some(engine.clone());
    let refused = engine.request(&asked("order-1"));
    // The ledger holds the executor no longer.
    *executor.lock().unwrap() = None;
    assert_eq!(*answered.lock().unwrap(), [Ok(Cancel::Accepted)]);
    not_confirmed(&world, &engine, refused.unwrap_err(), "released");
    assert_eq!(counted(&world), 0);
}

// --- settle, and the name it hands back ---------------------------------------------------------

/// A ledger that keeps the first name it made at admission and hands it back from
/// the run's end, where it made that one not: with nothing made there, or with
/// another name made there.
struct KeptForTheEnd {
    kept: Mutex<Option<MadeName>>,
    makes_another: bool,
}

impl Ledger for KeptForTheEnd {
    fn reserve(&self, _: &Reservation, step: &mut Step<'_, '_>) -> Result<MadeName, Declined> {
        *self.kept.lock().unwrap() = Some(step.record(&turn_of_no_run(1))?);
        Ok(step.record(&turn_of_no_run(2))?)
    }

    // It keeps no books of names: every admission stands.
    fn confirm(&self, _: &Reservation, _: &Reread<'_>) -> Result<bool, NotJudged> {
        Ok(true)
    }

    fn settle(&self, _: &Closing<'_>, step: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
        if self.makes_another {
            step.record(&turn_of_no_run(3))?;
        }
        match self.kept.lock().unwrap().take() {
            Some(kept) => Ok(Settled::Closed(kept)),
            None => Ok(Settled::Nothing),
        }
    }
}

fn turn_of_no_run(turn: u64) -> Record {
    Record::Turn {
        epoch: 0,
        turn: n(turn),
        by: TurnBy::Admission,
        run: None,
    }
}

#[test]
fn a_ledger_that_hands_back_from_a_runs_end_a_name_it_did_not_make_there_is_refused() {
    for makes_another in [false, true] {
        let world = World::new(1000);
        let ledger = Arc::new(KeptForTheEnd {
            kept: Mutex::default(),
            makes_another,
        });
        let engine = world.engine_with(250, ledger, |_| Ok(Produced::nothing()));
        let refused = engine.request(&asked("order-1")).unwrap_err();
        let CallError::Ledger { why } = &refused else {
            panic!("{refused:?}");
        };
        assert!(
            why.contains("did not make in this attempt at the run's end"),
            "{why}"
        );
        // The action ran, and its end was not published, with nothing the ledger
        // staged in it: the run is as it was admitted, and abandoned.
        assert_eq!(world.ran(), 1);
        let id = world.only_run();
        let generation = world.store.snapshot().unwrap();
        let run = generation.run(id.as_str()).unwrap();
        assert!(run.entries().get(record::RECORD_NAME).is_none());
        assert_eq!(
            world.names(),
            [
                format!("spend/reservations/{id}"),
                "spend/turn/0/1".to_owned(),
                "spend/turn/0/2".to_owned(),
            ]
        );
        assert!(generation.get("requests/order-1/ended").is_none());
        let watched = engine.status(&id).unwrap();
        assert!(
            matches!(watched.state(), RunState::Abandoned { .. }),
            "{watched:?}"
        );
    }
}
