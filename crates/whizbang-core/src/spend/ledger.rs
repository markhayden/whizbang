//! The real ledger (ADR 0016; plan 1B, task S2): what an executor is given to reserve
//! with ([`crate::contract::Engine::with_ledger`]).
//!
//! **It keeps nothing.** The books are the names a store holds under `spend/`, and
//! every answer here is the fold of one generation of them ([`fold`]): the one an
//! admission's step shows, and the one read again once the admission is durable. So
//! two executors over one store, and a process opened after a crash, judge on the same
//! books, and there is no balance in memory to disagree with them.
//!
//! **Reserving.** An executor asks for a call that may cost something, and for no
//! other: a call that may cost nothing is admitted without the ledger, whatever the
//! books say, and has no name here
//! ([`Engine::with_ledger`](crate::contract::Engine::with_ledger)). An admission it
//! is asked for is judged on the newest epoch
//! ([`Folded::balance`](super::fold::Folded::balance)). It is declined where that epoch
//! needs its owner, and where its amount is more than what is left; a store that no
//! reset ever set an allowance for has nothing left, so every call that may cost
//! something is refused there. Admitted, it makes two names in the
//! admission's publication: the hold, which states the amount and the epoch, and the
//! epoch's next turn. The turn is what keeps a judgement from being published on
//! books that changed: another admission, a settlement, a closing, a correction or a
//! reset that was published meanwhile took the same turn, the store refuses this
//! publication whole, and the executor asks again on the generation as it is then
//! (ADR 0016, "Nothing is published on a stale balance").
//!
//! **Confirming.** A publication admits nothing by itself. Whether the admission
//! stands is [`Folded::stands`](super::fold::Folded::stands) on the names read after
//! it, and nothing else.
//!
//! **Closing** (task S11). [`Ledger::settle`] is asked inside the publication that
//! ends a run nothing called, and closes that run's reservation there: the closing,
//! the reservation's next place, and the next turn of each epoch a hold of it names.
//! It reads the names of that one reservation and no other's. The closing releases,
//! as `released`, only where those names are a set the ledger could have written and
//! every dispatch under them has its settlement with its whole chain, an unresolved
//! one charged at its bound among them: the reservation then counts for what its
//! requests were charged, which may be more than was reserved, and the rest is free
//! for the next call. Where a dispatch has no settlement, as after a settlement that
//! could not be published or a backend's panic in a run it called, or where the names
//! are not the ledger's, it is closed as `held`: interrupted, at the larger of its
//! amount and what is known, with nothing released, until the owner's reset. A
//! reservation that a closing already names, and a run the books do not name, are
//! left as they are. Nothing else closes a reservation: one whose process died before
//! its run ended stays open and counts in full (ADR 0016, "The strand cut down").

use std::num::NonZeroU64;

use crate::contract::{
    Closing, Declined, Ledger, MadeName, NotJudged, Reread, Reservation, Settled, Step,
};

use super::fold::fold_newest;
use super::meter::next_turn;
use super::name::Amount;
use super::reading::Names;
use super::record::{ClosedBy, PlaceBy, Record, TurnBy};

/// What a ledger fails with when it is asked to reserve the largest number there is.
const NO_AMOUNT: &str = "the largest number there is is no amount to reserve";

/// What a closing fails with where its place or its turn would be nothing.
const NOT_FROM_ONE: &str = "a place and a turn are numbers from 1";

/// The ledger of a store: the books of names under `spend/`, folded each time it is
/// asked. It holds nothing, so one value serves any number of executors and stores.
#[derive(Debug, Clone, Copy, Default)]
pub struct StoreLedger;

impl Ledger for StoreLedger {
    fn reserve(
        &self,
        reservation: &Reservation,
        step: &mut Step<'_, '_>,
    ) -> Result<MadeName, Declined> {
        // The executor does not ask for a cost without a bound; were it to, nothing
        // is judged and nothing made.
        let Some(reserved) = Amount::new(reservation.micros()) else {
            return Err(NotJudged::saying(NO_AMOUNT).into());
        };
        // Only what bears on the newest epoch is read (plan 2C, task E1, item 6).
        let folded = fold_newest(step.generation(), |entry| step.read(entry), None);
        let run = reservation.run();
        // The run was named a moment ago, and the books already name it: other hands
        // made a name of it, or it is the id of a run that was deleted. A hold beside
        // what is there would be a second hold, or a hold under names the ledger did
        // not write: a frozen reservation, and with two holds a store that needs its
        // owner. Nothing is made.
        if folded.reservation(run).is_some() {
            return Err(NotJudged::saying(&format!(
                "the books already hold a name of the run {run}, which was named for this \
                 admission: nothing was reserved, and the request may be made again"
            ))
            .into());
        }
        // The judgement is of the generation the step shows, on the newest epoch.
        let balance = folded.balance();
        let epoch = balance.epoch;
        if balance.needs_owner {
            return Err(Declined::NeedsOwner { epoch });
        }
        if reserved > balance.left {
            return Err(Declined::OverAllowance {
                left_micros: balance.left.micros(),
            });
        }
        // Whatever changes what is left of this epoch takes its next turn: of two
        // that judged the same books the store refuses the later whole, and it is
        // judged again.
        let Some(turn) = NonZeroU64::new(folded.next_turn(epoch)) else {
            return Err(NotJudged::saying("a turn is a number from 1").into());
        };
        step.record(&Record::Hold {
            epoch,
            run: run.clone(),
            reserved,
        })?;
        let made = step.record(&Record::Turn {
            epoch,
            turn,
            by: TurnBy::Admission,
            run: Some(run.clone()),
        })?;
        Ok(made)
    }

    fn confirm(&self, reservation: &Reservation, reading: &Reread<'_>) -> Result<bool, NotJudged> {
        let Some(reserved) = Amount::new(reservation.micros()) else {
            return Ok(false);
        };
        let folded = fold_newest(
            reading.generation(),
            |entry| reading.read(entry),
            Some(reservation.run()),
        );
        Ok(folded.stands(reservation.run(), reserved))
    }

    /// Closes the run's reservation by its own names: released where every dispatch
    /// under it is settled, and held, with nothing released, where one is not.
    fn settle(&self, closing: &Closing<'_>, step: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
        let run = closing.run;
        let names = Names::of_run(run, step.generation(), |entry| step.read(entry));
        // The books name no such run, or a closing names it already: there is
        // nothing to close.
        let Some(names) = names.filter(|names| !names.closed()) else {
            return Ok(Settled::Nothing);
        };
        // One closing. It releases only where the names are a set the ledger could
        // have written and every place that holds a dispatch is settled whole.
        let released = names.reachable() && names.unsettled() == 0;
        let from_one =
            |number: u64| NonZeroU64::new(number).ok_or_else(|| NotJudged::saying(NOT_FROM_ONE));
        let made = step.record(&Record::Closing {
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
        // A turn in each epoch a hold of it names. A reservation with no hold counts
        // in no epoch, and takes no turn: the fold owes none for it.
        for epoch in names.epochs() {
            step.record(&Record::Turn {
                epoch,
                turn: from_one(next_turn(step.generation(), epoch))?,
                by: TurnBy::Closing,
                run: Some(run.clone()),
            })?;
        }
        Ok(Settled::Closed(made))
    }
}
