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
//! Beside the names the model keeps two things that are not in a store. What each of
//! two processes holds in memory: the runs it has open, each with a signed allowance,
//! and the requests it sent; a crash loses it. And the truth: what was really
//! reserved and in which epoch, what each run really dispatched, what the provider
//! really charged and what was settled, written down as it happens and never read by
//! the fold, so that [`Books::check`] can hold what the fold answers to it and to
//! nothing the fold itself worked out.

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
    /// `spend/epoch/<epoch>`: the epoch is opened, once: a reset's guard.
    Epoch(u64),
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
    /// `spend/adjusted/<run>/<over>/<micros>`: a person's correction, made over
    /// that many names of the reservation.
    Adjusted { run: Run, over: u64, micros: u64 },
    /// `spend/stray/<run>/<n>`: stands for any name under `spend/` that names this
    /// run where a reservation's run is named and is of none of the forms above.
    Stray { run: Run, count: u64 },
}

impl Name {
    /// The run whose reservation this name belongs to, when it belongs to one.
    pub fn of(&self) -> Option<Run> {
        match self {
            Name::Reset { .. } | Name::Epoch(_) | Name::Turn(_) => None,
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

    /// The place under its reservation that this name names, when it names one.
    pub fn place(&self) -> Option<u64> {
        match self {
            Name::Place { place, .. }
            | Name::Dispatched { place, .. }
            | Name::Settling { place, .. }
            | Name::Settled { place, .. } => Some(*place),
            _ => None,
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
            ["epoch", epoch] => Name::Epoch(from_one(epoch)?),
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
            ["adjusted", id, over, micros] => Name::Adjusted {
                run: run(id)?,
                over: from_one(over)?,
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
            Name::Epoch(epoch) => write!(f, "spend/epoch/{epoch}"),
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
            Name::Adjusted { run, over, micros } => {
                write!(f, "spend/adjusted/{run}/{over}/{micros}")
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// The epoch it falls under: the reset its one hold names. None when it has no
    /// hold, or more than one.
    pub epoch: Option<u64>,
    /// Every epoch a hold of it names: it counts in each. With no hold it counts in
    /// every epoch there is.
    pub epochs: BTreeSet<u64>,
    /// What was reserved: the largest amount a hold of it names.
    pub amount: u64,
    /// The smallest amount a hold of it names: what a dispatch under it is held to.
    pub least: u64,
    /// What its settlements say was charged, unresolved ones at their bounds.
    pub charged: u128,
    /// The part of `charged` that settlements with their whole chain say: what a
    /// correction cannot go under.
    pub settled: u128,
    /// What its dispatches hold that no settlement with its whole chain gave up.
    pub outstanding: u128,
    /// The part of `charged` and all of `outstanding` that no answer settled.
    pub unresolved: u128,
    /// The places under it that have no settlement with its whole chain: a closing
    /// releases only when there is none, whatever the bounds were.
    pub unsettled: u64,
    /// The places taken under it that nothing accounts for: each may be a dispatch
    /// whose name is lost, of any bound up to the amount.
    pub unexplained: u64,
    /// What it counts for against the allowance of each epoch it counts in.
    pub counts: u128,
    pub state: State,
    pub closed: bool,
}

impl Standing {
    /// What is known against it: its charges, and the bounds not settled.
    pub fn known(&self) -> u128 {
        self.charged + self.outstanding
    }

    /// The most the names can mean against it: what is known, and the amount once
    /// more for each place that nothing accounts for.
    pub fn exposed(&self) -> u128 {
        self.known() + u128::from(self.unexplained) * u128::from(self.amount)
    }

    /// What it counts for while nothing lowers it: the larger of its amount and the
    /// most the names can mean.
    pub fn most(&self) -> u128 {
        u128::from(self.amount).max(self.exposed())
    }

    /// What a dispatch under it may still take: its smallest amount less the most the
    /// names can mean.
    pub fn left(&self) -> u64 {
        u64::try_from(u128::from(self.least).saturating_sub(self.exposed())).unwrap_or(u64::MAX)
    }

    /// Whether it counts against this epoch's allowance.
    pub fn counts_in(&self, epoch: u64) -> bool {
        self.epochs.is_empty() || self.epochs.contains(&epoch)
    }
}

/// The store's balance for one epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Balance {
    pub epoch: u64,
    /// The allowance the epoch's reset set; nothing before the first reset, and
    /// nothing where the reset is not whole.
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
    /// The allowance of each epoch there is: nothing for one whose reset is not whole.
    pub resets: BTreeMap<u64, u64>,
    /// The next turn: the lowest that is not taken.
    pub turn: u64,
}

impl Folded {
    /// The newest epoch: nothing before the first reset.
    pub fn epoch(&self) -> u64 {
        self.resets.keys().next_back().copied().unwrap_or(0)
    }

    /// The balance of this epoch.
    pub fn balance_of(&self, epoch: u64) -> Balance {
        let allowed = self.resets.get(&epoch).copied().unwrap_or(0);
        let of_epoch = || {
            let counted = move |one: &&Standing| one.counts_in(epoch);
            self.reservations.values().filter(counted)
        };
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

/// The lowest number from 1 that is not among these.
fn lowest_free(taken: &BTreeSet<u64>) -> u64 {
    let mut free = 1;
    for one in taken {
        if *one == free {
            free += 1;
        }
    }
    free
}

/// The next place under a reservation: the lowest that no name of it names.
fn next_place(names: &BTreeSet<Name>, run: Run) -> u64 {
    let own = names.iter().filter(|name| name.of() == Some(run));
    lowest_free(&own.filter_map(Name::place).collect())
}

/// Folds the books: what the names say, and nothing else. `damaged` are the names
/// whose content cannot be read, or does not agree with its name.
///
/// A name that would lower what is counted lowers it only with its whole chain: a
/// settlement with its dispatch, both guards and nothing damaged; a closing that
/// released with its place and every settlement whole; a correction over exactly the
/// names that are there; a reset with its guard. Anything else freezes or raises.
pub fn fold(names: &BTreeSet<Name>, damaged: &BTreeSet<Name>) -> Folded {
    let mut folded = Folded::default();
    #[derive(Default)]
    struct Found {
        /// How many names name this run's reservation.
        count: u64,
        holds: Vec<(u64, u64)>,
        record: bool,
        places: BTreeSet<u64>,
        dispatched: BTreeMap<u64, Vec<(Run, u64)>>,
        settling: BTreeSet<u64>,
        settled: BTreeMap<u64, Vec<(Stage, u64)>>,
        /// The places a damaged name names.
        hurt: BTreeSet<u64>,
        /// The closings: whether each released.
        closed: Vec<bool>,
        /// The corrections: the names each was made over, its amount, and whether
        /// it is damaged.
        adjusted: Vec<(u64, u64, bool)>,
        stray: bool,
        damaged: bool,
    }
    let mut found: BTreeMap<Run, Found> = BTreeMap::new();
    let mut resets: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let mut guards: BTreeSet<u64> = BTreeSet::new();
    let mut spoiled: BTreeSet<u64> = BTreeSet::new();
    let mut turns: BTreeSet<u64> = BTreeSet::new();
    for name in names {
        let hurt = damaged.contains(name);
        match name {
            Name::Reset { epoch, micros } => {
                resets.entry(*epoch).or_default().push(*micros);
                if hurt {
                    spoiled.insert(*epoch);
                }
                continue;
            }
            Name::Epoch(epoch) => {
                guards.insert(*epoch);
                resets.entry(*epoch).or_default();
                if hurt {
                    spoiled.insert(*epoch);
                }
                continue;
            }
            Name::Turn(turn) => {
                turns.insert(*turn);
                continue;
            }
            _ => {}
        }
        let Some(run) = name.of() else { continue };
        let of = found.entry(run).or_default();
        of.count += 1;
        of.damaged |= hurt;
        match name {
            Name::Reset { .. } | Name::Epoch(_) | Name::Turn(_) => {}
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
            Name::Adjusted { over, micros, .. } => of.adjusted.push((*over, *micros, hurt)),
            Name::Stray { .. } => of.stray = true,
        }
        if let Some(place) = name.place().filter(|_| hurt) {
            of.hurt.insert(place);
        }
    }
    // An epoch is there when a reset or its guard names it. Its allowance is its
    // reset's only with the guard and with neither damaged; two resets of one epoch
    // are not the ledger's doing, and the smaller is the one that cannot allow too
    // much. Anything else allows nothing until the owner resets again.
    for (epoch, amounts) in &resets {
        let whole = guards.contains(epoch) && !spoiled.contains(epoch);
        let least = amounts.iter().min().copied().filter(|_| whole);
        folded.resets.insert(*epoch, least.unwrap_or(0));
    }
    folded.turn = lowest_free(&turns);
    for (run, of) in found {
        // The amount and the epoch: the hold's. Two holds, or none, are not the
        // ledger's doing: frozen, the larger amount for what is counted and the
        // smaller for what may still be dispatched, in every epoch a hold names.
        let amounts = || of.holds.iter().map(|(_, micros)| *micros);
        let amount = amounts().max().unwrap_or(0);
        let least = amounts().min().unwrap_or(0);
        let epochs: BTreeSet<u64> = of.holds.iter().map(|(epoch, _)| *epoch).collect();
        let mut unplaced = of.holds.len() != 1 || !of.record || of.stray;
        let (mut charged, mut settled) = (0u128, 0u128);
        let (mut outstanding, mut unresolved) = (0u128, 0u128);
        let (mut overruns, mut unsettled) = (0u64, 0u64);
        let at: BTreeSet<u64> = of
            .dispatched
            .keys()
            .chain(of.settled.keys())
            .copied()
            .collect();
        for place in &at {
            let sent = of.dispatched.get(place).map_or(&[][..], Vec::as_slice);
            let ended = of.settled.get(place).map_or(&[][..], Vec::as_slice);
            let bound = sent.iter().map(|(_, micros)| *micros).max().unwrap_or(0);
            let bound = u128::from(bound);
            let charge: u128 = ended.iter().map(|(_, micros)| u128::from(*micros)).sum();
            let open = ended
                .iter()
                .filter(|(stage, _)| *stage == Stage::Unresolved);
            let open: u128 = open.map(|(_, micros)| u128::from(*micros)).sum();
            // A settlement's whole chain: one dispatch with its place, one
            // settlement with its guard, and none of the four damaged.
            let guarded = of.places.contains(place);
            let whole = sent.len() == 1
                && ended.len() == 1
                && guarded
                && of.settling.contains(place)
                && !of.hurt.contains(place);
            // What cannot be placed: two dispatches at one place or none, a
            // dispatch without its place, a settlement without its whole chain.
            unplaced |= sent.len() != 1 || !guarded;
            unplaced |= !ended.is_empty() && !whole;
            // Only a settlement with its whole chain gives up what its dispatch's
            // bound held beyond the charge: without one, the place counts for the
            // larger of the largest bound and every charge named at it.
            let rest = if whole {
                0
            } else {
                bound.saturating_sub(charge)
            };
            if whole {
                settled += charge;
            } else {
                unsettled += 1;
            }
            // A charge over its bound took a place of its own.
            if !sent.is_empty() && charge > bound {
                overruns += 1;
            }
            charged += charge;
            outstanding += rest;
            unresolved += open + rest;
            // Every run a dispatch at this place names is counted for all of it.
            let by: BTreeSet<Run> = sent.iter().map(|(by, _)| *by).collect();
            for by in by {
                let spend = folded.runs.entry(by).or_default();
                spend.charged += charge;
                spend.outstanding += rest;
                spend.unresolved += open + rest;
            }
        }
        // A guard with no settlement behind it.
        unplaced |= of
            .settling
            .iter()
            .any(|place| !of.settled.contains_key(place));
        // Every place is a dispatch's, or was taken by a charge over its bound, a
        // closing or a correction. One more than those is a place nothing accounts
        // for, which may be a dispatch whose name is lost; one fewer is a guard lost.
        let bare = of.places.iter().filter(|place| !at.contains(place)).count() as u64;
        let due = overruns + of.closed.len() as u64 + of.adjusted.len() as u64;
        let unexplained = bare.saturating_sub(due);
        unplaced |= bare != due;
        // One closing; and one that released has a settlement for every dispatch.
        let closed = !of.closed.is_empty();
        let released = of.closed == [true];
        unplaced |= of.closed.len() > 1 || (released && unsettled > 0);
        // A correction stands when it is the last, alone at its number and not
        // damaged, of a reservation that is closed, and the reservation's names are
        // exactly those it was made over, itself and the place it took.
        let last = of.adjusted.iter().map(|(over, _, _)| *over).max();
        let stands = last.and_then(|last| {
            let mut at_last = of.adjusted.iter().filter(|(over, _, _)| *over == last);
            let (_, micros, hurt) = at_last.next()?;
            let alone = at_last.next().is_none();
            let over_these = of.count.checked_sub(2) == Some(last);
            (alone && !*hurt && closed && over_these).then_some(*micros)
        });
        unplaced |= !of.adjusted.is_empty() && stands.is_none();
        let mut one = Standing {
            epoch: epochs.first().copied().filter(|_| of.holds.len() == 1),
            epochs,
            amount,
            least,
            charged,
            settled,
            outstanding,
            unresolved,
            unsettled,
            unexplained,
            counts: 0,
            state: State::Open,
            closed,
        };
        let frozen = unplaced || of.damaged;
        (one.state, one.counts) = match stands {
            // A correction never counts under what settlements with their whole
            // chain say.
            Some(micros) => (State::Adjusted, u128::from(micros).max(settled)),
            None if frozen => (State::Frozen, one.most()),
            None if released => (State::Released, one.known()),
            None if closed => (State::Interrupted, one.most()),
            None => (State::Open, one.most()),
        };
        folded.reservations.insert(run, one);
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
    /// The provider charged the request at this place, and the process that sent it
    /// holds the answer. No name says so until the request is settled.
    Charge { root: Run, place: u64, micros: u64 },
    /// The request at this place under a reservation ended. Where the process holds
    /// an answer with a charge, that charge is what is settled.
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
    /// A name is lost: a restored backup, a careless hand.
    Remove(Name),
    /// A run is deleted and the store pruned: nothing under `spend/` changes.
    DeletePrune { run: Run },
    /// A person's command sets what a reservation is charged.
    Adjust { run: Run, micros: u64 },
    /// The event was judged when only the first `judged` events of the history had
    /// happened, and is published now.
    Stale { judged: usize, event: Box<Event> },
    /// The event happens in a second process over the same store, which has its own
    /// runs and its own memory.
    Second(Box<Event>),
}

impl Event {
    /// The event with what wraps it taken off: whether it happens in the second
    /// process, and when it was judged, if that was earlier.
    fn plain(&self) -> (bool, Option<usize>, &Event) {
        let (mut second, mut judged, mut event) = (false, None, self);
        loop {
            match event {
                Event::Second(inner) => (second, event) = (true, &**inner),
                Event::Stale {
                    judged: at,
                    event: inner,
                } => (judged, event) = (Some(*at), &**inner),
                _ => return (second, judged, event),
            }
        }
    }
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
    /// A request it sent is not settled: a run waits for each before it ends.
    RequestsOutstanding,
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

/// A request a process sent and has not settled: the run that sent it, its bound,
/// and the charge of the answer once the process holds one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Request {
    by: Run,
    bound: u64,
    answer: Option<u64>,
}

/// What one process holds in memory: a crash loses it.
#[derive(Debug, Clone, Default)]
struct Memory {
    live: BTreeMap<Run, Live>,
    sent: BTreeMap<(Run, u64), Request>,
}

/// What really became of one request, whatever the names say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Asked {
    by: Run,
    bound: u64,
    /// What the provider really charged, once it answered.
    answer: Option<u64>,
    /// What the ledger's settlement of it says.
    settled: Option<u64>,
    /// Whether that settlement's name is still there.
    recorded: bool,
}

impl Asked {
    /// What the books can be held to for it: the charge its settlement recorded;
    /// its bound while none does; and, of a settlement whose name is lost, no more
    /// than the bound, because the charge was in that name and nowhere else.
    fn floor(&self) -> u128 {
        u128::from(match self.settled {
            Some(charge) if self.recorded => charge,
            Some(charge) => charge.min(self.bound),
            None => self.bound,
        })
    }

    /// What it really took from its reservation, as the process that sent it knew.
    fn took(&self) -> u128 {
        u128::from(self.settled.unwrap_or(self.bound))
    }
}

/// What is really so of one reservation: written down as it happens, and never read
/// from a name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Truth {
    epoch: u64,
    amount: u64,
    places: BTreeMap<u64, Asked>,
}

/// What a publication does to memory once it is published.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Then {
    Nothing,
    Allowed {
        epoch: u64,
        micros: u64,
    },
    Open {
        run: Run,
        epoch: u64,
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
    /// The runs the acting process has open.
    live: BTreeMap<Run, Live>,
    /// The requests the acting process dispatched and has not settled, by the
    /// reservation and the place. A meter settles what it sent, from what it holds,
    /// and never from a name it reads.
    sent: BTreeMap<(Run, u64), Request>,
    /// The memory of the process that is not acting.
    other: Memory,
    /// Every run id that was given.
    ids: BTreeSet<Run>,
    /// The truth of each reservation the ledger made, and of each epoch's allowance.
    truth: BTreeMap<Run, Truth>,
    allowed: BTreeMap<u64, u64>,
    /// The names and the damage after each event, for what is judged late.
    past: Vec<(BTreeSet<Name>, BTreeSet<Name>)>,
    /// The names other hands made, and the names of the ledger's that were lost,
    /// each with the place of its event in the history.
    planted: BTreeMap<Name, usize>,
    removed: BTreeMap<Name, usize>,
    /// The events in which the ledger took a turn, and a place under a reservation.
    turned: Vec<usize>,
    placed: BTreeMap<Run, Vec<usize>>,
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
        self.fold().reservations.get(&run).cloned()
    }

    /// What one run sent.
    pub fn run(&self, run: Run) -> RunSpend {
        self.fold().runs.get(&run).copied().unwrap_or_default()
    }

    /// A run that is open in either process.
    fn open(&self, run: Run) -> Option<&Live> {
        self.live.get(&run).or_else(|| self.other.live.get(&run))
    }

    /// Every run that is open, in either process.
    fn all_open(&self) -> impl Iterator<Item = (&Run, &Live)> {
        self.live.iter().chain(self.other.live.iter())
    }

    /// What a run that is open can still spend: never below nothing.
    pub fn spendable(&self, run: Run) -> Option<u64> {
        let live = self.open(run)?;
        Some(u64::try_from(live.balance.max(0)).unwrap_or(u64::MAX))
    }

    /// A run's allowance as it is kept: signed, so that a debt is not forgotten.
    pub fn allowance(&self, run: Run) -> Option<i128> {
        self.open(run).map(|live| live.balance)
    }

    /// By how much a run's allowance is below nothing.
    pub fn owes(&self, run: Run) -> Option<u64> {
        let live = self.open(run)?;
        Some(u64::try_from((-live.balance).max(0)).unwrap_or(u64::MAX))
    }

    /// A run that executes in the acting process.
    fn executing(&self, run: Run) -> Option<&Live> {
        self.live
            .get(&run)
            .filter(|live| live.state == Liveness::Executing)
    }

    /// Whether a request this run sent is not settled yet.
    fn waits(&self, run: Run) -> bool {
        self.sent.values().any(|request| request.by == run)
    }

    fn give(&mut self, run: Run, micros: i128) {
        if let Some(live) = self.live.get_mut(&run) {
            live.balance += micros;
        }
    }

    /// The acting process and the other change places.
    fn switch(&mut self) {
        std::mem::swap(&mut self.live, &mut self.other.live);
        std::mem::swap(&mut self.sent, &mut self.other.sent);
    }

    /// What the event would publish, judged on these names.
    fn plan(&self, event: &Event, names: &BTreeSet<Name>, damaged: &BTreeSet<Name>) -> Plan {
        let folded = fold(names, damaged);
        let refused = |why| Plan {
            makes: Vec::new(),
            then: Then::Nothing,
            outcome: Outcome::Refused(why),
        };
        let turn = Name::Turn(folded.turn);
        match event {
            Event::Reset { micros } => {
                let epoch = folded.epoch() + 1;
                Plan {
                    makes: vec![
                        Name::Reset {
                            epoch,
                            micros: *micros,
                        },
                        Name::Epoch(epoch),
                        turn,
                    ],
                    then: Then::Allowed {
                        epoch,
                        micros: *micros,
                    },
                    outcome: Outcome::Done,
                }
            }
            Event::Admit { run, micros } => {
                if self.ids.contains(run) || folded.reservations.contains_key(run) {
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
                        epoch: folded.epoch(),
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
                // Once any closing names the reservation, nothing is dispatched.
                if standing.closed {
                    return refused(Why::AlreadyClosed);
                }
                let left = standing.left();
                if *micros > left {
                    return refused(Why::OverReservation { left });
                }
                let place = next_place(names, root);
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
                let Some(request) = self.sent.get(&(*root, *place)).copied() else {
                    return refused(Why::NoSuchPlace);
                };
                let (by, bound) = (request.by, request.bound);
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
                let (stage, charge) = match (request.answer, how) {
                    (Some(charge), _) => (Stage::Answered, charge),
                    (None, How::NotSent) => (Stage::NotSent, 0),
                    (None, How::Answered(Some(charge))) => (Stage::Answered, *charge),
                    (None, How::Answered(None)) => (Stage::Answered, bound),
                    (None, How::Unresolved) => (Stage::Unresolved, bound),
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
                        place: next_place(names, *root),
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
                let released = closes.state == State::Open && closes.unsettled == 0;
                Plan {
                    makes: vec![
                        Name::Closed {
                            run: *run,
                            released,
                        },
                        Name::Place {
                            run: *run,
                            place: next_place(names, *run),
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
                        released: one.state == State::Open && one.unsettled == 0,
                    });
                    makes.push(Name::Place {
                        run: *run,
                        place: next_place(names, *run),
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
                // Never under what settlements with their whole chain say.
                if asked < one.settled {
                    return refused(Why::BelowSettled {
                        settled: short(one.settled),
                    });
                }
                if asked > one.most() {
                    return refused(Why::OverCount {
                        most: short(one.most()),
                    });
                }
                // More than it counts for now has to be held by what is left, when
                // it counts in the epoch that is still spent from.
                let left = folded.balance().left;
                let more = asked.saturating_sub(one.counts);
                if one.counts_in(folded.epoch()) && more > u128::from(left) {
                    return refused(Why::CannotBeHeld { left });
                }
                // It is made over the names the reservation has, and takes the
                // reservation's next place and the next turn.
                let over = names.iter().filter(|name| name.of() == Some(*run));
                Plan {
                    makes: vec![
                        Name::Adjusted {
                            run: *run,
                            over: over.count() as u64,
                            micros: *micros,
                        },
                        Name::Place {
                            run: *run,
                            place: next_place(names, *run),
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
        // What the ledger itself took, written down apart from the names.
        let now = self.past.len();
        for name in &plan.makes {
            match name {
                Name::Turn(_) => self.turned.push(now),
                Name::Place { run, .. } => self.placed.entry(*run).or_default().push(now),
                _ => {}
            }
        }
        match &plan.then {
            Then::Nothing => {}
            Then::Allowed { epoch, micros } => {
                self.allowed.insert(*epoch, *micros);
            }
            Then::Open { run, epoch, micros } => {
                self.live.insert(
                    *run,
                    Live {
                        root: *run,
                        parent: None,
                        balance: i128::from(*micros),
                        state: Liveness::Executing,
                    },
                );
                self.ids.insert(*run);
                let truth = self.truth.entry(*run).or_default();
                (truth.epoch, truth.amount) = (*epoch, *micros);
            }
            Then::Sent {
                by,
                root,
                place,
                micros,
            } => {
                self.give(*by, -i128::from(*micros));
                let request = Request {
                    by: *by,
                    bound: *micros,
                    answer: None,
                };
                self.sent.insert((*root, *place), request);
                let asked = Asked {
                    by: *by,
                    bound: *micros,
                    answer: None,
                    settled: None,
                    recorded: false,
                };
                let truth = self.truth.entry(*root).or_default();
                truth.places.insert(*place, asked);
            }
            Then::Ended {
                root,
                place,
                by,
                bound,
                charge,
            } => {
                let truth = self.truth.entry(*root).or_default();
                if let Some(asked) = truth.places.get_mut(place) {
                    (asked.settled, asked.recorded) = (Some(*charge), true);
                }
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
            Event::Second(event) => {
                self.switch();
                let outcome = self.applied(event);
                self.switch();
                outcome
            }
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
                if self.ids.contains(child) {
                    return refused(Why::Taken);
                }
                let (root, left) = (above.root, self.spendable(*parent).unwrap_or(0));
                if *micros > left {
                    return refused(Why::OverRun { left });
                }
                self.give(*parent, -i128::from(*micros));
                self.ids.insert(*child);
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
            Event::Charge {
                root,
                place,
                micros,
            } => {
                let Some(request) = self.sent.get_mut(&(*root, *place)) else {
                    return refused(Why::NoSuchPlace);
                };
                if request.answer.is_some() {
                    return refused(Why::AlreadySettled);
                }
                request.answer = Some(*micros);
                let asked = self.truth.get_mut(root);
                if let Some(asked) = asked.and_then(|truth| truth.places.get_mut(place)) {
                    asked.answer = Some(*micros);
                }
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
                // A run waits for every request it sent before it ends: a request
                // is outstanding until its settlement is durable.
                if self.waits(*run) {
                    return refused(Why::RequestsOutstanding);
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
                Outcome::Done
            }
            Event::Plant(name) => {
                if self.names.insert(name.clone()) {
                    self.planted.insert(name.clone(), self.past.len());
                }
                Outcome::Done
            }
            Event::Remove(name) => {
                if !self.names.remove(name) {
                    return refused(Why::NoSuchName);
                }
                self.damaged.remove(name);
                // A name other hands made is taken away again; one of the ledger's
                // is lost, and what only it said is lost with it.
                if self.planted.remove(name).is_none() {
                    self.removed.insert(name.clone(), self.past.len());
                    if let Name::Settled { run, place, .. } = name {
                        let asked = self.truth.get_mut(run);
                        if let Some(asked) = asked.and_then(|truth| truth.places.get_mut(place)) {
                            asked.recorded = false;
                        }
                    }
                }
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
    /// judged: a run that is no longer executing sends nothing, a run spends only
    /// what it has, and a run does not end while a request it sent is outstanding.
    fn memory_allows(&self, event: &Event) -> bool {
        match event {
            Event::Dispatch { by, micros } => {
                self.executing(*by).is_some() && self.spendable(*by).unwrap_or(0) >= *micros
            }
            Event::Admit { run, .. } => !self.ids.contains(run),
            Event::End { run } => self.executing(*run).is_some() && !self.waits(*run),
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

    /// Whether a correction of this reservation stands, read from the names by
    /// other code than the fold's: the correction made over the most names is alone
    /// at that number and not damaged, the reservation has a closing, and its names
    /// are that many and two more.
    fn corrected(&self, run: Run) -> bool {
        let own: Vec<&Name> = self
            .names
            .iter()
            .filter(|name| name.of() == Some(run))
            .collect();
        let overs = own.iter().filter_map(|name| match name {
            Name::Adjusted { over, .. } => Some(*over),
            _ => None,
        });
        let Some(last) = overs.max() else {
            return false;
        };
        let at_last: Vec<&&Name> = own
            .iter()
            .filter(|name| matches!(name, Name::Adjusted { over, .. } if *over == last))
            .collect();
        let closed = own.iter().any(|name| matches!(name, Name::Closed { .. }));
        let sound = at_last.len() == 1 && !self.damaged.contains(*at_last[0]);
        closed && sound && own.len() as u64 == last.saturating_add(2)
    }

    /// Whether other hands made a name that names this place, or one is damaged.
    fn tampered(&self, run: Run, place: u64) -> bool {
        let here = |name: &Name| name.of() == Some(run) && name.place() == Some(place);
        self.planted.keys().any(here) || self.damaged.iter().any(here)
    }

    /// Whether other hands made a whole settlement of this place: the settlement
    /// and its guard. In the names it is a settlement like the ledger's own.
    fn forged(&self, run: Run, place: u64) -> bool {
        let guard = Name::Settling { run, place };
        let settled = |name: &Name| matches!(name, Name::Settled { run: of, place: at, .. } if *of == run && *at == place);
        self.planted.contains_key(&guard) && self.planted.keys().any(settled)
    }

    /// How many of the ledger's names of this reservation are lost, and how many
    /// names other hands made for it.
    fn lost_and_planted(&self, run: Run) -> (usize, usize) {
        let own = |name: &&Name| name.of() == Some(run);
        (
            self.removed.keys().filter(own).count(),
            self.planted.keys().filter(own).count(),
        )
    }

    /// Holds the books to the invariants of the plan, as far as one state shows
    /// them, against the truth and never against the fold's own answers: what is
    /// counted is never under what was really charged and is really outstanding, in
    /// the epoch it was really reserved in, for each reservation and for each run;
    /// a tree's runs can never spend past what is really left of what was really
    /// reserved; no more can be reserved than the allowance the owner really set
    /// less what is really charged and outstanding; a reservation that released has
    /// every dispatch settled, and one that was closed with nothing released is
    /// never released later; and every name is one the grammar reads back. What is
    /// of two states is held where a history is run ([`run_watched`]).
    ///
    /// What it leaves out, each stated in ADR 0016, "Limits": a reservation that
    /// lost two of the ledger's names, or one with a name other hands made; a place
    /// with a whole settlement other hands made; a correction that stands; and an
    /// allowance whose reset other hands made whole.
    pub fn check(&mut self) -> Result<(), String> {
        let folded = self.fold();
        let mut sent: BTreeMap<Run, u128> = BTreeMap::new();
        let mut really_counted = 0u128;
        let newest = self.allowed.keys().next_back().copied().unwrap_or(0);
        for (root, truth) in &self.truth {
            // 2. Debt is not forgotten: the signed allowances of a tree's runs come
            // to no more than what was really reserved less what its requests
            // really took. A run that was abandoned gives nothing, and still owes
            // what it owed.
            let of_tree = |(_, live): &(&Run, &Live)| live.root == *root;
            let held: i128 = self
                .all_open()
                .filter(of_tree)
                .map(|(_, live)| match live.state {
                    Liveness::Executing => live.balance,
                    Liveness::Abandoned => live.balance.min(0),
                })
                .sum();
            let took: u128 = truth.places.values().map(Asked::took).sum();
            let really = i128::from(truth.amount) - i128::try_from(took).unwrap_or(i128::MAX);
            if self.all_open().any(|open| of_tree(&open)) && held > really {
                return Err(format!(
                    "{root}: its runs hold {held} between them and {really} is left"
                ));
            }
            // A reservation whose books were destroyed, or made over by loss and
            // other hands together, is not held to the truth: one loss alone is.
            let (lost, planted) = self.lost_and_planted(*root);
            if lost >= 2 || (lost >= 1 && planted >= 1) {
                continue;
            }
            let Some(one) = folded.reservations.get(root) else {
                return Err(format!("{root}: no reservation is folded for it"));
            };
            // 1. Never under. A correction that stands is a person's statement of
            // what the reservation is charged: it writes off what is outstanding,
            // and what a settlement says that other hands or damage touched, and
            // nothing a whole settlement of the ledger's says.
            let corrected = self.corrected(*root);
            let (mut floor, mut unsettled) = (0u128, 0u64);
            for (place, asked) in &truth.places {
                if self.forged(*root, *place) {
                    continue;
                }
                let whole = asked.settled.is_some() && asked.recorded;
                unsettled += u64::from(!whole);
                let dispatch = |name: &Name| matches!(name, Name::Dispatched { run, place: at, .. } if run == root && at == place);
                if !self.removed.keys().any(dispatch) {
                    *sent.entry(asked.by).or_default() += asked.floor();
                }
                if corrected && (!whole || self.tampered(*root, *place)) {
                    continue;
                }
                floor += asked.floor();
            }
            if one.counts < floor {
                return Err(format!(
                    "{root}: counted {} and really {floor}: under",
                    one.counts
                ));
            }
            if !one.counts_in(truth.epoch) {
                return Err(format!(
                    "{root}: reserved in epoch {} and not counted there",
                    truth.epoch
                ));
            }
            if truth.epoch == newest {
                really_counted += floor;
            }
            // 3. A closing that releases has a settlement for every dispatch.
            if one.state == State::Released && unsettled > 0 {
                return Err(format!("{root}: released with {unsettled} not settled"));
            }
        }
        // Every run is counted for no less than it really sent.
        for (run, really) in sent {
            let found = folded.runs.get(&run).copied().unwrap_or_default();
            if found.charged + found.outstanding < really {
                return Err(format!(
                    "{run}: it really sent {really}, and {} is counted to it",
                    found.charged + found.outstanding
                ));
            }
        }
        for (run, one) in &folded.reservations {
            // 3. One closing, and what was closed with nothing released is not
            // released later, unless other hands made or lost a closing of it.
            let before = self.was.insert(*run, one.state);
            let closing = |name: &&Name| matches!(name, Name::Closed { run: of, .. } if of == run);
            let touched = self.planted.keys().any(|name| closing(&name))
                || self.removed.keys().any(|name| closing(&name));
            let later = before == Some(State::Interrupted) && one.state == State::Released;
            if later && !touched {
                return Err(format!(
                    "{run}: released after it was closed with nothing released"
                ));
            }
            let damaged = self.damaged.iter().any(|name| name.of() == Some(*run));
            if damaged && !matches!(one.state, State::Frozen | State::Adjusted) {
                return Err(format!("{run}: damaged, and {:?}", one.state));
            }
        }
        // 2. What can be reserved is the allowance less what is counted, or nothing;
        // and never more than the allowance the owner really set, less what is
        // really charged and outstanding under it.
        let balance = folded.balance();
        let over = u128::from(balance.left) + balance.counted > u128::from(balance.allowed);
        if over && balance.left > 0 {
            return Err(format!("{balance:?}: more is left than the allowance has"));
        }
        let of_allowance = |name: &&Name| matches!(name, Name::Reset { .. } | Name::Epoch(_));
        let lost = self.removed.keys().filter(of_allowance).count();
        let planted: Vec<&Name> = self.planted.keys().filter(of_allowance).collect();
        let whole = |epoch: u64| {
            let reset =
                |name: &&&Name| matches!(name, Name::Reset { epoch: of, .. } if *of == epoch);
            planted.iter().any(|name| reset(&name)) && planted.contains(&&Name::Epoch(epoch))
        };
        let made_whole = planted.iter().any(|name| match name {
            Name::Epoch(epoch) => whole(*epoch),
            _ => false,
        });
        if !(lost >= 2 || (lost >= 1 && !planted.is_empty()) || made_whole) {
            let really = self.allowed.get(&newest).copied().unwrap_or(0);
            let really = u128::from(really).saturating_sub(really_counted);
            if u128::from(balance.left) > really {
                return Err(format!(
                    "{} can be reserved, and really {really} is left of the allowance",
                    balance.left
                ));
            }
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
        let allowances: BTreeMap<Run, Live> = books
            .all_open()
            .map(|(run, live)| (*run, live.clone()))
            .collect();
        let (_, judged, plain) = event.plain();
        // The reservation a dispatch draws on, and the closings that name it now.
        let drawn = match plain {
            Event::Dispatch { by, .. } => books.open(*by).map(|live| live.root),
            _ => None,
        };
        let closings: Vec<Name> = books
            .names
            .iter()
            .filter(|name| matches!(name, Name::Closed { run, .. } if Some(*run) == drawn))
            .cloned()
            .collect();
        let outcome = books.apply(event);
        let after = books.fold();
        let broke = |what: String| format!("event {at}, {event:?}: {what}");
        // Whether the event was published as it was judged, late or not, and
        // whether it was published at all.
        let landed = !matches!(outcome, Outcome::Refused(_) | Outcome::AskedAgain(_));
        let done = match &outcome {
            Outcome::AskedAgain(inner) => !matches!(**inner, Outcome::Refused(_)),
            other => !matches!(other, Outcome::Refused(_)),
        };
        // 4. Damage freezes the one reservation it belongs to, and changes no other:
        // nor does a name other hands made, or a name that is lost.
        if let Event::Damage(name) | Event::Plant(name) | Event::Remove(name) = plain {
            let own = name.of();
            for (run, one) in &after.reservations {
                if Some(*run) != own && before.reservations.get(run) != Some(one) {
                    return Err(broke(format!("it changed the reservation of {run}")));
                }
            }
            let frozen = own.and_then(|run| after.reservations.get(&run));
            let held = |one: &Standing| one.state == State::Adjusted || one.counts >= one.most();
            let damage = matches!(plain, Event::Damage(_));
            if damage && matches!(outcome, Outcome::Done) && !frozen.is_none_or(held) {
                return Err(broke(
                    "its reservation counts for less than its amount".into(),
                ));
            }
        }
        // Losing one name of the ledger's lowers nothing: a reservation that lost
        // one, and that no other hands touched, is frozen and counts for no less
        // than it did. Two losses are not held to this: a settlement over its
        // bound, whose charge was in that name alone, leaves no less than the
        // amount; and the hold, the amount's one name, leaves what is known.
        if let (Event::Remove(name), Outcome::Done) = (plain, &outcome)
            && books.removed.contains_key(name)
        {
            let alone = |run: Run| books.lost_and_planted(run) == (1, 0);
            let own = name.of().filter(|run| alone(*run));
            let was = own.and_then(|run| before.reservations.get(&run));
            let is = own.and_then(|run| after.reservations.get(&run));
            if let (Some(was), Some(is)) = (was, is) {
                let over_bound = match name {
                    Name::Settled {
                        run, place, micros, ..
                    } => {
                        let asked = books
                            .truth
                            .get(run)
                            .and_then(|truth| truth.places.get(place));
                        asked.is_some_and(|asked| *micros > asked.bound)
                    }
                    _ => false,
                };
                let least = match name {
                    Name::Held { .. } => was.known(),
                    _ if over_bound => u128::from(was.amount),
                    _ => was.counts,
                };
                if is.state != State::Frozen || is.counts < least {
                    return Err(broke(format!(
                        "its reservation counted {} and is {:?} at {}",
                        was.counts, is.state, is.counts
                    )));
                }
            }
            // And losing the allowance's own names allows no more.
            let touched = |name: &Name| matches!(name, Name::Reset { .. } | Name::Epoch(_));
            let others = books.planted.keys().any(touched)
                || books.removed.keys().filter(|name| touched(name)).count() > 1;
            if touched(name) && !others && after.balance().left > before.balance().left {
                return Err(broke("more can be reserved than before".into()));
            }
        }
        // What was judged late and published as it was judged is held to what
        // happened since. One thing is not the ledger's to have stopped: a
        // publication is held to the books only by a name it makes, so a turn, or a
        // place under the reservation, whose name was lost since the judgement no
        // longer refuses it, and a number that was taken and lost can be taken again
        // by a judgement that did not see what was published in between.
        let late = judged.filter(|_| landed);
        let since = |when: &usize| late.is_some_and(|judged| *when >= judged && *when < at);
        let root = drawn.unwrap_or(Run(0));
        let guards = |name: &Name| match plain {
            Event::Dispatch { .. } => name.of() == Some(root) && name.place().is_some(),
            _ => matches!(name, Name::Turn(_)),
        };
        let lost_since = |(name, when): (&Name, &usize)| guards(name) && since(when);
        let empty = BTreeSet::new();
        let names_before = at.checked_sub(1).and_then(|before| books.past.get(before));
        let names_before = names_before.map_or(&empty, |(names, _)| names);
        let again = |made: &Name| match made {
            Name::Turn(_) => books.removed.contains_key(made),
            Name::Place { run, place } => books
                .removed
                .keys()
                .any(|lost| lost.of() == Some(*run) && lost.place() == Some(*place)),
            _ => false,
        };
        let retaken =
            books.removed.iter().any(lost_since) || books.names.difference(names_before).any(again);
        // No dispatch is published once a closing names its reservation, whenever it
        // was judged. One more thing is not the ledger's to have stopped: a closing
        // that other hands made after the dispatch was judged.
        if matches!(plain, Event::Dispatch { .. }) && done {
            let unseen = |name: &Name| books.planted.get(name).is_some_and(since);
            let closing = closings.iter().find(|name| !unseen(name));
            if let Some(closing) = closing.filter(|_| !retaken) {
                return Err(broke(format!("dispatched after {closing}")));
            }
        }
        // 6. No admission is published on a balance the ledger has lowered since it
        // was judged, and no dispatch on a reservation the ledger has taken a place
        // under since: whatever lowers either takes a turn or a place, which is
        // written down here apart from the names.
        let stale = match plain {
            Event::Admit { .. } => books.turned.iter().any(since),
            Event::Dispatch { .. } => books
                .placed
                .get(&root)
                .is_some_and(|taken| taken.iter().any(since)),
            _ => false,
        };
        if stale && !retaken {
            return Err(broke("published on a stale balance".into()));
        }

        // 2. Every return pays the debt first: what comes back is added to the
        // signed allowance, whatever it was, and to nothing else.
        let returned = match (plain, &outcome) {
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

    /// The runs that execute in the first process, or in the second, in order.
    pub fn executing_in(&self, second: bool) -> Vec<Run> {
        let live = if second { &self.other.live } else { &self.live };
        let executing = |(_, live): &(&Run, &Live)| live.state == Liveness::Executing;
        live.iter().filter(executing).map(|(run, _)| *run).collect()
    }

    /// The runs that execute in the first process, in order.
    pub fn executing_runs(&self) -> Vec<Run> {
        self.executing_in(false)
    }

    /// The requests the first process, or the second, sent and has not settled: the
    /// reservation and the place of each.
    pub fn unsettled_in(&self, second: bool) -> Vec<(Run, u64)> {
        let sent = if second { &self.other.sent } else { &self.sent };
        sent.keys().copied().collect()
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
    /// What this run sent that is charged or outstanding, as the names say it.
    Sent(Run, u64),
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
            Must::EpochOf(run, epoch) => match of(run)?.epoch {
                Some(found) => same(&format!("the epoch of {run}"), found.into(), *epoch),
                None => Err(format!("{run} falls under no one epoch")),
            },
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
            Must::Sent(run, micros) => {
                let spend = books.run(*run);
                let found = spend.charged + spend.outstanding;
                same(&format!("{run} sent"), found, *micros)
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

fn second(event: Event) -> Event {
    Event::Second(Box::new(event))
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
            Event::Plant(Name::Place {
                run: r(1),
                place: 2,
            }),
            end(1),
        ],
        vec![
            came(6, refused(Why::AlreadyClosed)),
            SameNamesAsAfter(5),
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
    // What the review round of 2026-11-14 found: each of its seven traces, as it was
    // given, with what the books must give now.
    let prefix = || vec![reset(20), admit(1, 10), dispatch(1, 5)];
    add(
        "43 a settlement other hands made without its guard: it lowers nothing, and \
         recovery releases nothing",
        with(
            prefix(),
            vec![
                dispatch(1, 5),
                settle(1, 1, charged(12)),
                Event::Plant(Name::Settled {
                    run: r(1),
                    place: 2,
                    stage: Stage::NotSent,
                    micros: 0,
                }),
                Event::Crash,
                Event::Recover,
            ],
        ),
        vec![
            state(r(1), State::Frozen),
            Counts(r(1), 17),
            Outstanding(r(1), 5),
            Left(3),
        ],
        vec![
            (4, Counts(r(1), 17)),
            (5, state(r(1), State::Frozen)),
            (5, Counts(r(1), 17)),
        ],
    );
    add(
        "44 a correction: one other hands made without its place is none; the owner's \
         writes off what is outstanding; and it says nothing once the names it was made \
         over have changed, or it is damaged",
        with(
            prefix(),
            vec![
                Event::Crash,
                Event::Recover,
                // The reservation has six names: a correction over six, with no place.
                Event::Plant(Name::Adjusted {
                    run: r(1),
                    over: 6,
                    micros: 0,
                }),
                adjust(1, 0),
                Event::Plant(Name::Closed {
                    run: r(1),
                    released: true,
                }),
                adjust(1, 0),
                Event::Damage(Name::Adjusted {
                    run: r(1),
                    over: 10,
                    micros: 0,
                }),
            ],
        ),
        vec![
            came(6, Outcome::Done),
            came(8, Outcome::Done),
            came(9, Outcome::Done),
            state(r(1), State::Frozen),
            Counts(r(1), 10),
            Left(10),
        ],
        vec![
            (4, state(r(1), State::Interrupted)),
            (5, state(r(1), State::Frozen)),
            (5, Counts(r(1), 10)),
            (6, state(r(1), State::Adjusted)),
            (6, Counts(r(1), 0)),
            (6, Left(20)),
            (7, state(r(1), State::Frozen)),
            (7, Counts(r(1), 10)),
            (8, state(r(1), State::Adjusted)),
            (8, Counts(r(1), 0)),
        ],
    );
    add(
        "45 a second hold in another epoch: frozen, and counted in every epoch a hold names",
        with(
            prefix(),
            vec![
                Event::Plant(Name::Held {
                    epoch: 2,
                    run: r(1),
                    micros: 10,
                }),
                reset(30),
            ],
        ),
        vec![state(r(1), State::Frozen), Epoch(2), Counted(10), Left(20)],
        vec![(3, Epoch(1)), (3, Counted(10)), (3, Left(10))],
    );
    add(
        "46 a second hold of more: counted at the larger, and nothing more is dispatched \
         than the smaller has left",
        with(
            prefix(),
            vec![
                child(1, 2, 5),
                settle(1, 1, charged(12)),
                Event::Plant(Name::Held {
                    epoch: 1,
                    run: r(1),
                    micros: 20,
                }),
                dispatch(2, 5),
            ],
        ),
        vec![
            came(6, refused(Why::OverReservation { left: 0 })),
            state(r(1), State::Frozen),
            Counts(r(1), 20),
            Left(0),
        ],
        vec![],
    );
    add(
        "47 a turn other hands made far ahead: an admission that judged before a \
         settlement over its bound is still asked again",
        with(
            prefix(),
            vec![
                Event::Plant(Name::Turn(100)),
                settle(1, 1, charged(12)),
                stale(3, admit(2, 10)),
            ],
        ),
        vec![
            came(5, again(refused(Why::OverAllowance { left: 8 }))),
            Absent(r(2)),
            Counted(12),
        ],
        vec![],
    );
    add(
        "48 a closing other hands made without its place: nothing is released and \
         nothing more is dispatched; a dispatch judged before it is published, and the \
         reservation stays frozen in full",
        vec![
            reset(20),
            admit(1, 10),
            Event::Plant(Name::Closed {
                run: r(1),
                released: true,
            }),
            dispatch(1, 5),
            stale(2, dispatch(1, 5)),
        ],
        vec![
            came(3, refused(Why::AlreadyClosed)),
            came(4, Outcome::Placed(1)),
            state(r(1), State::Frozen),
            Counts(r(1), 10),
            Outstanding(r(1), 5),
            Left(10),
        ],
        vec![(2, state(r(1), State::Frozen)), (2, Counts(r(1), 10))],
    );
    add(
        "49 a closing with its place, made by a second process: a dispatch judged before \
         it is asked again and refused",
        vec![
            reset(20),
            admit(1, 10),
            second(Event::Recover),
            stale(2, dispatch(1, 5)),
            end(1),
        ],
        vec![
            came(3, again(refused(Why::AlreadyClosed))),
            came(4, refused(Why::AlreadyClosed)),
            state(r(1), State::Released),
            Counts(r(1), 0),
            Left(20),
        ],
        vec![],
    );
    add(
        "50 a second dispatch at one place that names another run: frozen, and each run \
         named is counted for the larger bound",
        with(
            prefix(),
            vec![Event::Plant(Name::Dispatched {
                run: r(1),
                place: 1,
                by: r(0),
                micros: 0,
            })],
        ),
        vec![
            state(r(1), State::Frozen),
            Counts(r(1), 10),
            Must::Sent(r(1), 5),
            Must::Sent(r(0), 5),
        ],
        vec![],
    );
    add(
        "51 a run that ends while a request it sent is outstanding: refused, and ended \
         once the request is settled",
        with(
            prefix(),
            vec![end(1), stale(3, end(1)), settle(1, 1, charged(12)), end(1)],
        ),
        vec![
            came(3, refused(Why::RequestsOutstanding)),
            came(4, again(refused(Why::RequestsOutstanding))),
            came(6, Outcome::Done),
            state(r(1), State::Released),
            Counts(r(1), 12),
            Left(8),
        ],
        vec![(3, SameNamesAsAfter(2)), (3, state(r(1), State::Open))],
    );
    add(
        "52 a crash after the provider charged and before the settlement: the full amount",
        with(
            prefix(),
            vec![
                Event::Charge {
                    root: r(1),
                    place: 1,
                    micros: 3,
                },
                Event::Crash,
                Event::Recover,
            ],
        ),
        vec![
            state(r(1), State::Interrupted),
            Counts(r(1), 10),
            Outstanding(r(1), 5),
            Charged(r(1), 0),
            Left(10),
        ],
        vec![(3, SameNamesAsAfter(2))],
    );
    add(
        "53 a settlement at 12 over a bound of 5 whose name is lost: frozen, at no less \
         than the bound and the amount",
        with(
            prefix(),
            vec![
                settle(1, 1, charged(12)),
                Event::Remove(Name::Settled {
                    run: r(1),
                    place: 1,
                    stage: Stage::Answered,
                    micros: 12,
                }),
            ],
        ),
        vec![
            state(r(1), State::Frozen),
            Outstanding(r(1), 5),
            // The bound, and the amount once for the place the charge took, which
            // nothing accounts for now.
            Counts(r(1), 15),
            Left(5),
        ],
        vec![(3, Counts(r(1), 12)), (3, Left(8))],
    );
    add(
        "54 a second process recovers while the first still runs a request: closed with \
         nothing released, and what the request is charged afterwards is counted",
        with(
            prefix(),
            vec![
                second(Event::Recover),
                dispatch(1, 1),
                settle(1, 1, charged(12)),
                end(1),
            ],
        ),
        vec![
            came(4, refused(Why::AlreadyClosed)),
            came(5, Outcome::Done),
            came(6, refused(Why::AlreadyClosed)),
            state(r(1), State::Interrupted),
            Counts(r(1), 12),
            Left(8),
        ],
        vec![(3, state(r(1), State::Interrupted)), (3, Counts(r(1), 10))],
    );
    add(
        "55 a reset whose guard is lost, and one other hands made without a guard: \
         nothing is allowed until the owner resets again",
        vec![
            reset(20),
            Event::Remove(Name::Epoch(1)),
            admit(1, 1),
            reset(7),
            Event::Plant(Name::Reset {
                epoch: 3,
                micros: 1000,
            }),
            admit(2, 1),
            reset(9),
            admit(3, 9),
        ],
        vec![
            came(2, refused(Why::OverAllowance { left: 0 })),
            came(5, refused(Why::OverAllowance { left: 0 })),
            came(7, Outcome::Done),
            Epoch(4),
            Allowed(9),
            Left(0),
        ],
        vec![
            (1, Allowed(0)),
            (3, Allowed(7)),
            (4, Epoch(3)),
            (4, Allowed(0)),
        ],
    );
    // What the generated histories found, each under its seed and number.
    add(
        "56 a request with a bound of nothing that is not settled: the closing releases \
         nothing (seed 0x1b0020261114, history 585)",
        vec![
            reset(20),
            admit(1, 10),
            dispatch(1, 0),
            second(Event::Recover),
            settle(1, 1, charged(12)),
        ],
        vec![
            came(4, Outcome::Done),
            state(r(1), State::Interrupted),
            Outstanding(r(1), 0),
            Counts(r(1), 12),
            Left(8),
        ],
        vec![(3, state(r(1), State::Interrupted)), (3, Counts(r(1), 10))],
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
/// most of them apply; about one publication in five is judged late, and about one
/// event in four happens in a second process over the same store. Other hands make
/// names of every form, a whole settlement and a whole correction among them, and
/// names are damaged and lost.
pub fn generated(seed: u64, number: u64) -> Vec<Event> {
    let mut seeded = Seeded::new(seed ^ number.wrapping_mul(0xA24B_AED4_963E_E407));
    let length = 1 + seeded.below(GENERATED_EVENTS as u64) as usize;
    let mut books = Books::new();
    let mut events: Vec<Event> = Vec::new();
    let mut taken: Vec<(bool, Run, u64)> = Vec::new();
    // The second name of a pair other hands make, for the event after its first.
    let mut pending: Option<Event> = None;
    let mut made = 0u32;
    while events.len() < length {
        let other = seeded.below(4) == 0;
        let live = books.executing_in(other);
        let unsettled = books.unsettled_in(other);
        let folded = books.fold();
        let roots: Vec<Run> = folded.reservations.keys().copied().collect();
        let names: Vec<Name> = books.names().iter().cloned().collect();
        let mut new_run = || {
            made += 1;
            Run(made)
        };
        let any_run = |seeded: &mut Seeded| Run(1 + seeded.below(8) as u32);
        let own = |run: Run| names.iter().filter(|name| name.of() == Some(run)).count() as u64;
        let event = match seeded.below(30) {
            _ if pending.is_some() => pending.take(),
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
                // Mostly a request this process has not settled; at times any place.
                let at = match seeded.below(4) {
                    0 => seeded.pick(&roots).map(|root| (root, 1 + seeded.below(4))),
                    _ => seeded.pick(&unsettled),
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
            20 | 21 => {
                let (run, place) = (any_run(&mut seeded), 1 + seeded.below(4));
                let micros = seeded.below(15);
                Some(Event::Plant(match seeded.below(12) {
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
                    7 => Name::Turn(folded.turn + seeded.below(3) * 50),
                    8 => Name::Adjusted {
                        run,
                        over: (own(run) + seeded.below(3)).max(1),
                        micros,
                    },
                    9 => Name::Epoch(1 + seeded.below(3)),
                    10 => Name::Reset {
                        epoch: 1 + seeded.below(3),
                        micros: seeded.below(60),
                    },
                    _ => Name::Reservation(run),
                }))
            }
            22 => {
                // Mostly a reservation that is closed, and half the time a little.
                let closed = folded.reservations.iter().filter(|(_, one)| one.closed);
                let closed: Vec<Run> = closed.map(|(run, _)| *run).collect();
                let of = if seeded.below(4) == 0 {
                    &roots
                } else {
                    &closed
                };
                seeded.pick(of).map(|run| Event::Adjust {
                    run,
                    micros: seeded.below(15) / (1 + seeded.below(2) * 5),
                })
            }

            23 => match seeded.below(2) {
                0 => seeded.pick(&live).map(|run| Event::Take {
                    run,
                    micros: seeded.below(books.spendable(run).unwrap_or(0) + 1),
                }),
                _ => taken
                    .iter()
                    .rposition(|(of, _, _)| *of == other)
                    .map(|at| taken.remove(at))
                    .map(|(_, run, micros)| Event::GiveBack { run, micros }),
            },
            24 | 25 => seeded.pick(&unsettled).map(|(root, place)| Event::Charge {
                root,
                place,
                micros: seeded.below(15),
            }),
            26 | 27 => seeded.pick(&names).map(Event::Remove),
            28 => {
                // A whole chain, as other hands would make one: a settlement and its
                // guard at a place that is dispatched; or a correction, over the
                // names its reservation has, and the place it takes.
                let sent = names.iter().filter_map(|name| match name {
                    Name::Dispatched { run, place, .. } => Some((*run, *place)),
                    _ => None,
                });
                let sent: Vec<(Run, u64)> = sent.collect();
                match seeded.below(2) {
                    0 => seeded.pick(&sent).map(|(run, place)| {
                        pending = Some(Event::Plant(Name::Settled {
                            run,
                            place,
                            stage: Stage::ALL[seeded.below(3) as usize],
                            micros: seeded.below(6),
                        }));
                        Event::Plant(Name::Settling { run, place })
                    }),
                    _ => seeded.pick(&roots).map(|run| {
                        pending = Some(Event::Plant(Name::Place {
                            run,
                            place: next_place(books.names(), run),
                        }));
                        Event::Plant(Name::Adjusted {
                            run,
                            over: own(run).max(1),
                            micros: seeded.below(15),
                        })
                    }),
                }
            }
            _ => seeded.pick(&roots).map(|run| Event::DeletePrune { run }),
        };
        // Where nothing could be chosen, a call is admitted: there is always one.
        let event = event.unwrap_or_else(|| Event::Admit {
            run: new_run(),
            micros: seeded.below(13),
        });
        let hands = matches!(event, Event::Plant(_) | Event::Damage(_) | Event::Remove(_));
        let late = !hands && seeded.below(5) == 0;
        let event = if late {
            stale(seeded.below(events.len() as u64 + 1) as usize, event)
        } else {
            event
        };
        let event = if other && !hands {
            second(event)
        } else {
            event
        };
        let outcome = books.apply(&event);
        if let (Event::Take { run, micros }, Outcome::Done) = (event.plain().2, &outcome) {
            taken.push((other, *run, *micros));
        }
        // A share that could not be handed back is handed back to nobody.
        events.push(event);
    }
    events
}
