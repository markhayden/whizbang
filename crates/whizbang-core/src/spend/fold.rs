//! The fold (ADR 0016; plan 1B, task S1b): what a store's books say, across its
//! reservations, from the names of one generation.
//!
//! The books are the names under `spend/`. Of content the fold asks only whether each
//! name's can be read and agrees with it. One reservation's names are read by
//! themselves: whether they are a set the ledger could have written, what is charged
//! and outstanding under them, and the most they can mean. What is here is everything
//! that is of more than one reservation: the epochs and what each allows, the turns
//! each epoch's publications took and whether it needs its owner, whether a correction
//! stands, the state of each reservation and what it counts for, the store's balance,
//! and what one run sent.
//!
//! **Nothing under `spend/` makes the fold fail,** and nothing it cannot place lowers
//! a count: every sum stops at the largest number of 128 bits, a place is counted and
//! never walked to, and a name that is of no form either freezes the reservation it
//! names or is reported as unplaced ([`Folded::unplaced`]).

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::RunId;
use crate::store::{self, Entry, Generation};

use super::name::{Amount, EpochWord, Name};
use super::reading::{Counted, Names, Reading, RunSpend, Sent, Sum, lowest_free};

/// What a reservation is, as the fold finds it. The first that fits, in the order of
/// ADR 0016, "What a reservation counts for".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Not closed: its run executes, or was left and is not recovered yet. It counts
    /// for the larger of its amount and what is known against it.
    Open,
    /// Closed with every dispatch settled: it counts for its charges, and what it
    /// held beyond them is released.
    Released,
    /// Closed with nothing released: the larger of its amount and what is known.
    Interrupted,
    /// Its names are not a set the ledger could have written: it counts for the most
    /// they can mean, and nothing lowers it but its owner's correction.
    Frozen,
    /// Its owner corrected it, and the correction stands: it counts for what the
    /// correction says, and never under what its whole settlements say.
    Adjusted,
}

/// One reservation, as the fold answers it: what its own names say
/// ([`Standing::names`]), and what only the fold can say of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    names: Names,
    state: State,
    counts: Sum,
}

impl Standing {
    /// What its own names say, read by themselves: its holds, what is charged and
    /// outstanding under it, whether the ledger could have written them.
    pub fn names(&self) -> &Names {
        &self.names
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// The epoch it falls under: the one its one hold names. None when it has no
    /// hold, or more than one.
    pub fn epoch(&self) -> Option<u64> {
        self.names.epoch()
    }

    /// Whether a closing names it.
    pub fn closed(&self) -> bool {
        self.names.closed()
    }

    /// What it counts for against the allowance of each epoch it counts in: the
    /// number, which is the least it can be where [`Standing::unknown`] says so.
    pub fn counts(&self) -> Sum {
        self.counts
    }

    /// It is frozen and its holds are not exactly one: the ledger's own hold may be
    /// the one that is lost, whatever holds are there in its place, and then no
    /// name states its amount. What it counts for is unknown, and at least
    /// [`Standing::counts`].
    pub fn unknown(&self) -> bool {
        self.state == State::Frozen && self.epoch().is_none()
    }

    /// What it counts for: a number, or "unknown, at least" one.
    pub fn counted(&self) -> Counted {
        match self.unknown() {
            true => Counted::AtLeast(self.counts),
            false => Counted::Exactly(self.counts),
        }
    }

    /// What a dispatch under it may still take: nothing unless it is open, and
    /// nothing once any closing names it, whatever its state reads.
    pub fn left(&self) -> Amount {
        if self.closed() || self.state != State::Open {
            return Amount::ZERO;
        }
        short(Sum::of(self.names.amount()).less(self.names.known()))
    }

    /// Whether it counts against this epoch's allowance. A frozen reservation counts
    /// in every epoch there is: its holds are not trusted to say which. So does one
    /// that has no hold, or more than one, whatever it is: a correction says what it
    /// is charged, and not where.
    pub fn counts_in(&self, epoch: u64) -> bool {
        let named = |(held, _): &(u64, Amount)| *held == epoch;
        self.state == State::Frozen
            || self.epoch().is_none()
            || self.names.holds().iter().any(named)
    }
}

/// The store's balance for one epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Balance {
    pub epoch: u64,
    /// The allowance the epoch's reset set: nothing before the first reset, and
    /// nothing where the reset is not whole.
    pub allowed: Amount,
    /// What its reservations count for, together: a number, or "unknown, at least"
    /// one where a reservation that counts here has an amount no name can be
    /// trusted for.
    pub counted: Counted,
    /// What was charged under them.
    pub charged: Sum,
    /// What is held beyond what was charged.
    pub held: Sum,
    pub unresolved: Sum,
    /// What can still be reserved: never below nothing, and nothing while the epoch
    /// needs its owner.
    pub left: Amount,
    /// By how much what is counted is over the allowance.
    pub debt: Sum,
    /// Nothing is admitted in this epoch until the owner acts
    /// ([`Folded::needs_owner`]).
    pub needs_owner: bool,
}

/// Why a correction of a reservation to an amount is not taken (ADR 0016, "A
/// person's correction").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotCorrected {
    /// No reservation of that run, or one that is not closed, or that is neither
    /// interrupted, frozen nor corrected already.
    NotCorrectable,
    /// Under what its settlements with their whole chain say.
    BelowSettled { settled: Sum },
    /// Over the most its names can mean.
    OverCount { most: Sum },
    /// It raises what the reservation counts for in the newest epoch by more than
    /// that epoch has left.
    CannotBeHeld { left: Amount },
}

/// The books folded.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Folded {
    reservations: BTreeMap<RunId, Standing>,
    runs: BTreeMap<RunId, RunSpend>,
    resets: BTreeMap<u64, Amount>,
    turns: BTreeMap<u64, BTreeSet<u64>>,
    due: BTreeMap<u64, u64>,
    /// The epochs in which a text that begins as a turn's is of no form.
    garbled: BTreeSet<u64>,
    unplaced: u64,
}

impl Folded {
    /// Folds a reading of the names under `spend/`: everything that is of more than
    /// one reservation.
    pub fn of(reading: Reading) -> Folded {
        let mut folded = Folded::default();
        // An epoch is there when a reset or its guard names it. Its allowance is its
        // reset's only with the guard and with neither damaged; two resets of one
        // epoch are not the ledger's doing, and the smaller is the one that cannot
        // allow too much. Anything else allows nothing until the owner resets again:
        // a reset without its guard, a guard without its reset, and a text that
        // begins as either and is of no form.
        let mut resets: BTreeMap<u64, Vec<Amount>> = BTreeMap::new();
        let mut guards: BTreeSet<u64> = BTreeSet::new();
        let mut spoiled: BTreeSet<u64> = BTreeSet::new();
        for (name, damaged) in &reading.epochs {
            let epoch = match name {
                Name::Reset { epoch, micros } => {
                    resets.entry(epoch.get()).or_default().push(*micros);
                    epoch.get()
                }
                Name::Epoch(epoch) => {
                    guards.insert(epoch.get());
                    resets.entry(epoch.get()).or_default();
                    epoch.get()
                }
                Name::Turn { epoch, turn } => {
                    folded.turns.entry(*epoch).or_default().insert(turn.get());
                    continue;
                }
                _ => continue,
            };
            if *damaged {
                spoiled.insert(epoch);
            }
        }
        for formless in &reading.formless {
            match formless.epoch {
                // A turn of no form is a turn that no publication took.
                Some((EpochWord::Turn, epoch)) => {
                    folded.garbled.insert(epoch);
                }
                Some((EpochWord::Reset | EpochWord::Guard, epoch)) if epoch >= 1 => {
                    resets.entry(epoch).or_default();
                    spoiled.insert(epoch);
                }
                // A reset or a guard of the epoch before the first names an epoch
                // that no reset opens: it is placed nowhere, as a text that names
                // nothing is.
                Some(_) | None => folded.unplaced = folded.unplaced.saturating_add(1),
            }
        }
        for (epoch, amounts) in &resets {
            let whole = guards.contains(epoch) && !spoiled.contains(epoch);
            let least = amounts.iter().min().copied().filter(|_| whole);
            folded.resets.insert(*epoch, least.unwrap_or(Amount::ZERO));
        }
        let newest = folded.epoch();
        // What each epoch's turns are due: one for the reset that closed it, one for
        // each hold in it, and one for each settlement, closing and correction of a
        // reservation with a hold in it.
        let owe = |due: &mut BTreeMap<u64, u64>, epoch: u64, turns: u64| {
            let due = due.entry(epoch).or_default();
            *due = due.saturating_add(turns);
        };
        for epoch in folded.resets.keys() {
            owe(&mut folded.due, *epoch - 1, 1);
        }
        for (run, names) in reading.reservations {
            for (epoch, _) in names.holds() {
                owe(&mut folded.due, *epoch, 1);
            }
            for epoch in names.epochs() {
                owe(&mut folded.due, epoch, names.turns_beside_holds());
            }
            // Every run a dispatch at a place names is named for all of that place.
            for at in names.at() {
                for by in &at.by {
                    let spend = folded.runs.entry(by.clone()).or_default();
                    spend.charged = spend.charged.plus(at.charged);
                    spend.outstanding = spend.outstanding.plus(at.outstanding);
                    spend.unresolved = spend.unresolved.plus(at.unresolved);
                }
            }
            let standing = standing(names, newest, &folded.turns);
            folded.reservations.insert(run, standing);
        }
        folded
    }

    /// Every reservation the names give, by its run.
    pub fn reservations(&self) -> &BTreeMap<RunId, Standing> {
        &self.reservations
    }

    /// One reservation, by its run.
    pub fn reservation(&self, run: &RunId) -> Option<&Standing> {
        self.reservations.get(run)
    }

    /// What the dispatches name each run for, under whichever reservation.
    pub fn runs(&self) -> &BTreeMap<RunId, RunSpend> {
        &self.runs
    }

    /// The allowance of each epoch there is: nothing for one whose reset is not
    /// whole.
    pub fn resets(&self) -> &BTreeMap<u64, Amount> {
        &self.resets
    }

    /// The turns taken in each epoch.
    pub fn turns(&self) -> &BTreeMap<u64, BTreeSet<u64>> {
        &self.turns
    }

    /// How many turns the publications of this epoch account for.
    pub fn due(&self, epoch: u64) -> u64 {
        self.due.get(&epoch).copied().unwrap_or(0)
    }

    /// The newest epoch: 0 before the first reset.
    pub fn epoch(&self) -> u64 {
        self.resets.keys().next_back().copied().unwrap_or(0)
    }

    /// The next turn of an epoch: the lowest from 1 that is not taken, never a count
    /// and never one past the highest.
    pub fn next_turn(&self, epoch: u64) -> u64 {
        self.turns.get(&epoch).map_or(1, lowest_free)
    }

    /// The epochs a publication under this reservation takes a turn in: each its
    /// holds name, or the newest when it has none.
    pub fn turn_epochs(&self, run: &RunId) -> Vec<u64> {
        let epochs = self.reservations.get(run).map(|one| one.names.epochs());
        match epochs.filter(|epochs| !epochs.is_empty()) {
            Some(epochs) => epochs.into_iter().collect(),
            None => vec![self.epoch()],
        }
    }

    /// Whether no new paid work starts in an epoch until the owner acts. Its turns
    /// are other than exactly those its publications took: not 1 to their count, or
    /// more or fewer than the names account for, so that a name that took a turn may
    /// be gone, and with it an amount that nothing counts; or a text that begins as
    /// one of its turns is of no form. Or a reservation is frozen and its amount is
    /// unknown, which counts in every epoch for a number no name can be trusted for.
    /// A frozen reservation whose one hold states its amount stops nothing else.
    pub fn needs_owner(&self, epoch: u64) -> bool {
        let none = BTreeSet::new();
        let turns = self.turns.get(&epoch).unwrap_or(&none);
        let count = turns.len() as u64;
        // The turns are numbers from 1, each once and in order: they are 1 to their
        // count exactly when the last of them is their count. Nothing counts up to a
        // turn, which can be any number a name can say.
        let whole = turns.last().copied().unwrap_or(0) == count;
        !whole
            || count != self.due(epoch)
            || self.garbled.contains(&epoch)
            || self.reservations.values().any(Standing::unknown)
    }

    /// The balance of this epoch.
    pub fn balance_of(&self, epoch: u64) -> Balance {
        let allowed = self.resets.get(&epoch).copied().unwrap_or(Amount::ZERO);
        let (mut counted, mut charged, mut unresolved) = (Sum::ZERO, Sum::ZERO, Sum::ZERO);
        let mut unknown = false;
        for one in self.reservations.values() {
            if one.counts_in(epoch) {
                counted = counted.plus(one.counts);
                charged = charged.plus(one.names.charged());
                unresolved = unresolved.plus(one.names.unresolved());
                unknown |= one.unknown();
            }
        }
        let needs_owner = self.needs_owner(epoch);
        let left = match needs_owner {
            true => Sum::ZERO,
            false => Sum::of(allowed).less(counted),
        };
        Balance {
            epoch,
            allowed,
            counted: match unknown {
                true => Counted::AtLeast(counted),
                false => Counted::Exactly(counted),
            },
            charged,
            held: counted.less(charged),
            unresolved,
            left: short(left),
            debt: counted.less(Sum::of(allowed)),
            needs_owner,
        }
    }

    /// The balance of the newest epoch: what an admission is judged on.
    pub fn balance(&self) -> Balance {
        self.balance_of(self.epoch())
    }

    /// What a run sent under the reservation of the run at the root of its tree: a
    /// number where that reservation's names are a set the ledger could have
    /// written; "unknown, at most" where they are not; and "unknown" with no most
    /// where no name can be trusted for its amount either. The number is of every
    /// dispatch that names the run, under whichever reservation: a dispatch that
    /// other hands made elsewhere can only add to it.
    pub fn sent_by(&self, run: &RunId, root: &RunId) -> Sent {
        match self.reservations.get(root).map(Standing::names) {
            Some(names) if !names.reachable() && names.epoch().is_none() => Sent::Unbounded,
            Some(names) if !names.reachable() => Sent::Unknown {
                at_most: names.most(),
            },
            _ => Sent::Known(self.runs.get(run).copied().unwrap_or_default()),
        }
    }

    /// Whether an admission of this amount that was published stands, on this reading
    /// of the names made after its publication (ADR 0016, "An admission is confirmed
    /// before anything runs under it"): its reservation is open and is the record,
    /// one hold and nothing else that names the run; neither the epoch it fell under
    /// nor the newest needs its owner; and that epoch is not over its allowance with
    /// this reservation counted in full. A call of nothing adds nothing to what is
    /// counted and is not held to a debt that was there before it.
    ///
    /// **The amount is held to the hold.** It stands only where its one hold states
    /// exactly `micros`: a caller cannot pass nothing and skip the debt, and a hold
    /// that is not the one the admission made is not one it stands on.
    pub fn stands(&self, run: &RunId, micros: Amount) -> bool {
        let own = self.reservations.get(run);
        // Open, it is a set the ledger could have written, which has the record and
        // one hold: with two names, it has no other.
        let whole = |one: &&Standing| one.state == State::Open && one.names.count() == 2;
        let stated = |one: &&Standing| one.names.stated() == Some(micros);
        let Some(epoch) = own.filter(whole).filter(stated).and_then(Standing::epoch) else {
            return false;
        };
        if self.needs_owner(epoch) || self.needs_owner(self.epoch()) {
            return false;
        }
        micros == Amount::ZERO || self.balance_of(epoch).debt == Sum::ZERO
    }

    /// Whether the owner's correction of this reservation to this amount is taken:
    /// only of one that is closed and is interrupted, frozen or corrected already;
    /// from what its whole settlements say, as the least, to the most its names can
    /// mean; and, where it raises what the reservation counts for in the newest
    /// epoch, only when what is left there can hold the difference.
    pub fn correction(&self, run: &RunId, micros: Amount) -> Result<(), NotCorrected> {
        let Some(one) = self.reservations.get(run) else {
            return Err(NotCorrected::NotCorrectable);
        };
        let correctable = matches!(
            one.state,
            State::Interrupted | State::Frozen | State::Adjusted
        );
        if !one.closed() || !correctable {
            return Err(NotCorrected::NotCorrectable);
        }
        let asked = Sum::of(micros);
        let (settled, most) = (one.names.settled(), one.names.most());
        if asked < settled {
            return Err(NotCorrected::BelowSettled { settled });
        }
        if asked > most {
            return Err(NotCorrected::OverCount { most });
        }
        let left = self.balance().left;
        let more = asked.less(one.counts);
        if one.counts_in(self.epoch()) && more > Sum::of(left) {
            return Err(NotCorrected::CannotBeHeld { left });
        }
        Ok(())
    }

    /// How many texts under `spend/` are of no form and could be placed nowhere: in
    /// no reservation and in no epoch. They change no balance, and a caller that
    /// shows the books says that they are there (ADR 0016, "Limits", (h)).
    pub fn unplaced(&self) -> u64 {
        self.unplaced
    }
}

/// A sum as an amount: the largest amount there is where it is more.
fn short(sum: Sum) -> Amount {
    let micros = u64::try_from(sum.micros()).ok();
    micros.and_then(Amount::new).unwrap_or(Amount::MOST)
}

/// Folds the books of this generation. `read` gives the content of an entry; a
/// content that cannot be read, or that does not say what its name says, is damage,
/// and never a failure.
///
/// Content is read through a name of the generation and in no other way: bytes that
/// the store holds under no name are not in the books.
pub fn fold(generation: &Generation, read: impl Fn(&Entry) -> store::Result<Vec<u8>>) -> Folded {
    Folded::of(Reading::of(generation, read))
}

/// One reservation's state and what it counts for: the first row that fits of ADR
/// 0016, "What a reservation counts for".
fn standing(names: Names, newest: u64, turns: &BTreeMap<u64, BTreeSet<u64>>) -> Standing {
    // A correction stands when all that its own reservation's names can show of it
    // holds (it is not damaged, its reservation has a closing, the place it names is
    // there with nothing else at it, and its seal is of exactly the reservation's
    // other names as they are), and the turn it names is there, in the epoch of the
    // reservation's hold. Each is sealed over every other, so no two stand.
    let turn_epoch = names.holds().iter().map(|(epoch, _)| *epoch).min();
    let taken = turns.get(&turn_epoch.unwrap_or(newest));
    let stands = names.corrections().into_iter().find(|one| {
        one.stands_but_for_its_turn() && taken.is_some_and(|turns| turns.contains(&one.turn))
    });
    let (state, counts) = match stands {
        // A correction never counts under what settlements with their whole chain
        // say.
        Some(one) => (State::Adjusted, Sum::of(one.micros).max(names.settled())),
        None if !names.reachable() => (State::Frozen, names.most()),
        None if names.released() => (State::Released, names.known()),
        None if names.closed() => (State::Interrupted, names.most()),
        None => (State::Open, names.most()),
    };
    Standing {
        names,
        state,
        counts,
    }
}
