//! The meter (plan 1B, task S5; `SPEC.md` section 6, "Dispatch is recorded first"):
//! what stands in the meter's place between a handle and its backend
//! ([`Place::Meter`](crate::capability::Place), ADR 0015).
//!
//! **Before a paid request leaves** it is quoted ([`request_bound`]): a request to a
//! backend with no enforceable bound, or whose bound is over what its run has left,
//! is refused. Then its bound is debited from its run's allowance and from what the
//! process holds of its tree ([`Allowance::dispatch`](crate::contract::Allowance::dispatch)),
//! and its dispatch is made durable: one publication that makes the next place under
//! the reservation of the run at the root of its tree, and the dispatch at it (ADR
//! 0016, "What each publication makes"). Only then is the backend called. A request
//! that is refused, or whose publication the store refuses, is not sent, and its
//! debit is undone.
//!
//! **What it judges by** is what its process holds, the run's allowance and the
//! tree's tally, and the names of its own reservation, which it reads to take the
//! next place ([`Names::of_run`]). A reservation whose names read as closed, or as a
//! set the ledger could not have written, takes no dispatch, and neither does one
//! whose names leave less than the bound. It does not fold the store's books: an
//! epoch that admits nothing until a reset does not stop the requests of a run that
//! is already executing (ADR 0016, "The strand cut down").
//!
//! **What is not paid is not metered.** A request to a local backend that states no
//! price, and one to a backend that says it is free, pass with nothing held and
//! nothing recorded. So does a bounded request of a run nothing was reserved for,
//! which has no reservation to be recorded under.
//!
//! **Once it has debited, it holds something** ([`Open::holds`]): a backend's panic
//! under it leaves the run abandoned, with the dispatch durable and nothing to
//! settle it, so the request stays counted at its bound.
//!
//! **A request after its run's action has come back** never reaches the meter: the
//! handle's tally refuses it before any wrapper sees it (ADR 0015).
//!
//! **When the request ends** it is settled by the stage it reached (task S5b; ADR
//! 0010): not sent, nothing is charged; answered, by a response or by a fault, the
//! charge the answer gave, or the bound when it gave none; unresolved, a timeout and
//! a cancellation with no answer among them, the bound. The settlement is one
//! publication: its guard, the settlement, the next turn of the epoch the
//! reservation's hold names, and the next place when the charge is over the bound.
//! Once it is durable, what the bound held beyond the charge goes back to the run's
//! allowance, or the excess of a charge over its bound is taken from the run and
//! from each run above it ([`Dispatched::settled`]). A settlement that cannot be
//! published fails the wrapper: the run is abandoned (ADR 0015), the dispatch has no
//! settlement, and the request stays counted at its bound, in the process and in
//! the books: over, never under.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::num::NonZeroU64;
use std::sync::Arc;

use crate::capability::{Around, Ended, Fault, FaultKind, Nothing, Open, Stage, Stop, Wrapped};
use crate::contract::record::RECORD_KIND;
use crate::contract::{Clock, DispatchRefused, Dispatched, NotJudged, RefusalKind, RunId};
use crate::store::{self, Generation, Store};

use super::name::{Amount, Name};
use super::quote::request_bound;
use super::reading::{Names, Sum, lowest_free};
use super::record::{PlaceBy, Record, TurnBy};

/// How many times a place or a turn is chosen again when the store refuses it
/// because another publication took it meanwhile.
const ATTEMPTS: usize = 64;

/// The meter of one store: it writes each paid request's dispatch and settlement
/// there, at the time its clock says.
pub struct Meter {
    books: Books,
}

/// Where the meter writes, and its clock. A clone is the same store.
#[derive(Clone)]
struct Books {
    store: Arc<Store>,
    clock: Arc<dyn Clock>,
}

impl Meter {
    /// A meter that records in this store, which is the one its executor writes.
    pub fn new(store: Arc<Store>, clock: Arc<dyn Clock>) -> Meter {
        Meter {
            books: Books { store, clock },
        }
    }
}

impl Books {
    /// Judges a dispatch by the names of the root's reservation and publishes it:
    /// the next place and the dispatch at it. What comes back is the place. `failed`
    /// is told why, where the store failed.
    fn dispatch(
        &self,
        root: &RunId,
        by: &RunId,
        bound: Amount,
        failed: &RefCell<Option<String>>,
    ) -> Result<NonZeroU64, DispatchRefused> {
        let fail = |error: store::Error| {
            let why = error.without_paths();
            *failed.borrow_mut() = Some(why.clone());
            DispatchRefused::Failed(NotJudged::saying(&why))
        };
        for _ in 0..ATTEMPTS {
            let generation = self.store.snapshot().map_err(fail)?;
            let names = Names::of_run(root, &generation, |entry| self.store.read(entry));
            // A run that has an amount has a reservation: no name of it is a set
            // the ledger did not write.
            let Some(names) = names else {
                return Err(DispatchRefused::Frozen);
            };
            if names.closed() {
                return Err(DispatchRefused::Closed);
            }
            if !names.reachable() {
                return Err(DispatchRefused::Frozen);
            }
            let left = Sum::of(names.amount()).less(names.known());
            if Sum::of(bound) > left {
                let left_micros = u64::try_from(left.micros()).unwrap_or(u64::MAX);
                return Err(DispatchRefused::OverReservation { left_micros });
            }
            let Some(place) = NonZeroU64::new(names.next_place()) else {
                return Err(DispatchRefused::Frozen);
            };
            // The run's own dispatches before this one: its place among them.
            let before = names.at().iter().filter(|one| one.by.contains(by));
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
                    request: before.count() as u64,
                    bound,
                    at_ms: self.clock.now_ms(),
                },
            ];
            let mut publication = self.store.begin().map_err(fail)?;
            for record in &records {
                let name = record.name().to_string();
                publication
                    .create(&name, RECORD_KIND, &record.to_bytes())
                    .map_err(fail)?;
            }
            match publication.publish() {
                Ok(_) => return Ok(place),
                // Another dispatch of the tree took the place: it is chosen again.
                Err(store::Error::NameExists { .. }) => {}
                Err(error) => return Err(fail(error)),
            }
        }
        let why = format!("the store refused the place of a dispatch {ATTEMPTS} times");
        *failed.borrow_mut() = Some(why.clone());
        Err(DispatchRefused::Failed(NotJudged::saying(&why)))
    }

    /// Publishes the settlement of the request at this place under the root's
    /// reservation: its guard, the settlement, the next turn of each epoch a hold of
    /// the reservation names, and the next place when the charge is over the bound.
    /// What comes back is why it could not, in words.
    fn settle(
        &self,
        root: &RunId,
        place: NonZeroU64,
        bound: Amount,
        stage: Stage,
        charged: Amount,
    ) -> Result<(), String> {
        let fail = |error: store::Error| {
            format!(
                "the settlement was not published: {}",
                error.without_paths()
            )
        };
        let from_one = |number: u64| {
            NonZeroU64::new(number).ok_or_else(|| "a place and a turn are from 1".to_owned())
        };
        let over_bound = charged > bound;
        for _ in 0..ATTEMPTS {
            let generation = self.store.snapshot().map_err(fail)?;
            let names = Names::of_run(root, &generation, |entry| self.store.read(entry));
            let Some(names) = names else {
                return Err("the store names no reservation to settle under".to_owned());
            };
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
                    over_bound,
                    at_ms: self.clock.now_ms(),
                },
            ];
            // A reservation with no hold counts in no epoch, and takes no turn.
            for epoch in names.epochs() {
                records.push(Record::Turn {
                    epoch,
                    turn: from_one(next_turn(&generation, epoch))?,
                    by: TurnBy::Settlement,
                    run: Some(root.clone()),
                });
            }
            if over_bound {
                records.push(Record::Place {
                    run: root.clone(),
                    place: from_one(names.next_place())?,
                    by: PlaceBy::Settlement,
                });
            }
            let mut publication = self.store.begin().map_err(fail)?;
            for record in &records {
                let name = record.name().to_string();
                publication
                    .create(&name, RECORD_KIND, &record.to_bytes())
                    .map_err(fail)?;
            }
            match publication.publish() {
                Ok(_) => return Ok(()),
                // Another publication took the turn or the place: chosen again.
                Err(store::Error::NameExists { .. }) => {}
                Err(error) => return Err(fail(error)),
            }
        }
        Err(format!(
            "the settlement was not published: the store refused its turn or its place \
             {ATTEMPTS} times"
        ))
    }
}

/// The next turn of an epoch, by the names of this generation: the lowest from 1
/// that no turn of it names.
pub(super) fn next_turn(generation: &Generation, epoch: u64) -> u64 {
    let prefix = format!("spend/turn/{epoch}/");
    let under = generation
        .entries()
        .range(prefix.clone()..)
        .take_while(|(text, _)| text.starts_with(&prefix));
    let taken: BTreeSet<u64> = under
        .filter_map(|(text, _)| match Name::parse(text) {
            Ok(Name::Turn { turn, .. }) => Some(turn.get()),
            _ => None,
        })
        .collect();
    lowest_free(&taken)
}

/// The stage a request reached and what it is charged, by how it ended: the charge
/// an answer gave, the bound where an answer gave none and where nothing settled
/// the request, and nothing where it was not sent.
fn judged(ended: &Ended<'_>, bound: Amount) -> (Stage, Amount) {
    let (stage, charge) = match ended {
        Ended::Answered(charge) => (Stage::Answered, *charge),
        Ended::Faulted(fault) => (fault.stage(), fault.charge()),
    };
    // A charge is never the largest number, which is no amount: were one, the
    // request is held at its bound.
    let stated = charge.and_then(|charge| Amount::new(charge.micros()));
    let charged = match stage {
        Stage::NotSent => Amount::ZERO,
        Stage::Answered => stated.unwrap_or(bound),
        Stage::Unresolved => bound,
    };
    (stage, charged)
}

/// A refusal of this kind that says this: the fault the action is handed says the
/// same.
fn stop(kind: RefusalKind, why: &str) -> Result<Box<dyn Open>, Stop> {
    let fault = Fault::new(FaultKind::Rejected, why);
    match Stop::new(kind, why, fault) {
        Some(stop) => Err(stop),
        // Not reached: `Stop::new` answers nothing only for a kind that is not one
        // of a request, and the two the meter refuses under are
        // (`a_refusal_of_the_meter_is_of_a_kind_a_request_is_refused_under`, below).
        None => Ok(Box::new(Nothing)),
    }
}

impl Around for Meter {
    fn before(&self, asked: &Wrapped<'_>) -> Result<Box<dyn Open>, Stop> {
        let allowance = asked.allowance;
        let bound = match request_bound(asked.declaration, allowance.left_micros()) {
            Ok(Some(bound)) => bound,
            // Not paid: nothing is held for it.
            Ok(None) => return Ok(Box::new(Nothing)),
            Err(not) => return stop(not.kind(), &not.to_string()),
        };
        // Nothing was reserved for the run, so there is no reservation to record a
        // dispatch under, and nothing is counted.
        if allowance.total_micros().is_none() {
            return Ok(Box::new(Nothing));
        }
        let failed = RefCell::new(None);
        let publish = || self.books.dispatch(asked.root, asked.run, bound, &failed);
        match allowance.dispatch(bound, publish) {
            Ok((dispatched, place)) => Ok(Box::new(Sent {
                books: self.books.clone(),
                dispatched,
                place,
                bound,
            })),
            Err(refused) => {
                let why = match failed.into_inner() {
                    Some(failure) => format!("{refused}: {failure}"),
                    None => refused.to_string(),
                };
                // Whatever holds a dispatch back, nothing more may be spent under
                // its reservation now: a record has that one kind for it.
                stop(RefusalKind::RequestOverAllowance, &why)
            }
        }
    }
}

/// A paid request that was dispatched: its debit, its place and its bound, held
/// until the request is over.
struct Sent {
    books: Books,
    /// Dropped without being settled, it stays counted at its bound.
    dispatched: Dispatched,
    place: NonZeroU64,
    bound: Amount,
}

impl Open for Sent {
    fn after(self: Box<Self>, asked: &Wrapped<'_>, ended: &Ended<'_>) -> Result<(), String> {
        let Sent {
            books,
            mut dispatched,
            place,
            bound,
        } = *self;
        let (stage, charged) = judged(ended, bound);
        // A charge over its bound counts against the tree from now, before its
        // settlement is durable.
        dispatched.answered(charged);
        books.settle(asked.root, place, bound, stage, charged)?;
        dispatched.settled(charged);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    // The test kit's scratch folder holds none of this crate's types, so it can be
    // used here; the store is this crate's own, on the real disk.
    use whizbang_testkit::scratch::Scratch;

    use super::*;
    use crate::contract::SystemClock;
    use crate::store::fs::RealFs;

    #[test]
    fn a_refusal_of_the_meter_is_of_a_kind_a_request_is_refused_under() {
        // `stop` makes its refusal with `Stop::new`, which answers nothing for a
        // kind that is not one of a request: the request would then go by with
        // nothing held. The meter refuses under two kinds, and each is one.
        for kind in [
            RefusalKind::RequestNoBound,
            RefusalKind::RequestOverAllowance,
        ] {
            assert!(stop(kind, "refused").is_err(), "{kind:?}");
        }
    }

    #[test]
    fn a_reservation_with_no_hold_takes_no_turn_at_settlement() {
        let dir = Scratch::new("core-meter-no-hold");
        let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
        let run = RunId::parse("7000-1").unwrap();
        let place = NonZeroU64::new(1).unwrap();
        let bound = Amount::new(250_000).unwrap();
        // A dispatch at its place under a reservation whose hold is not there: no
        // name says which epoch it counts in, so the fold owes no turn for it.
        let dispatched = [
            Record::Place {
                run: run.clone(),
                place,
                by: PlaceBy::Dispatch,
            },
            Record::Dispatch {
                run: run.clone(),
                place,
                by: run.clone(),
                request: 0,
                bound,
                at_ms: 0,
            },
        ];
        let mut publication = store.begin().unwrap();
        for record in &dispatched {
            let name = record.name().to_string();
            publication
                .create(&name, RECORD_KIND, &record.to_bytes())
                .unwrap();
        }
        publication.publish().unwrap();
        let books = Books {
            store: store.clone(),
            clock: Arc::new(SystemClock),
        };
        let charged = Amount::new(100_000).unwrap();
        books
            .settle(&run, place, bound, Stage::Answered, charged)
            .unwrap();
        let generation = store.snapshot().unwrap();
        let names: Vec<&String> = generation.entries().keys().collect();
        // The settlement and its guard are made, and no turn of any epoch.
        assert!(names.contains(&&format!("spend/settled/{run}/1/answered/100000")));
        assert!(names.contains(&&format!("spend/settling/{run}/1")));
        assert!(
            !names.iter().any(|name| name.starts_with("spend/turn/")),
            "{names:?}"
        );
    }
}
