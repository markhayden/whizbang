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
//! Settling a request when it ends is task S5b's. Until then a dispatched request
//! stays counted at its bound, in the process and in the books: over, never under.

use std::cell::RefCell;
use std::num::NonZeroU64;
use std::sync::Arc;

use crate::capability::{Around, Ended, Fault, FaultKind, Nothing, Open, Stop, Wrapped};
use crate::contract::record::RECORD_KIND;
use crate::contract::{Clock, DispatchRefused, Dispatched, NotJudged, RefusalKind, RunId};
use crate::store::{self, Store};

use super::name::Amount;
use super::quote::request_bound;
use super::reading::{Names, Sum};
use super::record::{PlaceBy, Record};

/// How many times a place is chosen again when the store refuses it because another
/// publication took it meanwhile.
const ATTEMPTS: usize = 64;

/// The meter of one store: it writes each paid request's dispatch there, at the
/// time its clock says.
pub struct Meter {
    store: Arc<Store>,
    clock: Arc<dyn Clock>,
}

impl Meter {
    /// A meter that records in this store, which is the one its executor writes.
    pub fn new(store: Arc<Store>, clock: Arc<dyn Clock>) -> Meter {
        Meter { store, clock }
    }

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
}

/// A refusal of this kind that says this: the fault the action is handed says the
/// same.
fn stop(kind: RefusalKind, why: &str) -> Result<Box<dyn Open>, Stop> {
    let fault = Fault::new(FaultKind::Rejected, why);
    match Stop::new(kind, why, fault) {
        Some(stop) => Err(stop),
        // The kinds here are of a request: this is not reached.
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
        let publish = || self.dispatch(asked.root, asked.run, bound, &failed);
        match allowance.dispatch(bound, publish) {
            Ok((dispatched, place)) => Ok(Box::new(Sent {
                _dispatched: dispatched,
                _place: place,
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

/// A paid request that was dispatched: its debit and its place, held until the
/// request is over.
struct Sent {
    /// Dropped without being settled, it stays counted at its bound.
    _dispatched: Dispatched,
    _place: NonZeroU64,
}

impl Open for Sent {
    /// Settling is task S5b's: the request stays counted at its bound.
    fn after(self: Box<Self>, _: &Wrapped<'_>, _: &Ended<'_>) -> Result<(), String> {
        Ok(())
    }
}
