//! The fold (plan 1B, task S1b; ADR 0016), held to the model of the test kit: the same
//! histories are given to both, and what the real fold answers is compared with what
//! the model answers, field by field, after every event.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use whizbang_core::contract::RunId;
use whizbang_core::spend::fold::{self, Folded, NotCorrected, State};
use whizbang_core::spend::{Amount, ClosedBy, Counted, PlaceBy, Reading, Record, Seal, Sent};
use whizbang_core::spend::{Sum, TurnBy};
use whizbang_core::store::{Kind, Store};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::spend as model;
use whizbang_testkit::spend::{Books, Event, Must, Name, Outcome, Run, Why};

/// The seed of the generated histories: the one the model's own tests are run from.
const SEED: u64 = 0x1B00_2026_1114;

fn id(run: Run) -> RunId {
    RunId::parse(&run.to_string()).unwrap()
}

fn run_of(id: &RunId) -> Run {
    let count = id.as_str().strip_prefix("7000-").unwrap();
    Run(count.parse().unwrap())
}

fn amount(micros: u64) -> Amount {
    Amount::new(micros).unwrap()
}

/// The model's names as the names a store holds. Every form is written as the model
/// writes it, but for two. A correction's seal is the real one: the model's is a
/// digest of its own, of sixteen digits. And the model's name of no form is written
/// as a text that begins as a name of its reservation does and is none; a store
/// reads no content under such a text, so where the model has its content damaged,
/// which there changes what a seal is over, it is written as another text.
#[derive(Default)]
struct Texts {
    /// Each correction that was seen, with the text it was given when it was first
    /// there: the seal a correction carries is fixed when it is made.
    corrections: BTreeMap<Name, String>,
    written: std::cell::RefCell<BTreeMap<(Name, bool), String>>,
}

impl Texts {
    fn text(&self, name: &Name, damaged: bool) -> String {
        match name {
            Name::Adjusted { .. } => self.corrections[name].clone(),
            Name::Stray { run, count } if damaged => format!("spend/place/{run}/0{count}/hurt"),
            Name::Stray { run, count } => format!("spend/place/{run}/0{count}"),
            // What a name garbled past its run and its epoch is left as.
            Name::Unplaced { count } => format!("spend/h{count}ld/x/7000-1/1O"),
            other => other.to_string(),
        }
    }

    /// A name as a store holds it, with whether its content is damaged.
    fn held(&self, name: &Name, books: &Books) -> (String, bool) {
        let damaged = books.damaged().contains(name);
        let formless = matches!(name, Name::Stray { .. } | Name::Unplaced { .. });
        // The text of a name is written once: most names of a state were there in
        // the state before.
        let mut written = self.written.borrow_mut();
        let text = match written.get(&(name.clone(), damaged)) {
            Some(text) => text.clone(),
            None => {
                let text = self.text(name, damaged);
                written.insert((name.clone(), damaged), text.clone());
                text
            }
        };
        (text, damaged && !formless)
    }

    /// Gives every correction that is new in these books its text. One that the
    /// model sealed over the names as they are is sealed over them here, with the
    /// real seal; any other carries a seal that is of no names.
    fn see(&mut self, books: &Books) {
        for name in books.names() {
            let Name::Adjusted {
                run,
                place,
                turn,
                seal,
                micros,
            } = name
            else {
                continue;
            };
            if self.corrections.contains_key(name) {
                continue;
            }
            let guard = Name::Place {
                run: *run,
                place: *place,
            };
            let others =
                |other: &&Name| other.of() == Some(*run) && *other != name && **other != guard;
            let hurt = |other: &Name| books.damaged().contains(other);
            let over = model::seal(
                books
                    .names()
                    .iter()
                    .filter(others)
                    .map(|one| (one, hurt(one))),
            );
            let real = if over == *seal {
                let texts: Vec<(String, bool)> = books
                    .names()
                    .iter()
                    .filter(others)
                    .map(|one| self.held(one, books))
                    .collect();
                let over = texts.iter().map(|(text, hurt)| (text.as_str(), *hurt));
                Seal::over(over).to_string()
            } else {
                format!("{seal:032x}")
            };
            let text = format!("spend/adjusted/{run}/{place}/{turn}/{real}/{micros}");
            self.corrections.insert(name.clone(), text);
        }
    }

    /// The names of these books as a store holds them, in its order, each with
    /// whether its content is damaged.
    fn names(&mut self, books: &Books) -> BTreeMap<String, bool> {
        self.see(books);
        let named = |name: &Name| self.held(name, books);
        books.names().iter().map(named).collect()
    }
}

/// The fold of names that are in no store.
fn folded(names: &BTreeMap<String, bool>) -> Folded {
    let names = names.iter().map(|(name, hurt)| (name.as_str(), *hurt));
    Folded::of(Reading::of_names(names))
}

fn state(state: State) -> model::State {
    match state {
        State::Open => model::State::Open,
        State::Released => model::State::Released,
        State::Interrupted => model::State::Interrupted,
        State::Frozen => model::State::Frozen,
        State::Adjusted => model::State::Adjusted,
    }
}

/// One reservation as the real fold answered it, in the model's type: every field.
fn standing(one: &fold::Standing) -> model::Standing {
    let names = one.names();
    model::Standing {
        epoch: one.epoch(),
        epochs: names.epochs(),
        amount: names.amount().micros(),
        charged: names.charged().micros(),
        settled: names.settled().micros(),
        outstanding: names.outstanding().micros(),
        unresolved: names.unresolved().micros(),
        unsettled: names.unsettled(),
        unexplained: names.unexplained(),
        corrections: names.corrections_most().micros(),
        reachable: names.reachable(),
        names: names.count(),
        counts: one.counts().micros(),
        unknown: one.unknown(),
        state: state(one.state()),
        closed: one.closed(),
    }
}

fn spend(one: &whizbang_core::spend::RunSpend) -> model::RunSpend {
    model::RunSpend {
        charged: one.charged.micros(),
        outstanding: one.outstanding.micros(),
        unresolved: one.unresolved.micros(),
    }
}

/// What the real fold answered, in the model's types: every field of every
/// reservation, every run, epoch and turn.
fn as_model(real: &Folded) -> model::Folded {
    let epochs: BTreeSet<u64> = real.resets().keys().map(|epoch| epoch - 1).collect();
    let held = real.reservations().values();
    let held = held.flat_map(|one| one.names().epochs());
    let due = epochs.into_iter().chain(held);
    model::Folded {
        reservations: real
            .reservations()
            .iter()
            .map(|(run, one)| (run_of(run), standing(one)))
            .collect(),
        runs: real
            .runs()
            .iter()
            .map(|(run, one)| (run_of(run), spend(one)))
            .collect(),
        resets: real
            .resets()
            .iter()
            .map(|(epoch, allowed)| (*epoch, allowed.micros()))
            .collect(),
        turns: real.turns().clone(),
        // The model keeps what is due of each epoch that is owed a turn.
        due: due.map(|epoch| (epoch, real.due(epoch))).collect(),
        unplaced: real.unplaced(),
    }
}

fn counted(found: Counted) -> model::Counted {
    match found {
        Counted::Exactly(sum) => model::Counted::Exactly(sum.micros()),
        Counted::AtLeast(sum) => model::Counted::AtLeast(sum.micros()),
    }
}

fn sent(found: Sent) -> model::Sent {
    match found {
        Sent::Known(one) => model::Sent::Known(spend(&one)),
        Sent::Unknown { at_most } => model::Sent::Unknown {
            at_most: at_most.micros(),
        },
        Sent::Unbounded => model::Sent::Unbounded,
    }
}

/// A balance of the real fold, in the model's type.
fn plain(one: fold::Balance) -> model::Balance {
    model::Balance {
        epoch: one.epoch,
        allowed: one.allowed.micros(),
        counted: one.counted.least().micros(),
        unknown: matches!(one.counted, Counted::AtLeast(_)),
        charged: one.charged.micros(),
        held: one.held.micros(),
        unresolved: one.unresolved.micros(),
        left: one.left.micros(),
        debt: one.debt.micros(),
        needs_owner: one.needs_owner,
    }
}

/// Holds what the real fold answered of these books to what the model answers: the
/// fold itself, and every reading of it that the real fold works out in its own
/// code. And to the second reading of each reservation, which the model's checker
/// wrote apart from the recogniser.
fn compare(books: &Books, real: &Folded) -> Result<(), String> {
    let wanted = books.fold();
    let found = as_model(real);
    if found != wanted {
        for (run, one) in &wanted.reservations {
            let other = found.reservations.get(run);
            if other != Some(one) {
                return Err(format!(
                    "{run}: the fold gives {other:#?}, the model {one:#?}"
                ));
            }
        }
        return Err(format!("the fold gives {found:#?}, the model {wanted:#?}"));
    }
    if real.epoch() != wanted.epoch() {
        return Err(format!("the newest epoch: {}", real.epoch()));
    }
    let mut epochs: BTreeSet<u64> = wanted.resets.keys().copied().collect();
    epochs.extend(wanted.turns.keys().copied());
    epochs.extend([0, wanted.epoch()]);
    for epoch in &epochs {
        let (one, other) = (plain(real.balance_of(*epoch)), wanted.balance_of(*epoch));
        let same = one == other
            && real.needs_owner(*epoch) == wanted.needs_owner(*epoch)
            && real.next_turn(*epoch) == wanted.next_turn(*epoch);
        if !same {
            return Err(format!(
                "epoch {epoch}: the fold gives {one:?}, the model {other:?}"
            ));
        }
    }
    // The two readings of each reservation's names, the recogniser's and the one the
    // model's checker wrote apart from it, give one answer: the real fold's is the
    // real recogniser's, which was compared above.
    let apart = books.read_apart_of(&wanted);
    if !apart.is_empty() {
        return Err(format!("the second reading differs of {apart:?}"));
    }
    for (run, other) in &wanted.reservations {
        let root = id(*run);
        let one = &real.reservations()[&root];
        // An admission stands, or does not, as the model says of the amount its hold
        // states; and of any other amount it does not stand, whatever the model
        // says: the real fold holds what it is asked of to the hold.
        let whole = Amount::new(other.amount).is_some_and(|micros| {
            let stated = one.names().stated() == Some(micros);
            // The same amount with its last bit turned: another amount.
            let another = Amount::new(other.amount ^ 1);
            real.stands(&root, micros) == (stated && model::stands(&wanted, *run, other.amount))
                && another.is_none_or(|another| !real.stands(&root, another))
        });
        let same = one.names().most().micros() == other.most()
            && one.left().micros() == other.left()
            && one.names().known().micros() == other.known()
            && counted(one.counted()) == other.counted()
            && epochs
                .iter()
                .all(|epoch| one.counts_in(*epoch) == other.counts_in(*epoch))
            && whole;
        if !same {
            return Err(format!("{run}: a reading of {one:?} is not the model's"));
        }
        for by in wanted.runs.keys().chain([run]) {
            let (one, other) = (real.sent_by(&id(*by), &root), wanted.sent_by(*by, *run));
            if sent(one) != other {
                return Err(format!(
                    "{by} under {run}: sent {one:?}, the model {other:?}"
                ));
            }
        }
    }
    Ok(())
}

/// What the real fold says of a correction, as the model's refusal of one.
fn refusal(found: Result<(), NotCorrected>) -> Option<Why> {
    let short = |sum: Sum| u64::try_from(sum.micros()).unwrap_or(u64::MAX);
    match found {
        Ok(()) => None,
        Err(NotCorrected::NotCorrectable) => Some(Why::NotCorrectable),
        Err(NotCorrected::BelowSettled { settled }) => Some(Why::BelowSettled {
            settled: short(settled),
        }),
        Err(NotCorrected::OverCount { most }) => Some(Why::OverCount { most: short(most) }),
        Err(NotCorrected::CannotBeHeld { left }) => Some(Why::CannotBeHeld {
            left: left.micros(),
        }),
    }
}

/// One history, event by event, with the real fold beside the model.
struct Beside {
    books: Books,
    texts: Texts,
    names: BTreeMap<String, bool>,
    real: Folded,
    /// The model's names and damage as they were when they were last compared.
    seen: Option<(BTreeSet<Name>, BTreeSet<Name>)>,
    /// How many states were compared.
    states: u64,
    /// How many corrections the real fold judged as the model did.
    corrections: u64,
}

impl Beside {
    fn new() -> Beside {
        Beside {
            books: Books::new(),
            texts: Texts::default(),
            names: BTreeMap::new(),
            real: Folded::default(),
            seen: None,
            states: 0,
            corrections: 0,
        }
    }

    /// Does one event on the model, folds the names it leaves, and compares. A
    /// correction that is judged now is first judged by the real fold, on the names
    /// as they were, and held to what the model then does with it.
    fn step(&mut self, event: &Event) -> Result<Outcome, String> {
        let plain = match event {
            Event::Second(inner) => inner,
            other => other,
        };
        let judged = match plain {
            // The largest number is no amount, and is refused before anything is
            // judged.
            Event::Adjust { run, micros } => {
                Amount::new(*micros).map(|micros| refusal(self.real.correction(&id(*run), micros)))
            }
            _ => None,
        };
        let outcome = self.books.apply(event);
        if let Some(judged) = judged {
            let wanted = match &outcome {
                Outcome::Refused(why) => Some(*why),
                _ => None,
            };
            if judged != wanted {
                return Err(format!("the correction: {judged:?}, the model {wanted:?}"));
            }
            self.corrections += 1;
        }
        // An event that was refused left the names as they were, and what was
        // compared of them stands.
        let same = self.seen.as_ref().is_some_and(|(names, damaged)| {
            names == self.books.names() && damaged == self.books.damaged()
        });
        if !same {
            self.names = self.texts.names(&self.books);
            self.real = folded(&self.names);
            compare(&self.books, &self.real)?;
            self.seen = Some((self.books.names().clone(), self.books.damaged().clone()));
            self.states += 1;
        }
        Ok(outcome)
    }
}

/// Runs one generated history beside the model; what it returns is how many states
/// were compared, or what differed.
fn beside(events: &[Event]) -> Result<Beside, String> {
    let mut both = Beside::new();
    for (at, event) in events.iter().enumerate() {
        both.step(event)
            .map_err(|why| format!("event {at}, {event:?}: {why}"))?;
    }
    Ok(both)
}

/// Every history of one generated suite, from the seed, beside the model.
fn suite(count: u64, generated: fn(u64, u64) -> Vec<Event>) -> (u64, u64) {
    part(0..count, generated)
}

/// The histories of a suite whose numbers these are: a suite that takes long is run
/// as parts, each a test of its own.
fn part(numbers: std::ops::Range<u64>, generated: fn(u64, u64) -> Vec<Event>) -> (u64, u64) {
    let (mut frozen, mut corrections) = (0, 0);
    for number in numbers {
        let events = generated(SEED, number);
        match beside(&events) {
            Ok(both) => {
                let cold = |one: &&fold::Standing| one.state() == State::Frozen;
                frozen += both.real.reservations().values().filter(cold).count() as u64;
                corrections += both.corrections;
            }
            Err(why) => panic!("seed {SEED:#x}, history {number}: {why}\n{events:#?}"),
        }
    }
    (frozen, corrections)
}

#[test]
fn every_generated_history_folds_to_what_the_model_gives_after_every_event() {
    let (frozen, corrections) = suite(1000, model::generated);
    assert!(frozen > 125, "{frozen}");
    assert!(corrections > 25, "{corrections}");
}

#[test]
fn every_generated_history_of_whole_books_then_other_hands_folds_to_what_the_model_gives() {
    let (frozen, _) = part(0..500, model::generated_tampered);
    assert!(frozen > 125, "{frozen}");
}

#[test]
fn every_generated_history_of_whole_books_then_other_hands_folds_so_in_the_second_half_too() {
    let (frozen, _) = part(500..1000, model::generated_tampered);
    assert!(frozen > 125, "{frozen}");
}

#[test]
fn every_generated_history_of_amounts_near_the_largest_folds_to_what_the_model_gives() {
    suite(250, model::generated_large);
}

#[test]
fn every_generated_history_of_turns_moved_folds_to_what_the_model_gives() {
    suite(400, model::generated_turned);
}

#[test]
fn every_generated_history_of_two_admissions_that_wait_folds_to_what_the_model_gives() {
    suite(250, model::generated_paired);
}

#[test]
fn every_generated_history_of_whole_books_folds_to_what_the_model_gives() {
    suite(500, model::generated_whole);
}

/// Names of every form that other hands could make in these books, each alone: for
/// each reservation there is and for a run that has none, at numbers the books have
/// and at numbers far outside them; and of the forms that belong to an epoch, in an
/// epoch there is and in one far ahead.
fn made_alone(books: &Books) -> Vec<Name> {
    let folded = books.fold();
    let newest = folded.epoch();
    let mut runs: Vec<Run> = folded.reservations.keys().copied().collect();
    runs.push(Run(9_999));
    let mut made = Vec::new();
    for (at, run) in runs.into_iter().enumerate() {
        let run_at = at as u64;
        for place in [1, 2, 40 + run_at] {
            made.push(Name::Place { run, place });
            made.push(Name::Settling { run, place });
            made.push(Name::Dispatched {
                run,
                place,
                by: run,
                micros: 1 + run_at,
            });
            made.push(Name::Settled {
                run,
                place,
                stage: model::Stage::Answered,
                micros: 3,
            });
        }
        for epoch in [0, newest, newest + 2] {
            made.push(Name::Held {
                epoch,
                run,
                micros: 7 + run_at,
            });
        }
        made.push(Name::Closed {
            run,
            released: true,
        });
        made.push(Name::Closed {
            run,
            released: false,
        });
        made.push(Name::Reservation(run));
        made.push(Name::Stray { run, count: 1 });
    }
    for epoch in [newest, newest + 1, newest + 3] {
        for turn in [1, folded.next_turn(epoch), 900] {
            made.push(Name::Turn { epoch, turn });
        }
        if epoch >= 1 {
            made.push(Name::Epoch(epoch));
            made.push(Name::Reset { epoch, micros: 50 });
        }
    }
    made.push(Name::Unplaced { count: 1 });
    made.retain(|name| !books.names().contains(name));
    made.sort();
    made.dedup();
    made
}

/// One name changed, alone, in each of the first `count` books the ledger alone wrote
/// from this seed: every name lost, every name damaged, and a sample of names made.
/// After each, the real fold of the names is held to the model's, as after every
/// event of a history. What comes back is how many changes were compared, and how
/// many reservations were frozen after them.
fn one_name_changed(seed: u64, count: u64) -> (u64, u64) {
    let (mut changes, mut frozen) = (0, 0);
    for number in 0..count {
        let events = model::generated_whole(seed, number);
        let said =
            |why: String| format!("seed {seed:#x}, whole books {number}: {why}\n{events:#?}");
        let mut whole = beside(&events).unwrap_or_else(|why| panic!("{}", said(why)));
        let mut hands = Vec::new();
        for name in whole.books.names() {
            hands.push(Event::Remove(name.clone()));
            hands.push(Event::Damage(name.clone()));
        }
        hands.extend(made_alone(&whole.books).into_iter().map(Event::Plant));
        for hand in hands {
            let mut changed = whole.books.clone();
            changed.apply(&hand);
            let names = whole.texts.names(&changed);
            let real = folded(&names);
            if let Err(why) = compare(&changed, &real) {
                panic!("{}", said(format!("{hand:?}: {why}")));
            }
            let cold = |one: &&fold::Standing| one.state() == State::Frozen;
            frozen += real.reservations().values().filter(cold).count() as u64;
            changes += 1;
        }
    }
    (changes, frozen)
}

/// The model's own suite of one name changed (500 books) is held by the model's
/// fold. This is a sample of it against the real fold: 24 books from a seed, which
/// give more than 2,500 changes from either seed (2,603 and 2,978, with 1,973 and
/// 2,308 reservations frozen after them), in a test for each of two seeds, of which
/// the second is written here.
fn a_sample_of_one_name_changed(seed: u64) {
    let (changes, frozen) = one_name_changed(seed, 24);
    assert!(changes > 2_500, "seed {seed:#x}: {changes} changes");
    assert!(frozen > 750, "seed {seed:#x}: {frozen} frozen");
}

#[test]
fn one_name_lost_damaged_or_made_alone_in_whole_books_folds_to_what_the_model_gives() {
    a_sample_of_one_name_changed(SEED);
}

#[test]
fn one_name_lost_damaged_or_made_alone_folds_to_what_the_model_gives_from_another_seed() {
    a_sample_of_one_name_changed(0x5EED_0002_2026_1115);
}

/// A store that is given the names of a history as they come.
struct Shelf {
    scratch: Scratch,
    store: Store,
    there: BTreeMap<String, bool>,
    stores: u32,
    damages: usize,
}

/// The executor's record of a reservation, as it is stored.
fn reservation(run: &str) -> Vec<u8> {
    format!(
        "{{\"record\":\"reservation/1\",\"run\":\"{run}\",\"request\":null,\
         \"admission\":\"sha256:{}\",\"reserved_micros\":0,\"start_ms\":0}}\n",
        "0".repeat(64)
    )
    .into_bytes()
}

/// The record the ledger writes under a name: what the name says, and beside it
/// what a name does not say, which the fold never reads.
fn record(name: &whizbang_core::spend::Name) -> Option<Record> {
    use whizbang_core::spend::Name as Real;
    Some(match name.clone() {
        Real::Reservation(_) => return None,
        Real::Held { epoch, run, micros } => Record::Hold {
            epoch,
            run,
            reserved: micros,
        },
        Real::Turn { epoch, turn } => Record::Turn {
            epoch,
            turn,
            by: TurnBy::Admission,
            run: None,
        },
        Real::Reset { epoch, micros } => Record::Reset {
            epoch,
            allowance: micros,
            at_ms: 7,
        },
        Real::Epoch(epoch) => Record::Epoch { epoch },
        Real::Place { run, place } => Record::Place {
            run,
            place,
            by: PlaceBy::Dispatch,
        },
        Real::Dispatched {
            run,
            place,
            by,
            micros,
        } => Record::Dispatch {
            run,
            place,
            by,
            request: 1,
            bound: micros,
            at_ms: 7,
        },
        Real::Settling { run, place } => Record::Settling { run, place },
        Real::Settled {
            run,
            place,
            stage,
            micros,
        } => Record::Settlement {
            run,
            place,
            stage,
            charged: micros,
            over_bound: false,
            at_ms: 7,
        },
        Real::Closed { run, released } => Record::Closing {
            run,
            released,
            by: ClosedBy::End,
            charged: Sum::ZERO,
            unresolved: Sum::ZERO,
            at_ms: 7,
        },
        Real::Adjusted {
            run,
            place,
            turn,
            seal,
            micros,
        } => Record::Adjustment {
            run,
            place,
            turn,
            seal,
            charged: micros,
            at_ms: 7,
        },
    })
}

/// The content of a name whose record can be read and says what the name says: the
/// record of its kind, written through the record's type. A text that is no name of
/// the ledger has no record, and a store reads nothing under it.
fn sound(name: &str) -> Vec<u8> {
    let Ok(parsed) = whizbang_core::spend::Name::parse(name) else {
        return b"no record\n".to_vec();
    };
    match record(&parsed) {
        Some(record) => {
            assert_eq!(record.name(), parsed, "{name}");
            record.to_bytes()
        }
        None => reservation(name.strip_prefix("spend/reservations/").unwrap()),
    }
}

/// A content that is damage under this name, of one of three kinds by turns: bytes
/// that are no record; a record of the right kind that says something else than the
/// name; and the record of another name.
fn damaged(name: &str, turn: usize) -> Vec<u8> {
    let other = match whizbang_core::spend::Name::parse(name) {
        Ok(whizbang_core::spend::Name::Place { run, place }) => Some(Record::Place {
            run,
            place: place.checked_add(1).unwrap_or(place),
            by: PlaceBy::Dispatch,
        }),
        Ok(whizbang_core::spend::Name::Held { epoch, run, micros }) => Some(Record::Hold {
            epoch: epoch + 1,
            run,
            reserved: micros,
        }),
        _ => None,
    };
    match (turn % 3, other) {
        (0, _) => b"not a record\n".to_vec(),
        (1, Some(other)) => other.to_bytes(),
        _ => sound("spend/turn/9/9"),
    }
}

impl Shelf {
    fn new() -> Shelf {
        let scratch = Scratch::new("fold");
        let store = Store::open(&scratch.path().join("0"), Arc::new(ShimFs::new())).unwrap();
        Shelf {
            scratch,
            store,
            there: BTreeMap::new(),
            stores: 0,
            damages: 0,
        }
    }

    /// Brings the store to these names. Names that are new are made in one
    /// publication, as the ledger makes them. Where a name is gone, or its content
    /// no longer reads, a store cannot have come there by a publication: the names
    /// are written into a new one, the damaged ones with content that is not theirs.
    fn hold(&mut self, names: &BTreeMap<String, bool>) {
        let kept = |(name, hurt): (&String, &bool)| names.get(name) == Some(hurt);
        if !self.there.iter().all(kept) {
            self.stores += 1;
            let root = self.scratch.path().join(self.stores.to_string());
            self.store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
            self.there.clear();
        }
        let new: Vec<(&String, &bool)> = names
            .iter()
            .filter(|(name, _)| !self.there.contains_key(*name))
            .collect();
        if new.is_empty() {
            return;
        }
        let mut publication = self.store.begin().unwrap();
        for (name, hurt) in new {
            let bytes = match hurt {
                true => {
                    self.damages += 1;
                    damaged(name, self.damages)
                }
                false => sound(name),
            };
            publication.create(name, Kind::Diagnostic, &bytes).unwrap();
        }
        publication.publish().unwrap();
        self.there = names.clone();
    }

    fn fold(&self) -> Folded {
        let generation = self.store.snapshot().unwrap();
        fold::fold(&generation, |entry| self.store.read(entry))
    }
}

/// What a history must give, asked of the real fold. None where it is of what a
/// process holds in memory or of what became of an event, which no fold answers.
fn holds(must: &Must, real: &Folded, books: &Books) -> Option<Result<(), String>> {
    let balance = plain(real.balance());
    let of = |run: &Run| {
        let one = real.reservation(&id(*run)).map(standing);
        one.ok_or(format!("no reservation of {run}"))
    };
    let same = |what: &str, found: u128, wanted: u64| match found == u128::from(wanted) {
        true => Ok(()),
        false => Err(format!("{what}: {found}, and it must be {wanted}")),
    };
    let said = |what: String, fits: bool| match fits {
        true => Ok(()),
        false => Err(what),
    };
    let by = |run: &Run| {
        let root = books.root_of(*run).unwrap_or(*run);
        sent(real.sent_by(&id(*run), &id(root)))
    };
    Some(match must {
        Must::Epoch(epoch) => same("the epoch", balance.epoch.into(), *epoch),
        Must::Allowed(micros) => same("allowed", balance.allowed.into(), *micros),
        Must::Counted(micros) => match balance.unknown {
            false => same("counted", balance.counted, *micros),
            true => Err("the books cannot say how much is counted".to_owned()),
        },
        Must::CountedAtLeast(micros) => match balance.unknown {
            true => same("counted, at least", balance.counted, *micros),
            false => Err("the books say how much is counted".to_owned()),
        },
        Must::Left(micros) => same("left", balance.left.into(), *micros),
        Must::Debt(micros) => same("the debt", balance.debt, *micros),
        Must::Counts(run, micros) => of(run).and_then(|one| match one.counted() {
            model::Counted::Exactly(found) => same(&format!("{run} counts"), found, *micros),
            found => Err(format!("{run} counts {found:?}, and it must be known")),
        }),
        Must::CountsAtLeast(run, micros) => of(run).and_then(|one| match one.counted() {
            model::Counted::AtLeast(found) => {
                same(&format!("{run} counts at least"), found, *micros)
            }
            found => Err(format!("{run} counts {found:?}, and it must be unknown")),
        }),
        Must::Saturated(run) => of(run).and_then(|one| {
            let stopped = real
                .reservation(&id(*run))
                .map(|one| one.counts().stopped());
            said(
                format!("{run} counts {}", one.counts),
                stopped == Some(true),
            )
        }),
        Must::State(run, state) => {
            of(run).and_then(|one| said(format!("{run} is {:?}", one.state), one.state == *state))
        }
        Must::EpochOf(run, epoch) => of(run).and_then(|one| {
            said(
                format!("{run} is of {:?}", one.epoch),
                one.epoch == Some(*epoch),
            )
        }),
        Must::Charged(run, micros) => {
            of(run).and_then(|one| same(&format!("{run} charged"), one.charged, *micros))
        }
        Must::Outstanding(run, micros) => {
            of(run).and_then(|one| same(&format!("{run} outstanding"), one.outstanding, *micros))
        }
        Must::Unresolved(run, micros) => {
            of(run).and_then(|one| same(&format!("{run} unresolved"), one.unresolved, *micros))
        }
        Must::Absent(run) => said(
            format!("{run} has a reservation"),
            real.reservation(&id(*run)).is_none(),
        ),
        Must::Sent(run, micros) => match by(run) {
            model::Sent::Known(spend) => same(&format!("{run} sent"), spend.known(), *micros),
            found => Err(format!("{run} sent {found:?}, and it must be known")),
        },
        Must::SentAtMost(run, micros) => match by(run) {
            model::Sent::Unknown { at_most } => {
                same(&format!("{run} sent at most"), at_most, *micros)
            }
            found => Err(format!("{run} sent {found:?}, and it must be unknown")),
        },
        Must::SentUnbounded(run) => {
            said(format!("{run}: a most"), by(run) == model::Sent::Unbounded)
        }
        Must::NeedsOwner => said("the store needs nobody".to_owned(), balance.needs_owner),
        Must::Closed(run) => of(run).and_then(|one| said(format!("{run} is open"), one.closed)),
        Must::Unplaced(count) => same("placed nowhere", real.unplaced().into(), *count),
        Must::Spendable(..)
        | Must::Owes(..)
        | Must::Outcome(..)
        | Must::SameNamesAsAfter(_)
        | Must::NothingExecutes
        | Must::TreeLeft(..) => return None,
    })
}

/// One named history, written into a store event by event and folded from it. What
/// it returns is how many of the things it must give the fold was asked for.
fn through_a_store(history: &model::History) -> Result<u64, String> {
    let (mut both, mut shelf) = (Beside::new(), Shelf::new());
    let (mut asked, mut wrong) = (0, Vec::new());
    for (at, event) in history.events.iter().enumerate() {
        let said = |why: String| format!("{}: event {at}, {event:?}: {why}", history.name);
        both.step(event).map_err(said)?;
        shelf.hold(&both.names);
        let real = shelf.fold();
        compare(&both.books, &real).map_err(said)?;
        if real != both.real {
            return Err(said(
                "the store's fold is not the fold of its names".to_owned(),
            ));
        }
        let last = at + 1 == history.events.len();
        let earlier = history.earlier.iter().filter(|(when, _)| *when == at);
        let now = earlier.map(|(_, must)| must);
        for must in now.chain(history.must.iter().filter(|_| last)) {
            if let Some(found) = holds(must, &real, &both.books) {
                asked += 1;
                wrong.extend(found.err().map(|why| format!("after event {at}: {why}")));
            }
        }
    }
    match wrong.is_empty() {
        true => Ok(asked),
        false => Err(format!("{}: {}", history.name, wrong.join("; "))),
    }
}

#[test]
fn every_named_history_written_into_a_store_folds_to_what_the_model_gives_and_to_what_it_must() {
    let table = model::histories();
    assert!(table.len() >= 89, "{}", table.len());
    let mut asked = 0;
    let mut broken = Vec::new();
    for history in &table {
        match through_a_store(history) {
            Ok(count) => asked += count,
            Err(why) => broken.push(why),
        }
    }
    assert!(broken.is_empty(), "{}", broken.join("\n"));
    // What the histories must give is, for the most part, the fold's to answer.
    assert!(asked > 400, "{asked}");
}

/// These names, none damaged, folded.
fn of(names: &[&str]) -> Folded {
    with_damage(names, &[])
}

/// These names folded, with the content of some of them damaged.
fn with_damage(names: &[&str], damaged: &[&str]) -> Folded {
    let names: BTreeMap<String, bool> = names
        .iter()
        .map(|name| ((*name).to_owned(), damaged.contains(name)))
        .collect();
    assert!(damaged.iter().all(|name| names.contains_key(*name)));
    folded(&names)
}

fn one() -> RunId {
    RunId::parse("7000-1").unwrap()
}

fn two() -> RunId {
    RunId::parse("7000-2").unwrap()
}

/// An allowance of 30 in the first epoch, and a reservation of 10 under it for the
/// first run: what a reset and an admission publish.
const ADMITTED: [&str; 6] = [
    "spend/reset/1/30",
    "spend/epoch/1",
    "spend/turn/0/1",
    "spend/reservations/7000-1",
    "spend/held/1/7000-1/10",
    "spend/turn/1/1",
];

/// Two requests of 5 dispatched under it.
const TWO_SENT: [&str; 4] = [
    "spend/place/7000-1/1",
    "spend/dispatched/7000-1/1/7000-1/5",
    "spend/place/7000-1/2",
    "spend/dispatched/7000-1/2/7000-1/5",
];

/// The first of them settled at 12, over its bound: it took a place and a turn.
const FIRST_AT_12: [&str; 4] = [
    "spend/settling/7000-1/1",
    "spend/settled/7000-1/1/answered/12",
    "spend/place/7000-1/3",
    "spend/turn/1/2",
];

/// The second settled at nothing.
const SECOND_AT_0: [&str; 3] = [
    "spend/settling/7000-1/2",
    "spend/settled/7000-1/2/answered/0",
    "spend/turn/1/3",
];

#[test]
fn an_admission_stands_under_each_of_its_three_conditions_and_only_for_the_amount_its_hold_states()
{
    // A reservation of 10 that is all of an allowance of 10, as its admission's
    // publication left it: the record, one hold, the epoch's first turn.
    const OWN: [&str; 6] = [
        "spend/reset/1/10",
        "spend/epoch/1",
        "spend/turn/0/1",
        "spend/reservations/7000-1",
        "spend/held/1/7000-1/10",
        "spend/turn/1/1",
    ];
    // Another admission in the same epoch, of 1, which other hands made whole.
    const OTHER: [&str; 3] = [
        "spend/reservations/7000-2",
        "spend/held/1/7000-2/1",
        "spend/turn/1/2",
    ];
    let ten = amount(10);
    let stands = |more: &[&str]| of(&joined(&[&OWN, more])).stands(&one(), ten);
    assert!(stands(&[]));

    // The first condition: the record and one hold, and nothing else that names the
    // run; and open.
    assert!(!stands(&["spend/place/7000-1/1"]));
    assert!(!stands(&[
        "spend/place/7000-1/1",
        "spend/dispatched/7000-1/1/7000-1/5"
    ]));
    assert!(!stands(&["spend/held/1/7000-1/3"]));
    assert!(!stands(&[
        "spend/closed/7000-1/released",
        "spend/place/7000-1/1",
        "spend/turn/1/2"
    ]));
    let without = |gone: &str| {
        let left: Vec<&str> = OWN.iter().copied().filter(|name| *name != gone).collect();
        of(&left).stands(&one(), ten)
    };
    assert!(!without("spend/reservations/7000-1"));
    assert!(!without("spend/held/1/7000-1/10"));
    let hurt = with_damage(&OWN, &["spend/held/1/7000-1/10"]);
    assert!(!hurt.stands(&one(), ten));
    // A run the books do not name has no admission that stands.
    assert!(!of(&OWN).stands(&two(), ten));

    // The second: neither the epoch it fell under nor the newest needs its owner.
    assert!(!stands(&["spend/turn/1/5"]));
    assert!(!without("spend/turn/1/1"));
    const RESET: [&str; 3] = ["spend/reset/2/50", "spend/epoch/2", "spend/turn/1/2"];
    assert!(stands(&RESET));
    assert!(!stands(&joined(&[&RESET, &["spend/turn/2/3"]])));
    // A frozen reservation whose amount no name states makes every epoch need it.
    assert!(!stands(&["spend/place/7000-9/1"]));
    // One whose hold states its amount, and that the allowance covers, does not.
    let beside = [
        "spend/reset/1/12",
        "spend/epoch/1",
        "spend/turn/0/1",
        "spend/reservations/7000-1",
        "spend/held/1/7000-1/10",
        "spend/turn/1/1",
        "spend/held/1/7000-9/2",
        "spend/turn/1/2",
    ];
    assert!(of(&beside).stands(&one(), ten));

    // The third: its own epoch is not over its allowance, with it counted in full.
    // That is of the epoch its hold names, whatever the newest has left.
    assert!(!stands(&OTHER));
    assert!(!stands(&joined(&[
        &OTHER,
        &["spend/reset/2/50", "spend/epoch/2", "spend/turn/1/3"]
    ])));
    assert!(stands(&[
        "spend/reset/2/0",
        "spend/epoch/2",
        "spend/turn/1/2"
    ]));

    // A call of nothing adds nothing to what is counted, and is not held to a debt
    // that was there before it.
    let nothing = [
        "spend/reset/1/10",
        "spend/epoch/1",
        "spend/turn/0/1",
        "spend/reservations/7000-1",
        "spend/held/1/7000-1/0",
        "spend/turn/1/1",
        "spend/reservations/7000-2",
        "spend/held/1/7000-2/11",
        "spend/turn/1/2",
    ];
    let in_debt = of(&nothing);
    assert_eq!(in_debt.balance_of(1).debt, Sum::new(1));
    assert!(in_debt.stands(&one(), Amount::ZERO));
    assert!(!in_debt.stands(&two(), amount(11)));

    // It stands only for the amount its hold states. Asked of nothing, where the
    // hold states 10 and the epoch is in debt, it does not: no caller is let past
    // the debt by the amount it passes. Nor of any other amount.
    let over = of(&joined(&[&OWN, &OTHER]));
    assert!(!over.stands(&one(), Amount::ZERO));
    assert!(!of(&OWN).stands(&one(), Amount::ZERO));
    assert!(!of(&OWN).stands(&one(), amount(9)));
    assert!(!of(&OWN).stands(&one(), amount(11)));
}

fn joined<'a>(parts: &[&[&'a str]]) -> Vec<&'a str> {
    parts.iter().flat_map(|part| part.iter().copied()).collect()
}

fn state_and_count(folded: &Folded, run: &RunId) -> (State, model::Counted) {
    let one = &folded.reservations()[run];
    (one.state(), counted(one.counted()))
}

/// One reservation of the real fold, in plain numbers.
fn res(folded: &Folded, run: &RunId) -> model::Standing {
    standing(&folded.reservations()[run])
}

/// What a dispatch under a reservation may still take.
fn left_of(folded: &Folded, run: &RunId) -> u64 {
    folded.reservations()[run].left().micros()
}

fn state_of(folded: &Folded, run: &RunId) -> State {
    folded.reservations()[run].state()
}

/// The balance of the newest epoch, in plain numbers.
fn newest(folded: &Folded) -> model::Balance {
    plain(folded.balance())
}

/// What the fold says of a correction to an amount: why it is not taken, or None.
fn correct(folded: &Folded, run: &RunId, micros: u64) -> Option<Why> {
    refusal(folded.correction(run, amount(micros)))
}

fn sent_by(folded: &Folded, run: &RunId, root: &RunId) -> model::Sent {
    sent(folded.sent_by(run, root))
}

#[test]
fn reserve_10_dispatch_two_of_5_and_one_settles_at_12_counts_17_then_12_or_17() {
    // While the second is not settled, an admission sees 17 counted: the charge of
    // 12, and the bound of 5 that is outstanding.
    let between = of(&joined(&[&ADMITTED, &TWO_SENT, &FIRST_AT_12]));
    assert_eq!(
        state_and_count(&between, &one()),
        (State::Open, model::Counted::Exactly(17))
    );
    let balance = newest(&between);
    assert_eq!(
        (balance.allowed, balance.counted, balance.left),
        (30, 17, 13)
    );
    assert_eq!(
        (balance.charged, balance.held, balance.unresolved),
        (12, 5, 5)
    );
    assert!(!balance.needs_owner);
    // The run ends by itself once the second is settled at nothing: closed at 12,
    // which is over what it reserved, and nothing is released beyond that.
    let closing = [
        "spend/closed/7000-1/released",
        "spend/place/7000-1/4",
        "spend/turn/1/4",
    ];
    let ended = of(&joined(&[
        &ADMITTED,
        &TWO_SENT,
        &FIRST_AT_12,
        &SECOND_AT_0,
        &closing,
    ]));
    assert_eq!(
        state_and_count(&ended, &one()),
        (State::Released, model::Counted::Exactly(12))
    );
    assert_eq!(newest(&ended).left, 18);
    // Interrupted with the second not settled: 17, and nothing is released.
    let recovered = [
        "spend/closed/7000-1/held",
        "spend/place/7000-1/4",
        "spend/turn/1/3",
    ];
    let interrupted = of(&joined(&[&ADMITTED, &TWO_SENT, &FIRST_AT_12, &recovered]));
    assert_eq!(
        state_and_count(&interrupted, &one()),
        (State::Interrupted, model::Counted::Exactly(17))
    );
    assert_eq!(left_of(&interrupted, &one()), 0);
}

#[test]
fn an_allowance_driven_below_nothing_by_an_overrun_reads_nothing_left_and_keeps_the_debt() {
    let mut names = joined(&[&ADMITTED, &TWO_SENT, &FIRST_AT_12, &SECOND_AT_0]);
    names.retain(|name| *name != "spend/reset/1/30");
    names.push("spend/reset/1/10");
    let balance = newest(&of(&names));
    assert_eq!((balance.allowed, balance.counted), (10, 12));
    assert_eq!((balance.left, balance.debt), (0, 2));
}

#[test]
fn a_request_settled_over_its_bound_under_an_open_reservation_counts_the_amount_while_it_covers() {
    // Reserve 10, dispatch 5, settle at 6, the run still open: 10, not 6.
    let names = joined(&[
        &ADMITTED,
        &[
            "spend/place/7000-1/1",
            "spend/dispatched/7000-1/1/7000-1/5",
            "spend/settling/7000-1/1",
            "spend/settled/7000-1/1/answered/6",
            "spend/place/7000-1/2",
            "spend/turn/1/2",
        ],
    ]);
    let folded = of(&names);
    assert_eq!(
        state_and_count(&folded, &one()),
        (State::Open, model::Counted::Exactly(10))
    );
    assert_eq!(left_of(&folded, &one()), 4);
}

#[test]
fn a_reset_closes_the_books_before_it() {
    // A reservation of 10 under the first epoch, then a reset to 20: the new epoch
    // counts nothing of it, and the first still counts it.
    let names = joined(&[
        &ADMITTED,
        &["spend/reset/2/20", "spend/epoch/2", "spend/turn/1/2"],
    ]);
    let folded = of(&names);
    assert_eq!(folded.epoch(), 2);
    let (new, old) = (newest(&folded), plain(folded.balance_of(1)));
    assert_eq!((new.allowed, new.counted, new.left), (20, 0, 20));
    assert_eq!((old.allowed, old.counted, old.left), (30, 10, 20));
    assert_eq!(res(&folded, &one()).epoch, Some(1));
    assert!(!new.needs_owner && !old.needs_owner);
}

#[test]
fn dispatches_by_a_child_are_under_its_roots_reservation_and_named_for_the_child() {
    let names = joined(&[
        &ADMITTED,
        &[
            "spend/place/7000-1/1",
            "spend/dispatched/7000-1/1/7000-2/4",
            "spend/place/7000-1/2",
            "spend/dispatched/7000-1/2/7000-1/3",
            "spend/settling/7000-1/1",
            "spend/settled/7000-1/1/unresolved/4",
            "spend/turn/1/2",
        ],
    ]);
    let folded = of(&names);
    // The child has no reservation: the tree has one, at its root.
    assert!(!folded.reservations().contains_key(&two()));
    let root = res(&folded, &one());
    assert_eq!((root.charged, root.outstanding, root.unresolved), (4, 3, 7));
    let model::Sent::Known(child) = sent_by(&folded, &two(), &one()) else {
        panic!("the child's spend is known");
    };
    assert_eq!(
        (child.charged, child.outstanding, child.unresolved),
        (4, 0, 4)
    );
    let model::Sent::Known(parent) = sent_by(&folded, &one(), &one()) else {
        panic!("the root's spend is known");
    };
    assert_eq!((parent.charged, parent.outstanding), (0, 3));
}

/// A reservation of 10 that recovery closed with nothing dispatched, and a second
/// run's reservation of 5 beside it.
const TWO_RESERVED: [&str; 12] = [
    "spend/reset/1/30",
    "spend/epoch/1",
    "spend/turn/0/1",
    "spend/reservations/7000-1",
    "spend/held/1/7000-1/10",
    "spend/turn/1/1",
    "spend/closed/7000-1/released",
    "spend/place/7000-1/1",
    "spend/turn/1/2",
    "spend/reservations/7000-2",
    "spend/held/1/7000-2/5",
    "spend/turn/1/3",
];

#[test]
fn a_reservation_whose_content_is_damaged_is_frozen_at_20_and_stops_nothing_else() {
    // Whole, it released everything. With its record damaged it is frozen at the
    // most its names can mean: its amount, and its amount once more for the place
    // its closing took. The second reservation is as it was, and the rest of the
    // allowance can be reserved: the store needs nobody.
    let whole = of(&TWO_RESERVED);
    assert_eq!(
        state_and_count(&whole, &one()),
        (State::Released, model::Counted::Exactly(0))
    );
    let frozen = with_damage(&TWO_RESERVED, &["spend/reservations/7000-1"]);
    assert_eq!(
        state_and_count(&frozen, &one()),
        (State::Frozen, model::Counted::Exactly(20))
    );
    assert_eq!(res(&frozen, &two()), res(&whole, &two()));
    let balance = newest(&frozen);
    assert_eq!((balance.counted, balance.left), (25, 5));
    assert!(!balance.needs_owner && !balance.unknown);
    // It counts so in every epoch there is: a reset does not clear it.
    let mut later = TWO_RESERVED.to_vec();
    later.extend(["spend/reset/2/40", "spend/epoch/2", "spend/turn/1/4"]);
    let after = with_damage(&later, &["spend/reservations/7000-1"]);
    assert_eq!((newest(&after).counted, newest(&after).left), (20, 20));
}

#[test]
fn a_reservation_damaged_after_it_released_at_3_counts_27() {
    // Reserve 10, two requests of 2 settled at 1 and 2, closed with the rest
    // released; then its record is damaged. The amount, the two bounds, the two
    // charges, and the amount once more for the place its closing took.
    let names = joined(&[
        &ADMITTED,
        &[
            "spend/place/7000-1/1",
            "spend/dispatched/7000-1/1/7000-1/2",
            "spend/settling/7000-1/1",
            "spend/settled/7000-1/1/answered/1",
            "spend/turn/1/2",
            "spend/place/7000-1/2",
            "spend/dispatched/7000-1/2/7000-1/2",
            "spend/settling/7000-1/2",
            "spend/settled/7000-1/2/answered/2",
            "spend/turn/1/3",
            "spend/closed/7000-1/released",
            "spend/place/7000-1/3",
            "spend/turn/1/4",
        ],
    ]);
    assert_eq!(
        state_and_count(&of(&names), &one()),
        (State::Released, model::Counted::Exactly(3))
    );
    let frozen = with_damage(&names, &["spend/reservations/7000-1"]);
    assert_eq!(
        state_and_count(&frozen, &one()),
        (State::Frozen, model::Counted::Exactly(27))
    );
    assert_eq!(newest(&frozen).left, 3);
}

#[test]
fn a_record_the_fold_cannot_place_freezes_the_reservation_its_name_gives_and_no_other() {
    let whole = of(&TWO_RESERVED);
    // A settlement that names no dispatch.
    let mut unplaced = TWO_RESERVED.to_vec();
    unplaced.push("spend/settled/7000-2/1/answered/3");
    let folded = of(&unplaced);
    assert_eq!(state_of(&folded, &two()), State::Frozen);
    assert_eq!(res(&folded, &one()), res(&whole, &one()));
    // Two dispatches at one place: each may be a request, so both bounds count.
    let mut twice = TWO_RESERVED.to_vec();
    twice.extend([
        "spend/place/7000-2/1",
        "spend/dispatched/7000-2/1/7000-2/2",
        "spend/dispatched/7000-2/1/7000-2/3",
    ]);
    let folded = of(&twice);
    assert_eq!(
        state_and_count(&folded, &two()),
        (State::Frozen, model::Counted::Exactly(10))
    );
    assert_eq!(res(&folded, &one()), res(&whole, &one()));
    assert!(!newest(&folded).needs_owner);
    // What either run sent under it is unknown, and at most what it counts for.
    assert_eq!(
        sent_by(&folded, &two(), &two()),
        model::Sent::Unknown { at_most: 10 }
    );
}

/// The correction of a reservation to an amount, at this place and turn, sealed
/// over these names of it, none damaged.
fn correction(run: &str, place: u64, turn: u64, micros: u64, over: &[&str]) -> String {
    let mut over: Vec<&str> = over
        .iter()
        .copied()
        .filter(|name| name.contains(run) && *name != format!("spend/place/{run}/{place}"))
        .collect();
    over.sort_unstable();
    let seal = Seal::over(over.into_iter().map(|name| (name, false)));
    format!("spend/adjusted/{run}/{place}/{turn}/{seal}/{micros}")
}

/// A reservation of 10 that recovery closed as interrupted, with 5 outstanding and
/// a request of 3 settled at 2.
fn interrupted() -> Vec<&'static str> {
    joined(&[
        &ADMITTED,
        &[
            "spend/place/7000-1/1",
            "spend/dispatched/7000-1/1/7000-1/3",
            "spend/settling/7000-1/1",
            "spend/settled/7000-1/1/answered/2",
            "spend/turn/1/2",
            "spend/place/7000-1/2",
            "spend/dispatched/7000-1/2/7000-1/5",
            "spend/closed/7000-1/held",
            "spend/place/7000-1/3",
            "spend/turn/1/3",
        ],
    ])
}

#[test]
fn an_adjustment_after_an_interruption_and_after_a_freeze_counts_what_it_says() {
    let names = interrupted();
    assert_eq!(
        state_and_count(&of(&names), &one()),
        (State::Interrupted, model::Counted::Exactly(10))
    );
    // Corrected to 4, at the next place and the next turn, over its names.
    let adjusted = correction("7000-1", 4, 4, 4, &names);
    let made = [adjusted.as_str(), "spend/place/7000-1/4", "spend/turn/1/4"];
    let corrected = joined(&[&names, &made]);
    let folded = of(&corrected);
    assert_eq!(
        state_and_count(&folded, &one()),
        (State::Adjusted, model::Counted::Exactly(4))
    );
    assert_eq!(newest(&folded).left, 26);
    assert!(!newest(&folded).needs_owner);
    // Frozen by a damaged record, it counts 10 + 3 + 2 + 5 + 10; the correction no
    // longer stands, because a name it was sealed over changed. Its owner corrects
    // it again, over the names as they are, damage and all.
    let frozen = with_damage(&corrected, &["spend/reservations/7000-1"]);
    assert_eq!(
        state_and_count(&frozen, &one()),
        (
            State::Frozen,
            model::Counted::Exactly(10 + 3 + 2 + 5 + 2 * 10)
        )
    );
    let over: Vec<(&str, bool)> = corrected
        .iter()
        .filter(|name| name.contains("7000-1"))
        .map(|name| (*name, *name == "spend/reservations/7000-1"))
        .collect();
    let seal = Seal::over(over);
    let again = format!("spend/adjusted/7000-1/5/5/{seal}/0");
    let made = [again.as_str(), "spend/place/7000-1/5", "spend/turn/1/5"];
    let ended = with_damage(
        &joined(&[&corrected, &made]),
        &["spend/reservations/7000-1"],
    );
    // Never under what its settlement with its whole chain says: 2.
    assert_eq!(
        state_and_count(&ended, &one()),
        (State::Adjusted, model::Counted::Exactly(2))
    );
}

#[test]
fn a_correction_counts_no_less_than_what_whole_settlements_say_and_is_refused_below_them() {
    // The floor, in the fold: a correction that says nothing, whoever wrote it,
    // counts for the 2 that the settlement with its whole chain says.
    let names = interrupted();
    let forged = correction("7000-1", 4, 4, 0, &names);
    let made = [forged.as_str(), "spend/place/7000-1/4", "spend/turn/1/4"];
    let folded = of(&joined(&[&names, &made]));
    assert_eq!(
        state_and_count(&folded, &one()),
        (State::Adjusted, model::Counted::Exactly(2))
    );
    // And in what the command is told: from what is settled, to the most.
    let before = of(&names);
    assert_eq!(
        correct(&before, &one(), 1),
        Some(Why::BelowSettled { settled: 2 })
    );
    assert_eq!(correct(&before, &one(), 2), None);
    assert_eq!(correct(&before, &one(), 10), None);
    assert_eq!(
        correct(&before, &one(), 11),
        Some(Why::OverCount { most: 10 })
    );
    // Only of a reservation that is closed, and not of one that released.
    let open = of(&ADMITTED);
    assert_eq!(correct(&open, &one(), 5), Some(Why::NotCorrectable));
    assert_eq!(correct(&open, &two(), 5), Some(Why::NotCorrectable));
    let released = of(&TWO_RESERVED);
    assert_eq!(correct(&released, &one(), 0), Some(Why::NotCorrectable));
}

#[test]
fn a_correction_upward_is_taken_only_when_what_is_left_can_hold_the_difference() {
    // An allowance of 12: the interrupted reservation is corrected to 4, and a
    // second run then reserves 7 of the 8 that are left.
    let mut names = interrupted();
    names.retain(|name| *name != "spend/reset/1/30");
    names.push("spend/reset/1/12");
    let adjusted = correction("7000-1", 4, 4, 4, &names);
    names.extend([
        "spend/place/7000-1/4",
        "spend/turn/1/4",
        "spend/reservations/7000-2",
        "spend/held/1/7000-2/7",
        "spend/turn/1/5",
    ]);
    names.push(adjusted.as_str());
    let folded = of(&names);
    assert_eq!((newest(&folded).counted, newest(&folded).left), (11, 1));
    // Up by 1 is held by the 1 that is left; up by 2 is not.
    assert_eq!(correct(&folded, &one(), 5), None);
    assert_eq!(
        correct(&folded, &one(), 6),
        Some(Why::CannotBeHeld { left: 1 })
    );
    // Downward needs no room.
    assert_eq!(correct(&folded, &one(), 2), None);
    // After a reset the reservation is of the closed books: a correction upward
    // changes nothing the new allowance can spend, and needs no room there.
    names.extend(["spend/reset/2/0", "spend/epoch/2", "spend/turn/1/6"]);
    let later = of(&names);
    assert_eq!(newest(&later).left, 0);
    assert_eq!(correct(&later, &one(), 10), None);
}

#[test]
fn the_next_turn_is_the_lowest_that_is_free_and_never_one_past_the_highest() {
    let folded = of(&ADMITTED);
    assert_eq!((folded.next_turn(1), folded.next_turn(0)), (2, 2));
    assert_eq!(folded.next_turn(7), 1);
    // A publication under a reservation takes its turn in the epoch its hold
    // names: in each, where other hands gave it more than one hold, and in the
    // newest where it has none.
    assert_eq!(folded.turn_epochs(&one()), [1]);
    assert_eq!(folded.turn_epochs(&two()), [1]);
    let mut held = ADMITTED.to_vec();
    held.extend([
        "spend/reset/2/5",
        "spend/epoch/2",
        "spend/turn/1/2",
        "spend/held/2/7000-1/3",
        "spend/place/7000-2/1",
    ]);
    let later = of(&held);
    assert_eq!(later.turn_epochs(&one()), [1, 2]);
    assert_eq!(later.turn_epochs(&two()), [2]);
    // A turn other hands made far ahead does not move the next one: a judgement
    // made before a settlement still makes the turn the settlement took.
    let mut ahead = ADMITTED.to_vec();
    ahead.extend(["spend/turn/1/100", "spend/turn/1/18446744073709551615"]);
    let folded = of(&ahead);
    assert_eq!(folded.next_turn(1), 2);
    assert!(folded.needs_owner(1));
    // A turn that is lost is the next again.
    let mut lost = ADMITTED.to_vec();
    lost.retain(|name| *name != "spend/turn/1/1");
    lost.extend(["spend/turn/1/2", "spend/turn/1/3"]);
    let folded = of(&lost);
    assert_eq!(folded.next_turn(1), 1);
    assert!(folded.needs_owner(1));
    assert_eq!(newest(&folded).left, 0);
}

#[test]
fn an_answer_that_no_name_states_is_not_in_the_fold_which_counts_the_bound_until_it_is_settled() {
    // A request of 5 was answered at 12 and is not settled: no name says 12. The
    // fold counts the reservation in full and reads 5 as left of it, which is why a
    // dispatch is also held to what its process holds of the tree (ADR 0016, "An
    // allowance holds a signed balance"): that is in memory, and not the fold's.
    let names = joined(&[
        &ADMITTED,
        &["spend/place/7000-1/1", "spend/dispatched/7000-1/1/7000-1/5"],
    ]);
    let before = of(&names);
    assert_eq!(
        state_and_count(&before, &one()),
        (State::Open, model::Counted::Exactly(10))
    );
    assert_eq!(left_of(&before, &one()), 5);
    // Once its settlement is durable the charge is counted, and nothing is left.
    let settled = [
        "spend/settling/7000-1/1",
        "spend/settled/7000-1/1/answered/12",
        "spend/place/7000-1/2",
        "spend/turn/1/2",
    ];
    let after = of(&joined(&[&names, &settled]));
    assert_eq!(
        state_and_count(&after, &one()),
        (State::Open, model::Counted::Exactly(12))
    );
    assert_eq!(left_of(&after, &one()), 0);
}

#[test]
fn a_name_can_name_any_place_and_state_any_amount_and_the_fold_still_ends() {
    // A place at the largest number there is, and a hold of the largest amount, for
    // each of two reservations: the places that hold no dispatch are counted and not
    // walked, and the store's sum stops at the largest number.
    let most = u64::MAX - 1;
    let mut names: Vec<String> = ADMITTED.iter().map(|name| (*name).to_owned()).collect();
    names.extend([
        "spend/reservations/7000-2".to_owned(),
        "spend/held/1/7000-2/10".to_owned(),
        "spend/turn/1/2".to_owned(),
    ]);
    for run in ["7000-1", "7000-2"] {
        names.push(format!("spend/place/{run}/{}", u64::MAX));
        names.push(format!("spend/held/1/{run}/{most}"));
    }
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let folded = of(&names);
    let first = res(&folded, &one());
    assert_eq!(state_of(&folded, &one()), State::Frozen);
    assert_eq!(first.unexplained, u64::MAX);
    let places = u128::from(u64::MAX) * u128::from(most);
    assert_eq!(
        first.counted(),
        model::Counted::AtLeast(u128::from(most) + places)
    );
    let balance = newest(&folded);
    assert_eq!((balance.counted, balance.left), (u128::MAX, 0));
    assert!(balance.needs_owner && balance.unknown);
    // A turn at the largest number is read without counting up to it.
    let mut far = ADMITTED.to_vec();
    far.push("spend/turn/1/18446744073709551615");
    assert_eq!(of(&far).next_turn(1), 2);
}

#[test]
fn sums_of_amounts_one_under_the_largest_do_not_wrap() {
    // A reservation of one under the largest number, with three requests each
    // settled at one under the largest: what is charged is three times that, in
    // 128 bits, and is over any allowance.
    let most = u64::MAX - 1;
    let mut names = vec![
        format!("spend/reset/1/{most}"),
        "spend/epoch/1".to_owned(),
        "spend/turn/0/1".to_owned(),
        "spend/reservations/7000-1".to_owned(),
        format!("spend/held/1/7000-1/{most}"),
        "spend/turn/1/1".to_owned(),
    ];
    for place in 1..=3u64 {
        names.push(format!("spend/place/7000-1/{}", 2 * place - 1));
        names.push(format!(
            "spend/dispatched/7000-1/{}/7000-1/0",
            2 * place - 1
        ));
        names.push(format!("spend/settling/7000-1/{}", 2 * place - 1));
        names.push(format!(
            "spend/settled/7000-1/{}/answered/{most}",
            2 * place - 1
        ));
        names.push(format!("spend/place/7000-1/{}", 2 * place));
        names.push(format!("spend/turn/1/{}", place + 1));
    }
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let folded = of(&names);
    let own = res(&folded, &one());
    assert_eq!(state_of(&folded, &one()), State::Open);
    assert_eq!(own.charged, 3 * u128::from(most));
    assert_eq!(own.counted(), model::Counted::Exactly(3 * u128::from(most)));
    let balance = newest(&folded);
    assert_eq!((balance.left, balance.debt), (0, 2 * u128::from(most)));
    assert!(!balance.needs_owner);
    let model::Sent::Known(sent) = sent_by(&folded, &one(), &one()) else {
        panic!("what it sent is known");
    };
    assert_eq!(sent.known(), 3 * u128::from(most));
}

#[test]
fn the_largest_number_is_no_amount_and_a_name_that_states_it_is_of_no_form() {
    let largest = u64::MAX;
    // As a hold's amount: the name is not a hold, so the reservation has none, and
    // is frozen with an amount that is unknown.
    let hold = format!("spend/held/1/7000-1/{largest}");
    let mut names = ADMITTED.to_vec();
    names.retain(|name| *name != "spend/held/1/7000-1/10");
    names.push(hold.as_str());
    let folded = of(&names);
    assert_eq!(
        state_and_count(&folded, &one()),
        (State::Frozen, model::Counted::AtLeast(0))
    );
    assert!(newest(&folded).needs_owner);
    assert_eq!(sent_by(&folded, &one(), &one()), model::Sent::Unbounded);
    // As a bound and as a charge: the reservation is frozen at what its other names
    // say, and not at a number that no name can state.
    for name in [
        format!("spend/dispatched/7000-1/1/7000-1/{largest}"),
        format!("spend/settled/7000-1/1/answered/{largest}"),
    ] {
        let mut names = ADMITTED.to_vec();
        names.push(name.as_str());
        let folded = of(&names);
        assert_eq!(
            state_and_count(&folded, &one()),
            (State::Frozen, model::Counted::Exactly(10)),
            "{name}"
        );
    }
    // As an allowance: the reset is none, and its epoch allows nothing.
    let reset = format!("spend/reset/1/{largest}");
    let mut names = ADMITTED.to_vec();
    names.retain(|name| *name != "spend/reset/1/30");
    names.push(reset.as_str());
    let balance = newest(&of(&names));
    assert_eq!((balance.epoch, balance.allowed, balance.left), (1, 0, 0));
}

#[test]
fn a_name_of_no_form_freezes_the_reservation_it_starts_as_a_name_of_or_is_reported_unplaced() {
    let whole = of(&TWO_RESERVED);
    assert_eq!(whole.unplaced(), 0);
    // One that starts as a name of a reservation does: that reservation is frozen,
    // and no other.
    for stray in [
        "spend/place/7000-2/01",
        "spend/held/1/7000-2/5/more",
        "spend/dispatched/7000-2",
        "spend/closed/7000-2/gone",
        "spend/settled/7000-2/1/sent/3",
        "spend/reservations/7000-2/again",
    ] {
        let mut names = TWO_RESERVED.to_vec();
        names.push(stray);
        let folded = of(&names);
        assert_eq!(state_of(&folded, &two()), State::Frozen, "{stray}");
        assert_eq!(res(&folded, &one()), res(&whole, &one()), "{stray}");
        assert_eq!(folded.unplaced(), 0, "{stray}");
    }
    // One for a run that has no reservation is a frozen reservation of nothing whose
    // amount no name states: the store needs its owner.
    let mut names = TWO_RESERVED.to_vec();
    names.push("spend/place/7000-9/x");
    let folded = of(&names);
    let nine = RunId::parse("7000-9").unwrap();
    assert_eq!(
        state_and_count(&folded, &nine),
        (State::Frozen, model::Counted::AtLeast(0))
    );
    assert!(newest(&folded).needs_owner);
    // One that starts as nothing the ledger writes, or whose run is no run id, is
    // placed nowhere: it changes no balance, and the fold says that it is there.
    let mut names = TWO_RESERVED.to_vec();
    names.extend([
        "spend/fake/3",
        "spend/place/NOT-A-RUN/1",
        "spend/",
        "spend/turn",
    ]);
    let folded = of(&names);
    assert_eq!(folded.unplaced(), 4);
    assert_eq!(folded.reservations(), whole.reservations());
    assert_eq!(newest(&folded), newest(&whole));
}

#[test]
fn a_turn_a_reset_or_a_guard_that_does_not_parse_never_allows_more_than_the_name_it_stands_for() {
    let whole = of(&TWO_RESERVED);
    assert_eq!(newest(&whole).left, 25);
    // Each name of an epoch, garbled where it stood: the name it was is gone.
    for (name, garbled) in [
        ("spend/turn/1/2", "spend/turn/1/02"),
        ("spend/turn/1/2", "spend/turn/1/2/x"),
        ("spend/epoch/1", "spend/epoch/1/x"),
        ("spend/epoch/1", "spend/epoch/01"),
        ("spend/reset/1/30", "spend/reset/1/030"),
        ("spend/reset/1/30", "spend/reset/1"),
    ] {
        let mut names = TWO_RESERVED.to_vec();
        names.retain(|other| *other != name);
        names.push(garbled);
        let balance = newest(&of(&names));
        assert_eq!((balance.epoch, balance.left), (1, 0), "{garbled}");
    }
    // Made beside the names that are there, each can only stop: a turn of no form
    // leaves its epoch needing its owner, and a reset or a guard of no form leaves
    // its epoch allowing nothing, the newest among them.
    for (made, epoch) in [
        ("spend/turn/1/x", 1),
        ("spend/reset/1/x", 1),
        ("spend/epoch/1/x", 1),
        ("spend/reset/2/x", 2),
        ("spend/epoch/2/", 2),
    ] {
        let mut names = TWO_RESERVED.to_vec();
        names.push(made);
        let balance = newest(&of(&names));
        assert_eq!((balance.epoch, balance.left), (epoch, 0), "{made}");
        assert_eq!(of(&names).reservations(), whole.reservations(), "{made}");
    }
    // A reset or a guard of no form that names the epoch before the first spoils
    // nothing: no reset opens that epoch. It is placed nowhere, and counted.
    let mut names = TWO_RESERVED.to_vec();
    names.extend(["spend/reset/0/x", "spend/epoch/0/x"]);
    let folded = of(&names);
    assert_eq!(folded, {
        let mut names = TWO_RESERVED.to_vec();
        names.extend(["spend/fake/1", "spend/fake/2"]);
        of(&names)
    });
    assert_eq!((folded.unplaced(), newest(&folded).left), (2, 25));
}

#[test]
fn the_fold_reads_records_through_names_only() {
    // The names of two reservations in a store, and one more publication that the
    // store refuses whole, because one of its names is taken: its content is under
    // `objects/` already, and no name holds it.
    let mut shelf = Shelf::new();
    let names: BTreeMap<String, bool> = TWO_RESERVED
        .iter()
        .map(|name| ((*name).to_owned(), false))
        .collect();
    shelf.hold(&names);
    let before = shelf.fold();
    assert_eq!(before, folded(&names));
    assert_eq!(newest(&before).left, 25);
    let objects = |shelf: &Shelf| {
        let root = shelf.scratch.path().join("0").join("objects");
        std::fs::read_dir(root).unwrap().count()
    };
    let held = objects(&shelf);
    let mut refused = shelf.store.begin().unwrap();
    for name in [
        "spend/held/1/7000-3/20",
        "spend/reservations/7000-3",
        "spend/turn/1/3",
    ] {
        refused
            .create(name, Kind::Diagnostic, &sound(name))
            .unwrap();
    }
    assert!(refused.publish().is_err());
    assert_eq!(objects(&shelf), held + 2);
    assert_eq!(shelf.fold(), before);
    // A record whose content does not agree with its name is damage: the store's
    // fold freezes that reservation, as the fold of its names does.
    let mut hurt = names.clone();
    hurt.insert("spend/held/1/7000-2/5".to_owned(), true);
    shelf.hold(&hurt);
    let frozen = shelf.fold();
    assert_eq!(frozen, folded(&hurt));
    assert_eq!(state_of(&frozen, &two()), State::Frozen);
    assert_eq!(res(&frozen, &one()), res(&before, &one()));
}
