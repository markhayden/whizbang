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

pub use temporary::{Counted, RunSpend, Sent};
use temporary::{Reading, Store};

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

/// One reservation, as the fold answers it. Amounts are in millionths of a US dollar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// The epoch it falls under: the one its one hold names. None when it has no
    /// hold, or more than one.
    pub epoch: Option<u64>,
    /// Every epoch a hold of it names.
    pub epochs: BTreeSet<u64>,
    /// What was reserved: the largest amount a hold of it names.
    pub amount: u64,
    /// What its settlements say was charged, unresolved ones at their bounds.
    pub charged: u128,
    /// The part of `charged` that settlements with their whole chain say: what a
    /// correction cannot go under.
    pub settled: u128,
    /// The bounds of its dispatches that are not settled; and where its names are
    /// not reachable, the bound of every dispatch named.
    pub outstanding: u128,
    /// The part of `charged` and all of `outstanding` that no answer settled.
    pub unresolved: u128,
    /// The places under it that have no settlement with its whole chain.
    pub unsettled: u64,
    /// Where the names are not reachable: how many of the numbers from 1 to the
    /// highest place any of them names hold no dispatch.
    pub unexplained: u64,
    /// The largest amount a correction of it names.
    pub corrections: u64,
    /// Whether its names, corrections' seals aside, are a set the ledger could have
    /// written.
    pub reachable: bool,
    /// How many names name its run, of any form.
    pub names: u64,
    /// What it counts for against the allowance of each epoch it counts in.
    pub counts: u128,
    /// It is frozen and its holds are not exactly one: no name can be trusted for
    /// its amount, and what it counts for is unknown and at least `counts`.
    pub unknown: bool,
    pub state: State,
    /// A closing names it.
    pub closed: bool,
    /// The most its names can mean ([`Standing::most`]).
    most: u128,
}

impl Standing {
    /// What it counts for: a number, or "unknown, at least" one.
    pub fn counted(&self) -> Counted {
        match self.unknown {
            true => Counted::AtLeast(self.counts),
            false => Counted::Exactly(self.counts),
        }
    }

    /// What is known against it: its charges, and the bounds that are outstanding.
    pub fn known(&self) -> u128 {
        self.charged.saturating_add(self.outstanding)
    }

    /// The most its names can mean: the most its owner's correction may say.
    pub fn most(&self) -> u128 {
        self.most
    }

    /// What a dispatch under it may still take: nothing unless it is open, and
    /// nothing once any closing names it, whatever its state reads.
    pub fn left(&self) -> u64 {
        if self.closed || self.state != State::Open {
            return 0;
        }
        short(u128::from(self.amount).saturating_sub(self.known()))
    }

    /// Whether it counts against this epoch's allowance. A frozen reservation counts
    /// in every epoch there is: its holds are not trusted to say which. So does one
    /// that has no hold, or more than one, whatever it is: a correction says what it
    /// is charged, and not where.
    pub fn counts_in(&self, epoch: u64) -> bool {
        self.state == State::Frozen || self.epoch.is_none() || self.epochs.contains(&epoch)
    }
}

/// The store's balance for one epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Balance {
    pub epoch: u64,
    /// The allowance the epoch's reset set: nothing before the first reset, and
    /// nothing where the reset is not whole.
    pub allowed: u64,
    /// What its reservations count for, together.
    pub counted: u128,
    /// The books cannot say how much is counted: `counted` is the least it can be.
    pub unknown: bool,
    /// What was charged under them.
    pub charged: u128,
    /// What is held beyond what was charged.
    pub held: u128,
    pub unresolved: u128,
    /// What can still be reserved: never below nothing, and nothing while the epoch
    /// needs its owner.
    pub left: u64,
    /// By how much what is counted is over the allowance.
    pub debt: u128,
    /// Nothing is admitted in this epoch until the owner acts
    /// ([`Folded::needs_owner`]).
    pub needs_owner: bool,
}

impl Balance {
    /// What is counted: a number, or "unknown, at least" one.
    pub fn counted(&self) -> Counted {
        match self.unknown {
            true => Counted::AtLeast(self.counted),
            false => Counted::Exactly(self.counted),
        }
    }
}

/// Why a correction of a reservation to an amount is not taken (ADR 0016, "A
/// person's correction").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotCorrected {
    /// No reservation of that run, or one that is not closed, or that is neither
    /// interrupted, frozen nor corrected already.
    NotCorrectable,
    /// Under what its settlements with their whole chain say.
    BelowSettled { settled: u128 },
    /// Over the most its names can mean.
    OverCount { most: u128 },
    /// It raises what the reservation counts for in the newest epoch by more than
    /// that epoch has left.
    CannotBeHeld { left: u64 },
}

/// The books folded.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Folded {
    /// Every reservation the names give, by its run.
    pub reservations: BTreeMap<RunId, Standing>,
    /// What the dispatches name each run for.
    pub runs: BTreeMap<RunId, RunSpend>,
    /// The allowance of each epoch there is: nothing for one whose reset is not whole.
    pub resets: BTreeMap<u64, u64>,
    /// The turns taken in each epoch.
    pub turns: BTreeMap<u64, BTreeSet<u64>>,
    /// How many turns each epoch's publications account for.
    pub due: BTreeMap<u64, u64>,
    /// The epochs in which a name that starts as a turn's is of no form.
    garbled: BTreeSet<u64>,
    /// The names under `spend/` that are of no form and belong to no reservation and
    /// to no epoch.
    unplaced: u64,
}

impl Folded {
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
        let epochs = self.reservations.get(run).map(|one| &one.epochs);
        match epochs.filter(|epochs| !epochs.is_empty()) {
            Some(epochs) => epochs.iter().copied().collect(),
            None => vec![self.epoch()],
        }
    }

    /// Whether no new paid work starts in an epoch until the owner acts. Its turns
    /// are other than exactly those its publications took: not 1 to their count, or
    /// more or fewer than the names account for, so that a name that took a turn may
    /// be gone, and with it an amount that nothing counts; or a name that starts as
    /// one of its turns is of no form. Or a reservation is frozen and its amount is
    /// unknown, which counts in every epoch for a number no name can be trusted for.
    /// A frozen reservation whose one hold states its amount stops nothing else.
    pub fn needs_owner(&self, epoch: u64) -> bool {
        let none = BTreeSet::new();
        let turns = self.turns.get(&epoch).unwrap_or(&none);
        let count = turns.len() as u64;
        let due = self.due.get(&epoch).copied().unwrap_or(0);
        // The turns are a set in order: they are 1 to their count exactly when the
        // last of them is their count.
        let whole = turns.last().copied().unwrap_or(0) == count;
        !whole
            || count != due
            || self.garbled.contains(&epoch)
            || self.reservations.values().any(|one| one.unknown)
    }

    /// The balance of this epoch.
    pub fn balance_of(&self, epoch: u64) -> Balance {
        let allowed = self.resets.get(&epoch).copied().unwrap_or(0);
        let (mut counted, mut charged, mut unresolved) = (0u128, 0u128, 0u128);
        let mut unknown = false;
        for one in self
            .reservations
            .values()
            .filter(|one| one.counts_in(epoch))
        {
            counted = counted.saturating_add(one.counts);
            charged = charged.saturating_add(one.charged);
            unresolved = unresolved.saturating_add(one.unresolved);
            unknown |= one.unknown;
        }
        let needs_owner = self.needs_owner(epoch);
        let left = match needs_owner {
            true => 0,
            false => u128::from(allowed).saturating_sub(counted),
        };
        Balance {
            epoch,
            allowed,
            counted,
            unknown,
            charged,
            held: counted.saturating_sub(charged),
            unresolved,
            left: short(left),
            debt: counted.saturating_sub(u128::from(allowed)),
            needs_owner,
        }
    }

    /// The balance of the newest epoch: what an admission is judged on.
    pub fn balance(&self) -> Balance {
        self.balance_of(self.epoch())
    }

    /// What a run sent under the reservation of the run at the root of its tree: a
    /// number where that reservation's names are a set the ledger could have written;
    /// "unknown, at most" where they are not; and "unknown" with no most where no
    /// name can be trusted for its amount either.
    pub fn sent_by(&self, run: &RunId, root: &RunId) -> Sent {
        match self.reservations.get(root) {
            Some(one) if !one.reachable && one.epoch.is_none() => Sent::Unbounded,
            Some(one) if !one.reachable => Sent::Unknown {
                at_most: one.most(),
            },
            _ => Sent::Known(self.runs.get(run).copied().unwrap_or_default()),
        }
    }

    /// Whether an admission that was published stands, on this reading of the names
    /// made after its publication (ADR 0016, "An admission is confirmed before
    /// anything runs under it"): its reservation is open and is the record, one hold
    /// and nothing else that names the run; neither the epoch it fell under nor the
    /// newest needs its owner; and that epoch is not over its allowance with this
    /// reservation counted in full. A call of nothing adds nothing to what is
    /// counted and is not held to a debt that was there before it.
    pub fn stands(&self, run: &RunId, micros: u64) -> bool {
        let own = self.reservations.get(run);
        // Open, it is a set the ledger could have written, which has the record and
        // one hold: with two names, it has no other.
        let Some(epoch) = own
            .filter(|one| one.state == State::Open && one.names == 2)
            .and_then(|one| one.epoch)
        else {
            return false;
        };
        if self.needs_owner(epoch) || self.needs_owner(self.epoch()) {
            return false;
        }
        micros == 0 || self.balance_of(epoch).debt == 0
    }

    /// Whether the owner's correction of this reservation to this amount is taken:
    /// only of one that is closed and is interrupted, frozen or corrected already;
    /// from what its whole settlements say, as the least, to the most its names can
    /// mean; and, where it raises what the reservation counts for in the newest
    /// epoch, only when what is left there can hold the difference.
    pub fn correction(&self, run: &RunId, micros: u64) -> Result<(), NotCorrected> {
        let Some(one) = self.reservations.get(run) else {
            return Err(NotCorrected::NotCorrectable);
        };
        let correctable = matches!(
            one.state,
            State::Interrupted | State::Frozen | State::Adjusted
        );
        if !one.closed || !correctable {
            return Err(NotCorrected::NotCorrectable);
        }
        let asked = u128::from(micros);
        if asked < one.settled {
            return Err(NotCorrected::BelowSettled {
                settled: one.settled,
            });
        }
        if asked > one.most() {
            return Err(NotCorrected::OverCount { most: one.most() });
        }
        let left = self.balance().left;
        let more = asked.saturating_sub(one.counts);
        if one.counts_in(self.epoch()) && more > u128::from(left) {
            return Err(NotCorrected::CannotBeHeld { left });
        }
        Ok(())
    }

    /// How many names under `spend/` are of no form and could be placed nowhere: in
    /// no reservation and in no epoch. They change no balance, and a caller that
    /// shows the books says that they are there (ADR 0016, "Limits").
    pub fn unplaced(&self) -> u64 {
        self.unplaced
    }
}

/// An amount as a number of 64 bits: the largest where it is more.
fn short(amount: u128) -> u64 {
    u64::try_from(amount).unwrap_or(u64::MAX)
}

/// The lowest number from 1 that is not among these. It follows the numbers that
/// are there and never counts up to one.
fn lowest_free(taken: &BTreeSet<u64>) -> u64 {
    let mut free = 1;
    for one in taken {
        if *one == free {
            free = free.saturating_add(1);
        }
    }
    free
}

/// Folds the books of this generation. `read` gives the content of an entry; a
/// content that cannot be read, or that does not agree with its name, is damage, and
/// never a failure.
///
/// Content is read through a name of the generation and in no other way: bytes that
/// the store holds under no name are not in the books.
pub fn fold(generation: &Generation, read: impl Fn(&Entry) -> store::Result<Vec<u8>>) -> Folded {
    across(temporary::gather(generation, read))
}

/// Folds names that were gathered already, each with whether its content is damaged:
/// what [`fold`] does once it has read the generation.
pub fn fold_names<'a>(names: impl Iterator<Item = (&'a str, bool)>) -> Folded {
    across(temporary::gather_names(names))
}

/// Everything that is of more than one reservation.
fn across(store: Store) -> Folded {
    let mut folded = Folded {
        turns: store.turns,
        garbled: store.garbled,
        unplaced: store.unplaced,
        ..Folded::default()
    };
    // An epoch is there when a reset or its guard names it. Its allowance is its
    // reset's only with the guard and with neither damaged; two resets of one epoch
    // are not the ledger's doing, and the smaller is the one that cannot allow too
    // much. Anything else allows nothing until the owner resets again.
    for (epoch, of) in &store.epochs {
        let whole = of.guard && !of.spoiled;
        let least = of.resets.iter().min().copied().filter(|_| whole);
        folded.resets.insert(*epoch, least.unwrap_or(0));
    }
    let newest = folded.epoch();
    // What each epoch's turns are due: one for the reset that closed it, one for
    // each hold in it, and one for each settlement, closing and correction of a
    // reservation with a hold in it.
    let owe = |due: &mut BTreeMap<u64, u64>, epoch: u64, turns: u64| {
        let due = due.entry(epoch).or_default();
        *due = due.saturating_add(turns);
    };
    for epoch in folded.resets.keys().filter(|epoch| **epoch > 0) {
        owe(&mut folded.due, *epoch - 1, 1);
    }
    for (run, of) in store.reservations {
        let epochs: BTreeSet<u64> = of.holds.iter().map(|(epoch, _)| *epoch).collect();
        for (epoch, _) in &of.holds {
            owe(&mut folded.due, *epoch, 1);
        }
        for epoch in &epochs {
            owe(&mut folded.due, *epoch, of.turns_taken());
        }
        for (by, sent) in &of.by {
            let spend = folded.runs.entry(by.clone()).or_default();
            spend.charged = spend.charged.saturating_add(sent.charged);
            spend.outstanding = spend.outstanding.saturating_add(sent.outstanding);
            spend.unresolved = spend.unresolved.saturating_add(sent.unresolved);
        }
        let standing = standing(&of, epochs, newest, &folded.turns);
        folded.reservations.insert(run, standing);
    }
    folded
}

/// One reservation's state and what it counts for.
fn standing(
    of: &Reading,
    epochs: BTreeSet<u64>,
    newest: u64,
    turns: &BTreeMap<u64, BTreeSet<u64>>,
) -> Standing {
    let closed = !of.closings.is_empty();
    // A correction stands when all that its own reservation's names can show of it
    // holds (it is not damaged, the place it names is there with nothing else at it,
    // and its seal is of exactly the reservation's other names as they are), its
    // reservation has a closing, and the turn it names is there, in the epoch of the
    // reservation's hold. Each is sealed over every other, so no two stand.
    let turn_epoch = epochs.first().copied().unwrap_or(newest);
    let taken = turns.get(&turn_epoch);
    let stands = |one: &&temporary::Correction| {
        one.sealed && closed && taken.is_some_and(|turns| turns.contains(&one.turn))
    };
    let stands = of.corrections.iter().find(stands).map(|one| one.micros);
    let corrections = of.corrections.iter().map(|one| one.micros).max();
    let epoch = epochs.first().copied().filter(|_| of.holds.len() == 1);
    let most = of.most();
    let known = of.charged.saturating_add(of.outstanding);
    let (state, counts) = match stands {
        // A correction never counts under what settlements with their whole chain
        // say.
        Some(micros) => (State::Adjusted, u128::from(micros).max(of.settled)),
        None if !of.reachable => (State::Frozen, most),
        None if of.closings == [true] => (State::Released, known),
        None if closed => (State::Interrupted, most),
        None => (State::Open, most),
    };
    Standing {
        epoch,
        epochs,
        amount: of.amount(),
        charged: of.charged,
        settled: of.settled,
        outstanding: of.outstanding,
        unresolved: of.unresolved,
        unsettled: of.unsettled,
        unexplained: of.unexplained,
        corrections: corrections.unwrap_or(0),
        reachable: of.reachable,
        names: of.names,
        counts,
        // The ledger's own hold may be the one that is lost, whatever holds are
        // there in its place: a frozen reservation that has not exactly one says
        // that its amount is unknown.
        unknown: state == State::Frozen && epoch.is_none(),
        state,
        closed,
        most,
    }
}

/// TEMPORARY, and not for review: a reader of the names under `spend/` and a reading
/// of one reservation, written after the model of ADR 0016
/// (`crates/whizbang-testkit/src/spend.rs`) so that the fold above could be built and
/// held to the model before the second part of task I2 merged. That task owns the
/// one parser (`spend::name`), the records (`spend::record`) and the reading of one
/// reservation with its recogniser, its frozen sum, its seal and the shapes of a
/// reading (`spend::reading`). This module is deleted when the fold moves onto them.
pub mod temporary {
    use std::collections::{BTreeMap, BTreeSet};

    use sha2::{Digest as _, Sha256};

    use crate::contract::RunId;
    use crate::contract::ledger::{RESERVATIONS_PREFIX, Reservation, SPEND_PREFIX};
    use crate::store::{self, Entry, Generation};

    /// What a reservation or an epoch counts for, as far as the books can say.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Counted {
        Exactly(u128),
        /// The books cannot say how much: no less than this.
        AtLeast(u128),
    }

    /// What one run sent under its tree's reservation.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct RunSpend {
        pub charged: u128,
        pub outstanding: u128,
        pub unresolved: u128,
    }

    impl RunSpend {
        pub fn known(&self) -> u128 {
            self.charged.saturating_add(self.outstanding)
        }
    }

    /// What the books can say of one run's spend.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Sent {
        Known(RunSpend),
        Unknown { at_most: u128 },
        Unbounded,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Stage {
        NotSent,
        Answered,
        Unresolved,
    }

    enum Name {
        Reset {
            epoch: u64,
            micros: u64,
        },
        Epoch(u64),
        Turn {
            epoch: u64,
            turn: u64,
        },
        Reservation(RunId),
        Held {
            epoch: u64,
            run: RunId,
            micros: u64,
        },
        Place {
            run: RunId,
            place: u64,
        },
        Dispatched {
            run: RunId,
            place: u64,
            by: RunId,
            micros: u64,
        },
        Settling {
            run: RunId,
            place: u64,
        },
        Settled {
            run: RunId,
            place: u64,
            stage: Stage,
            micros: u64,
        },
        Closed {
            run: RunId,
            released: bool,
        },
        Adjusted {
            run: RunId,
            place: u64,
            turn: u64,
            seal: u128,
            micros: u64,
        },
        /// Of no form, and it starts as a name of this run's reservation does.
        Stray(RunId),
        /// Of no form, and it starts as a turn of this epoch does.
        GarbledTurn(u64),
        /// Of no form, and it starts as a reset or a guard of this epoch does.
        GarbledEpoch(u64),
        Unplaced,
    }

    fn number(part: &str) -> Option<u64> {
        let plain = !part.is_empty()
            && part.len() <= 20
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'));
        plain.then(|| part.parse().ok()).flatten()
    }

    fn formed(parts: &[&str]) -> Option<Name> {
        let run = |part: &str| RunId::parse(part).ok();
        let amount = |part: &str| number(part).filter(|micros| *micros != u64::MAX);
        let from_one = |part: &str| number(part).filter(|count| *count >= 1);
        let sealed = |part: &str| -> Option<u128> {
            let plain = part.len() == 32
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
            plain.then(|| u128::from_str_radix(part, 16).ok()).flatten()
        };
        Some(match parts {
            ["reset", epoch, micros] => Name::Reset {
                epoch: from_one(epoch)?,
                micros: amount(micros)?,
            },
            ["epoch", epoch] => Name::Epoch(from_one(epoch)?),
            ["turn", epoch, turn] => Name::Turn {
                epoch: number(epoch)?,
                turn: from_one(turn)?,
            },
            ["reservations", id] => Name::Reservation(run(id)?),
            ["held", epoch, id, micros] => Name::Held {
                epoch: number(epoch)?,
                run: run(id)?,
                micros: amount(micros)?,
            },
            ["place", id, place] => Name::Place {
                run: run(id)?,
                place: from_one(place)?,
            },
            ["dispatched", id, place, by, micros] => Name::Dispatched {
                run: run(id)?,
                place: from_one(place)?,
                by: run(by)?,
                micros: amount(micros)?,
            },
            ["settling", id, place] => Name::Settling {
                run: run(id)?,
                place: from_one(place)?,
            },
            ["settled", id, place, stage, micros] => Name::Settled {
                run: run(id)?,
                place: from_one(place)?,
                stage: match *stage {
                    "not_sent" => Stage::NotSent,
                    "answered" => Stage::Answered,
                    "unresolved" => Stage::Unresolved,
                    _ => return None,
                },
                micros: amount(micros)?,
            },
            ["closed", id, how @ ("released" | "held")] => Name::Closed {
                run: run(id)?,
                released: *how == "released",
            },
            ["adjusted", id, place, turn, seal, micros] => Name::Adjusted {
                run: run(id)?,
                place: from_one(place)?,
                turn: from_one(turn)?,
                seal: sealed(seal)?,
                micros: amount(micros)?,
            },
            _ => return None,
        })
    }

    fn parse(text: &str) -> Name {
        let Some(rest) = text.strip_prefix(SPEND_PREFIX) else {
            return Name::Unplaced;
        };
        let parts: Vec<&str> = rest.split('/').collect();
        if text.len() <= 200
            && let Some(name) = formed(&parts)
        {
            return name;
        }
        let at = |index: usize| parts.get(index).copied();
        let run = |part: Option<&str>| part.and_then(|part| RunId::parse(part).ok());
        let stray = |part| run(part).map_or(Name::Unplaced, Name::Stray);
        match at(0) {
            Some("held") => stray(at(2)),
            Some(
                "reservations" | "place" | "dispatched" | "settling" | "settled" | "closed"
                | "adjusted",
            ) => stray(at(1)),
            Some("turn") => at(1)
                .and_then(number)
                .map_or(Name::Unplaced, Name::GarbledTurn),
            Some("reset" | "epoch") => at(1)
                .and_then(number)
                .filter(|epoch| *epoch >= 1)
                .map_or(Name::Unplaced, Name::GarbledEpoch),
            _ => Name::Unplaced,
        }
    }

    /// The seal of a correction: the first 32 digits of base sixteen of a SHA-256
    /// over these names, each as its text, then `!` when it is damaged, then a
    /// newline.
    pub fn seal<'a>(names: impl Iterator<Item = (&'a str, bool)>) -> u128 {
        let mut hash = Sha256::new();
        for (name, damaged) in names {
            hash.update(name.as_bytes());
            hash.update(if damaged { &b"!\n"[..] } else { &b"\n"[..] });
        }
        let digest: [u8; 32] = hash.finalize().into();
        let mut first = [0u8; 16];
        first.copy_from_slice(&digest[..16]);
        u128::from_be_bytes(first)
    }

    /// One epoch's resets and guard.
    #[derive(Default)]
    pub(super) struct Epoch {
        pub(super) resets: Vec<u64>,
        pub(super) guard: bool,
        pub(super) spoiled: bool,
    }

    /// What a generation's names under `spend/` are, sorted.
    #[derive(Default)]
    pub(super) struct Store {
        pub(super) epochs: BTreeMap<u64, Epoch>,
        pub(super) turns: BTreeMap<u64, BTreeSet<u64>>,
        pub(super) garbled: BTreeSet<u64>,
        pub(super) unplaced: u64,
        pub(super) reservations: BTreeMap<RunId, Reading>,
    }

    pub(super) struct Correction {
        pub(super) turn: u64,
        pub(super) micros: u64,
        /// All that the reservation's own names can show of it holds.
        pub(super) sealed: bool,
    }

    /// One reservation, read by itself.
    #[derive(Default)]
    pub(super) struct Reading {
        pub(super) holds: Vec<(u64, u64)>,
        pub(super) settlings: u64,
        pub(super) closings: Vec<bool>,
        pub(super) corrections: Vec<Correction>,
        pub(super) charged: u128,
        pub(super) settled: u128,
        pub(super) outstanding: u128,
        pub(super) unresolved: u128,
        pub(super) unsettled: u64,
        pub(super) unexplained: u64,
        pub(super) reachable: bool,
        pub(super) names: u64,
        pub(super) by: BTreeMap<RunId, RunSpend>,
    }

    impl Reading {
        pub(super) fn amount(&self) -> u64 {
            let amounts = self.holds.iter().map(|(_, micros)| *micros);
            amounts.max().unwrap_or(0)
        }

        /// The turns its publications took in each epoch a hold of it names.
        pub(super) fn turns_taken(&self) -> u64 {
            self.settlings
                .saturating_add(self.closings.len() as u64)
                .saturating_add(self.corrections.len() as u64)
        }

        pub(super) fn most(&self) -> u128 {
            let amount = u128::from(self.amount());
            let known = self.charged.saturating_add(self.outstanding);
            let named = match self.reachable {
                true => known,
                false => {
                    let places = u128::from(self.unexplained).saturating_mul(amount);
                    amount.saturating_add(known).saturating_add(places)
                }
            };
            let corrected = self.corrections.iter().map(|one| one.micros).max();
            u128::from(self.amount().max(corrected.unwrap_or(0))).max(named)
        }
    }

    struct Adjusted {
        text: String,
        place: u64,
        turn: u64,
        seal: u128,
        micros: u64,
        damaged: bool,
    }

    #[derive(Default)]
    struct Found {
        all: Vec<(String, bool)>,
        holds: Vec<(u64, u64)>,
        record: bool,
        places: BTreeSet<u64>,
        dispatched: BTreeMap<u64, Vec<(RunId, u64)>>,
        settling: BTreeSet<u64>,
        settled: BTreeMap<u64, Vec<(Stage, u64)>>,
        hurt: BTreeSet<u64>,
        closed: Vec<bool>,
        adjusted: Vec<Adjusted>,
        stray: bool,
        damaged: bool,
    }

    fn allowed(stage: Stage, charge: u64, bound: u64) -> bool {
        match stage {
            Stage::NotSent => charge == 0,
            Stage::Unresolved => charge == bound,
            Stage::Answered => true,
        }
    }

    fn total(amounts: impl Iterator<Item = u64>) -> u128 {
        amounts.fold(0, |all, one| all.saturating_add(u128::from(one)))
    }

    fn reachable(of: &Found) -> bool {
        if of.damaged || of.stray || !of.record || of.holds.len() != 1 {
            return false;
        }
        let count = of.places.len() as u64;
        if of.places.last().copied().unwrap_or(0) != count {
            return false;
        }
        let mut overruns = 0u64;
        for (place, sent) in &of.dispatched {
            let [(_, bound)] = sent.as_slice() else {
                return false;
            };
            if !of.places.contains(place) {
                return false;
            }
            match of.settled.get(place).map(Vec::as_slice) {
                None => {
                    if of.settling.contains(place) {
                        return false;
                    }
                }
                Some([(stage, charge)]) => {
                    if !of.settling.contains(place) || !allowed(*stage, *charge, *bound) {
                        return false;
                    }
                    overruns += u64::from(charge > bound);
                }
                Some(_) => return false,
            }
        }
        let mut ended = of.settled.keys().chain(&of.settling);
        if ended.any(|place| !of.dispatched.contains_key(place)) {
            return false;
        }
        if of.closed.len() > 1 {
            return false;
        }
        let unsettled = |place: &u64| !of.settled.contains_key(place);
        if of.closed == [true] && of.dispatched.keys().any(unsettled) {
            return false;
        }
        let mut taken = BTreeSet::new();
        for one in &of.adjusted {
            let own = of.places.contains(&one.place) && !of.dispatched.contains_key(&one.place);
            if of.closed != [false] || !own || !taken.insert(one.place) {
                return false;
            }
        }
        let amount = of
            .holds
            .first()
            .map_or(0, |(_, micros)| u128::from(*micros));
        let last = of.dispatched.keys().next_back().copied().unwrap_or(0);
        let mut known = 0u128;
        let mut excesses: BTreeSet<(u64, u64)> = BTreeSet::new();
        for place in of.places.range(..=last) {
            let Some([(_, bound)]) = of.dispatched.get(place).map(Vec::as_slice) else {
                let Some((excess, _)) = excesses.pop_first() else {
                    return false;
                };
                known = known.saturating_add(u128::from(excess));
                continue;
            };
            if u128::from(*bound) > amount.saturating_sub(known) {
                return false;
            }
            let charge = of.settled.get(place).and_then(|ended| ended.first());
            let charge = charge.map_or(*bound, |(_, charge)| *charge);
            known = known.saturating_add(u128::from(charge.min(*bound)));
            if charge > *bound {
                excesses.insert((charge - *bound, *place));
            }
        }
        if taken
            .first()
            .is_some_and(|lowest| *lowest <= last.saturating_add(1))
        {
            return false;
        }
        let bare = count - of.dispatched.len() as u64;
        bare == overruns + of.closed.len() as u64 + of.adjusted.len() as u64
    }

    fn read(run: &RunId, of: &Found) -> Reading {
        let base = reachable(of);
        let mut reading = Reading {
            holds: of.holds.clone(),
            settlings: of.settling.len() as u64,
            closings: of.closed.clone(),
            reachable: base,
            names: of.all.len() as u64,
            ..Reading::default()
        };
        let at: BTreeSet<u64> = of
            .dispatched
            .keys()
            .chain(of.settled.keys())
            .copied()
            .collect();
        for place in &at {
            let sent = of.dispatched.get(place).map_or(&[][..], Vec::as_slice);
            let ended = of.settled.get(place).map_or(&[][..], Vec::as_slice);
            let bounds = total(sent.iter().map(|(_, micros)| *micros));
            let charge = total(ended.iter().map(|(_, micros)| *micros));
            let open = ended
                .iter()
                .filter(|(stage, _)| *stage == Stage::Unresolved);
            let open = total(open.map(|(_, micros)| *micros));
            let guarded = of.settling.contains(place);
            let whole = match (sent, ended) {
                ([(_, bound)], [(stage, charge)]) => {
                    of.places.contains(place)
                        && guarded
                        && !of.hurt.contains(place)
                        && allowed(*stage, *charge, *bound)
                }
                _ => false,
            };
            let rest = match whole && base {
                true => 0,
                false => bounds,
            };
            if whole {
                reading.settled = reading.settled.saturating_add(charge);
            } else {
                reading.unsettled = reading.unsettled.saturating_add(1);
            }
            let open = open.saturating_add(rest);
            reading.charged = reading.charged.saturating_add(charge);
            reading.outstanding = reading.outstanding.saturating_add(rest);
            reading.unresolved = reading.unresolved.saturating_add(open);
            let by: BTreeSet<&RunId> = sent.iter().map(|(by, _)| by).collect();
            for by in by {
                let spend = reading.by.entry(by.clone()).or_default();
                spend.charged = spend.charged.saturating_add(charge);
                spend.outstanding = spend.outstanding.saturating_add(rest);
                spend.unresolved = spend.unresolved.saturating_add(open);
            }
        }
        if !base {
            let corrected = of.adjusted.iter().map(|one| &one.place);
            let named = of
                .places
                .iter()
                .chain(&at)
                .chain(&of.settling)
                .chain(corrected);
            let highest = named.max().copied().unwrap_or(0);
            reading.unexplained = highest.saturating_sub(of.dispatched.len() as u64);
        }
        for one in &of.adjusted {
            let guard = format!("{SPEND_PREFIX}place/{run}/{}", one.place);
            let others = of
                .all
                .iter()
                .filter(|(name, _)| *name != one.text && *name != guard);
            let over = seal(others.map(|(name, hurt)| (name.as_str(), *hurt)));
            let sealed = !one.damaged
                && of.places.contains(&one.place)
                && !of.hurt.contains(&one.place)
                && !at.contains(&one.place)
                && !of.settling.contains(&one.place)
                && one.seal == over;
            reading.corrections.push(Correction {
                turn: one.turn,
                micros: one.micros,
                sealed,
            });
        }
        reading
    }

    pub(super) fn gather_names<'a>(names: impl Iterator<Item = (&'a str, bool)>) -> Store {
        let mut store = Store::default();
        let mut found: BTreeMap<RunId, Found> = BTreeMap::new();
        for (text, hurt) in names {
            if !text.starts_with(SPEND_PREFIX) {
                continue;
            }
            let name = parse(text);
            let run = match &name {
                Name::Reset { epoch, micros } => {
                    let of = store.epochs.entry(*epoch).or_default();
                    of.resets.push(*micros);
                    of.spoiled |= hurt;
                    continue;
                }
                Name::Epoch(epoch) => {
                    let of = store.epochs.entry(*epoch).or_default();
                    of.guard = true;
                    of.spoiled |= hurt;
                    continue;
                }
                Name::GarbledEpoch(epoch) => {
                    store.epochs.entry(*epoch).or_default().spoiled = true;
                    continue;
                }
                Name::Turn { epoch, turn } => {
                    store.turns.entry(*epoch).or_default().insert(*turn);
                    continue;
                }
                Name::GarbledTurn(epoch) => {
                    store.garbled.insert(*epoch);
                    continue;
                }
                Name::Unplaced => {
                    store.unplaced = store.unplaced.saturating_add(1);
                    continue;
                }
                Name::Reservation(run)
                | Name::Held { run, .. }
                | Name::Place { run, .. }
                | Name::Dispatched { run, .. }
                | Name::Settling { run, .. }
                | Name::Settled { run, .. }
                | Name::Closed { run, .. }
                | Name::Adjusted { run, .. }
                | Name::Stray(run) => run.clone(),
            };
            let of = found.entry(run).or_default();
            of.all.push((text.to_owned(), hurt));
            of.damaged |= hurt;
            let place = match &name {
                Name::Place { place, .. }
                | Name::Dispatched { place, .. }
                | Name::Settling { place, .. }
                | Name::Settled { place, .. } => Some(*place),
                _ => None,
            };
            if let Some(place) = place.filter(|_| hurt) {
                of.hurt.insert(place);
            }
            match name {
                Name::Reservation(_) => of.record = true,
                Name::Held { epoch, micros, .. } => of.holds.push((epoch, micros)),
                Name::Place { place, .. } => {
                    of.places.insert(place);
                }
                Name::Dispatched {
                    place, by, micros, ..
                } => {
                    of.dispatched.entry(place).or_default().push((by, micros));
                }
                Name::Settling { place, .. } => {
                    of.settling.insert(place);
                }
                Name::Settled {
                    place,
                    stage,
                    micros,
                    ..
                } => {
                    of.settled.entry(place).or_default().push((stage, micros));
                }
                Name::Closed { released, .. } => of.closed.push(released),
                Name::Adjusted {
                    place,
                    turn,
                    seal,
                    micros,
                    ..
                } => of.adjusted.push(Adjusted {
                    text: text.to_owned(),
                    place,
                    turn,
                    seal,
                    micros,
                    damaged: hurt,
                }),
                Name::Stray(_) => of.stray = true,
                _ => {}
            }
        }
        for (run, of) in found {
            let reading = read(&run, &of);
            store.reservations.insert(run, reading);
        }
        store
    }

    /// The content this temporary reader takes as sound for a name that is not the
    /// executor's reservation: the name and a newline.
    pub fn content_of(name: &str) -> Vec<u8> {
        format!("{name}\n").into_bytes()
    }

    pub(super) fn gather(
        generation: &Generation,
        read: impl Fn(&Entry) -> store::Result<Vec<u8>>,
    ) -> Store {
        let mut names: Vec<(&str, bool)> = Vec::new();
        for (name, entry) in generation.entries().range(SPEND_PREFIX.to_owned()..) {
            if !name.starts_with(SPEND_PREFIX) {
                break;
            }
            let sound = entry.bytes <= 1024
                && read(entry).is_ok_and(|bytes| match name.strip_prefix(RESERVATIONS_PREFIX) {
                    Some(run) => Reservation::from_bytes(&bytes)
                        .is_ok_and(|reservation| reservation.run().as_str() == run),
                    None => bytes == content_of(name),
                });
            names.push((name.as_str(), !sound));
        }
        gather_names(names.into_iter())
    }
}
