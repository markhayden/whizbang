//! A model of the ledger's books (ADR 0016): pure, with no store.
//!
//! The books are a set of names, as the store's generation holds them under `spend/`,
//! and a note of which of them have content that cannot be read. Every amount is in a
//! name. [`Books::apply`] takes one [`Event`] of a history and does what the ledger's
//! code is to do with it: it judges on the names, and publishes the names the event
//! makes, all of them or, when one of them is there already, none. [`fold`] answers
//! what is allowed, counted, charged and left, for the store, for one reservation and
//! for one run, from the names alone.
//!
//! Beside the names the model keeps two things that are not in a store. What a process
//! holds in memory: the runs it has open, each with a signed allowance; a crash loses
//! it. And the truth: what was really dispatched and charged, written down as it
//! happens and never read by the fold, so that [`Books::check`] can hold what the fold
//! answers to it.
//!
//! The table of histories is [`histories`]: each history the reviews of the plan for
//! checkpoint 1B produced, by name, with the balances it must give. A later ledger
//! task adds its histories there, and holds the real fold to the same answers.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A run, by a number: its id in a name is `7000-<number>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Run(pub u32);

impl fmt::Display for Run {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "7000-{}", self.0)
    }
}

/// How far a request got, as a settlement's name says it (ADR 0010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    NotSent,
    Answered,
    Unresolved,
}

impl Stage {
    pub const ALL: [Stage; 3] = [Stage::NotSent, Stage::Answered, Stage::Unresolved];

    pub fn name(self) -> &'static str {
        match self {
            Stage::NotSent => "not_sent",
            Stage::Answered => "answered",
            Stage::Unresolved => "unresolved",
        }
    }
}

/// The longest name under `spend/`, in characters (ADR 0007).
pub const NAME_LIMIT: usize = 200;

/// A name under `spend/`: every form the grammar of ADR 0016 has, and one for a name
/// of no form that still names a run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Name {
    /// `spend/reset/<epoch>/<micros>`: the allowance of an epoch.
    Reset { epoch: u64, micros: u64 },
    /// `spend/turn/<n>`: made by whatever can lower what is left.
    Turn(u64),
    /// `spend/reservations/<run>`: the executor's record of a reservation.
    Reservation(Run),
    /// `spend/held/<epoch>/<run>/<micros>`: a reservation's amount, and its epoch.
    Held { epoch: u64, run: Run, micros: u64 },
    /// `spend/place/<run>/<n>`: the place `n` under a reservation is taken.
    Place { run: Run, place: u64 },
    /// `spend/dispatched/<run>/<n>/<by>/<micros>`: a request's bound, before it left.
    Dispatched {
        run: Run,
        place: u64,
        by: Run,
        micros: u64,
    },
    /// `spend/settling/<run>/<n>`: the place `n` is settled, once.
    Settling { run: Run, place: u64 },
    /// `spend/settled/<run>/<n>/<stage>/<micros>`: what the request was charged.
    Settled {
        run: Run,
        place: u64,
        stage: Stage,
        micros: u64,
    },
    /// `spend/closed/<run>/released` or `spend/closed/<run>/held`: the reservation
    /// is closed, and whether its closing released what it held beyond its charges.
    Closed { run: Run, released: bool },
    /// `spend/adjusted/<run>/<n>/<micros>`: a person's `n`th correction.
    Adjusted { run: Run, count: u64, micros: u64 },
    /// `spend/stray/<run>/<n>`: stands for any name under `spend/` that names this
    /// run where a reservation's run is named and is of none of the forms above.
    Stray { run: Run, count: u64 },
}

impl Name {
    /// The run whose reservation this name belongs to, when it belongs to one.
    pub fn of(&self) -> Option<Run> {
        match self {
            Name::Reset { .. } | Name::Turn(_) => None,
            Name::Reservation(run) => Some(*run),
            Name::Closed { run, .. }
            | Name::Held { run, .. }
            | Name::Place { run, .. }
            | Name::Dispatched { run, .. }
            | Name::Settling { run, .. }
            | Name::Settled { run, .. }
            | Name::Adjusted { run, .. }
            | Name::Stray { run, .. } => Some(*run),
        }
    }

    /// The name this text is, read strictly: a number is decimal digits with no
    /// leading zero, nothing is left over, and a name of no form is none.
    pub fn parse(text: &str) -> Option<Name> {
        if text.len() > NAME_LIMIT {
            return None;
        }
        let number = |part: &str| -> Option<u64> {
            let plain = !part.is_empty()
                && part.len() <= 20
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part == "0" || !part.starts_with('0'));
            plain.then(|| part.parse().ok()).flatten()
        };
        let run = |part: &str| -> Option<Run> {
            let count = number(part.strip_prefix("7000-")?)?;
            u32::try_from(count).ok().map(Run)
        };
        let amount = |part: &str| number(part).filter(|micros| *micros != u64::MAX);
        let from_one = |part: &str| number(part).filter(|count| *count >= 1);
        let parts: Vec<&str> = text.strip_prefix("spend/")?.split('/').collect();
        Some(match parts.as_slice() {
            ["reset", epoch, micros] => Name::Reset {
                epoch: from_one(epoch)?,
                micros: amount(micros)?,
            },
            ["turn", count] => Name::Turn(from_one(count)?),
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
                stage: Stage::ALL.into_iter().find(|one| one.name() == *stage)?,
                micros: amount(micros)?,
            },
            ["closed", id, how @ ("released" | "held")] => Name::Closed {
                run: run(id)?,
                released: *how == "released",
            },
            ["adjusted", id, count, micros] => Name::Adjusted {
                run: run(id)?,
                count: from_one(count)?,
                micros: amount(micros)?,
            },
            ["stray", id, count] => Name::Stray {
                run: run(id)?,
                count: from_one(count)?,
            },
            _ => return None,
        })
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Name::Reset { epoch, micros } => write!(f, "spend/reset/{epoch}/{micros}"),
            Name::Turn(count) => write!(f, "spend/turn/{count}"),
            Name::Reservation(run) => write!(f, "spend/reservations/{run}"),
            Name::Held { epoch, run, micros } => write!(f, "spend/held/{epoch}/{run}/{micros}"),
            Name::Place { run, place } => write!(f, "spend/place/{run}/{place}"),
            Name::Dispatched {
                run,
                place,
                by,
                micros,
            } => write!(f, "spend/dispatched/{run}/{place}/{by}/{micros}"),
            Name::Settling { run, place } => write!(f, "spend/settling/{run}/{place}"),
            Name::Settled {
                run,
                place,
                stage,
                micros,
            } => write!(f, "spend/settled/{run}/{place}/{}/{micros}", stage.name()),
            Name::Closed { run, released } => {
                let how = if *released { "released" } else { "held" };
                write!(f, "spend/closed/{run}/{how}")
            }
            Name::Adjusted { run, count, micros } => {
                write!(f, "spend/adjusted/{run}/{count}/{micros}")
            }
            Name::Stray { run, count } => write!(f, "spend/stray/{run}/{count}"),
        }
    }
}

/// What a reservation is, as the fold finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Not closed: its run executes, or was left and is not recovered yet.
    Open,
    /// Closed with every dispatch settled: it counts for its charges, and what it
    /// held beyond them is released.
    Released,
    /// Closed with nothing released.
    Interrupted,
    /// Something under it is damaged or cannot be placed: nothing of it is released,
    /// whatever its closing said, until a person's correction.
    Frozen,
    /// A person corrected it: it counts for what the last correction says.
    Adjusted,
}

/// One reservation, as the fold answers it. Amounts are in millionths of a US dollar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    /// The epoch it falls under: the reset its hold names.
    pub epoch: u64,
    /// What was reserved: the amount its hold names.
    pub amount: u64,
    /// What its settlements say was charged, unresolved ones at their bounds.
    pub charged: u128,
    /// The bounds of its dispatches that have no settlement.
    pub outstanding: u128,
    /// The part of `charged` and all of `outstanding` that no answer settled.
    pub unresolved: u128,
    /// What it counts for against its epoch's allowance.
    pub counts: u128,
    pub state: State,
    pub closed: bool,
}

impl Standing {
    /// What is known against it: its charges, and the bounds not settled.
    pub fn known(&self) -> u128 {
        self.charged + self.outstanding
    }

    /// What a dispatch under it may still take: its amount less what is known.
    pub fn left(&self) -> u64 {
        u64::try_from(u128::from(self.amount).saturating_sub(self.known())).unwrap_or(u64::MAX)
    }
}

/// The store's balance for one epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Balance {
    pub epoch: u64,
    /// The allowance the epoch's reset set; nothing before the first reset.
    pub allowed: u64,
    /// What its reservations count for, together.
    pub counted: u128,
    /// What was charged under them.
    pub charged: u128,
    /// What is held beyond what was charged.
    pub held: u128,
    pub unresolved: u128,
    /// What can still be reserved: never below nothing.
    pub left: u64,
    /// By how much what is counted is over the allowance.
    pub debt: u128,
}

/// What one run sent under its tree's reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunSpend {
    pub charged: u128,
    pub outstanding: u128,
    pub unresolved: u128,
}

/// The books folded: every reservation the names give, by its run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Folded {
    pub reservations: BTreeMap<Run, Standing>,
    pub runs: BTreeMap<Run, RunSpend>,
    /// The allowance of each epoch that has a reset.
    pub resets: BTreeMap<u64, u64>,
    pub turns: u64,
}

impl Folded {
    /// The newest epoch: nothing before the first reset.
    pub fn epoch(&self) -> u64 {
        self.resets.keys().next_back().copied().unwrap_or(0)
    }

    /// The balance of this epoch.
    pub fn balance_of(&self, epoch: u64) -> Balance {
        let allowed = self.resets.get(&epoch).copied().unwrap_or(0);
        let of_epoch = || self.reservations.values().filter(|one| one.epoch == epoch);
        let counted: u128 = of_epoch().map(|one| one.counts).sum();
        let charged: u128 = of_epoch().map(|one| one.charged).sum();
        let unresolved: u128 = of_epoch().map(|one| one.unresolved).sum();
        let left = u128::from(allowed).saturating_sub(counted);
        Balance {
            epoch,
            allowed,
            counted,
            charged,
            held: counted.saturating_sub(charged),
            unresolved,
            left: u64::try_from(left).unwrap_or(u64::MAX),
            debt: counted.saturating_sub(u128::from(allowed)),
        }
    }

    /// The balance of the newest epoch: what an admission is judged on.
    pub fn balance(&self) -> Balance {
        self.balance_of(self.epoch())
    }
}

/// How many places are taken under a reservation: the next is one more.
fn places(names: &BTreeSet<Name>, run: Run) -> u64 {
    let of_run = |name: &&Name| matches!(name, Name::Place { run: of, .. } if *of == run);
    names.iter().filter(of_run).count() as u64
}

/// Folds the books: what the names say, and nothing else. `damaged` are the names
/// whose content cannot be read, or does not agree with its name.
pub fn fold(names: &BTreeSet<Name>, damaged: &BTreeSet<Name>) -> Folded {
    let mut folded = Folded::default();
    #[derive(Default)]
    struct Found {
        holds: Vec<(u64, u64)>,
        record: bool,
        places: BTreeSet<u64>,
        dispatched: BTreeMap<u64, Vec<(Run, u64)>>,
        settling: BTreeSet<u64>,
        settled: BTreeMap<u64, Vec<(Stage, u64)>>,
        /// The closings: whether each released.
        closed: Vec<bool>,
        adjusted: BTreeMap<u64, Vec<u64>>,
        stray: bool,
        damaged: bool,
    }
    let mut found: BTreeMap<Run, Found> = BTreeMap::new();
    for name in names {
        match name {
            Name::Reset { epoch, micros } => {
                // Two resets of one epoch are not the ledger's doing: the smaller
                // allowance is the one that cannot count too little.
                let allowed = folded.resets.entry(*epoch).or_insert(*micros);
                *allowed = (*allowed).min(*micros);
                continue;
            }
            Name::Turn(_) => {
                folded.turns += 1;
                continue;
            }
            _ => {}
        }
        let Some(run) = name.of() else { continue };
        let of = found.entry(run).or_default();
        of.damaged |= damaged.contains(name);
        match name {
            Name::Reset { .. } | Name::Turn(_) => {}
            Name::Reservation(_) => of.record = true,
            Name::Held { epoch, micros, .. } => of.holds.push((*epoch, *micros)),
            Name::Place { place, .. } => {
                of.places.insert(*place);
            }
            Name::Dispatched {
                place, by, micros, ..
            } => of
                .dispatched
                .entry(*place)
                .or_default()
                .push((*by, *micros)),
            Name::Settling { place, .. } => {
                of.settling.insert(*place);
            }
            Name::Settled {
                place,
                stage,
                micros,
                ..
            } => of
                .settled
                .entry(*place)
                .or_default()
                .push((*stage, *micros)),
            Name::Closed { released, .. } => of.closed.push(*released),
            Name::Adjusted { count, micros, .. } => {
                of.adjusted.entry(*count).or_default().push(*micros);
            }
            Name::Stray { .. } => of.stray = true,
        }
    }
    let newest = folded.epoch();
    for (run, of) in found {
        // The amount and the epoch: the hold's. Two holds, or none, are not the
        // ledger's doing: the larger amount, the newer epoch, and frozen.
        let amount = of
            .holds
            .iter()
            .map(|(_, micros)| *micros)
            .max()
            .unwrap_or(0);
        let epoch = of.holds.iter().map(|(epoch, _)| *epoch).max();
        let mut unplaced = of.holds.len() != 1 || !of.record || of.stray;
        let (mut charged, mut outstanding, mut unresolved) = (0u128, 0u128, 0u128);
        let places: BTreeSet<u64> = of
            .dispatched
            .keys()
            .chain(of.settled.keys())
            .copied()
            .collect();
        for place in places {
            let sent = of.dispatched.get(&place).map_or(&[][..], Vec::as_slice);
            let ended = of.settled.get(&place).map_or(&[][..], Vec::as_slice);
            // What cannot be placed: a settlement with no dispatch, two dispatches
            // or two settlements at one place, either without its guard.
            unplaced |= sent.len() != 1 || ended.len() > 1;
            unplaced |= !of.places.contains(&place);
            unplaced |= !ended.is_empty() != of.settling.contains(&place);
            // Counted at the most the names can mean: every settlement's charge,
            // and the largest bound of a place that has none.
            let by = sent.first().map(|(by, _)| *by);
            let spend = by.map(|by| folded.runs.entry(by).or_default());
            if ended.is_empty() {
                let bound = sent.iter().map(|(_, micros)| *micros).max().unwrap_or(0);
                outstanding += u128::from(bound);
                unresolved += u128::from(bound);
                if let Some(spend) = spend {
                    spend.outstanding += u128::from(bound);
                    spend.unresolved += u128::from(bound);
                }
            } else {
                let charge: u128 = ended.iter().map(|(_, micros)| u128::from(*micros)).sum();
                let open = ended
                    .iter()
                    .filter(|(stage, _)| *stage == Stage::Unresolved);
                let open: u128 = open.map(|(_, micros)| u128::from(*micros)).sum();
                charged += charge;
                unresolved += open;
                if let Some(spend) = spend {
                    spend.charged += charge;
                    spend.unresolved += open;
                }
            }
        }
        // A place that is settled as one thing and guarded as another.
        unplaced |= of
            .settling
            .iter()
            .any(|place| !of.settled.contains_key(place));
        // One closing; and one that released has a settlement for every dispatch.
        let closed = !of.closed.is_empty();
        let released = of.closed == [true];
        unplaced |= of.closed.len() > 1 || (released && outstanding > 0);
        // Corrections are numbered from one, each once.
        let corrections = of.adjusted.len() as u64;
        let in_order = of.adjusted.keys().copied().eq(1..=corrections);
        let single = of.adjusted.values().all(|micros| micros.len() == 1);
        unplaced |= !(in_order && single);
        // A correction is of a reservation that is closed.
        unplaced |= !of.adjusted.is_empty() && !closed;
        let last = of.adjusted.values().next_back();
        let adjusted = last.and_then(|micros| micros.iter().max().copied());
        let known = charged + outstanding;
        let most = u128::from(amount).max(known);
        let frozen = unplaced || of.damaged;
        let (state, counts) = match adjusted {
            // A correction never counts under what the settlements say, and a
            // reservation corrected and then found unplaceable again is frozen.
            Some(micros) if in_order && single && closed => {
                (State::Adjusted, u128::from(micros).max(charged))
            }
            _ if frozen => (State::Frozen, most),
            _ if released => (State::Released, known),
            _ if closed => (State::Interrupted, most),
            _ => (State::Open, most),
        };
        folded.reservations.insert(
            run,
            Standing {
                epoch: epoch.unwrap_or(newest),
                amount,
                charged,
                outstanding,
                unresolved,
                counts,
                state,
                closed,
            },
        );
    }
    folded
}

/// How a request ended, in a history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// It did not leave: nothing is charged.
    NotSent,
    /// An answer settled it: at the charge it gave, or at its bound when it gave none.
    Answered(Option<u64>),
    /// It left, and no answer settles it: its bound, counted as unresolved.
    Unresolved,
}

/// One event of a history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The owner's command sets the allowance: a new epoch.
    Reset { micros: u64 },
    /// A call that nothing called is admitted with this reservation.
    Admit { run: Run, micros: u64 },
    /// A call from inside `parent` is admitted as `child`, with this share.
    Child {
        parent: Run,
        child: Run,
        micros: u64,
    },
    /// A share is taken from a run's allowance for a call that is not admitted yet.
    Take { run: Run, micros: u64 },
    /// A share that was taken is handed back: its call's admission was refused.
    GiveBack { run: Run, micros: u64 },
    /// A run is about to send a paid request with this bound.
    Dispatch { by: Run, micros: u64 },
    /// The request at this place under a reservation ended.
    Settle { root: Run, place: u64, how: How },
    /// A run ends by itself.
    End { run: Run },
    /// A run is abandoned in a live process: nothing ends it, and nothing is released.
    Abandon { run: Run },
    /// The process dies: what it held in memory is gone.
    Crash,
    /// The next open to write recovers the store.
    Recover,
    /// The content of a name can no longer be read.
    Damage(Name),
    /// Other hands make a name.
    Plant(Name),
    /// A run is deleted and the store pruned: nothing under `spend/` changes.
    DeletePrune { run: Run },
    /// A person's command sets what a reservation is charged.
    Adjust { run: Run, micros: u64 },
    /// The event was judged when only the first `judged` events of the history had
    /// happened, and is published now.
    Stale { judged: usize, event: Box<Event> },
}

/// Why an event changed nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// What is left of the allowance does not cover the reservation.
    OverAllowance {
        left: u64,
    },
    /// What the run can spend does not cover it.
    OverRun {
        left: u64,
    },
    /// What is left of the reservation does not cover it.
    OverReservation {
        left: u64,
    },
    /// The run is not executing in this process.
    NotExecuting,
    /// The run's id, or its reservation's name, is taken.
    Taken,
    /// No such place is dispatched under that reservation.
    NoSuchPlace,
    AlreadySettled,
    /// The reservation is closed already: it is closed once.
    AlreadyClosed,
    /// A run it called still executes.
    ChildrenExecuting,
    /// No reservation of that run, or one that is neither interrupted nor frozen.
    NotCorrectable,
    /// Under what its readable settlements say.
    BelowSettled {
        settled: u64,
    },
    /// Over what it counted for.
    OverCount {
        most: u64,
    },
    /// More than it counted for, by more than the allowance has left.
    CannotBeHeld {
        left: u64,
    },
    /// No such name.
    NoSuchName,
}

/// What became of an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// A dispatch was published at this place.
    Placed(u64),
    Refused(Why),
    /// The store refused the publication, because a name it makes was made since it
    /// was judged; it was judged again on the books as they are, with this outcome.
    AskedAgain(Box<Outcome>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Liveness {
    Executing,
    Abandoned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Live {
    root: Run,
    parent: Option<Run>,
    /// Signed: an overrun can put it below nothing.
    balance: i128,
    state: Liveness,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Truth {
    charged: u128,
    outstanding: BTreeMap<u64, u64>,
}

/// What a publication does to memory once it is published.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Then {
    Nothing,
    Open {
        run: Run,
        micros: u64,
    },
    Sent {
        by: Run,
        root: Run,
        place: u64,
        micros: u64,
    },
    Ended {
        root: Run,
        place: u64,
        by: Run,
        bound: u64,
        charge: u64,
    },
    Closed {
        run: Run,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Plan {
    makes: Vec<Name>,
    then: Then,
    outcome: Outcome,
}

/// The books, and what stands beside them.
#[derive(Debug, Clone, Default)]
pub struct Books {
    names: BTreeSet<Name>,
    damaged: BTreeSet<Name>,
    live: BTreeMap<Run, Live>,
    /// The requests this process dispatched and has not settled, by the reservation
    /// and the place: the run that sent each, and its bound. A meter settles what it
    /// sent, from what it holds, and never from a name it reads.
    sent: BTreeMap<(Run, u64), (Run, u64)>,
    truth: BTreeMap<Run, Truth>,
    /// The names and the damage after each event, for what is judged late.
    past: Vec<(BTreeSet<Name>, BTreeSet<Name>)>,
    /// Where the last name was planted or damaged, as a count of events: a balance
    /// that other hands changed is not one the ledger was asked to notice.
    tampered: usize,
    was: BTreeMap<Run, State>,
}

impl Books {
    pub fn new() -> Books {
        Books::default()
    }

    /// The books after these events, with what became of each.
    pub fn after(events: &[Event]) -> (Books, Vec<Outcome>) {
        let mut books = Books::new();
        let outcomes = events.iter().map(|event| books.apply(event)).collect();
        (books, outcomes)
    }

    pub fn names(&self) -> &BTreeSet<Name> {
        &self.names
    }

    pub fn damaged(&self) -> &BTreeSet<Name> {
        &self.damaged
    }

    pub fn fold(&self) -> Folded {
        fold(&self.names, &self.damaged)
    }

    /// The store's balance for the newest epoch.
    pub fn balance(&self) -> Balance {
        self.fold().balance()
    }

    /// One reservation, by its run.
    pub fn reservation(&self, run: Run) -> Option<Standing> {
        self.fold().reservations.get(&run).copied()
    }

    /// What one run sent.
    pub fn run(&self, run: Run) -> RunSpend {
        self.fold().runs.get(&run).copied().unwrap_or_default()
    }

    /// What a run that executes in this process can still spend: never below nothing.
    pub fn spendable(&self, run: Run) -> Option<u64> {
        let live = self.live.get(&run)?;
        Some(u64::try_from(live.balance.max(0)).unwrap_or(u64::MAX))
    }

    /// A run's allowance as it is kept: signed, so that a debt is not forgotten.
    pub fn allowance(&self, run: Run) -> Option<i128> {
        self.live.get(&run).map(|live| live.balance)
    }

    /// By how much a run's allowance is below nothing.
    pub fn owes(&self, run: Run) -> Option<u64> {
        let live = self.live.get(&run)?;
        Some(u64::try_from((-live.balance).max(0)).unwrap_or(u64::MAX))
    }

    fn executing(&self, run: Run) -> Option<&Live> {
        self.live
            .get(&run)
            .filter(|live| live.state == Liveness::Executing)
    }

    fn give(&mut self, run: Run, micros: i128) {
        if let Some(live) = self.live.get_mut(&run) {
            live.balance += micros;
        }
    }

    /// What the event would publish, judged on these names.
    fn plan(&self, event: &Event, names: &BTreeSet<Name>, damaged: &BTreeSet<Name>) -> Plan {
        let folded = fold(names, damaged);
        let refused = |why| Plan {
            makes: Vec::new(),
            then: Then::Nothing,
            outcome: Outcome::Refused(why),
        };
        let turn = Name::Turn(folded.turns + 1);
        match event {
            Event::Reset { micros } => Plan {
                makes: vec![
                    Name::Reset {
                        epoch: folded.epoch() + 1,
                        micros: *micros,
                    },
                    turn,
                ],
                then: Then::Nothing,
                outcome: Outcome::Done,
            },
            Event::Admit { run, micros } => {
                if self.live.contains_key(run) || folded.reservations.contains_key(run) {
                    return refused(Why::Taken);
                }
                let left = folded.balance().left;
                if *micros > left {
                    return refused(Why::OverAllowance { left });
                }
                Plan {
                    makes: vec![
                        Name::Reservation(*run),
                        Name::Held {
                            epoch: folded.epoch(),
                            run: *run,
                            micros: *micros,
                        },
                        turn,
                    ],
                    then: Then::Open {
                        run: *run,
                        micros: *micros,
                    },
                    outcome: Outcome::Done,
                }
            }
            Event::Dispatch { by, micros } => {
                let Some(live) = self.executing(*by) else {
                    return refused(Why::NotExecuting);
                };
                let spendable = self.spendable(*by).unwrap_or(0);
                if *micros > spendable {
                    return refused(Why::OverRun { left: spendable });
                }
                let root = live.root;
                let Some(standing) = folded.reservations.get(&root) else {
                    return refused(Why::NotExecuting);
                };
                if standing.closed {
                    return refused(Why::AlreadyClosed);
                }
                let left = standing.left();
                if *micros > left {
                    return refused(Why::OverReservation { left });
                }
                let place = places(names, root) + 1;
                Plan {
                    makes: vec![
                        Name::Place { run: root, place },
                        Name::Dispatched {
                            run: root,
                            place,
                            by: *by,
                            micros: *micros,
                        },
                    ],
                    then: Then::Sent {
                        by: *by,
                        root,
                        place,
                        micros: *micros,
                    },
                    outcome: Outcome::Placed(place),
                }
            }
            Event::Settle { root, place, how } => {
                let Some((by, bound)) = self.sent.get(&(*root, *place)).copied() else {
                    return refused(Why::NoSuchPlace);
                };
                // A request is settled by the run that sent it, while it is open:
                // nothing settles one for a run that was abandoned or is gone.
                if self.executing(by).is_none() {
                    return refused(Why::NotExecuting);
                }
                let settling = Name::Settling {
                    run: *root,
                    place: *place,
                };
                if names.contains(&settling) {
                    return refused(Why::AlreadySettled);
                }
                let (stage, charge) = match how {
                    How::NotSent => (Stage::NotSent, 0),
                    How::Answered(Some(charge)) => (Stage::Answered, *charge),
                    How::Answered(None) => (Stage::Answered, bound),
                    How::Unresolved => (Stage::Unresolved, bound),
                };
                let mut makes = vec![
                    settling,
                    Name::Settled {
                        run: *root,
                        place: *place,
                        stage,
                        micros: charge,
                    },
                ];
                // A charge over its bound lowers what is left, of the reservation
                // and of the allowance: it takes the next place and the next turn,
                // so that whatever judged before it is refused by the store.
                if charge > bound {
                    makes.push(Name::Place {
                        run: *root,
                        place: places(names, *root) + 1,
                    });
                    makes.push(turn);
                }
                Plan {
                    makes,
                    then: Then::Ended {
                        root: *root,
                        place: *place,
                        by,
                        bound,
                        charge,
                    },
                    outcome: Outcome::Done,
                }
            }
            Event::End { run } => {
                // Only the end of a run that nothing called publishes under `spend/`.
                let Some(closes) = folded.reservations.get(run) else {
                    return refused(Why::NotExecuting);
                };
                if closes.closed {
                    return refused(Why::AlreadyClosed);
                }
                // A closing takes the next place, so that a dispatch judged before
                // it, and a closing judged before a dispatch, are refused by the
                // store. It releases only when every dispatch has its settlement.
                let released = closes.state == State::Open && closes.outstanding == 0;
                Plan {
                    makes: vec![
                        Name::Closed {
                            run: *run,
                            released,
                        },
                        Name::Place {
                            run: *run,
                            place: places(names, *run) + 1,
                        },
                    ],
                    then: Then::Closed { run: *run },
                    outcome: Outcome::Done,
                }
            }
            Event::Recover => {
                let mut makes = Vec::new();
                for (run, one) in &folded.reservations {
                    if one.closed || self.live.contains_key(run) {
                        continue;
                    }
                    makes.push(Name::Closed {
                        run: *run,
                        released: one.state == State::Open && one.outstanding == 0,
                    });
                    makes.push(Name::Place {
                        run: *run,
                        place: places(names, *run) + 1,
                    });
                }
                Plan {
                    makes,
                    then: Then::Nothing,
                    outcome: Outcome::Done,
                }
            }
            Event::Adjust { run, micros } => {
                let Some(one) = folded.reservations.get(run) else {
                    return refused(Why::NotCorrectable);
                };
                let correctable = matches!(
                    one.state,
                    State::Interrupted | State::Frozen | State::Adjusted
                );
                if !one.closed || !correctable {
                    return refused(Why::NotCorrectable);
                }
                let asked = u128::from(*micros);
                let short = |amount: u128| u64::try_from(amount).unwrap_or(u64::MAX);
                if asked < one.charged {
                    return refused(Why::BelowSettled {
                        settled: short(one.charged),
                    });
                }
                let most = u128::from(one.amount).max(one.known());
                if asked > most {
                    return refused(Why::OverCount { most: short(most) });
                }
                // More than it counts for now has to be held by what is left, when
                // its epoch is the one that is still spent from.
                let left = folded.balance().left;
                let more = asked.saturating_sub(one.counts);
                if one.epoch == folded.epoch() && more > u128::from(left) {
                    return refused(Why::CannotBeHeld { left });
                }
                let count = names
                    .iter()
                    .filter(|name| matches!(name, Name::Adjusted { run: of, .. } if of == run));
                Plan {
                    makes: vec![
                        Name::Adjusted {
                            run: *run,
                            count: count.count() as u64 + 1,
                            micros: *micros,
                        },
                        turn,
                    ],
                    then: Then::Nothing,
                    outcome: Outcome::Done,
                }
            }
            // Nothing else publishes under `spend/`.
            _ => Plan {
                makes: Vec::new(),
                then: Then::Nothing,
                outcome: Outcome::Done,
            },
        }
    }

    /// Publishes a plan: every name it makes, or, when the store holds one of them,
    /// none. True when it was published.
    fn publish(&mut self, plan: &Plan) -> bool {
        if plan.makes.iter().any(|name| self.names.contains(name)) {
            return false;
        }
        self.names.extend(plan.makes.iter().cloned());
        match &plan.then {
            Then::Nothing => {}
            Then::Open { run, micros } => {
                self.live.insert(
                    *run,
                    Live {
                        root: *run,
                        parent: None,
                        balance: i128::from(*micros),
                        state: Liveness::Executing,
                    },
                );
                self.truth.entry(*run).or_default();
            }
            Then::Sent {
                by,
                root,
                place,
                micros,
            } => {
                self.give(*by, -i128::from(*micros));
                self.sent.insert((*root, *place), (*by, *micros));
                let truth = self.truth.entry(*root).or_default();
                truth.outstanding.insert(*place, *micros);
            }
            Then::Ended {
                root,
                place,
                by,
                bound,
                charge,
            } => {
                let truth = self.truth.entry(*root).or_default();
                truth.outstanding.remove(place);
                truth.charged += u128::from(*charge);
                self.sent.remove(&(*root, *place));
                let by = *by;
                if charge <= bound {
                    // What the bound held beyond the charge goes back to the run
                    // that sent the request, and pays its debt first.
                    self.give(by, i128::from(*bound) - i128::from(*charge));
                } else {
                    // The excess is taken from the run and from each run above it.
                    let excess = i128::from(*charge) - i128::from(*bound);
                    let mut at = Some(by);
                    while let Some(run) = at {
                        self.give(run, -excess);
                        at = self.live.get(&run).and_then(|live| live.parent);
                    }
                }
            }
            Then::Closed { run } => {
                self.live.retain(|_, live| live.root != *run);
            }
        }
        true
    }

    /// Does one event of a history.
    pub fn apply(&mut self, event: &Event) -> Outcome {
        let outcome = self.applied(event);
        self.past.push((self.names.clone(), self.damaged.clone()));
        outcome
    }

    fn applied(&mut self, event: &Event) -> Outcome {
        let refused = |why| Outcome::Refused(why);
        match event {
            Event::Stale { judged, event } => {
                // What publishes nothing under `spend/` is judged nowhere: it is
                // done now.
                if !self.publishes(event) {
                    return self.applied(event);
                }
                let empty = (BTreeSet::new(), BTreeSet::new());
                let judged = (*judged).min(self.past.len());
                let (names, damaged) = match judged.checked_sub(1) {
                    None => &empty,
                    Some(at) => self.past.get(at).unwrap_or(&empty),
                };
                let plan = self.plan(event, names, damaged);
                if matches!(plan.outcome, Outcome::Refused(_)) {
                    return plan.outcome;
                }
                if self.memory_allows(event) && self.publish(&plan) {
                    return plan.outcome;
                }
                Outcome::AskedAgain(Box::new(self.applied(event)))
            }
            Event::Child {
                parent,
                child,
                micros,
            } => {
                let Some(above) = self.executing(*parent) else {
                    return refused(Why::NotExecuting);
                };
                if self.live.contains_key(child) || self.truth.contains_key(child) {
                    return refused(Why::Taken);
                }
                let (root, left) = (above.root, self.spendable(*parent).unwrap_or(0));
                if *micros > left {
                    return refused(Why::OverRun { left });
                }
                self.give(*parent, -i128::from(*micros));
                self.live.insert(
                    *child,
                    Live {
                        root,
                        parent: Some(*parent),
                        balance: i128::from(*micros),
                        state: Liveness::Executing,
                    },
                );
                Outcome::Done
            }
            Event::Take { run, micros } => {
                if self.executing(*run).is_none() {
                    return refused(Why::NotExecuting);
                }
                let left = self.spendable(*run).unwrap_or(0);
                if *micros > left {
                    return refused(Why::OverRun { left });
                }
                self.give(*run, -i128::from(*micros));
                Outcome::Done
            }
            Event::GiveBack { run, micros } => {
                if self.executing(*run).is_none() {
                    return refused(Why::NotExecuting);
                }
                self.give(*run, i128::from(*micros));
                Outcome::Done
            }
            Event::End { run } => {
                let Some(live) = self.executing(*run).cloned() else {
                    return refused(Why::NotExecuting);
                };
                let under =
                    |other: &Live| other.parent == Some(*run) && other.state == Liveness::Executing;
                if self.live.values().any(under) {
                    return refused(Why::ChildrenExecuting);
                }
                match live.parent {
                    // A call's end publishes nothing under `spend/`: what it did
                    // not use goes back to its parent, while the parent executes.
                    Some(parent) => {
                        self.live.remove(run);
                        if self.executing(parent).is_some() {
                            self.give(parent, live.balance.max(0));
                        }
                        Outcome::Done
                    }
                    None => self.judged_now(event),
                }
            }
            Event::Abandon { run } => match self.live.get_mut(run) {
                Some(live) if live.state == Liveness::Executing => {
                    live.state = Liveness::Abandoned;
                    Outcome::Done
                }
                _ => refused(Why::NotExecuting),
            },
            Event::Crash => {
                self.live.clear();
                self.sent.clear();
                Outcome::Done
            }
            Event::Damage(name) => {
                if !self.names.contains(name) {
                    return refused(Why::NoSuchName);
                }
                self.damaged.insert(name.clone());
                self.tampered = self.past.len() + 1;
                Outcome::Done
            }
            Event::Plant(name) => {
                self.names.insert(name.clone());
                self.tampered = self.past.len() + 1;
                Outcome::Done
            }
            Event::DeletePrune { run } => {
                // A run that is deleted while it executes is told to stop, and its
                // end finds no run: it is abandoned.
                if let Some(live) = self.live.get_mut(run) {
                    live.state = Liveness::Abandoned;
                }
                Outcome::Done
            }
            Event::Reset { .. }
            | Event::Admit { .. }
            | Event::Dispatch { .. }
            | Event::Settle { .. }
            | Event::Recover
            | Event::Adjust { .. } => self.judged_now(event),
        }
    }

    /// Whether the event is a publication under `spend/`.
    fn publishes(&self, event: &Event) -> bool {
        match event {
            Event::Reset { .. }
            | Event::Admit { .. }
            | Event::Dispatch { .. }
            | Event::Settle { .. }
            | Event::Recover
            | Event::Adjust { .. } => true,
            Event::End { run } => self.live.get(run).is_some_and(|live| live.parent.is_none()),
            _ => false,
        }
    }

    /// What memory says of an event at the moment it is published, whenever it was
    /// judged: a run that is no longer executing sends nothing, and a run spends
    /// only what it has.
    fn memory_allows(&self, event: &Event) -> bool {
        match event {
            Event::Dispatch { by, micros } => {
                self.executing(*by).is_some() && self.spendable(*by).unwrap_or(0) >= *micros
            }
            Event::Admit { run, .. } => !self.live.contains_key(run),
            Event::End { run } => self.executing(*run).is_some(),
            _ => true,
        }
    }

    fn judged_now(&mut self, event: &Event) -> Outcome {
        let plan = self.plan(event, &self.names, &self.damaged);
        if matches!(plan.outcome, Outcome::Refused(_)) {
            return plan.outcome;
        }
        // Judged on the books as they are, a publication makes no name they hold.
        if self.publish(&plan) {
            plan.outcome
        } else {
            Outcome::Refused(Why::Taken)
        }
    }

    /// Holds the books to the invariants of the plan, 1 to 6, as far as one state
    /// shows them: what is counted is never under the truth; an allowance is never
    /// reported below nothing and a tree can never spend past what is really left;
    /// a reservation that released has every dispatch settled, and one that was
    /// frozen or interrupted is never released later; and every name is one the
    /// grammar reads back. Invariants 4 and 6 are of two states, and are held where
    /// a history is run ([`run_checked`]).
    pub fn check(&mut self) -> Result<(), String> {
        let folded = self.fold();
        for (root, truth) in &self.truth {
            let outstanding: u128 = truth.outstanding.values().map(|m| u128::from(*m)).sum();
            let Some(one) = folded.reservations.get(root) else {
                return Err(format!("{root}: no reservation is folded for it"));
            };
            // 1. Never under.
            let floor = match one.state {
                State::Adjusted => truth.charged,
                _ => truth.charged + outstanding,
            };
            if one.counts < floor {
                return Err(format!(
                    "{root}: counted {} and really {floor}: under",
                    one.counts
                ));
            }
            // 3. A closing that releases has a settlement for every dispatch.
            if one.state == State::Released && (outstanding > 0 || one.counts != truth.charged) {
                return Err(format!("{root}: released with {outstanding} not settled"));
            }
            // 2. Debt is not forgotten: the signed allowances of a tree's runs come
            // to no more than its amount less what is known against it. A run that
            // was abandoned gives nothing, and still owes what it owed.
            let held: i128 = self
                .live
                .values()
                .filter(|live| live.root == *root)
                .map(|live| match live.state {
                    Liveness::Executing => live.balance,
                    Liveness::Abandoned => live.balance.min(0),
                })
                .sum();
            let known = i128::try_from(truth.charged + outstanding).unwrap_or(i128::MAX);
            let really = i128::from(one.amount) - known;
            let open = self.live.values().any(|live| live.root == *root);
            if open && held > really {
                return Err(format!(
                    "{root}: its runs hold {held} between them and {really} is left"
                ));
            }
        }
        for (run, one) in &folded.reservations {
            // 3. One closing, and what was frozen or interrupted is not released.
            let before = self.was.insert(*run, one.state);
            if before == Some(State::Interrupted) && one.state == State::Released {
                return Err(format!(
                    "{run}: released after it was closed with nothing released"
                ));
            }
            let damaged = self.damaged.iter().any(|name| name.of() == Some(*run));
            if damaged && !matches!(one.state, State::Frozen | State::Adjusted) {
                return Err(format!("{run}: damaged, and {:?}", one.state));
            }
            if one.state == State::Adjusted && one.counts < one.charged {
                return Err(format!("{run}: adjusted under its settlements"));
            }
        }
        // 2. What can be reserved is the allowance less what is counted, or nothing.
        let balance = folded.balance();
        let over = u128::from(balance.left) + balance.counted > u128::from(balance.allowed);
        if over && balance.left > 0 {
            return Err(format!("{balance:?}: more is left than the allowance has"));
        }
        // 5. Every name is of the grammar, and the fold is of the names alone.
        let mut read = BTreeSet::new();
        for name in &self.names {
            let text = name.to_string();
            let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/');
            if text.len() > NAME_LIMIT || !text.chars().all(plain) {
                return Err(format!("{text}: not a name the store takes"));
            }
            match Name::parse(&text) {
                Some(back) if back == *name => read.insert(back),
                other => return Err(format!("{text}: read back as {other:?}")),
            };
        }
        if fold(&read, &self.damaged) != folded {
            return Err("the fold of the names as they are written is another".to_owned());
        }
        Ok(())
    }
}

/// Runs a history on new books, holding the books to every invariant after every
/// event. What it returns is the books and what became of each event, or what broke.
pub fn run_checked(events: &[Event]) -> Result<(Books, Vec<Outcome>), String> {
    run_watched(events, |_, _, _| Ok(()))
}

/// As [`run_checked`], showing `watch` the books after each event: the event's place
/// from 0, the books, and what became of the events so far.
pub fn run_watched(
    events: &[Event],
    mut watch: impl FnMut(usize, &Books, &[Outcome]) -> Result<(), String>,
) -> Result<(Books, Vec<Outcome>), String> {
    let mut books = Books::new();
    let mut outcomes = Vec::new();
    for (at, event) in events.iter().enumerate() {
        let before = books.fold();
        let tampered = books.tampered;
        let allowances = books.live.clone();
        let outcome = books.apply(event);
        let after = books.fold();
        let broke = |what: String| format!("event {at}, {event:?}: {what}");
        // 4. Damage freezes the one reservation it belongs to, and changes no other.
        if let Event::Damage(name) | Event::Plant(name) = event {
            let own = name.of();
            for (run, one) in &after.reservations {
                if Some(*run) != own && before.reservations.get(run) != Some(one) {
                    return Err(broke(format!("it changed the reservation of {run}")));
                }
            }
            let frozen = own.and_then(|run| after.reservations.get(&run));
            let most = |one: &Standing| u128::from(one.amount).max(one.known());
            let held = |one: &Standing| one.state == State::Adjusted || one.counts >= most(one);
            let damage = matches!(event, Event::Damage(_));
            if damage && matches!(outcome, Outcome::Done) && !frozen.is_none_or(held) {
                return Err(broke(
                    "its reservation counts for less than its amount".into(),
                ));
            }
        }
        // 6. No admission, and no dispatch, is published on a balance that the
        // ledger's own records have lowered since it was judged. What other hands
        // planted or damaged meanwhile is not the ledger's to have noticed.
        if let Event::Stale { judged, event } = event {
            let landed = !matches!(outcome, Outcome::Refused(_) | Outcome::AskedAgain(_));
            let clean = *judged >= tampered;
            match &**event {
                Event::Admit { micros, .. } if landed && clean => {
                    if *micros > before.balance().left {
                        return Err(broke(format!(
                            "admitted on a stale balance: {} was left",
                            before.balance().left
                        )));
                    }
                }
                Event::Dispatch { by, micros } if landed && clean => {
                    let root = books.live.get(by).map(|live| live.root);
                    let left = root
                        .and_then(|root| before.reservations.get(&root))
                        .map_or(0, Standing::left);
                    if *micros > left {
                        return Err(broke(format!(
                            "dispatched on a stale balance: {left} was left"
                        )));
                    }
                }
                _ => {}
            }
        }
        // 2. Every return pays the debt first: what comes back is added to the
        // signed allowance, whatever it was, and to nothing else.
        let returned = match (event, &outcome) {
            (Event::GiveBack { run, micros }, Outcome::Done) => Some((*run, i128::from(*micros))),
            (Event::End { run }, Outcome::Done) => allowances.get(run).and_then(|ended| {
                let parent = ended.parent?;
                let executes = allowances.get(&parent)?.state == Liveness::Executing;
                executes.then_some((parent, ended.balance.max(0)))
            }),
            _ => None,
        };
        if let Some((run, micros)) = returned {
            let was = allowances.get(&run).map(|live| live.balance);
            let is = books.allowance(run);
            if was.map(|was| was + micros) != is {
                return Err(broke(format!(
                    "{micros} came back to {run}, which had {was:?} and has {is:?}"
                )));
            }
        }
        books.check().map_err(broke)?;
        outcomes.push(outcome);
        watch(at, &books, &outcomes).map_err(broke)?;
    }
    Ok((books, outcomes))
}

impl Books {
    /// The names as they were after the first `events` events.
    pub fn names_after(&self, events: usize) -> Option<&BTreeSet<Name>> {
        let at = events.checked_sub(1)?;
        self.past.get(at).map(|(names, _)| names)
    }

    /// The runs that execute in this process, in order.
    pub fn executing_runs(&self) -> Vec<Run> {
        let executing = |(_, live): &(&Run, &Live)| live.state == Liveness::Executing;
        self.live
            .iter()
            .filter(executing)
            .map(|(run, _)| *run)
            .collect()
    }
}

/// What a history must give: of the store's newest epoch, of one reservation, of one
/// run's allowance, or of one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Must {
    Epoch(u64),
    Allowed(u64),
    Counted(u64),
    Left(u64),
    Debt(u64),
    /// What the reservation of this run counts for.
    Counts(Run, u64),
    State(Run, State),
    EpochOf(Run, u64),
    Charged(Run, u64),
    Outstanding(Run, u64),
    Unresolved(Run, u64),
    /// No reservation of this run is in the books.
    Absent(Run),
    /// What this run can still spend, and by how much its allowance is below nothing.
    Spendable(Run, u64),
    Owes(Run, u64),
    /// What became of the event at this place in the history, from 0.
    Outcome(usize, Outcome),
    /// The names are the ones there were after the event at this place.
    SameNamesAsAfter(usize),
}

/// A named history: its events, and what the books must give, after the last event
/// and, where it is said, after an earlier one.
#[derive(Debug, Clone)]
pub struct History {
    pub name: &'static str,
    pub events: Vec<Event>,
    pub must: Vec<Must>,
    /// What must hold after the event at this place, from 0.
    pub earlier: Vec<(usize, Must)>,
}

impl Must {
    /// Whether these books give it; `outcomes` are of the events so far.
    pub fn holds(&self, books: &Books, outcomes: &[Outcome]) -> Result<(), String> {
        let balance = books.balance();
        let of = |run: &Run| {
            books
                .reservation(*run)
                .ok_or(format!("no reservation of {run}"))
        };
        let same = |what: &str, found: u128, wanted: u64| {
            if found == u128::from(wanted) {
                Ok(())
            } else {
                Err(format!("{what}: {found}, and it must be {wanted}"))
            }
        };
        match self {
            Must::Epoch(epoch) => same("the epoch", balance.epoch.into(), *epoch),
            Must::Allowed(micros) => same("allowed", balance.allowed.into(), *micros),
            Must::Counted(micros) => same("counted", balance.counted, *micros),
            Must::Left(micros) => same("left", balance.left.into(), *micros),
            Must::Debt(micros) => same("the debt", balance.debt, *micros),
            Must::Counts(run, micros) => same(&format!("{run} counts"), of(run)?.counts, *micros),
            Must::EpochOf(run, epoch) => same(
                &format!("the epoch of {run}"),
                of(run)?.epoch.into(),
                *epoch,
            ),
            Must::Charged(run, micros) => {
                same(&format!("{run} charged"), of(run)?.charged, *micros)
            }
            Must::Outstanding(run, micros) => {
                same(&format!("{run} outstanding"), of(run)?.outstanding, *micros)
            }
            Must::Unresolved(run, micros) => {
                same(&format!("{run} unresolved"), of(run)?.unresolved, *micros)
            }
            Must::State(run, state) => match of(run)?.state {
                found if found == *state => Ok(()),
                found => Err(format!("{run} is {found:?}, and it must be {state:?}")),
            },
            Must::Absent(run) => match books.reservation(*run) {
                None => Ok(()),
                Some(found) => Err(format!("{run} has a reservation: {found:?}")),
            },
            Must::Spendable(run, micros) => {
                let found = books.spendable(*run).ok_or(format!("{run} is not open"))?;
                same(&format!("{run} can spend"), found.into(), *micros)
            }
            Must::Owes(run, micros) => {
                let found = books.owes(*run).ok_or(format!("{run} is not open"))?;
                same(&format!("{run} owes"), found.into(), *micros)
            }
            Must::Outcome(at, outcome) => match outcomes.get(*at) {
                Some(found) if found == outcome => Ok(()),
                found => Err(format!("event {at} came to {found:?}, not {outcome:?}")),
            },
            Must::SameNamesAsAfter(at) => {
                if books.names_after(at + 1) == Some(books.names()) {
                    Ok(())
                } else {
                    Err(format!("the names are not those after event {at}"))
                }
            }
        }
    }
}

impl History {
    /// Runs the history on the model, holding every state to the invariants, and
    /// holds the books to what the history must give.
    pub fn verify(&self) -> Result<Books, String> {
        let mut after = Vec::new();
        let (books, outcomes) = run_watched(&self.events, |at, books, outcomes| {
            for (_, must) in self.earlier.iter().filter(|(when, _)| *when == at) {
                must.holds(books, outcomes)
                    .map_err(|why| format!("after event {at}: {why}"))?;
            }
            after.push(at);
            Ok(())
        })
        .map_err(|why| format!("{}: {why}", self.name))?;
        if let Some((at, _)) = self.earlier.iter().find(|(at, _)| !after.contains(at)) {
            return Err(format!("{}: it has no event {at}", self.name));
        }
        for must in &self.must {
            must.holds(&books, &outcomes)
                .map_err(|why| format!("{}: {why}", self.name))?;
        }
        Ok(books)
    }
}

fn reset(micros: u64) -> Event {
    Event::Reset { micros }
}

fn admit(run: u32, micros: u64) -> Event {
    Event::Admit {
        run: Run(run),
        micros,
    }
}

fn child(parent: u32, child: u32, micros: u64) -> Event {
    Event::Child {
        parent: Run(parent),
        child: Run(child),
        micros,
    }
}

fn dispatch(by: u32, micros: u64) -> Event {
    Event::Dispatch {
        by: Run(by),
        micros,
    }
}

fn settle(root: u32, place: u64, how: How) -> Event {
    Event::Settle {
        root: Run(root),
        place,
        how,
    }
}

fn charged(micros: u64) -> How {
    How::Answered(Some(micros))
}

fn end(run: u32) -> Event {
    Event::End { run: Run(run) }
}

fn adjust(run: u32, micros: u64) -> Event {
    Event::Adjust {
        run: Run(run),
        micros,
    }
}

fn stale(judged: usize, event: Event) -> Event {
    Event::Stale {
        judged,
        event: Box::new(event),
    }
}

fn refused(why: Why) -> Outcome {
    Outcome::Refused(why)
}

fn again(outcome: Outcome) -> Outcome {
    Outcome::AskedAgain(Box::new(outcome))
}

/// The table of histories. The first 38 are the histories the six review rounds of
/// the plan for checkpoint 1B produced, in the plan's order and under its numbers.
/// Amounts are whole units, for reading; the unit is a millionth of a US dollar.
/// A later ledger task adds its histories after them.
pub fn histories() -> Vec<History> {
    use Must::{
        Absent, Allowed, Charged, Counted, Counts, Debt, Epoch, EpochOf, Left, Outstanding, Owes,
        SameNamesAsAfter, Spendable, Unresolved,
    };
    let r = Run;
    let state = Must::State;
    let came = Must::Outcome;
    let mut table = Vec::new();
    let mut add = |name, events: Vec<Event>, must: Vec<Must>, earlier: Vec<(usize, Must)>| {
        table.push(History {
            name,
            events,
            must,
            earlier,
        });
    };
    // The overrun the reviews kept coming back to: reserve 10, two dispatches of 5,
    // and the first settles at 12.
    let overrun = || {
        vec![
            reset(20),
            admit(1, 10),
            dispatch(1, 5),
            dispatch(1, 5),
            settle(1, 1, charged(12)),
        ]
    };
    let with = |mut events: Vec<Event>, more: Vec<Event>| {
        events.extend(more);
        events
    };
    // A reservation of 10 whose record is damaged after its process died: frozen.
    let frozen = || {
        vec![
            reset(10),
            admit(1, 10),
            Event::Crash,
            Event::Damage(Name::Reservation(r(1))),
            Event::Recover,
        ]
    };

    add(
        "01 two admissions, each told yes for the last of the allowance: one is admitted",
        vec![reset(10), admit(1, 10), stale(1, admit(2, 10))],
        vec![
            came(1, Outcome::Done),
            came(2, again(refused(Why::OverAllowance { left: 0 }))),
            Counted(10),
            Left(0),
            Absent(r(2)),
        ],
        vec![],
    );
    add(
        "02 two requests of one run, each within what is left alone and not together: one leaves",
        vec![
            reset(10),
            admit(1, 10),
            dispatch(1, 6),
            stale(2, dispatch(1, 6)),
        ],
        vec![
            came(2, Outcome::Placed(1)),
            came(3, refused(Why::OverRun { left: 4 })),
            Outstanding(r(1), 6),
            Counts(r(1), 10),
        ],
        vec![],
    );
    add(
        "03 two requests that both fit, of different bounds, choosing one place: two places",
        vec![
            reset(10),
            admit(1, 10),
            dispatch(1, 3),
            stale(2, dispatch(1, 4)),
        ],
        vec![
            came(2, Outcome::Placed(1)),
            came(3, again(Outcome::Placed(2))),
            Outstanding(r(1), 7),
            Spendable(r(1), 3),
        ],
        vec![],
    );
    add(
        "04 a request settled under its bound: the difference can be spent again",
        vec![
            reset(10),
            admit(1, 10),
            dispatch(1, 10),
            settle(1, 1, charged(4)),
            dispatch(1, 6),
        ],
        vec![
            came(4, Outcome::Placed(2)),
            Charged(r(1), 4),
            Outstanding(r(1), 6),
            Spendable(r(1), 0),
        ],
        vec![(2, Spendable(r(1), 0)), (3, Spendable(r(1), 6))],
    );
    add(
        "05 a request that succeeds and reports no charge: its bound",
        vec![
            reset(10),
            admit(1, 10),
            dispatch(1, 5),
            settle(1, 1, How::Answered(None)),
        ],
        vec![Charged(r(1), 5), Unresolved(r(1), 0), Spendable(r(1), 5)],
        vec![],
    );
    add(
        "06 a fault at each stage: not sent, nothing; answered, its charge; unresolved, its bound",
        vec![
            reset(30),
            admit(1, 30),
            dispatch(1, 5),
            dispatch(1, 5),
            dispatch(1, 5),
            settle(1, 1, How::NotSent),
            settle(1, 2, charged(2)),
            settle(1, 3, How::Unresolved),
        ],
        vec![
            Charged(r(1), 7),
            Unresolved(r(1), 5),
            Outstanding(r(1), 0),
            Spendable(r(1), 23),
        ],
        vec![(5, Charged(r(1), 0)), (6, Charged(r(1), 2))],
    );
    add(
        "07 a first hop handed data and a second refused: unresolved, not \"not sent\"",
        // The sender says how far the request got; the books count what it says.
        vec![
            reset(10),
            admit(1, 10),
            dispatch(1, 5),
            settle(1, 1, How::Unresolved),
        ],
        vec![Charged(r(1), 5), Unresolved(r(1), 5), Spendable(r(1), 5)],
        vec![],
    );
    add(
        "08 reserve 10, two dispatches of 5, the first settles at 12, the second at 0 and \
         gives back 5: nothing can be spent; the debt is 2",
        with(overrun(), vec![settle(1, 2, charged(0))]),
        vec![
            Spendable(r(1), 0),
            Owes(r(1), 2),
            state(r(1), State::Open),
            Counts(r(1), 12),
            Left(8),
        ],
        vec![(4, Owes(r(1), 7)), (4, Counts(r(1), 17))],
    );
    add(
        "09 the same, the run ending by itself: closed at 12, nothing released",
        with(overrun(), vec![settle(1, 2, charged(0)), end(1)]),
        vec![state(r(1), State::Released), Counts(r(1), 12), Left(8)],
        vec![],
    );
    add(
        "10 the same, interrupted with the second unsettled: 17",
        with(overrun(), vec![Event::Crash, Event::Recover]),
        vec![state(r(1), State::Interrupted), Counts(r(1), 17), Left(3)],
        vec![],
    );
    add(
        "11 the same, with an admission made while the second is unsettled: it sees 17 counted",
        with(overrun(), vec![admit(2, 4), admit(3, 3)]),
        vec![
            came(5, refused(Why::OverAllowance { left: 3 })),
            came(6, Outcome::Done),
            Absent(r(2)),
            Counted(20),
            Left(0),
        ],
        vec![(4, Counted(17))],
    );
    add(
        "12 an admission that judged before a settlement over its bound and publishes after: \
         it is asked again",
        vec![
            reset(20),
            admit(1, 10),
            dispatch(1, 5),
            settle(1, 1, charged(12)),
            stale(3, admit(2, 10)),
        ],
        vec![
            came(4, again(refused(Why::OverAllowance { left: 8 }))),
            Absent(r(2)),
            Counted(12),
        ],
        vec![(2, Left(10))],
    );
    add(
        "13 a settlement at 12 over a bound of 5, then its content damaged, the run deleted \
         and pruned: no less than 12",
        vec![
            reset(20),
            admit(1, 5),
            dispatch(1, 5),
            settle(1, 1, charged(12)),
            Event::Damage(Name::Settled {
                run: r(1),
                place: 1,
                stage: Stage::Answered,
                micros: 12,
            }),
            Event::DeletePrune { run: r(1) },
            Event::Crash,
            Event::Recover,
        ],
        vec![state(r(1), State::Frozen), Counts(r(1), 12), Left(8)],
        vec![],
    );
    add(
        "14 a reservation's content damaged, its run deleted and pruned, no request id: \
         frozen at the amount its name states",
        vec![
            reset(20),
            admit(1, 10),
            Event::Crash,
            Event::Damage(Name::Reservation(r(1))),
            Event::DeletePrune { run: r(1) },
            Event::Recover,
        ],
        vec![state(r(1), State::Frozen), Counts(r(1), 10), Left(10)],
        vec![],
    );
    add(
        "15 reserve 10, every dispatch settled for 3 in all, then the reservation damaged: 10",
        vec![
            reset(20),
            admit(1, 10),
            dispatch(1, 2),
            dispatch(1, 2),
            settle(1, 1, charged(1)),
            settle(1, 2, charged(2)),
            end(1),
            Event::Damage(Name::Reservation(r(1))),
        ],
        vec![state(r(1), State::Frozen), Counts(r(1), 10), Left(10)],
        vec![(6, state(r(1), State::Released)), (6, Counts(r(1), 3))],
    );
    add(
        "16 reserve 10, dispatch 5, settle at 6, the run still open: 10, not 6",
        vec![
            reset(20),
            admit(1, 10),
            dispatch(1, 5),
            settle(1, 1, charged(6)),
        ],
        vec![
            state(r(1), State::Open),
            Counts(r(1), 10),
            Charged(r(1), 6),
            Spendable(r(1), 4),
        ],
        vec![],
    );
    add(
        "17 a settlement that names no dispatch, and two dispatches at one place: that \
         reservation frozen, the others as they were",
        vec![
            reset(30),
            admit(1, 10),
            admit(2, 10),
            admit(3, 5),
            dispatch(1, 5),
            Event::Plant(Name::Settled {
                run: r(2),
                place: 3,
                stage: Stage::Answered,
                micros: 4,
            }),
            Event::Plant(Name::Dispatched {
                run: r(1),
                place: 1,
                by: r(1),
                micros: 8,
            }),
        ],
        vec![
            state(r(1), State::Frozen),
            Counts(r(1), 10),
            state(r(2), State::Frozen),
            Counts(r(2), 10),
            state(r(3), State::Open),
            Counts(r(3), 5),
            Left(5),
        ],
        vec![(4, Left(5)), (5, state(r(1), State::Open))],
    );
    add(
        "18 a child given 5 whose request settles at 12: nothing back; each run above 7 less",
        vec![
            reset(30),
            admit(1, 20),
            child(1, 2, 10),
            child(2, 3, 5),
            dispatch(3, 5),
            settle(1, 1, charged(12)),
            end(3),
        ],
        vec![
            Spendable(r(2), 0),
            Owes(r(2), 2),
            Spendable(r(1), 3),
            Counts(r(1), 20),
            Charged(r(1), 12),
        ],
        vec![
            (4, Spendable(r(2), 5)),
            (4, Spendable(r(1), 10)),
            (5, Owes(r(3), 7)),
        ],
    );
    add(
        "19 one child overruns and a second then returns its unused share: the debt is paid first",
        vec![
            reset(30),
            admit(1, 10),
            child(1, 2, 4),
            child(1, 3, 4),
            dispatch(2, 4),
            settle(1, 1, charged(9)),
            end(3),
        ],
        vec![Spendable(r(1), 1), Owes(r(1), 0)],
        vec![(5, Owes(r(1), 3)), (5, Spendable(r(1), 0))],
    );
    add(
        "20 a share handed back after a child's admission was refused, to a parent in debt: \
         the debt is paid first",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 5),
            Event::Take {
                run: r(1),
                micros: 5,
            },
            settle(1, 1, charged(8)),
            Event::GiveBack {
                run: r(1),
                micros: 5,
            },
        ],
        vec![Spendable(r(1), 2), Owes(r(1), 0)],
        vec![(4, Owes(r(1), 3)), (4, Spendable(r(1), 0))],
    );
    add(
        "21 a parent that calls 20 children in turn, each using nothing: all of it at the end",
        (0..20u32).fold(vec![reset(30), admit(1, 10)], |events, at| {
            with(events, vec![child(1, 10 + at, 5), end(10 + at)])
        }),
        vec![Spendable(r(1), 10), Counts(r(1), 10)],
        vec![(2, Spendable(r(1), 5))],
    );
    add(
        "22 a child's settlement fails, or its backend panics, and the root then ends: \
         closed as interrupted, nothing released",
        vec![
            reset(30),
            admit(1, 10),
            child(1, 2, 5),
            dispatch(2, 5),
            Event::Abandon { run: r(2) },
            end(1),
        ],
        vec![state(r(1), State::Interrupted), Counts(r(1), 10), Left(20)],
        vec![],
    );
    add(
        "23 a crash between dispatch and settlement: the full amount",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 3),
            Event::Crash,
            Event::Recover,
        ],
        vec![state(r(1), State::Interrupted), Counts(r(1), 10), Left(20)],
        vec![],
    );
    add(
        "24 a crash after every request was settled and before the end: the settled total",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 3),
            settle(1, 1, charged(2)),
            Event::Crash,
            Event::Recover,
        ],
        vec![state(r(1), State::Released), Counts(r(1), 2), Left(28)],
        vec![(4, Counts(r(1), 10))],
    );
    add(
        "25 a crash with nothing dispatched: nothing",
        vec![reset(30), admit(1, 10), Event::Crash, Event::Recover],
        vec![state(r(1), State::Released), Counts(r(1), 0), Left(30)],
        vec![],
    );
    add(
        "26 recovery, then recovery again: the second changes nothing",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 3),
            admit(2, 5),
            Event::Crash,
            Event::Recover,
            Event::Recover,
        ],
        vec![SameNamesAsAfter(5), Counted(10), Left(20)],
        vec![],
    );
    add(
        "27 a recovery whose last sync failed, then an open: confirmed, nothing settled twice",
        // The generation of a recovery that was not confirmed is visible and stays:
        // the next open finds every name it made.
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 3),
            settle(1, 1, charged(2)),
            Event::Crash,
            Event::Recover,
            Event::Crash,
            Event::Recover,
        ],
        vec![SameNamesAsAfter(5), Counts(r(1), 2), Left(28)],
        vec![],
    );
    add(
        "28 a reservation ended while its settlement is already named: not settled again",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 3),
            settle(1, 1, charged(2)),
            Event::Plant(Name::Closed {
                run: r(1),
                released: true,
            }),
            end(1),
        ],
        vec![
            came(5, refused(Why::AlreadyClosed)),
            SameNamesAsAfter(4),
            Counts(r(1), 2),
        ],
        vec![],
    );
    add(
        "29 two runs that end at once: each counted once",
        vec![
            reset(30),
            admit(1, 10),
            admit(2, 10),
            dispatch(1, 4),
            dispatch(2, 6),
            settle(1, 1, How::Answered(None)),
            settle(2, 1, How::Answered(None)),
            end(1),
            stale(7, end(2)),
        ],
        vec![
            came(8, Outcome::Done),
            Counts(r(1), 4),
            Counts(r(2), 6),
            Counted(10),
            Left(20),
        ],
        vec![],
    );
    add(
        "30 a reset, then a reservation, then a reset: the reservation is of the closed books",
        vec![reset(10), admit(1, 10), reset(20)],
        vec![
            Epoch(2),
            Allowed(20),
            Counted(0),
            Left(20),
            EpochOf(r(1), 1),
            Counts(r(1), 10),
        ],
        vec![(1, Left(0))],
    );
    add(
        "31 freeze 10, reset to 20, adjust the frozen one to nothing: 20 to spend, not 30",
        with(frozen(), vec![reset(20), adjust(1, 0)]),
        vec![
            Left(20),
            Allowed(20),
            state(r(1), State::Adjusted),
            Counts(r(1), 0),
        ],
        vec![(4, state(r(1), State::Frozen)), (4, Left(0))],
    );
    add(
        "32 adjust, then reset: the reset's amount",
        with(frozen(), vec![adjust(1, 0), reset(5)]),
        vec![Left(5), Allowed(5), Epoch(2)],
        vec![(5, Left(10))],
    );
    add(
        "33 an adjustment below a reservation's readable settlements: refused",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 5),
            settle(1, 1, charged(4)),
            dispatch(1, 5),
            Event::Crash,
            Event::Recover,
            adjust(1, 3),
            adjust(1, 4),
        ],
        vec![
            came(7, refused(Why::BelowSettled { settled: 4 })),
            came(8, Outcome::Done),
            state(r(1), State::Adjusted),
            Counts(r(1), 4),
            Left(26),
        ],
        vec![(6, state(r(1), State::Interrupted)), (6, Counts(r(1), 10))],
    );
    add(
        "34 an adjustment upward after the difference was spent: refused when it cannot be held",
        vec![
            reset(10),
            admit(1, 10),
            dispatch(1, 5),
            Event::Crash,
            Event::Recover,
            adjust(1, 0),
            admit(2, 8),
            adjust(1, 10),
            adjust(1, 2),
        ],
        vec![
            came(7, refused(Why::CannotBeHeld { left: 2 })),
            came(8, Outcome::Done),
            Counts(r(1), 2),
            Left(0),
            Debt(0),
        ],
        vec![],
    );
    add(
        "35 a cancellation after dispatch, no settling answer: the bound, unresolved",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 5),
            settle(1, 1, How::Unresolved),
            end(1),
        ],
        vec![
            state(r(1), State::Released),
            Counts(r(1), 5),
            Unresolved(r(1), 5),
            Left(25),
        ],
        vec![],
    );
    add(
        "36 a cancellation after dispatch, then an answer with a charge: that charge",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 5),
            settle(1, 1, charged(3)),
            end(1),
        ],
        vec![
            state(r(1), State::Released),
            Counts(r(1), 3),
            Unresolved(r(1), 0),
            Left(27),
        ],
        vec![],
    );
    add(
        "37 a run abandoned in a live process: held in full until the process is gone",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 3),
            settle(1, 1, charged(1)),
            Event::Abandon { run: r(1) },
            Event::Recover,
            Event::Crash,
            Event::Recover,
        ],
        vec![state(r(1), State::Released), Counts(r(1), 1), Left(29)],
        vec![
            (5, state(r(1), State::Open)),
            (5, Counts(r(1), 10)),
            (5, SameNamesAsAfter(4)),
        ],
    );
    add(
        "38 a store with no allowance: a call of nothing is admitted, a call over nothing is not",
        vec![admit(1, 0), admit(2, 1)],
        vec![
            came(0, Outcome::Done),
            came(1, refused(Why::OverAllowance { left: 0 })),
            Epoch(0),
            EpochOf(r(1), 0),
            Absent(r(2)),
        ],
        vec![],
    );
    // What ADR 0016 decides beyond the plan's histories.
    add(
        "39 a parent's request settles over its bound while its child holds a share: the \
         child's request is held to what the reservation has left",
        vec![
            reset(30),
            admit(1, 10),
            child(1, 2, 5),
            dispatch(1, 5),
            settle(1, 1, charged(12)),
            stale(4, dispatch(2, 5)),
            dispatch(2, 1),
        ],
        vec![
            // It judged while 5 were left, and chose the place the settlement took.
            came(5, again(refused(Why::OverReservation { left: 0 }))),
            came(6, refused(Why::OverReservation { left: 0 })),
            Spendable(r(2), 5),
            Owes(r(1), 7),
            Counts(r(1), 12),
            Outstanding(r(1), 0),
        ],
        vec![],
    );
    add(
        "40 an end and a recovery that judged the same books: one closing",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 3),
            settle(1, 1, charged(2)),
            end(1),
            stale(4, Event::Recover),
        ],
        vec![
            came(5, again(Outcome::Done)),
            SameNamesAsAfter(4),
            state(r(1), State::Released),
            Counts(r(1), 2),
        ],
        vec![],
    );
    add(
        "41 two corrections that judged the same books: the second is judged again",
        with(frozen(), vec![adjust(1, 5), stale(5, adjust(1, 0))]),
        vec![
            came(5, Outcome::Done),
            came(6, again(Outcome::Done)),
            state(r(1), State::Adjusted),
            Counts(r(1), 0),
            Left(10),
        ],
        vec![(5, Counts(r(1), 5)), (5, Left(5))],
    );
    add(
        "42 a correction upward of a reservation of the closed books: taken, whatever the \
         new allowance has left",
        with(frozen(), vec![adjust(1, 0), reset(3), adjust(1, 10)]),
        vec![
            came(7, Outcome::Done),
            Counts(r(1), 10),
            EpochOf(r(1), 1),
            Epoch(2),
            Left(3),
        ],
        vec![],
    );
    table
}

/// Numbers from a seed: the same seed gives the same numbers on every machine.
#[derive(Debug, Clone)]
pub struct Seeded(u64);

impl Seeded {
    pub fn new(seed: u64) -> Seeded {
        Seeded(seed)
    }

    fn next(&mut self) -> u64 {
        // SplitMix64.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.0;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ (mixed >> 31)
    }

    /// A number under `bound`, which is over nothing.
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1)
    }

    fn pick<T: Clone>(&mut self, of: &[T]) -> Option<T> {
        let at = self.below(of.len() as u64) as usize;
        of.get(at).cloned()
    }
}

/// The most events a generated history has.
pub const GENERATED_EVENTS: usize = 24;

/// A generated history: at most [`GENERATED_EVENTS`] events, the same for the same
/// seed and number. The events are chosen against the books as they stand, so that
/// most of them apply; about one publication in five is judged late.
pub fn generated(seed: u64, number: u64) -> Vec<Event> {
    let mut seeded = Seeded::new(seed ^ number.wrapping_mul(0xA24B_AED4_963E_E407));
    let length = 1 + seeded.below(GENERATED_EVENTS as u64) as usize;
    let mut books = Books::new();
    let mut events: Vec<Event> = Vec::new();
    let mut taken: Vec<(Run, u64)> = Vec::new();
    let mut made = 0u32;
    while events.len() < length {
        let live = books.executing_runs();
        let folded = books.fold();
        let roots: Vec<Run> = folded.reservations.keys().copied().collect();
        let names: Vec<Name> = books.names().iter().cloned().collect();
        let mut new_run = || {
            made += 1;
            Run(made)
        };
        let any_run = |seeded: &mut Seeded| Run(1 + seeded.below(8) as u32);
        let event = match seeded.below(24) {
            0 => Some(reset(seeded.below(40))),
            1..=3 => Some(Event::Admit {
                run: new_run(),
                micros: seeded.below(13),
            }),
            4 | 5 => seeded.pick(&live).map(|parent| Event::Child {
                parent,
                child: new_run(),
                micros: seeded.below(books.spendable(parent).unwrap_or(0) + 2),
            }),
            6..=9 => seeded.pick(&live).map(|by| Event::Dispatch {
                by,
                micros: seeded.below(8),
            }),
            10..=13 => {
                // Mostly a place that was dispatched, settled or not; at times any.
                let sent = names.iter().filter_map(|name| match name {
                    Name::Dispatched { run, place, .. } => Some((*run, *place)),
                    _ => None,
                });
                let sent: Vec<(Run, u64)> = sent.collect();
                let at = match seeded.below(4) {
                    0 => seeded.pick(&roots).map(|root| (root, 1 + seeded.below(4))),
                    _ => seeded.pick(&sent),
                };
                at.map(|(root, place)| Event::Settle {
                    root,
                    place,
                    how: match seeded.below(4) {
                        0 => How::NotSent,
                        1 => How::Answered(None),
                        2 => How::Unresolved,
                        _ => How::Answered(Some(seeded.below(15))),
                    },
                })
            }
            14 | 15 => seeded.pick(&live).map(|run| Event::End { run }),
            16 => seeded.pick(&live).map(|run| Event::Abandon { run }),
            17 => Some(Event::Crash),
            18 => Some(Event::Recover),
            19 => seeded.pick(&names).map(Event::Damage),
            20 => {
                let (run, place) = (any_run(&mut seeded), 1 + seeded.below(4));
                let micros = seeded.below(15);
                Some(Event::Plant(match seeded.below(8) {
                    0 => Name::Held {
                        epoch: seeded.below(3),
                        run,
                        micros,
                    },
                    1 => Name::Dispatched {
                        run,
                        place,
                        by: any_run(&mut seeded),
                        micros,
                    },
                    2 => Name::Settled {
                        run,
                        place,
                        stage: Stage::ALL[seeded.below(3) as usize],
                        micros,
                    },
                    3 => Name::Closed {
                        run,
                        released: seeded.below(2) == 0,
                    },
                    4 => Name::Place { run, place },
                    5 => Name::Settling { run, place },
                    6 => Name::Stray { run, count: place },
                    _ => Name::Reservation(run),
                }))
            }
            21 => seeded.pick(&roots).map(|run| Event::Adjust {
                run,
                micros: seeded.below(15),
            }),
            22 => match seeded.below(2) {
                0 => seeded.pick(&live).map(|run| Event::Take {
                    run,
                    micros: seeded.below(books.spendable(run).unwrap_or(0) + 1),
                }),
                _ => taken
                    .pop()
                    .map(|(run, micros)| Event::GiveBack { run, micros }),
            },
            _ => seeded.pick(&roots).map(|run| Event::DeletePrune { run }),
        };
        // Where nothing could be chosen, a call is admitted: there is always one.
        let event = event.unwrap_or_else(|| Event::Admit {
            run: new_run(),
            micros: seeded.below(13),
        });
        let late = !matches!(event, Event::Plant(_) | Event::Damage(_)) && seeded.below(5) == 0;
        let event = if late {
            stale(seeded.below(events.len() as u64 + 1) as usize, event)
        } else {
            event
        };
        let outcome = books.apply(&event);
        if let (Event::Take { run, micros }, Outcome::Done) = (&event, &outcome) {
            taken.push((*run, *micros));
        }
        // A share that could not be handed back is handed back to nobody.
        events.push(event);
    }
    events
}
