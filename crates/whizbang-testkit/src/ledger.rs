//! A fake of the ledger (ADR 0007, "The reservation"; ADR 0016). It spends nothing and
//! knows no price: it has an allowance a test sets, declines what the allowance does
//! not cover, and counts.
//!
//! **It keeps books of the ledger's names,** as ADR 0016 has them, in the store: an
//! admission makes a hold and the next turn of its epoch, the end of a run closes its
//! reservation, and a dispatch a test asks for takes a place. It keeps no list of its
//! own, so what it says was reserved is what a ledger opened after a crash would find.
//!
//! **What it judges on is the model's fold** ([`crate::spend::fold`]) over the names
//! the store holds under `spend/`, read with the core's one parser. That is what makes
//! it a fake: the real fold is the spend strand's. Two things it does not know. Its
//! allowance is a number in memory, which stands for the allowance of the epoch
//! before any reset while the store names no reset; a store whose names hold a reset
//! is judged by that reset. And a person's correction never stands for it: the
//! model's seal is not the core's.
//!
//! It counts two things, and they are not the same. [`FakeLedger::asked`] is how many
//! times admission asked it: once for every attempt at an admission's publication, the
//! attempts the store refused among them. [`FakeLedger::reservations`] is what was
//! reserved: the reservations the store's current generation names.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use whizbang_core::artifact::Contents;
use whizbang_core::capability::Stage;
use whizbang_core::contract::{
    CallError, Closing, Declined, DispatchRefused, Ledger, MadeName, NotJudged, Reread,
    Reservation, RunId, Settled, Step, reservations,
};
use whizbang_core::spend::{
    Amount, ClosedBy, EpochWord, Name, PlaceBy, Reading, Record, TurnBy, lowest_free,
};
use whizbang_core::store::{self, Entry, Generation, Kind, Store};

use crate::spend as model;

/// A fake ledger. It may be shared across threads and across executors.
#[derive(Debug, Default)]
pub struct FakeLedger {
    /// What may be reserved in all before any reset, in millionths of a US dollar;
    /// none is no limit.
    allowance: Mutex<Option<u64>>,
    asked: AtomicU64,
    confirmed: AtomicU64,
    settled: AtomicU64,
}

/// The names under `spend/` of one generation, as the model reads them.
pub struct Books {
    pub folded: model::Folded,
    /// The model's number for each run the names name.
    pub runs: BTreeMap<RunId, model::Run>,
}

impl Books {
    /// The model's number for this run: one no name has, when no name names it.
    pub fn run(&self, run: &RunId) -> model::Run {
        let unknown = model::Run(u32::MAX);
        self.runs.get(run).copied().unwrap_or(unknown)
    }

    /// The reservation of this run, as the model's fold answers it.
    pub fn reservation(&self, run: &RunId) -> Option<&model::Standing> {
        self.folded.reservations.get(&self.run(run))
    }
}

impl FakeLedger {
    /// A ledger that declines nothing.
    pub fn new() -> FakeLedger {
        FakeLedger::default()
    }

    /// A ledger that lets this much be reserved in all, in millionths of a US dollar.
    pub fn with_allowance(micros: u64) -> FakeLedger {
        let ledger = FakeLedger::new();
        ledger.allow(Some(micros));
        ledger
    }

    /// Sets the allowance, or takes the limit away.
    pub fn allow(&self, micros: Option<u64>) {
        *self
            .allowance
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = micros;
    }

    /// How many times admission asked this ledger for a reservation. An attempt the
    /// store refused, or the ledger declined, was asked for too.
    pub fn asked(&self) -> u64 {
        self.asked.load(Ordering::SeqCst)
    }

    /// How many admissions it was asked to confirm.
    pub fn confirmed(&self) -> u64 {
        self.confirmed.load(Ordering::SeqCst)
    }

    /// How many times it was asked to close a reservation at the end of a run.
    pub fn settled(&self) -> u64 {
        self.settled.load(Ordering::SeqCst)
    }

    /// What was reserved: every reservation the current generation of this store
    /// names. Content that no name leads to is not looked at.
    pub fn reservations(&self, contents: &dyn Contents) -> Result<Vec<Reservation>, CallError> {
        let generation = contents.snapshot().map_err(|error| CallError::Ledger {
            why: error.without_paths(),
        })?;
        reservations(&generation, |entry| contents.read_record(entry))
    }

    /// The books of this generation, folded by the model.
    pub fn books(
        &self,
        generation: &Generation,
        read: impl Fn(&Entry) -> store::Result<Vec<u8>>,
    ) -> Books {
        let reading = Reading::of(generation, read);
        let mut runs: BTreeMap<RunId, model::Run> = BTreeMap::new();
        let mut number = |run: &RunId| {
            let next = model::Run(runs.len() as u32 + 1);
            *runs.entry(run.clone()).or_insert(next)
        };
        let mut names = BTreeSet::new();
        let mut damaged = BTreeSet::new();
        let mut put = |name: model::Name, hurt: bool| {
            if hurt {
                damaged.insert(name.clone());
            }
            names.insert(name);
        };
        for (name, hurt) in &reading.epochs {
            put(modelled(name, &mut number), *hurt);
        }
        for (run, of) in &reading.reservations {
            for (name, hurt) in of.all() {
                put(modelled(name, &mut number), *hurt);
            }
            let run = number(run);
            for count in 1..=of.formless().len() as u64 {
                put(model::Name::Stray { run, count }, false);
            }
        }
        for formless in &reading.formless {
            match formless.epoch {
                // A reset or a guard of no form: the epoch is there, and not whole.
                Some((EpochWord::Reset | EpochWord::Guard, epoch)) if epoch >= 1 => {
                    put(model::Name::Reset { epoch, micros: 0 }, true);
                }
                // A turn of no form: a turn no publication took.
                Some((EpochWord::Turn, epoch)) => {
                    let turn = u64::MAX;
                    put(model::Name::Turn { epoch, turn }, false);
                }
                _ => {}
            }
        }
        let mut folded = model::fold(&names, &damaged);
        if folded.resets.is_empty() {
            let allowance = *self
                .allowance
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            folded.resets.insert(0, allowance.unwrap_or(u64::MAX - 1));
        }
        Books { folded, runs }
    }

    /// The books of the store's current generation, or the store's failure.
    pub fn books_of(&self, store: &Store) -> store::Result<Books> {
        let generation = store.snapshot()?;
        Ok(self.books(&generation, |entry| store.read(entry)))
    }

    /// Judges a dispatch by the names and publishes it: the next place under the
    /// root's reservation and the dispatch at it, asked again when the store refuses
    /// the place. It is what a test hands [`whizbang_core::contract::Allowance::dispatch`]
    /// as what publishes; the meter is the spend strand's. What comes back is the
    /// place.
    pub fn place(
        &self,
        store: &Store,
        root: &RunId,
        by: &RunId,
        bound: Amount,
    ) -> Result<u64, DispatchRefused> {
        let failed = |error: store::Error| {
            DispatchRefused::Failed(NotJudged::from(CallError::Ledger {
                why: error.without_paths(),
            }))
        };
        // Two show a race: a place is chosen again a bounded number of times.
        for _ in 0..64 {
            let generation = store.snapshot().map_err(failed)?;
            let books = self.books(&generation, |entry| store.read(entry));
            let Some(one) = books.reservation(root) else {
                return Err(DispatchRefused::Frozen);
            };
            if one.closed {
                return Err(DispatchRefused::Closed);
            }
            if one.state != model::State::Open {
                return Err(DispatchRefused::Frozen);
            }
            let epoch = one.epoch.unwrap_or(books.folded.epoch());
            if books.folded.needs_owner(epoch) {
                return Err(DispatchRefused::NeedsOwner { epoch });
            }
            if bound.micros() > one.left() {
                return Err(DispatchRefused::OverReservation {
                    left_micros: one.left(),
                });
            }
            let place = next_place(&generation, root);
            let Some(place) = NonZeroU64::new(place) else {
                return Err(DispatchRefused::Frozen);
            };
            let records = [
                Record::Place {
                    run: root.clone(),
                    place,
                    by: PlaceBy::Dispatch,
                },
                Record::Dispatch {
                    run: root.clone(),
                    place,
                    by: by.clone(),
                    request: 0,
                    bound,
                    at_ms: 0,
                },
            ];
            let mut publication = store.begin().map_err(failed)?;
            for record in &records {
                publication
                    .create(
                        &record.name().to_string(),
                        Kind::Diagnostic,
                        &record.to_bytes(),
                    )
                    .map_err(failed)?;
            }
            match publication.publish() {
                Ok(_) => return Ok(place.get()),
                Err(store::Error::NameExists { .. }) => {}
                Err(error) => return Err(failed(error)),
            }
        }
        Err(DispatchRefused::Failed(NotJudged::saying(
            "the fake ledger chose a place 64 times and the store refused each",
        )))
    }

    /// Settles the request at a place, as the meter is to: its guard, its settlement
    /// and the next turn; and the next place when the charge is over the bound. What
    /// comes back is why it could not, in words.
    pub fn settle_at(
        &self,
        store: &Store,
        root: &RunId,
        place: u64,
        bound: Amount,
        stage: Stage,
        charged: Amount,
    ) -> Result<(), String> {
        let failed = |error: store::Error| error.without_paths();
        let from_one =
            |number: u64| NonZeroU64::new(number).ok_or("a place and a turn are from 1".to_owned());
        let place = from_one(place)?;
        // A turn is chosen again a bounded number of times.
        for _ in 0..64 {
            let generation = store.snapshot().map_err(failed)?;
            let books = self.books(&generation, |entry| store.read(entry));
            let Some(one) = books.reservation(root) else {
                return Err("the store names no such reservation".to_owned());
            };
            let epoch = one.epoch.unwrap_or(books.folded.epoch());
            let mut records = vec![
                Record::Settling {
                    run: root.clone(),
                    place,
                },
                Record::Settlement {
                    run: root.clone(),
                    place,
                    stage,
                    charged,
                    over_bound: charged > bound,
                    at_ms: 0,
                },
                Record::Turn {
                    epoch,
                    turn: from_one(books.folded.next_turn(epoch))?,
                    by: TurnBy::Settlement,
                    run: Some(root.clone()),
                },
            ];
            if charged > bound {
                records.push(Record::Place {
                    run: root.clone(),
                    place: from_one(next_place(&generation, root))?,
                    by: PlaceBy::Settlement,
                });
            }
            let mut publication = store.begin().map_err(failed)?;
            for record in &records {
                let name = record.name().to_string();
                publication
                    .create(&name, Kind::Diagnostic, &record.to_bytes())
                    .map_err(failed)?;
            }
            match publication.publish() {
                Ok(_) => return Ok(()),
                Err(store::Error::NameExists { .. }) => {}
                Err(error) => return Err(failed(error)),
            }
        }
        Err("the fake ledger chose a turn 64 times and the store refused each".to_owned())
    }
}

/// The next place under a reservation, by the names of this generation: the lowest
/// that no place, dispatch, settlement or settlement's guard of it names.
fn next_place(generation: &Generation, root: &RunId) -> u64 {
    let taken: BTreeSet<u64> = generation
        .entries()
        .keys()
        .filter_map(|text| Name::parse(text).ok())
        .filter(|name| name.run() == Some(root))
        .filter_map(|name| name.place())
        .map(NonZeroU64::get)
        .collect();
    lowest_free(&taken)
}

/// A name of the core's as the model has it. A seal is cut to the model's length: a
/// correction never stands for the fake.
fn modelled(name: &Name, number: &mut impl FnMut(&RunId) -> model::Run) -> model::Name {
    let stage = |stage: &Stage| match stage {
        Stage::NotSent => model::Stage::NotSent,
        Stage::Answered => model::Stage::Answered,
        Stage::Unresolved => model::Stage::Unresolved,
    };
    match name {
        Name::Reservation(run) => model::Name::Reservation(number(run)),
        Name::Held { epoch, run, micros } => model::Name::Held {
            epoch: *epoch,
            run: number(run),
            micros: micros.micros(),
        },
        Name::Turn { epoch, turn } => model::Name::Turn {
            epoch: *epoch,
            turn: turn.get(),
        },
        Name::Reset { epoch, micros } => model::Name::Reset {
            epoch: epoch.get(),
            micros: micros.micros(),
        },
        Name::Epoch(epoch) => model::Name::Epoch(epoch.get()),
        Name::Place { run, place } => model::Name::Place {
            run: number(run),
            place: place.get(),
        },
        Name::Dispatched {
            run,
            place,
            by,
            micros,
        } => model::Name::Dispatched {
            run: number(run),
            place: place.get(),
            by: number(by),
            micros: micros.micros(),
        },
        Name::Settling { run, place } => model::Name::Settling {
            run: number(run),
            place: place.get(),
        },
        Name::Settled {
            run,
            place,
            stage: reached,
            micros,
        } => model::Name::Settled {
            run: number(run),
            place: place.get(),
            stage: stage(reached),
            micros: micros.micros(),
        },
        Name::Closed { run, released } => model::Name::Closed {
            run: number(run),
            released: *released,
        },
        Name::Adjusted {
            run,
            place,
            turn,
            micros,
            ..
        } => model::Name::Adjusted {
            run: number(run),
            place: place.get(),
            turn: turn.get(),
            seal: 0,
            micros: micros.micros(),
        },
    }
}

fn from_one(number: u64) -> Result<NonZeroU64, NotJudged> {
    NonZeroU64::new(number).ok_or_else(|| NotJudged::saying("a place and a turn are from 1"))
}

impl Ledger for FakeLedger {
    fn reserve(
        &self,
        reservation: &Reservation,
        step: &mut Step<'_, '_>,
    ) -> Result<MadeName, Declined> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        let books = self.books(step.generation(), |entry| step.read(entry));
        // The judgement is of the generation the step shows, on the newest epoch.
        let balance = books.folded.balance();
        let micros = reservation.micros();
        if balance.needs_owner {
            return Err(Declined::NeedsOwner {
                epoch: balance.epoch,
            });
        }
        if micros > balance.left {
            return Err(Declined::OverAllowance {
                left_micros: balance.left,
            });
        }
        let Some(reserved) = Amount::new(micros) else {
            return Err(Declined::Failed(NotJudged::saying(
                "the largest number there is is no amount to reserve",
            )));
        };
        // Another admission may reserve before this one is published. Every
        // admission takes the next turn of its epoch: of two that judged the same
        // books, the store refuses the later whole, and it is asked again.
        let epoch = balance.epoch;
        let turn = from_one(books.folded.next_turn(epoch))?;
        step.record(&Record::Hold {
            epoch,
            run: reservation.run().clone(),
            reserved,
        })?;
        let made = step.record(&Record::Turn {
            epoch,
            turn,
            by: TurnBy::Admission,
            run: Some(reservation.run().clone()),
        })?;
        Ok(made)
    }

    fn confirm(&self, reservation: &Reservation, reading: &Reread<'_>) -> Result<bool, NotJudged> {
        self.confirmed.fetch_add(1, Ordering::SeqCst);
        let books = self.books(reading.generation(), |entry| reading.read(entry));
        let run = books.run(reservation.run());
        Ok(model::stands(&books.folded, run, reservation.micros()))
    }

    fn settle(&self, closing: &Closing<'_>, step: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
        self.settled.fetch_add(1, Ordering::SeqCst);
        let reading = Reading::of(step.generation(), |entry| step.read(entry));
        let Some(names) = reading.reservations.get(closing.run) else {
            return Ok(Settled::Nothing);
        };
        if names.closed() {
            return Ok(Settled::Nothing);
        }
        let books = self.books(step.generation(), |entry| step.read(entry));
        // One closing: it releases only where the reservation is open, nothing under
        // it is damaged or unplaced, and every place under it is settled whole.
        let released = names.reachable() && names.unsettled() == 0;
        let run = closing.run.clone();
        step.record(&Record::Closing {
            run: run.clone(),
            released,
            by: ClosedBy::End,
            charged: names.charged(),
            unresolved: names.unresolved(),
            at_ms: closing.at_ms,
        })?;
        step.record(&Record::Place {
            run: run.clone(),
            place: from_one(names.next_place())?,
            by: PlaceBy::Closing,
        })?;
        // A turn in each epoch its holds name, or in the newest where it has none.
        let mut epochs: Vec<u64> = names.epochs().into_iter().collect();
        if epochs.is_empty() {
            epochs.push(books.folded.epoch());
        }
        let mut made = None;
        for epoch in epochs {
            made = Some(step.record(&Record::Turn {
                epoch,
                turn: from_one(books.folded.next_turn(epoch))?,
                by: TurnBy::Closing,
                run: Some(run.clone()),
            })?);
        }
        match made {
            Some(made) => Ok(Settled::Closed(made)),
            None => Ok(Settled::Nothing),
        }
    }
}
