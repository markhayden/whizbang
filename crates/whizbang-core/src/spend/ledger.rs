//! The real ledger (ADR 0016; plan 1B, task S2): what an executor is given to reserve
//! with ([`crate::contract::Engine::with_ledger`]).
//!
//! **It keeps nothing.** The books are the names a store holds under `spend/`, and
//! every answer here is the fold of one generation of them ([`fold`]): the one an
//! admission's step shows, and the one read again once the admission is durable. So
//! two executors over one store, and a process opened after a crash, judge on the same
//! books, and there is no balance in memory to disagree with them.
//!
//! **Reserving.** An admission is judged on the newest epoch
//! ([`Folded::balance`](super::fold::Folded::balance)). It is declined where that epoch
//! needs its owner, a call of nothing among them, and where its amount is more than
//! what is left; a store that no reset ever set an allowance for has nothing left, so
//! it takes calls of nothing and no other. Admitted, it makes two names in the
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
//! **Closing is not built here.** [`Ledger::settle`] answers that it closed nothing
//! (plan 1B, task S11, which has the charges of a run's requests to close with). Until
//! then a reservation this ledger made stays open when its run ends, and when its
//! admission was not confirmed, and counts in full: over, never under.

use std::num::NonZeroU64;

use crate::contract::{
    Closing, Declined, Ledger, MadeName, NotJudged, Reread, Reservation, Settled, Step,
};

use super::fold::fold;
use super::name::Amount;
use super::record::{Record, TurnBy};

/// What a ledger fails with when it is asked to reserve the largest number there is.
const NO_AMOUNT: &str = "the largest number there is is no amount to reserve";

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
        let folded = fold(step.generation(), |entry| step.read(entry));
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
        let folded = fold(reading.generation(), |entry| reading.read(entry));
        Ok(folded.stands(reservation.run(), reserved))
    }

    /// It closes nothing: the closing of a reservation when its run ends is task
    /// S11's. The reservation stays open and counts in full.
    fn settle(&self, _: &Closing<'_>, _: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
        Ok(Settled::Nothing)
    }
}
