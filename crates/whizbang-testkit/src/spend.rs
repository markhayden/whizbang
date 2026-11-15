//! A model of the ledger's books (ADR 0016): pure, with no store.
//!
//! The books are a set of names, as the store's generation holds them under `spend/`,
//! and a note of which of them have content that cannot be read. Every amount is in a
//! name. [`Books::apply`] takes one [`Event`] of a history and does what the ledger's
//! code is to do with it: it judges on the names, and publishes the names the event
//! makes, all of them or, when one of them is there already, none. An admission
//! that is published is then read back by the process that made it, and stands or
//! is backed out ([`stands`]); nothing runs under it before. [`fold`] answers
//! what is allowed, counted, charged and left, for the store, for one reservation and
//! for one run, from the names alone. A reservation counts by the ledger's arithmetic
//! only when its names are a set the ledger could have written, which a recogniser
//! decides; any other is frozen at the most its names can mean.
//!
//! Beside the names the model keeps two things that are not in a store. What each of
//! two processes holds in memory: the runs it has open, each with a signed allowance,
//! the requests it sent, and what is left of the reservation of each tree it runs,
//! which every dispatch is held to whatever the names say; a crash loses it. And the
//! truth: what was really
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
use std::sync::Arc;

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
    /// `spend/turn/<epoch>/<n>`: made by whatever can change what is left.
    Turn { epoch: u64, turn: u64 },
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
    /// `spend/adjusted/<run>/<place>/<turn>/<seal>/<micros>`: a person's correction,
    /// with the place and the turn it took and a seal of the names it was made over.
    Adjusted {
        run: Run,
        place: u64,
        turn: u64,
        seal: u64,
        micros: u64,
    },
    /// `spend/stray/<run>/<n>`: stands for any name under `spend/` that names this
    /// run where a reservation's run is named and is of none of the forms above.
    Stray { run: Run, count: u64 },
}

impl Name {
    /// The run whose reservation this name belongs to, when it belongs to one.
    pub fn of(&self) -> Option<Run> {
        match self {
            Name::Reset { .. } | Name::Epoch(_) | Name::Turn { .. } => None,
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
        // A seal is sixteen digits of base sixteen, in lower case.
        let sealed = |part: &str| -> Option<u64> {
            let plain = part.len() == 16
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
            plain.then(|| u64::from_str_radix(part, 16).ok()).flatten()
        };
        let parts: Vec<&str> = text.strip_prefix("spend/")?.split('/').collect();
        Some(match parts.as_slice() {
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
                stage: Stage::ALL.into_iter().find(|one| one.name() == *stage)?,
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
            Name::Turn { epoch, turn } => write!(f, "spend/turn/{epoch}/{turn}"),
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
            Name::Adjusted {
                run,
                place,
                turn,
                seal,
                micros,
            } => write!(
                f,
                "spend/adjusted/{run}/{place}/{turn}/{seal:016x}/{micros}"
            ),
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
    /// Its names are not a set the ledger could have written: it counts for the most
    /// they can mean, and nothing lowers it but a person's correction.
    Frozen,
    /// A person corrected it: it counts for what the correction says.
    Adjusted,
}

/// One reservation, as the fold answers it. Amounts are in millionths of a US dollar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// The epoch it falls under: the reset its one hold names. None when it has no
    /// hold, or more than one.
    pub epoch: Option<u64>,
    /// Every epoch a hold of it names. It counts in the one its one hold names; and
    /// in every epoch there is where it is frozen, or has no hold or more than one.
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
    /// Where the names are not reachable: the numbers from 1 to the highest place
    /// any of them names that hold no dispatch. Each may be a dispatch whose name is
    /// lost, of any bound up to the amount. None where the names are reachable.
    pub unexplained: u64,
    /// The largest amount a correction of it names.
    pub corrections: u64,
    /// Whether its names, corrections aside, are a set the ledger could have written.
    pub reachable: bool,
    /// How many names name its run, of any form: two, the record and one hold, where
    /// the admission's publication made them and nothing was made under it since.
    pub names: u64,
    /// What it counts for against the allowance of each epoch it counts in.
    pub counts: u128,
    /// It is frozen and its holds are not exactly one: the ledger's own hold may be
    /// the one that is lost, and then no name states its amount. What it counts for
    /// is unknown, and at least `counts`.
    pub unknown: bool,
    pub state: State,
    pub closed: bool,
}

/// What a reservation counts for, as far as the books can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counted {
    Exactly(u128),
    /// The books cannot say how much: no less than this.
    AtLeast(u128),
}

impl Standing {
    /// What it counts for: a number, or "unknown, at least" one.
    pub fn counted(&self) -> Counted {
        match self.unknown {
            true => Counted::AtLeast(self.counts),
            false => Counted::Exactly(self.counts),
        }
    }

    /// What is known against it: its charges, and what its places count for beyond.
    pub fn known(&self) -> u128 {
        self.charged.saturating_add(self.outstanding)
    }

    /// The most its names can mean. Of a reachable reservation: its amount, what is
    /// known, or what a correction says, whichever is most. Of any other, with no
    /// name trusted to explain another: its amount, and every bound and every charge
    /// named under it, and its amount once more for each place that holds no
    /// dispatch; or what a correction says, when that is more. A name can state any
    /// amount and name any place, so the sum stops at the largest number there is,
    /// which is over any allowance.
    pub fn most(&self) -> u128 {
        let amount = u128::from(self.amount);
        let named = match self.reachable {
            true => self.known(),
            false => {
                let places = u128::from(self.unexplained).saturating_mul(amount);
                amount.saturating_add(self.known()).saturating_add(places)
            }
        };
        u128::from(self.amount.max(self.corrections)).max(named)
    }

    /// What a dispatch under it may still take: nothing unless it is open, and
    /// nothing once any closing names it, whatever its state reads.
    pub fn left(&self) -> u64 {
        if self.closed || self.state != State::Open {
            return 0;
        }
        u64::try_from(u128::from(self.amount).saturating_sub(self.known())).unwrap_or(u64::MAX)
    }

    /// Whether it counts against this epoch's allowance.
    pub fn counts_in(&self, epoch: u64) -> bool {
        // A frozen reservation counts in every epoch there is: its holds are not
        // trusted to say which. So does one that has no hold, or more than one,
        // whatever it is: a correction says what it is charged, and not where.
        self.state == State::Frozen || self.epoch.is_none() || self.epochs.contains(&epoch)
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
    /// The books cannot say how much is counted: a reservation that counts here has
    /// an amount no name can be trusted for, and `counted` is the least it can be.
    pub unknown: bool,
    /// What was charged under them.
    pub charged: u128,
    /// What is held beyond what was charged.
    pub held: u128,
    pub unresolved: u128,
    /// What can still be reserved: never below nothing, and nothing while the
    /// epoch's turns are not what its publications account for.
    pub left: u64,
    /// By how much what is counted is over the allowance.
    pub debt: u128,
    /// The epoch's turns are not exactly those its publications took: nothing is
    /// admitted until the owner resets.
    pub needs_owner: bool,
}

/// What one run sent under its tree's reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunSpend {
    pub charged: u128,
    pub outstanding: u128,
    pub unresolved: u128,
}

impl RunSpend {
    /// What it sent that is charged or outstanding.
    pub fn known(&self) -> u128 {
        self.charged.saturating_add(self.outstanding)
    }
}

/// What the books can say of one run's spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// Its reservation's names are whole: this is what it sent.
    Known(RunSpend),
    /// Its reservation is frozen: which run sent what cannot be read, and no run
    /// under it sent more than this.
    Unknown { at_most: u128 },
    /// Its reservation is frozen and its amount cannot be read either: no most can
    /// be said.
    Unbounded,
}

/// The books folded: every reservation the names give, by its run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Folded {
    pub reservations: BTreeMap<Run, Standing>,
    /// What the dispatches name each run for.
    pub runs: BTreeMap<Run, RunSpend>,
    /// The allowance of each epoch there is: nothing for one whose reset is not whole.
    pub resets: BTreeMap<u64, u64>,
    /// The turns taken in each epoch, and how many its publications account for.
    pub turns: BTreeMap<u64, BTreeSet<u64>>,
    pub due: BTreeMap<u64, u64>,
}

impl Folded {
    /// The newest epoch: nothing before the first reset.
    pub fn epoch(&self) -> u64 {
        self.resets.keys().next_back().copied().unwrap_or(0)
    }

    /// The next turn of an epoch: the lowest that is not taken.
    pub fn next_turn(&self, epoch: u64) -> u64 {
        self.turns.get(&epoch).map_or(1, lowest_free)
    }

    /// Whether no new paid work starts in an epoch until the owner acts. Its turns
    /// are other than exactly those its publications took: not 1 to their count, or
    /// more or fewer than the names account for, so that a name that took a turn
    /// may be gone, and with it an amount that nothing counts. Or a reservation is
    /// frozen and its amount is unknown, which counts in every epoch for a number
    /// no name can be trusted for. A frozen reservation whose one hold states its
    /// amount stops nothing else: it is counted at the most its names can mean.
    pub fn needs_owner(&self, epoch: u64) -> bool {
        let none = BTreeSet::new();
        let turns = self.turns.get(&epoch).unwrap_or(&none);
        let count = turns.len() as u64;
        let due = self.due.get(&epoch).copied().unwrap_or(0);
        let unknown = |one: &Standing| one.unknown;
        !turns.iter().copied().eq(1..=count)
            || count != due
            || self.reservations.values().any(unknown)
    }

    /// The balance of this epoch.
    pub fn balance_of(&self, epoch: u64) -> Balance {
        let allowed = self.resets.get(&epoch).copied().unwrap_or(0);
        let of_epoch = || {
            let counted = move |one: &&Standing| one.counts_in(epoch);
            self.reservations.values().filter(counted)
        };
        // A name can state any amount and name any place, so a sum of what frozen
        // reservations count for stops at the largest number there is.
        let sum = |of: fn(&Standing) -> u128| of_epoch().map(of).fold(0, u128::saturating_add);
        let counted = sum(|one| one.counts);
        let charged = sum(|one| one.charged);
        let unresolved = sum(|one| one.unresolved);
        let unknown = of_epoch().any(|one| one.unknown);
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
            left: u64::try_from(left).unwrap_or(u64::MAX),
            debt: counted.saturating_sub(u128::from(allowed)),
            needs_owner,
        }
    }

    /// The balance of the newest epoch: what an admission is judged on.
    pub fn balance(&self) -> Balance {
        self.balance_of(self.epoch())
    }

    /// What a run sent under the reservation of the run at the root of its tree.
    pub fn sent_by(&self, run: Run, root: Run) -> Sent {
        let known = Sent::Known(self.runs.get(&run).copied().unwrap_or_default());
        match self.reservations.get(&root) {
            // With no hold, or more than one, no name is trusted for the amount,
            // and the most rests on the amount.
            Some(one) if !one.reachable && one.epoch.is_none() => Sent::Unbounded,
            Some(one) if !one.reachable => Sent::Unknown {
                at_most: one.most(),
            },
            _ => known,
        }
    }

    /// The epochs a publication under this reservation takes a turn in: each its
    /// holds name, or the newest when it has none.
    fn turn_epochs(&self, run: Run) -> Vec<u64> {
        let epochs = self.reservations.get(&run).map(|one| &one.epochs);
        match epochs.filter(|epochs| !epochs.is_empty()) {
            Some(epochs) => epochs.iter().copied().collect(),
            None => vec![self.epoch()],
        }
    }
}

/// The lowest number from 1 that is not among these.
fn lowest_free(taken: &BTreeSet<u64>) -> u64 {
    let mut free = 1;
    for one in taken {
        if *one == free {
            free = free.saturating_add(1);
        }
    }
    free
}

/// A sum of amounts that stops at the largest number there is.
fn total(amounts: impl Iterator<Item = u64>) -> u128 {
    amounts.fold(0, |all, one| all.saturating_add(u128::from(one)))
}

/// The next place under a reservation: the lowest that no name of it names.
fn next_place(names: &BTreeSet<Name>, run: Run) -> u64 {
    let own = names.iter().filter(|name| name.of() == Some(run));
    lowest_free(&own.filter_map(Name::place).collect())
}

/// What a correction is sealed over: these names, each with whether it is damaged.
/// The model's seal is FNV-1a over their text; the real one is task I2's second part.
pub fn seal<'a>(names: impl Iterator<Item = (&'a Name, bool)>) -> u64 {
    let mut hash = 0xCBF2_9CE4_8422_2325u64;
    for (name, damaged) in names {
        let mark = if damaged { "!\n" } else { "\n" };
        for byte in name.to_string().bytes().chain(mark.bytes()) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    hash
}

/// Whether a settlement's stage allows its charge for this bound (ADR 0010): not
/// sent, nothing; unresolved, the bound; answered, any charge.
fn allowed(stage: Stage, charge: u64, bound: u64) -> bool {
    match stage {
        Stage::NotSent => charge == 0,
        Stage::Unresolved => charge == bound,
        Stage::Answered => true,
    }
}

/// A correction, as its name says it.
struct Correction {
    name: Name,
    place: u64,
    turn: u64,
    seal: u64,
    micros: u64,
    damaged: bool,
}

/// The names of one reservation, sorted by what they are.
#[derive(Default)]
struct Found {
    /// Every name of it, with whether it is damaged.
    all: Vec<(Name, bool)>,
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
    adjusted: Vec<Correction>,
    stray: bool,
    damaged: bool,
}

/// The recogniser: whether these names, corrections' seals aside, are exactly a set
/// that some sequence of the ledger's own publications makes (ADR 0016, "What the
/// ledger can have written").
fn reachable(of: &Found) -> bool {
    // An admission made one record and one hold; no name is of another form, and
    // none is damaged.
    if of.damaged || of.stray || !of.record || of.holds.len() != 1 {
        return false;
    }
    // The places taken are 1 to their count.
    let count = of.places.len() as u64;
    if !of.places.iter().copied().eq(1..=count) {
        return false;
    }
    let mut overruns = 0u64;
    for (place, sent) in &of.dispatched {
        // A dispatch is alone at its place, with the place.
        let [(_, bound)] = sent.as_slice() else {
            return false;
        };
        if !of.places.contains(place) {
            return false;
        }
        // Its settlement is one, with its guard, at a charge its stage allows.
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
    // A settlement is of a dispatch.
    let mut ended = of.settled.keys().chain(&of.settling);
    if ended.any(|place| !of.dispatched.contains_key(place)) {
        return false;
    }
    // One closing at most, and one that released has every dispatch settled.
    if of.closed.len() > 1 {
        return false;
    }
    let unsettled = |place: &u64| !of.settled.contains_key(place);
    if of.closed == [true] && of.dispatched.keys().any(unsettled) {
        return false;
    }
    // A correction is of a closed reservation, at a place of its own.
    let mut taken = BTreeSet::new();
    for one in &of.adjusted {
        let own = of.places.contains(&one.place) && !of.dispatched.contains_key(&one.place);
        if of.closed != [false] || !own || !taken.insert(one.place) {
            return false;
        }
    }
    // What the ledger checked, replayed in the order of the places: each dispatch
    // fit what the reservation had left when it took its place. A place below a
    // dispatch that holds none was taken by a charge over its bound, of a request
    // sent before it: nothing is dispatched after a closing. What the names do not
    // say is when a request was settled under
    // its bound, and which charge over its bound took which place: there the
    // replay takes what leaves the most, a request at the least of its bound and
    // its charge from the start, and the smallest excess first.
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
    // A correction follows the closing, which follows every dispatch: the closing's
    // place lies between the last dispatch and the lowest place a correction names,
    // so that no correction's place is below a dispatch either.
    if taken
        .first()
        .is_some_and(|lowest| *lowest <= last.saturating_add(1))
    {
        return false;
    }
    // Every place that is not a dispatch's was taken by a charge over its bound, by
    // the closing or by a correction.
    let bare = count - of.dispatched.len() as u64;
    bare == overruns + of.closed.len() as u64 + of.adjusted.len() as u64
}

/// Folds the books: what the names say, and nothing else. `damaged` are the names
/// whose content cannot be read, or does not agree with its name.
///
/// A reservation whose names are a set the ledger could have written counts by the
/// ledger's arithmetic. Any other is frozen at the most its names can mean.
pub fn fold(names: &BTreeSet<Name>, damaged: &BTreeSet<Name>) -> Folded {
    let mut folded = Folded::default();
    let mut found: BTreeMap<Run, Found> = BTreeMap::new();
    let mut resets: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let mut guards: BTreeSet<u64> = BTreeSet::new();
    let mut spoiled: BTreeSet<u64> = BTreeSet::new();
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
            Name::Turn { epoch, turn } => {
                folded.turns.entry(*epoch).or_default().insert(*turn);
                continue;
            }
            _ => {}
        }
        let Some(run) = name.of() else { continue };
        let of = found.entry(run).or_default();
        of.all.push((name.clone(), hurt));
        of.damaged |= hurt;
        match name {
            Name::Reset { .. } | Name::Epoch(_) | Name::Turn { .. } => {}
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
            Name::Adjusted {
                place,
                turn,
                seal,
                micros,
                ..
            } => of.adjusted.push(Correction {
                name: name.clone(),
                place: *place,
                turn: *turn,
                seal: *seal,
                micros: *micros,
                damaged: hurt,
            }),
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
    let newest = folded.epoch();
    // What each epoch's turns are due: one for the reset that closed it, one for
    // each hold in it, and below, one for each settlement, closing and correction
    // of a reservation that counts in it.
    let owe = |due: &mut BTreeMap<u64, u64>, epoch: u64, turns: u64| {
        let due = due.entry(epoch).or_default();
        *due = due.saturating_add(turns);
    };
    for epoch in folded.resets.keys().filter(|epoch| **epoch > 0) {
        owe(&mut folded.due, *epoch - 1, 1);
    }
    for (run, of) in found {
        // The amount and the epochs: the holds'.
        let amount = of.holds.iter().map(|(_, micros)| *micros).max();
        let amount = amount.unwrap_or(0);
        let epochs: BTreeSet<u64> = of.holds.iter().map(|(epoch, _)| *epoch).collect();
        for (epoch, _) in &of.holds {
            owe(&mut folded.due, *epoch, 1);
        }
        let turns = (of.settling.len() + of.closed.len() + of.adjusted.len()) as u64;
        for epoch in &epochs {
            owe(&mut folded.due, *epoch, turns);
        }
        let base = reachable(&of);
        let (mut charged, mut settled) = (0u128, 0u128);
        let (mut outstanding, mut unresolved) = (0u128, 0u128);
        let mut unsettled = 0u64;
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
            // A settlement's whole chain: one dispatch with its place, one
            // settlement with its guard at a charge its stage allows, none of the
            // four damaged.
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
            // What the place counts for beyond its charges. Where the names are
            // reachable, a place is settled or it is not: nothing, or its bound.
            // Where they are not, no settlement gives up a bound: every bound named
            // there, beside every charge.
            let rest = match whole && base {
                true => 0,
                false => bounds,
            };
            if whole {
                settled = settled.saturating_add(charge);
            } else {
                unsettled = unsettled.saturating_add(1);
            }
            let open = open.saturating_add(rest);
            charged = charged.saturating_add(charge);
            outstanding = outstanding.saturating_add(rest);
            unresolved = unresolved.saturating_add(open);
            // Every run a dispatch at this place names is named for all of it.
            let by: BTreeSet<Run> = sent.iter().map(|(by, _)| *by).collect();
            for by in by {
                let spend = folded.runs.entry(by).or_default();
                spend.charged = spend.charged.saturating_add(charge);
                spend.outstanding = spend.outstanding.saturating_add(rest);
                spend.unresolved = spend.unresolved.saturating_add(open);
            }
        }
        // Where the names are not reachable, no name is trusted to account for a
        // place: every number up to the highest place named that holds no dispatch
        // may be a dispatch whose name is lost.
        let unexplained = if base {
            0
        } else {
            let corrected = of.adjusted.iter().map(|one| &one.place);
            let named = of
                .places
                .iter()
                .chain(&at)
                .chain(&of.settling)
                .chain(corrected);
            let highest = named.max().copied().unwrap_or(0);
            // Every place a dispatch is at is one of the numbers up to the highest:
            // the rest of them hold none. Counted, never walked: a name can name
            // any place a number can say.
            highest.saturating_sub(of.dispatched.len() as u64)
        };
        let closed = !of.closed.is_empty();
        // A correction stands when it is not damaged, its reservation has a closing,
        // the place and the turn it names are there, nothing else is at its place,
        // and its seal is of exactly the other names of the reservation as they
        // are, its own place aside.
        let turn_epoch = epochs.first().copied().unwrap_or(newest);
        let stands = |one: &&Correction| {
            let guard = Name::Place {
                run,
                place: one.place,
            };
            let others = of
                .all
                .iter()
                .filter(|(name, _)| *name != one.name && *name != guard);
            let over = seal(others.map(|(name, hurt)| (name, *hurt)));
            let turn = folded.turns.get(&turn_epoch);
            !one.damaged
                && closed
                && of.places.contains(&one.place)
                && !of.hurt.contains(&one.place)
                && !at.contains(&one.place)
                && !of.settling.contains(&one.place)
                && turn.is_some_and(|turns| turns.contains(&one.turn))
                && one.seal == over
        };
        // Each is sealed over every other, so no two stand.
        let stands = of.adjusted.iter().find(stands).map(|one| one.micros);
        let corrections = of.adjusted.iter().map(|one| one.micros).max();
        let mut one = Standing {
            epoch: epochs.first().copied().filter(|_| of.holds.len() == 1),
            epochs,
            amount,
            charged,
            settled,
            outstanding,
            unresolved,
            unsettled,
            unexplained,
            corrections: corrections.unwrap_or(0),
            reachable: base,
            names: of.all.len() as u64,
            counts: 0,
            unknown: false,
            state: State::Open,
            closed,
        };
        (one.state, one.counts) = match stands {
            // A correction never counts under what settlements with their whole
            // chain say.
            Some(micros) => (State::Adjusted, u128::from(micros).max(settled)),
            None if !base => (State::Frozen, one.most()),
            None if of.closed == [true] => (State::Released, one.known()),
            None if closed => (State::Interrupted, one.most()),
            None => (State::Open, one.most()),
        };
        // The ledger's own hold may be the one that is lost, whatever holds are
        // there in its place: a frozen reservation that has not exactly one says
        // that its amount is unknown.
        one.unknown = one.state == State::Frozen && one.epoch.is_none();
        folded.reservations.insert(run, one);
    }
    folded
}

/// Whether an admission that was published stands, on a reading of the names made
/// after its publication (ADR 0016, "An admission is confirmed"): its reservation
/// is a set the ledger could have written, and open, and is the record, one hold
/// and nothing else that names the run; neither the epoch it fell
/// under nor the newest needs its owner, which a frozen reservation whose amount
/// is unknown makes every epoch do; and that epoch is not over its allowance with this reservation
/// counted in full. A call of nothing adds nothing to what is counted, and is not
/// held to a debt that was there before it, as it is not when it is judged.
///
/// Nothing is done under a reservation before it is confirmed, so a place, a
/// dispatch or a settlement under it at that reading is not the ledger's, though
/// the set may be one the ledger could have written later.
pub fn stands(folded: &Folded, run: Run, micros: u64) -> bool {
    let own = folded.reservations.get(&run);
    // Reachable, it has the record and one hold: with two names, it has no other.
    let Some(epoch) = own
        .filter(|one| one.state == State::Open && one.names == 2)
        .and_then(|one| one.epoch)
    else {
        return false;
    };
    if folded.needs_owner(epoch) || folded.needs_owner(folded.epoch()) {
        return false;
    }
    micros == 0 || folded.balance_of(epoch).debt == 0
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
    /// A call that nothing called is admitted with this reservation: its admission
    /// is published, and the process that made it then reads the names again and
    /// confirms it or backs it out, at once ([`Event::Publish`], [`Event::Confirm`]).
    Admit { run: Run, micros: u64 },
    /// The admission of a call that nothing called is published, and no more yet:
    /// the process that made it has not read the names again. Nothing runs under it.
    Publish { run: Run, micros: u64 },
    /// The process that published this run's admission reads the names again. The
    /// admission stands, and the run executes, only if on that reading its
    /// reservation is as the ledger wrote it and open, neither its epoch nor the
    /// newest needs its owner, and its epoch is not over its allowance with this
    /// reservation counted in full. Otherwise the process closes the reservation,
    /// which releases all it held, and the call is refused: nothing ran under it.
    Confirm { run: Run },
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
    /// The amount the event states, when it states one: an allowance, a
    /// reservation, a share, a bound, a charge or a correction.
    fn amount(&self) -> Option<u64> {
        match self {
            Event::Reset { micros }
            | Event::Admit { micros, .. }
            | Event::Publish { micros, .. }
            | Event::Child { micros, .. }
            | Event::Take { micros, .. }
            | Event::GiveBack { micros, .. }
            | Event::Dispatch { micros, .. }
            | Event::Charge { micros, .. }
            | Event::Adjust { micros, .. } => Some(*micros),
            Event::Settle { how, .. } => match how {
                How::Answered(charge) => *charge,
                How::NotSent | How::Unresolved => None,
            },
            Event::Second(inner) | Event::Stale { event: inner, .. } => inner.amount(),
            Event::Confirm { .. }
            | Event::End { .. }
            | Event::Abandon { .. }
            | Event::Crash
            | Event::Recover
            | Event::Damage(_)
            | Event::Plant(_)
            | Event::Remove(_)
            | Event::DeletePrune { .. } => None,
        }
    }

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
    /// The epoch's turns are not those its publications took, or a reservation is
    /// frozen with an amount that is unknown: nothing is admitted, and nothing
    /// dispatched, until the owner acts.
    NeedsOwner,
    /// What the run can spend does not cover it.
    OverRun {
        left: u64,
    },
    /// What is left of the reservation does not cover it.
    OverReservation {
        left: u64,
    },
    /// The reservation is frozen: nothing is dispatched under it.
    Frozen,
    /// What is left of the tree's reservation, as the process that runs it holds it
    /// in memory, does not cover it, whatever the names say.
    OverTree {
        left: u64,
    },
    /// The run is not executing in this process.
    NotExecuting,
    /// The admission was published, and on the names as they were read after it
    /// the books were not sound and within the allowance: its reservation was
    /// closed, and nothing ran under it.
    NotConfirmed,
    /// No admission of that run was published by this process and waits for it.
    NotPublished,
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
    /// Under what its whole settlements say.
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
    /// The amount stated is the largest number there is, which is no amount: no
    /// name states it (ADR 0007).
    NoAmount,
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

/// A request a process sent and has not settled: its reservation and place, the run
/// that sent it, its bound, the charge of the answer once the process holds one,
/// and where the truth keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Request {
    root: Run,
    place: u64,
    by: Run,
    bound: u64,
    answer: Option<u64>,
    truth: usize,
}

/// What a process holds of the reservation of one tree it runs: the amount, and
/// what the requests sent under it took of it, each at its bound until it is
/// settled and at its charge from then on, a charge over its bound among them, and
/// that from the moment the process holds the answer that says so. It
/// is kept as each request is sent and settled, and is never read from a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tree {
    amount: u64,
    taken: u128,
}

impl Tree {
    /// What a dispatch under it may still take: never below nothing.
    fn left(&self) -> u64 {
        let left = u128::from(self.amount).saturating_sub(self.taken);
        u64::try_from(left).unwrap_or(u64::MAX)
    }
}

/// What one process holds in memory: a crash loses it.
#[derive(Debug, Clone, Default)]
struct Memory {
    live: BTreeMap<Run, Live>,
    sent: Vec<Request>,
    trees: BTreeMap<Run, Tree>,
    waiting: BTreeMap<Run, u64>,
}

/// What really became of one request, whatever the names say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Asked {
    place: u64,
    by: Run,
    bound: u64,
    /// What the ledger's settlement of it says.
    settled: Option<u64>,
    /// Whether that settlement's name is still there.
    recorded: bool,
}

impl Asked {
    /// What the books can be held to for it: the charge its settlement recorded;
    /// its bound while none does; and, of a settlement whose name is lost, no more
    /// than the bound, because the charge was in that name and nowhere else (ADR
    /// 0016, limit (b)). A charge the provider made that was never settled is not
    /// written down here at all: no name ever stated it.
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
/// from a name. Every request is kept, two at one place among them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Truth {
    epoch: u64,
    amount: u64,
    asked: Vec<Asked>,
    /// The names and the damage of the reservation as the owner's last correction
    /// left them.
    corrected: Option<(BTreeSet<Name>, BTreeSet<Name>)>,
    /// The process that published its admission confirmed it: only then does
    /// anything run under it.
    confirmed: bool,
    /// The ledger closed it and released what it held beyond its charges.
    released: bool,
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
        at: usize,
        charge: u64,
    },
    Closed {
        run: Run,
    },
    Corrected {
        run: Run,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Plan {
    makes: Vec<Name>,
    then: Then,
    outcome: Outcome,
}

/// The turns a publication takes, one after another in each epoch.
struct Turns<'a> {
    folded: &'a Folded,
    taken: BTreeMap<u64, BTreeSet<u64>>,
}

impl Turns<'_> {
    /// The next turn of this epoch, and it is taken.
    fn take(&mut self, epoch: u64) -> Name {
        let taken = self.taken.entry(epoch).or_insert_with(|| {
            let there = self.folded.turns.get(&epoch);
            there.cloned().unwrap_or_default()
        });
        let turn = lowest_free(taken);
        taken.insert(turn);
        Name::Turn { epoch, turn }
    }

    /// A turn in each epoch a publication under this reservation takes one in.
    fn of(&mut self, run: Run) -> Vec<Name> {
        let epochs = self.folded.turn_epochs(run);
        epochs.into_iter().map(|epoch| self.take(epoch)).collect()
    }
}

/// The books, and what stands beside them.
#[derive(Debug, Clone, Default)]
pub struct Books {
    names: BTreeSet<Name>,
    damaged: BTreeSet<Name>,
    /// The runs the acting process has open.
    live: BTreeMap<Run, Live>,
    /// The requests the acting process dispatched and has not settled. A meter
    /// settles what it sent, from what it holds, and never from a name it reads.
    sent: Vec<Request>,
    /// The reservation of each tree the acting process runs, as it holds it.
    trees: BTreeMap<Run, Tree>,
    /// The admissions the acting process published and has not confirmed, each
    /// with its amount: their runs do not execute.
    waiting: BTreeMap<Run, u64>,
    /// The memory of the process that is not acting.
    other: Memory,
    /// Every run id that was given, with the run at the root of its tree.
    ids: BTreeMap<Run, Run>,
    /// The truth of each reservation the ledger made, and of each epoch's allowance.
    truth: BTreeMap<Run, Truth>,
    allowed: BTreeMap<u64, u64>,
    /// The names and the damage after each event, for what is judged late: shared,
    /// so that a copy of the books does not copy them.
    past: Vec<Arc<(BTreeSet<Name>, BTreeSet<Name>)>>,
    /// The names other hands made, and the names of the ledger's that were lost,
    /// each with the place of its event in the history; and the place of every
    /// event in which other hands did anything.
    planted: BTreeMap<Name, usize>,
    removed: BTreeMap<Name, usize>,
    hands: Vec<usize>,
    /// The reservations of which other hands ever made, lost or damaged a name.
    meddled: BTreeSet<Run>,
    /// The events in which the ledger took a turn, with the epoch.
    turned: Vec<(usize, u64)>,
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

    /// What one run sent, under the reservation of the run at the root of its tree.
    pub fn run(&self, run: Run) -> Sent {
        let root = self.ids.get(&run).copied().unwrap_or(run);
        self.fold().sent_by(run, root)
    }

    /// The seal of a correction of this reservation that takes this place, over the
    /// names as they are.
    pub fn seal_for(&self, run: Run, place: u64) -> u64 {
        let guard = Name::Place { run, place };
        let own = |name: &&Name| name.of() == Some(run) && **name != guard;
        let own = self.names.iter().filter(own);
        seal(own.map(|name| (name, self.damaged.contains(name))))
    }

    /// The next place under a reservation, as the names are.
    pub fn next_place(&self, run: Run) -> u64 {
        next_place(&self.names, run)
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
        self.sent.iter().any(|request| request.by == run)
    }

    /// Where the acting process holds the last request it sent at this place.
    fn request(&self, root: Run, place: u64) -> Option<usize> {
        let here = |request: &Request| request.root == root && request.place == place;
        self.sent.iter().rposition(here)
    }

    fn give(&mut self, run: Run, micros: i128) {
        if let Some(live) = self.live.get_mut(&run) {
            live.balance = live.balance.saturating_add(micros);
        }
    }

    /// The acting process and the other change places.
    fn switch(&mut self) {
        std::mem::swap(&mut self.live, &mut self.other.live);
        std::mem::swap(&mut self.sent, &mut self.other.sent);
        std::mem::swap(&mut self.trees, &mut self.other.trees);
        std::mem::swap(&mut self.waiting, &mut self.other.waiting);
    }

    /// What the event would publish, judged on these names.
    fn plan(&self, event: &Event, names: &BTreeSet<Name>, damaged: &BTreeSet<Name>) -> Plan {
        let folded = fold(names, damaged);
        let refused = |why| Plan {
            makes: Vec::new(),
            then: Then::Nothing,
            outcome: Outcome::Refused(why),
        };
        let mut turns = Turns {
            folded: &folded,
            taken: BTreeMap::new(),
        };
        let newest = folded.epoch();
        match event {
            Event::Reset { micros } => {
                // A reset closes the newest epoch with a turn in it, so that an
                // admission judged before it is asked again.
                // After the largest epoch a name can say there is none: the reset
                // names that one again, and the store refuses it.
                let epoch = newest.saturating_add(1);
                Plan {
                    makes: vec![
                        Name::Reset {
                            epoch,
                            micros: *micros,
                        },
                        Name::Epoch(epoch),
                        turns.take(newest),
                    ],
                    then: Then::Allowed {
                        epoch,
                        micros: *micros,
                    },
                    outcome: Outcome::Done,
                }
            }
            Event::Admit { run, micros } | Event::Publish { run, micros } => {
                if self.ids.contains_key(run) || folded.reservations.contains_key(run) {
                    return refused(Why::Taken);
                }
                let balance = folded.balance();
                if balance.needs_owner {
                    return refused(Why::NeedsOwner);
                }
                if *micros > balance.left {
                    return refused(Why::OverAllowance { left: balance.left });
                }
                Plan {
                    makes: vec![
                        Name::Reservation(*run),
                        Name::Held {
                            epoch: newest,
                            run: *run,
                            micros: *micros,
                        },
                        turns.take(newest),
                    ],
                    then: Then::Open {
                        run: *run,
                        epoch: newest,
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
                // Once any closing names the reservation, nothing is dispatched;
                // and nothing under one that is frozen.
                if standing.closed {
                    return refused(Why::AlreadyClosed);
                }
                if standing.state != State::Open {
                    return refused(Why::Frozen);
                }
                // Nor under a reservation of an epoch that needs its owner: where
                // the books are not what the ledger wrote, no new paid work starts
                // until a person has looked. A frozen reservation whose amount is
                // unknown makes every epoch need its owner; one whose hold states
                // its amount stops nothing but its own dispatches.
                if standing.epoch.is_none_or(|epoch| folded.needs_owner(epoch)) {
                    return refused(Why::NeedsOwner);
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
                let Some(at) = self.request(*root, *place) else {
                    return refused(Why::NoSuchPlace);
                };
                let request = self.sent[at];
                let bound = request.bound;
                // A request is settled by the run that sent it, while it is open:
                // nothing settles one for a run that was abandoned or is gone.
                if self.executing(request.by).is_none() {
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
                // A charge over its bound lowers what is left of the reservation:
                // it takes the next place, so that a dispatch judged before it is
                // refused by the store.
                if charge > bound {
                    makes.push(Name::Place {
                        run: *root,
                        place: next_place(names, *root),
                    });
                }
                // Every settlement takes a turn: it changes what is left.
                makes.extend(turns.of(*root));
                Plan {
                    makes,
                    then: Then::Ended { at, charge },
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
                // store; and a turn. It releases only when the reservation is open
                // and every dispatch has its settlement.
                let released = closes.state == State::Open && closes.unsettled == 0;
                let mut makes = vec![
                    Name::Closed {
                        run: *run,
                        released,
                    },
                    Name::Place {
                        run: *run,
                        place: next_place(names, *run),
                    },
                ];
                makes.extend(turns.of(*run));
                Plan {
                    makes,
                    then: Then::Closed { run: *run },
                    outcome: Outcome::Done,
                }
            }
            Event::Recover => {
                let mut makes = Vec::new();
                for (run, one) in &folded.reservations {
                    // An admission this process published and has not confirmed
                    // is its own to confirm or to close.
                    let own = self.live.contains_key(run) || self.waiting.contains_key(run);
                    if one.closed || own {
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
                    makes.extend(turns.of(*run));
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
                if one.counts_in(newest) && more > u128::from(left) {
                    return refused(Why::CannotBeHeld { left });
                }
                // It takes the reservation's next place and a turn, names both, and
                // is sealed over the names the reservation has.
                let place = next_place(names, *run);
                let taken = turns.of(*run);
                let turn = match taken.first() {
                    Some(Name::Turn { turn, .. }) => *turn,
                    _ => 1,
                };
                let own = names.iter().filter(|name| name.of() == Some(*run));
                let over = seal(own.map(|name| (name, damaged.contains(name))));
                let mut makes = vec![
                    Name::Adjusted {
                        run: *run,
                        place,
                        turn,
                        seal: over,
                        micros: *micros,
                    },
                    Name::Place { run: *run, place },
                ];
                makes.extend(taken);
                Plan {
                    makes,
                    then: Then::Corrected { run: *run },
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

    /// The names and the damage of one reservation, as they are.
    fn own(&self, run: Run) -> (BTreeSet<Name>, BTreeSet<Name>) {
        let of = |name: &&Name| name.of() == Some(run);
        (
            self.names.iter().filter(of).cloned().collect(),
            self.damaged.iter().filter(of).cloned().collect(),
        )
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
            if let Name::Turn { epoch, .. } = name {
                self.turned.push((now, *epoch));
            }
        }
        for name in &plan.makes {
            if let Name::Closed {
                run,
                released: true,
            } = name
                && let Some(truth) = self.truth.get_mut(run)
            {
                truth.released = true;
            }
        }
        match &plan.then {
            Then::Nothing => {}
            Then::Allowed { epoch, micros } => {
                self.allowed.insert(*epoch, *micros);
            }
            Then::Open { run, epoch, micros } => {
                // The reservation is made, and its run's id is given. The run does
                // not execute yet: its admission waits to be confirmed.
                self.ids.insert(*run, *run);
                self.waiting.insert(*run, *micros);
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
                if let Some(tree) = self.trees.get_mut(root) {
                    tree.taken = tree.taken.saturating_add(u128::from(*micros));
                }
                let truth = self.truth.entry(*root).or_default();
                truth.asked.push(Asked {
                    place: *place,
                    by: *by,
                    bound: *micros,
                    settled: None,
                    recorded: false,
                });
                self.sent.push(Request {
                    root: *root,
                    place: *place,
                    by: *by,
                    bound: *micros,
                    answer: None,
                    truth: truth.asked.len() - 1,
                });
            }
            Then::Ended { at, charge } => {
                let request = self.sent.remove(*at);
                let truth = self.truth.entry(request.root).or_default();
                if let Some(asked) = truth.asked.get_mut(request.truth) {
                    (asked.settled, asked.recorded) = (Some(*charge), true);
                }
                let (by, bound) = (request.by, request.bound);
                if let Some(tree) = self.trees.get_mut(&request.root) {
                    // It was held at its bound, or at the answer's charge where
                    // the process held one over it.
                    let held = request.answer.map_or(bound, |answer| answer.max(bound));
                    let rest = tree.taken.saturating_sub(u128::from(held));
                    tree.taken = rest.saturating_add(u128::from(*charge));
                }
                if *charge <= bound {
                    // What the bound held beyond the charge goes back to the run
                    // that sent the request, and pays its debt first.
                    self.give(by, i128::from(bound) - i128::from(*charge));
                } else {
                    // The excess is taken from the run and from each run above it.
                    let excess = i128::from(*charge) - i128::from(bound);
                    let mut at = Some(by);
                    while let Some(run) = at {
                        self.give(run, -excess);
                        at = self.live.get(&run).and_then(|live| live.parent);
                    }
                }
            }
            Then::Closed { run } => {
                self.live.retain(|_, live| live.root != *run);
                self.trees.remove(run);
            }
            Then::Corrected { run } => {
                let own = self.own(*run);
                if let Some(truth) = self.truth.get_mut(run) {
                    truth.corrected = Some(own);
                }
            }
        }
        true
    }

    /// Does one event of a history.
    pub fn apply(&mut self, event: &Event) -> Outcome {
        let outcome = self.applied(event);
        let now = (self.names.clone(), self.damaged.clone());
        self.past.push(Arc::new(now));
        outcome
    }

    fn applied(&mut self, event: &Event) -> Outcome {
        let refused = |why| Outcome::Refused(why);
        // The largest number there is is no amount: whatever states it is refused
        // before anything is judged, and makes no name.
        if event.amount() == Some(u64::MAX) {
            return refused(Why::NoAmount);
        }
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
                    Some(at) => self.past.get(at).map_or(&empty, |past| &**past),
                };
                let plan = self.plan(event, names, damaged);
                if matches!(plan.outcome, Outcome::Refused(_)) {
                    return plan.outcome;
                }
                if self.memory_allows(event) && self.publish(&plan) {
                    return self.published(event, plan.outcome);
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
                if self.ids.contains_key(child) {
                    return refused(Why::Taken);
                }
                let (root, left) = (above.root, self.spendable(*parent).unwrap_or(0));
                if *micros > left {
                    return refused(Why::OverRun { left });
                }
                self.give(*parent, -i128::from(*micros));
                self.ids.insert(*child, root);
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
                // The answer is of the last request sent at the place that has none.
                let here =
                    |request: &&mut Request| request.root == *root && request.place == *place;
                let mut at = self.sent.iter_mut().filter(here).collect::<Vec<_>>();
                let Some(last) = at.pop() else {
                    return refused(Why::NoSuchPlace);
                };
                if last.answer.is_some() {
                    return refused(Why::AlreadySettled);
                }
                last.answer = Some(*micros);
                // What the process knows was charged over the bound is taken from
                // what is left of the tree at once, before any name says it.
                let over = micros.saturating_sub(last.bound);
                if let Some(tree) = self.trees.get_mut(root) {
                    tree.taken = tree.taken.saturating_add(u128::from(over));
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
                self.trees.clear();
                self.waiting.clear();
                Outcome::Done
            }
            Event::Damage(name) => {
                if !self.names.contains(name) {
                    return refused(Why::NoSuchName);
                }
                self.damaged.insert(name.clone());
                self.hands.push(self.past.len());
                self.meddle(name);
                Outcome::Done
            }
            Event::Plant(name) => {
                if self.names.insert(name.clone()) {
                    self.planted.insert(name.clone(), self.past.len());
                    // A name of the ledger's that was lost and is made again as it
                    // was is no longer lost, and says again what only it said.
                    if self.removed.remove(name).is_some() {
                        self.record(name, true);
                    }
                }
                self.meddle(name);
                self.hands.push(self.past.len());
                Outcome::Done
            }
            Event::Remove(name) => {
                if !self.names.remove(name) {
                    return refused(Why::NoSuchName);
                }
                self.damaged.remove(name);
                self.hands.push(self.past.len());
                // A name other hands made is taken away again; one of the ledger's
                // is lost, and what only it said is lost with it.
                if self.planted.remove(name).is_none() {
                    self.removed.insert(name.clone(), self.past.len());
                    self.record(name, false);
                }
                self.meddle(name);
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
            Event::Confirm { run } => self.confirm(*run),
            Event::Reset { .. }
            | Event::Admit { .. }
            | Event::Publish { .. }
            | Event::Dispatch { .. }
            | Event::Settle { .. }
            | Event::Recover
            | Event::Adjust { .. } => self.judged_now(event),
        }
    }

    /// What follows a publication at once, in the process that made it: an
    /// admission is confirmed or backed out before anything else is done under it.
    fn published(&mut self, event: &Event, outcome: Outcome) -> Outcome {
        match event {
            Event::Admit { run, .. } => self.confirm(*run),
            _ => outcome,
        }
    }

    /// The process that published this run's admission reads the names again, as
    /// they are now, and the admission stands or is backed out ([`stands`]). One
    /// that stands makes its run execute. One that does not has its reservation
    /// closed as any reservation is closed, by the one rule of a closing: nothing
    /// was dispatched under it, so an open one releases all it held. Where it
    /// cannot be closed, because a closing other hands made names it already or
    /// its names are gone, it is left as it is, counted for what its names say.
    /// Either way the run never executes, and the call is refused.
    fn confirm(&mut self, run: Run) -> Outcome {
        let Some(micros) = self.waiting.remove(&run) else {
            return Outcome::Refused(Why::NotPublished);
        };
        if stands(&self.fold(), run, micros) {
            self.live.insert(
                run,
                Live {
                    root: run,
                    parent: None,
                    balance: i128::from(micros),
                    state: Liveness::Executing,
                },
            );
            let tree = Tree {
                amount: micros,
                taken: 0,
            };
            self.trees.insert(run, tree);
            if let Some(truth) = self.truth.get_mut(&run) {
                truth.confirmed = true;
            }
            return Outcome::Done;
        }
        let closing = self.plan(&Event::End { run }, &self.names, &self.damaged);
        if !matches!(closing.outcome, Outcome::Refused(_)) {
            self.publish(&closing);
        }
        Outcome::Refused(Why::NotConfirmed)
    }

    /// Writes down that other hands made, lost or damaged a name of a reservation.
    fn meddle(&mut self, name: &Name) {
        self.meddled.extend(name.of());
    }

    /// Writes down whether the name of a settlement the ledger made is there: what
    /// the request was charged is in that name and nowhere else.
    fn record(&mut self, name: &Name, recorded: bool) {
        let Name::Settled {
            run, place, micros, ..
        } = name
        else {
            return;
        };
        let asked = self.truth.get_mut(run).map(|truth| truth.asked.iter_mut());
        let said = |asked: &&mut Asked| {
            asked.place == *place && asked.recorded != recorded && asked.settled == Some(*micros)
        };
        if let Some(asked) = asked.and_then(|mut asked| asked.find(said)) {
            asked.recorded = recorded;
        }
    }

    /// Whether the event is a publication under `spend/`.
    fn publishes(&self, event: &Event) -> bool {
        match event {
            Event::Reset { .. }
            | Event::Admit { .. }
            | Event::Publish { .. }
            | Event::Dispatch { .. }
            | Event::Settle { .. }
            | Event::Recover
            | Event::Adjust { .. } => true,
            Event::End { run } => self.live.get(run).is_some_and(|live| live.parent.is_none()),
            _ => false,
        }
    }

    /// What memory says against an event at the moment it is published, whenever
    /// it was judged and whatever the names say: a run that is no longer executing
    /// sends nothing; a run spends only what it has; a dispatch takes no more than
    /// is left of its tree's reservation as the process holds it, which is the
    /// amount less everything charged and everything outstanding under it, a
    /// charge over its bound of any run of the tree among them; and a run does
    /// not end while a request it sent is outstanding. Memory is as it is now, also
    /// where the reading of the names is old.
    fn memory_refuses(&self, event: &Event) -> Option<Why> {
        match event {
            Event::Dispatch { by, micros } => {
                let Some(live) = self.executing(*by) else {
                    return Some(Why::NotExecuting);
                };
                let spendable = self.spendable(*by).unwrap_or(0);
                if *micros > spendable {
                    return Some(Why::OverRun { left: spendable });
                }
                let left = self.trees.get(&live.root).map_or(0, Tree::left);
                (*micros > left).then_some(Why::OverTree { left })
            }
            Event::Admit { run, .. } | Event::Publish { run, .. } => {
                self.ids.contains_key(run).then_some(Why::Taken)
            }
            Event::End { run } if self.executing(*run).is_none() => Some(Why::NotExecuting),
            Event::End { run } => self.waits(*run).then_some(Why::RequestsOutstanding),
            _ => None,
        }
    }

    fn memory_allows(&self, event: &Event) -> bool {
        self.memory_refuses(event).is_none()
    }

    fn judged_now(&mut self, event: &Event) -> Outcome {
        let plan = self.plan(event, &self.names, &self.damaged);
        if matches!(plan.outcome, Outcome::Refused(_)) {
            return plan.outcome;
        }
        // The names allow it: memory is asked too.
        if let Some(why) = self.memory_refuses(event) {
            return Outcome::Refused(why);
        }
        // Judged on the books as they are, a publication makes no name they hold.
        if self.publish(&plan) {
            self.published(event, plan.outcome)
        } else {
            Outcome::Refused(Why::Taken)
        }
    }

    /// Whether other hands made, lost or damaged a name of this reservation that
    /// matters now: one they made that is there, one of the ledger's that is gone,
    /// or one that is damaged.
    fn touched(&self, run: Run) -> bool {
        let own = |name: &Name| name.of() == Some(run);
        self.planted.keys().any(own) || self.removed.keys().any(own)
    }

    /// Whether the names of this reservation are a set the ledger could have
    /// written, worked out here by counting names of each form, apart from the
    /// fold's recogniser: the checker believes a set other hands touched only when
    /// both say so.
    fn believable(&self, run: Run) -> bool {
        let (own, damaged) = self.own(run);
        let count = |wanted: &dyn Fn(&Name) -> bool| own.iter().filter(|name| wanted(name)).count();
        let at = |place: u64, wanted: &dyn Fn(&Name) -> bool| {
            count(&|name| name.place() == Some(place) && wanted(name))
        };
        let dispatch = |name: &Name| matches!(name, Name::Dispatched { .. });
        let settlement = |name: &Name| matches!(name, Name::Settled { .. });
        let guard = |name: &Name| matches!(name, Name::Settling { .. });
        let place = |name: &Name| matches!(name, Name::Place { .. });
        let closing = |name: &Name| matches!(name, Name::Closed { .. });
        let correction = |name: &Name| matches!(name, Name::Adjusted { .. });
        let mut overruns = 0;
        for name in &own {
            let sound = match name {
                Name::Dispatched { place: here, .. } => {
                    at(*here, &dispatch) == 1 && at(*here, &place) == 1
                }
                Name::Settled {
                    place: here,
                    stage,
                    micros,
                    ..
                } => {
                    let bound = own.iter().find_map(|other| match other {
                        Name::Dispatched { place, micros, .. } if place == here => Some(*micros),
                        _ => None,
                    });
                    overruns += usize::from(bound.is_some_and(|bound| *micros > bound));
                    at(*here, &settlement) == 1
                        && at(*here, &guard) == 1
                        && at(*here, &dispatch) == 1
                        && bound.is_some_and(|bound| match stage {
                            Stage::NotSent => *micros == 0,
                            Stage::Unresolved => *micros == bound,
                            Stage::Answered => true,
                        })
                }
                Name::Settling { place: here, .. } => at(*here, &settlement) == 1,
                Name::Closed { released, .. } => {
                    count(&closing) == 1 && (!released || count(&dispatch) == count(&settlement))
                }
                Name::Adjusted { place: here, .. } => {
                    let same = |other: &Name| matches!(other, Name::Adjusted { place, .. } if place == here);
                    // A correction is of a reservation closed with nothing released.
                    let held = |name: &Name| {
                        matches!(
                            name,
                            Name::Closed {
                                released: false,
                                ..
                            }
                        )
                    };
                    count(&closing) == 1
                        && count(&held) == 1
                        && at(*here, &place) == 1
                        && at(*here, &dispatch) == 0
                        && count(&same) == 1
                }
                Name::Stray { .. } => false,
                _ => true,
            };
            if !sound {
                return false;
            }
        }
        let places: Vec<u64> = own
            .iter()
            .filter(|name| place(name))
            .filter_map(Name::place)
            .collect();
        let counted = damaged.is_empty()
            && count(&|name| matches!(name, Name::Reservation(_))) == 1
            && count(&|name| matches!(name, Name::Held { .. })) == 1
            && places.iter().copied().eq(1..=places.len() as u64)
            && places.len() == count(&dispatch) + overruns + count(&closing) + count(&correction);
        counted && Books::fitted(&own)
    }

    /// Whether each dispatch among these names of one reservation, which has one
    /// hold and whose places are 1 to their count, fit what the reservation had
    /// left when it was published: the second reading's own replay of what the
    /// ledger checked, written apart from the recogniser's. Before a dispatch the
    /// amount is less by every request sent before it, each at its bound, or at its
    /// charge where it was settled under it; and by the excess of one charge over
    /// its bound for each place below it that holds no dispatch, the smallest that
    /// had been sent by then, since the names do not say which. A place below a
    /// dispatch that holds none and has no such charge to account for it is a
    /// closing's, and nothing is dispatched after one. And a correction follows the
    /// closing: the first place a correction names is above the one after the last
    /// dispatch.
    fn fitted(own: &BTreeSet<Name>) -> bool {
        let hold = own.iter().find_map(|name| match name {
            Name::Held { micros, .. } => Some(i128::from(*micros)),
            _ => None,
        });
        let mut requests: Vec<(u64, u64, Option<u64>)> = Vec::new();
        for name in own {
            if let Name::Dispatched { place, micros, .. } = name {
                let charge = own.iter().find_map(|other| match other {
                    Name::Settled {
                        place: at, micros, ..
                    } if at == place => Some(*micros),
                    _ => None,
                });
                requests.push((*place, *micros, charge));
            }
        }
        requests.sort_unstable();
        let mut left = hold.unwrap_or(0);
        let mut owed: Vec<u64> = Vec::new();
        let mut next = 1;
        for (place, bound, charge) in requests {
            // The places between the last dispatch and this one hold none.
            while next < place {
                owed.sort_unstable_by(|one, other| other.cmp(one));
                match owed.pop() {
                    Some(excess) => left = left.saturating_sub(i128::from(excess)),
                    None => return false,
                }
                next += 1;
            }
            next = place.saturating_add(1);
            if i128::from(bound) > left.max(0) {
                return false;
            }
            let charge = charge.unwrap_or(bound);
            left = left.saturating_sub(i128::from(charge.min(bound)));
            if charge > bound {
                owed.push(charge - bound);
            }
        }
        // The closing took a place after the last dispatch and before the first
        // correction.
        let first = own.iter().filter_map(|name| match name {
            Name::Adjusted { place, .. } => Some(*place),
            _ => None,
        });
        first.min().is_none_or(|first| first > next)
    }

    /// Whether the hold the ledger made for this reservation is lost: the amount
    /// was in that name alone, whatever holds other hands leave in its place (ADR
    /// 0016, limit (c)).
    fn amount_lost(&self, root: Run, truth: &Truth) -> bool {
        let own = Name::Held {
            epoch: truth.epoch,
            run: root,
            micros: truth.amount,
        };
        self.removed.contains_key(&own)
    }

    /// Whether no name states an amount for this request: the ledger's own hold of
    /// its reservation is lost, and its dispatch is lost too (ADR 0016, limit (c)).
    fn unnamed(&self, root: Run, truth: &Truth, asked: &Asked) -> bool {
        let dispatch = |name: &Name| matches!(name, Name::Dispatched { run, place, .. } if *run == root && *place == asked.place);
        self.amount_lost(root, truth) && self.removed.keys().any(dispatch)
    }

    /// Whether a correction that other hands made is whole: the name, the place and
    /// the turn it names, and a seal of exactly the other names as they are.
    fn forged_correction(&self, run: Run, epoch: u64) -> bool {
        self.planted.keys().any(|name| match name {
            Name::Adjusted {
                run: of,
                place,
                turn,
                seal,
                ..
            } if *of == run => {
                let guard = Name::Place { run, place: *place };
                let turn = Name::Turn { epoch, turn: *turn };
                let mut others = self.own(run).0;
                others.remove(name);
                others.remove(&guard);
                let over = self::seal(
                    others
                        .iter()
                        .map(|other| (other, self.damaged.contains(other))),
                );
                self.names.contains(&guard) && self.names.contains(&turn) && *seal == over
            }
            _ => false,
        })
    }

    /// What is really left of the newest epoch's allowance, from the truth: what
    /// the owner really set, less what was really charged and is really outstanding
    /// under each reservation really made in it. A reservation whose names other
    /// hands touched and that is believed counts as the fold has it. None where a
    /// whole epoch is other hands' (limit (a)) or the newest is lost (limit (d)).
    fn really_left(&self, folded: &Folded) -> Option<u128> {
        let newest = self.allowed.keys().next_back().copied().unwrap_or(0);
        // An epoch after the newest that other hands made whole, its reset and its
        // guard, is believed; so is the loss of the newest. An epoch made without
        // its guard allows nothing, and is held to that.
        let epoch = folded.epoch();
        let made = |name: &Name| self.planted.contains_key(name);
        let reset = |name: &Name| matches!(name, Name::Reset { epoch: of, .. } if *of == epoch);
        let forged = made(&Name::Epoch(epoch)) && self.planted.keys().any(reset);
        if epoch < newest || (epoch > newest && forged) {
            return None;
        }
        if epoch > newest {
            return Some(0);
        }
        let mut counted = 0u128;
        for (root, truth) in self.truth.iter().filter(|(_, truth)| truth.epoch == newest) {
            // Where its names are believed, so is the epoch its hold names: it
            // counts here as the fold has it, which is not at all when a hold other
            // hands made in the place of the ledger's names another epoch.
            let here = |one: &&Standing| one.counts_in(newest);
            counted = counted.saturating_add(match self.held_to(*root, truth, folded) {
                Ok(Some(floor)) => floor,
                _ => folded
                    .reservations
                    .get(root)
                    .filter(here)
                    .map_or(0, |one| one.counts),
            });
        }
        // The allowance the owner really set, while the name of that reset is there.
        // Where it is lost, the allowance is what the names say: nothing, or what a
        // reset other hands made says with the guard, which is believed as a whole
        // chain is (limit (a)). With the owner's reset there, a second reset says
        // nothing more than the smaller of the two.
        let allowed = self.allowed.get(&newest).copied().unwrap_or(0);
        let owners = Name::Reset {
            epoch: newest,
            micros: allowed,
        };
        let allowed = match newest == 0 || self.names.contains(&owners) {
            true => allowed,
            false => folded.balance_of(newest).allowed,
        };
        Some(u128::from(allowed).saturating_sub(counted))
    }

    /// What really counts against the allowance of one epoch, and that allowance,
    /// from the truth: what an admission that stands is held to. Each reservation
    /// really made in the epoch counts in full, at its amount or at what its
    /// requests really took where that is more, until the ledger closed it and
    /// released the rest or its owner corrected it; one whose names other hands
    /// touched and that is believed counts as the fold has it (limits (a) and
    /// (d)); and a request whose settlement's name is lost counts at its bound
    /// (limit (b)). The allowance is the one the owner really set while that
    /// reset's name is there, and the names' where it is lost. None where the
    /// whole epoch is other hands', reset and guard (limit (a)).
    fn really_in(&self, epoch: u64, folded: &Folded) -> Option<(u128, u128)> {
        let allowed = match self.allowed.get(&epoch) {
            Some(allowed) => {
                let owners = Name::Reset {
                    epoch,
                    micros: *allowed,
                };
                match self.names.contains(&owners) {
                    true => *allowed,
                    false => folded.balance_of(epoch).allowed,
                }
            }
            None => {
                let reset =
                    |name: &Name| matches!(name, Name::Reset { epoch: of, .. } if *of == epoch);
                let made = self.planted.contains_key(&Name::Epoch(epoch));
                if epoch > 0 && made && self.planted.keys().any(reset) {
                    return None;
                }
                0
            }
        };
        let mut counted = 0u128;
        for (root, truth) in self.truth.iter().filter(|(_, truth)| truth.epoch == epoch) {
            let found = folded.reservations.get(root);
            let here = found.filter(|one| one.counts_in(epoch));
            let believed = here.map_or(0, |one| one.counts);
            let corrected = truth.corrected.as_ref() == Some(&self.own(*root));
            let whole = !corrected && !self.touched(*root) && !truth.released;
            counted = counted.saturating_add(match self.held_to(*root, truth, folded) {
                Ok(Some(floor)) if whole => floor.max(u128::from(truth.amount)),
                Ok(Some(floor)) => floor.min(believed),
                _ => believed,
            });
        }
        Some((counted, u128::from(allowed)))
    }

    /// What one reservation's count is held to, from the truth: the floor of every
    /// request really sent under it. None where the checker believes the names
    /// instead, which is only where a stated limit says so (ADR 0016, "Limits").
    fn held_to(&self, root: Run, truth: &Truth, folded: &Folded) -> Result<Option<u128>, String> {
        let one = folded.reservations.get(&root);
        let (own, damaged) = self.own(root);
        // The owner's correction stands while the names are those it left: it
        // writes off what is outstanding, and what a place says that other hands
        // touched. That is its meaning, and what it can cost is limit (g).
        let corrected = truth.corrected.as_ref() == Some(&(own.clone(), damaged));
        let hurt = |place: u64| {
            let here = |name: &Name| name.of() == Some(root) && name.place() == Some(place);
            let touched = self.planted.keys().chain(self.removed.keys()).any(here);
            touched || self.damaged.iter().any(here)
        };
        // With the ledger's own hold lost, a request whose dispatch is lost too has
        // no name that states an amount.
        let lost = |asked: &Asked| self.unnamed(root, truth, asked);
        let whole = |asked: &Asked| asked.settled.is_some() && asked.recorded && !hurt(asked.place);
        let counted = |asked: &&Asked| {
            if corrected {
                whole(asked)
            } else {
                !lost(asked)
            }
        };
        let floor = truth.asked.iter().filter(counted).map(Asked::floor);
        let floor = floor.fold(0u128, u128::saturating_add);
        if corrected {
            return Ok(Some(floor));
        }
        if !self.touched(root) {
            return Ok(Some(floor));
        }
        match one {
            // Every name of it is lost: a loss that leaves a reachable set, which is
            // none (limit (d)).
            None if own.is_empty() => Ok(None),
            None => Err(format!("{root}: no reservation is folded for it")),
            Some(one) if one.state == State::Frozen => Ok(Some(floor)),
            // A correction other hands made whole is a correction (limit (a)).
            Some(one) if one.state == State::Adjusted => {
                match self
                    .forged_correction(root, one.epochs.first().copied().unwrap_or(folded.epoch()))
                {
                    true => Ok(None),
                    false => Err(format!(
                        "{root}: corrected by a correction that is not whole"
                    )),
                }
            }
            // A set both readings take is believed: one other hands made a name of
            // (limit (a)), and one of which names are only lost (limit (d)).
            Some(one) => match self.believable(root) {
                true => Ok(None),
                false => Err(format!(
                    "{root}: {:?}, and its names are not what the ledger writes",
                    one.state
                )),
            },
        }
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
    /// of two states is held where an event is done ([`Books::checked`]).
    ///
    /// What it leaves out is what ADR 0016 lists against its limits (a) to (e) and
    /// (g), and nothing else; of an admission that stands, nothing. Under limit (c) it also holds the books to saying what they
    /// cannot say: a reservation whose own hold is lost, frozen with no hold or
    /// with several, counts "unknown, at least".
    pub fn check(&mut self) -> Result<(), String> {
        let folded = self.fold();
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
            let took: u128 = truth.asked.iter().map(Asked::took).sum();
            let really = i128::from(truth.amount) - i128::try_from(took).unwrap_or(i128::MAX);
            if self.all_open().any(|open| of_tree(&open)) && held > really {
                return Err(format!(
                    "{root}: its runs hold {held} between them and {really} is left"
                ));
            }
            // 1. Never under.
            let Some(floor) = self.held_to(*root, truth, &folded)? else {
                continue;
            };
            let Some(one) = folded.reservations.get(root) else {
                return Err(format!("{root}: no reservation is folded for it"));
            };
            if one.counts < floor {
                return Err(format!(
                    "{root}: counted {} and really {floor}: under",
                    one.counts
                ));
            }
            // Limit (c), said by the books: where the ledger's own hold is lost and
            // the reservation is frozen with no hold or several, its count is
            // "unknown, at least", and so is the store's in every epoch. Where the
            // books say so and the ledger's own hold is there among several, the
            // amount is at least its own, and nothing is lost: the floor above was
            // held, and so is the amount.
            let lost = self.amount_lost(*root, truth);
            if lost && one.state == State::Frozen && one.epoch.is_none() && !one.unknown {
                return Err(format!(
                    "{root}: its hold is lost, and the books say what it counts for"
                ));
            }
            if one.unknown && !folded.balance().unknown {
                return Err(format!(
                    "{root}: its amount is unknown, and the store says what is counted"
                ));
            }
            if one.unknown && !lost && one.counts < u128::from(truth.amount) {
                return Err(format!(
                    "{root}: its hold is there, and it counts {} under its amount",
                    one.counts
                ));
            }
            // It counts in the epoch it was really reserved in. The epoch is in the
            // hold alone: one hold that other hands made in the place of the
            // ledger's is believed for it (limit (a)). With the ledger's hold there,
            // alone or not, and with no hold or several, there is no such excuse.
            let own = Name::Held {
                epoch: truth.epoch,
                run: *root,
                micros: truth.amount,
            };
            let replaced = one.epoch.is_some() && !self.names.contains(&own);
            if !one.counts_in(truth.epoch) && !replaced {
                return Err(format!(
                    "{root}: reserved in epoch {} and not counted there",
                    truth.epoch
                ));
            }
            // 3. A closing that releases has a settlement for every dispatch.
            let unsettled = |asked: &&Asked| !(asked.settled.is_some() && asked.recorded);
            let unsettled = truth.asked.iter().filter(unsettled).count();
            if one.state == State::Released && unsettled > 0 {
                return Err(format!("{root}: released with {unsettled} not settled"));
            }
            // Every run is counted for no less than it really sent, or is said to
            // be unknown with a most that is no less. A correction that stands
            // writes off what it writes off of a run too.
            if one.state == State::Adjusted {
                continue;
            }
            let mut sent: BTreeMap<Run, u128> = BTreeMap::new();
            for asked in truth
                .asked
                .iter()
                .filter(|asked| !self.unnamed(*root, truth, asked))
            {
                let sent: &mut u128 = sent.entry(asked.by).or_default();
                *sent = sent.saturating_add(asked.floor());
            }
            for (run, really) in sent {
                let found = match folded.sent_by(run, *root) {
                    Sent::Known(spend) => spend.known(),
                    Sent::Unknown { at_most } => at_most,
                    Sent::Unbounded => continue,
                };
                if found < really {
                    return Err(format!(
                        "{run}: it really sent {really}, and {found} is counted to it"
                    ));
                }
            }
        }
        for (run, one) in &folded.reservations {
            // 3. One closing, and what was closed with nothing released is not
            // released later: no one event turns the one into the other, the
            // ledger's or other hands'.
            let before = self.was.insert(*run, one.state);
            let later = before == Some(State::Interrupted) && one.state == State::Released;
            if later {
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
        // and never more than is really left of the allowance the owner really set.
        let balance = folded.balance();
        let over = u128::from(balance.left).saturating_add(balance.counted);
        let over = over > u128::from(balance.allowed);
        if over && balance.left > 0 {
            return Err(format!("{balance:?}: more is left than the allowance has"));
        }
        if let Some(really) = self.really_left(&folded)
            && u128::from(balance.left) > really
        {
            return Err(format!(
                "{} can be reserved, and really {really} is left of the allowance",
                balance.left
            ));
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
        outcomes.push(books.checked(event)?);
        let broke = |what: String| format!("event {at}, {event:?}: {what}");
        watch(at, &books, &outcomes).map_err(broke)?;
    }
    Ok((books, outcomes))
}

impl Books {
    /// Does one event as [`Books::apply`] does, and holds the books to every
    /// invariant: what is of the state after it ([`Books::check`]), and what is of
    /// the two states, before it and after. What it returns is what became of the
    /// event, or what broke.
    pub fn checked(&mut self, event: &Event) -> Result<Outcome, String> {
        let books = self;
        let at = books.past.len();
        let before = books.fold();
        let allowances: BTreeMap<Run, Live> = books
            .all_open()
            .map(|(run, live)| (*run, live.clone()))
            .collect();
        let (_, judged, plain) = event.plain();
        // The reservation a dispatch draws on: what is really left of it, and
        // whether a closing names it.
        let drawn = match plain {
            Event::Dispatch { by, .. } => books.open(*by).map(|live| live.root),
            _ => None,
        };
        let room = drawn.and_then(|root| books.truth.get(&root)).map(|truth| {
            let took: u128 = truth.asked.iter().map(Asked::took).sum();
            u128::from(truth.amount).saturating_sub(took)
        });
        let is_closing =
            |name: &&Name| matches!(name, Name::Closed { run, .. } if Some(*run) == drawn);
        let closing = books.names.iter().find(is_closing).cloned();
        // Whether the reservation a dispatch draws on is of an epoch that needs its
        // owner: on the names as they are, and on the names it was judged on, where
        // that was earlier.
        let stopped = |folded: &Folded| {
            let one = drawn.and_then(|root| folded.reservations.get(&root));
            one.is_some_and(|one| one.epoch.is_none_or(|epoch| folded.needs_owner(epoch)))
        };
        let stopped_now = stopped(&before);
        let stopped_then = judged.map(|judged| {
            let seen = judged.min(at).checked_sub(1);
            let seen = seen.and_then(|seen| books.past.get(seen));
            seen.is_some_and(|seen| stopped(&fold(&seen.0, &seen.1)))
        });
        // Whether this is the first thing other hands do to the reservation a name
        // that is lost belongs to.
        let first = match plain {
            Event::Remove(name) => name.of().is_some_and(|run| !books.meddled.contains(&run)),
            _ => false,
        };
        // Whether the ledger had made the reservation an admission asks for.
        let had = match plain {
            Event::Admit { run, .. } | Event::Publish { run, .. } => books.truth.contains_key(run),
            _ => true,
        };
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
        // What other hands did between a judgement and its publication is not the
        // ledger's to have seen: a publication is held to the books only by the
        // names it makes (limit (e)).
        let between = judged.is_some_and(|judged| {
            let since = |when: &usize| *when >= judged && *when < at;
            books.hands.iter().any(since)
        });
        let unseen = landed && between;
        // 4. Damage freezes the one reservation it belongs to, and changes no other:
        // nor does a name other hands made, or a name that is lost.
        if let Event::Damage(name) | Event::Plant(name) | Event::Remove(name) = plain
            && let Some(own) = name.of()
        {
            for (run, one) in &after.reservations {
                if *run != own && before.reservations.get(run) != Some(one) {
                    return Err(broke(format!("it changed the reservation of {run}")));
                }
            }
            let frozen = after.reservations.get(&own);
            let held = |one: &Standing| one.state == State::Adjusted || one.counts >= one.most();
            let damage = matches!(plain, Event::Damage(_));
            if damage && matches!(outcome, Outcome::Done) && !frozen.is_none_or(held) {
                return Err(broke(
                    "its reservation counts for less than its amount".into(),
                ));
            }
        }
        // Losing one name of the ledger's leaves no set the ledger could have
        // written: a reservation that lost one, and that no other hands touched
        // before, is frozen and counts for no less than it did. Two names are the
        // only place of their amount: a settlement over its bound leaves no less
        // than the amount; and the hold leaves what is known. The rule is of books
        // that were whole: a reservation other hands touched before counts each
        // name it has, a correction made over their names among them, and can count
        // less with one fewer.
        if let (Event::Remove(name), Outcome::Done) = (plain, &outcome)
            && books.removed.contains_key(name)
        {
            let was = name.of().and_then(|run| before.reservations.get(&run));
            let is = name.of().and_then(|run| after.reservations.get(&run));
            if let (Some(was), Some(is), true) = (was, is, first) {
                let over_bound = match name {
                    Name::Settled {
                        run, place, micros, ..
                    } => {
                        let truth = books.truth.get(run);
                        let mut asked = truth.iter().flat_map(|truth| &truth.asked);
                        asked.any(|asked| asked.place == *place && *micros > asked.bound)
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
            // Losing a turn, or one of the allowance's own names, allows no more.
            let store = matches!(
                name,
                Name::Reset { .. } | Name::Epoch(_) | Name::Turn { .. }
            );
            let (was, is) = (before.balance(), after.balance());
            let alone = books.hands.len() == 1;
            if store && alone && was.epoch == is.epoch && is.left > was.left {
                return Err(broke("more can be reserved than before".into()));
            }
        }
        if let Event::Dispatch { micros, .. } = plain
            && done
        {
            // No dispatch is published once a closing names its reservation,
            // whenever it was judged. Limit (e): one judged before other hands
            // acted is published all the same, where it was its reservation they
            // touched.
            let excused = unseen && drawn.is_some_and(|root| books.touched(root));
            if let Some(closing) = closing.filter(|_| !excused) {
                return Err(broke(format!("dispatched after {closing}")));
            }
            // And none that what was really reserved does not really cover,
            // whatever the names say and whoever made them: a run's allowance, in
            // the memory of its process, holds it.
            let room = room.unwrap_or(0);
            if u128::from(*micros) > room {
                return Err(broke(format!("dispatched, and really {room} was left")));
            }
        }
        // No dispatch is published where the store needs its owner, unless it was
        // judged before the store came to need it (limit (e)): one judged then, or
        // judged again then, is never let by.
        if matches!(plain, Event::Dispatch { .. }) && done && stopped_now {
            let before_it = landed && stopped_then == Some(false);
            if !before_it {
                return Err(broke(
                    "dispatched where the store needs its owner, and not judged before".into(),
                ));
            }
        }
        // Nothing runs under a reservation whose admission was not confirmed: no
        // request is sent, and no call is made from inside its run.
        let under = match plain {
            Event::Dispatch { .. } => drawn,
            Event::Child { child, .. } => books.root_of(*child),
            _ => None,
        };
        let confirmed = |root: &Run| books.truth.get(root).is_some_and(|truth| truth.confirmed);
        if let Some(root) = under.filter(|root| done && !confirmed(root)) {
            return Err(broke(format!(
                "done under {root}, whose admission was not confirmed"
            )));
        }
        // 6. No admission stands that what is really left of the allowance does not
        // cover, and none is published as it was judged once the ledger has taken a
        // turn in its epoch since.
        // Limit (e): the store refuses a publication only for a name it makes, so
        // an admission judged before other hands moved a turn may be published.
        // That is all the limit gives it. It stands only when the process that
        // made it has read the names again and found them sound and within the
        // allowance, and then it is held here with no excuse: to the truth, which
        // has limits (a) to (d) in it and no other, and to that reading.
        let last = match &outcome {
            Outcome::AskedAgain(inner) => &**inner,
            other => other,
        };
        if let Event::Admit { run, .. } | Event::Publish { run, .. } = plain
            && let Some(truth) = books.truth.get(run).filter(|_| !had)
        {
            let since = |(when, of): &(usize, u64)| {
                judged.is_some_and(|judged| *when >= judged && *when < at) && *of == truth.epoch
            };
            let as_judged = !matches!(outcome, Outcome::AskedAgain(_));
            if as_judged && !between && books.turned.iter().any(since) {
                return Err(broke("admitted on a stale balance".into()));
            }
        }
        let asked = match plain {
            Event::Admit { run, .. } | Event::Publish { run, .. } => (!had).then_some(run),
            Event::Confirm { run } => Some(run),
            _ => None,
        };
        let asked = asked.and_then(|run| Some((*run, books.truth.get(run)?)));
        let waits = matches!(plain, Event::Publish { .. });
        if let Some((run, truth)) = asked.filter(|_| done && !waits) {
            // It stands. The truth had room for it, counted in full.
            let really = books.really_in(truth.epoch, &after);
            if let Some((counted, allowed)) = really.filter(|_| truth.amount > 0)
                && counted > allowed
            {
                let others = counted.saturating_sub(u128::from(truth.amount));
                let room = allowed.saturating_sub(others);
                return Err(broke(format!("admitted, and really {room} was left")));
            }
            // And the reading made after its publication was sound and within the
            // allowance: the names as they are, with its own among them.
            // Written here apart from the rule the ledger confirms by.
            // Its own reservation: the record and one hold, neither damaged, and
            // no other name that names the run, read here from the names
            // themselves.
            let (names, damaged) = books.own(run);
            let record = |name: &Name| matches!(name, Name::Reservation(_));
            let hold = |name: &Name| matches!(name, Name::Held { .. });
            let bare = damaged.is_empty()
                && names.iter().filter(|name| record(name)).count() == 1
                && names.iter().filter(|name| hold(name)).count() == 1
                && names.iter().all(|name| record(name) || hold(name));
            let own = after.reservations.get(&run).filter(|_| bare);
            let open = |one: &&Standing| one.reachable && !one.closed && one.corrections == 0;
            let fell = own.filter(open).and_then(|one| one.epoch);
            let fell = fell.map(|epoch| after.balance_of(epoch));
            let sound = |fell: &Balance| !fell.needs_owner && !after.balance().needs_owner;
            let within = |fell: &Balance| truth.amount == 0 || fell.counted <= fell.allowed.into();
            if !fell.is_some_and(|fell| sound(&fell) && within(&fell)) {
                return Err(broke(
                    "confirmed, and the books do not read sound and within the allowance".into(),
                ));
            }
            if !truth.confirmed || books.executing_now(run).is_none() {
                return Err(broke(format!("{run} stands and does not execute")));
            }
        }
        let backed_out = *last == Outcome::Refused(Why::NotConfirmed);
        if let Some((run, truth)) = asked.filter(|_| backed_out || (waits && done)) {
            // It does not stand, or not yet: nothing runs under it, and nothing
            // was dispatched under it.
            if truth.confirmed || books.open(run).is_some() || !truth.asked.is_empty() {
                return Err(broke(format!(
                    "{run} was not confirmed, and something ran under it"
                )));
            }
            // Backed out, its reservation is closed with everything released, or
            // still counts in full; unless other hands touched its names, which
            // are then held as any are, under limits (a) to (d).
            let full = |one: &Standing| {
                one.state == State::Released || one.counts >= u128::from(truth.amount)
            };
            let kept = after.reservations.get(&run).is_some_and(full);
            if backed_out && !kept && !books.touched(run) {
                return Err(broke(format!(
                    "{run} was not confirmed, and is neither released nor counted in full"
                )));
            }
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
        Ok(outcome)
    }
}

impl Books {
    /// What is really so of the reservation the ledger made for this run, whatever
    /// the names say: the epoch it was reserved in, its amount, and what its
    /// requests really took of it, which is the charge of each that was settled,
    /// its name lost or not, and the bound of each that was not.
    pub fn really(&self, run: Run) -> Option<(u64, u64, u128)> {
        let truth = self.truth.get(&run)?;
        let took = truth.asked.iter().map(Asked::took).sum();
        Some((truth.epoch, truth.amount, took))
    }

    /// Whether the names of this reservation are a set the ledger could have
    /// written, as the fold's recogniser reads them and as the checker's second
    /// reading does: for a test that holds the two to one answer.
    pub fn read_twice(&self, run: Run) -> Option<(bool, bool)> {
        let first = self.fold().reservations.get(&run)?.reachable;
        Some((first, self.believable(run)))
    }

    /// The reservations of which the two readings give two answers: none, while
    /// the two are one rule.
    pub fn read_apart(&self) -> Vec<Run> {
        let folded = self.fold();
        let apart = |(run, one): (&Run, &Standing)| {
            (one.reachable != self.believable(*run)).then_some(*run)
        };
        folded.reservations.iter().filter_map(apart).collect()
    }

    /// The run at the root of this run's tree, when the run was given an id.
    pub fn root_of(&self, run: Run) -> Option<Run> {
        self.ids.get(&run).copied()
    }

    /// The names as they were after the first `events` events.
    pub fn names_after(&self, events: usize) -> Option<&BTreeSet<Name>> {
        let at = events.checked_sub(1)?;
        self.past.get(at).map(|past| &past.0)
    }

    /// The runs that execute in the first process, or in the second, in order.
    pub fn executing_in(&self, second: bool) -> Vec<Run> {
        let live = if second { &self.other.live } else { &self.live };
        let executing = |(_, live): &(&Run, &Live)| live.state == Liveness::Executing;
        live.iter().filter(executing).map(|(run, _)| *run).collect()
    }

    /// The runs whose admissions the first process, or the second, published and
    /// has not confirmed, in order.
    pub fn waiting_in(&self, second: bool) -> Vec<Run> {
        let waiting = if second {
            &self.other.waiting
        } else {
            &self.waiting
        };
        waiting.keys().copied().collect()
    }

    /// What is left of the reservation of the tree this run is the root of, as the
    /// process that runs it holds it in memory.
    pub fn tree_left(&self, run: Run) -> Option<u64> {
        let tree = self.trees.get(&run).or_else(|| self.other.trees.get(&run));
        tree.map(Tree::left)
    }

    /// Whether a dispatch by this run would be refused because the epoch of its
    /// tree's reservation needs its owner: judged on the names as they were after
    /// the first `judged` events, or as they are.
    pub fn dispatch_needs_owner(&self, by: Run, judged: Option<usize>) -> bool {
        let Some(root) = self.open(by).map(|live| live.root) else {
            return false;
        };
        let folded = match judged {
            None => self.fold(),
            Some(judged) => {
                let seen = judged.min(self.past.len()).checked_sub(1);
                match seen.and_then(|seen| self.past.get(seen)) {
                    Some(seen) => fold(&seen.0, &seen.1),
                    None => Folded::default(),
                }
            }
        };
        let needs = |one: &Standing| one.epoch.is_none_or(|epoch| folded.needs_owner(epoch));
        folded.reservations.get(&root).is_some_and(needs)
    }

    /// Whether the process that published this run's admission confirmed it.
    pub fn confirmed(&self, run: Run) -> bool {
        self.truth.get(&run).is_some_and(|truth| truth.confirmed)
    }

    /// What really counts against the allowance of the epoch this run's
    /// reservation was made in, and that allowance, as the checker works them out
    /// from the truth: None where no reservation was made for the run, or the
    /// epoch is other hands' (ADR 0016, limit (a)).
    pub fn really_counted_with(&self, run: Run) -> Option<(u128, u128)> {
        let truth = self.truth.get(&run)?;
        self.really_in(truth.epoch, &self.fold())
    }

    /// A run that executes, in either process.
    fn executing_now(&self, run: Run) -> Option<&Live> {
        self.open(run)
            .filter(|live| live.state == Liveness::Executing)
    }

    /// The runs that execute in the first process, in order.
    pub fn executing_runs(&self) -> Vec<Run> {
        self.executing_in(false)
    }

    /// The requests the first process, or the second, sent and has not settled: the
    /// reservation and the place of each.
    pub fn unsettled_in(&self, second: bool) -> Vec<(Run, u64)> {
        let sent = if second { &self.other.sent } else { &self.sent };
        sent.iter()
            .map(|request| (request.root, request.place))
            .collect()
    }
}

/// What a history must give: of the store's newest epoch, of one reservation, of one
/// run's allowance, or of one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Must {
    Epoch(u64),
    Allowed(u64),
    /// What is counted in the newest epoch, where the books can say it.
    Counted(u64),
    Left(u64),
    Debt(u64),
    /// What the reservation of this run counts for, where the books can say it.
    Counts(Run, u64),
    /// What the reservation of this run counts for is unknown, and at least this.
    CountsAtLeast(Run, u64),
    /// The reservation of this run counts for the largest number there is.
    Saturated(Run),
    /// What is counted in the newest epoch is unknown, and at least this.
    CountedAtLeast(u64),
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
    /// What this run sent is unknown, and at most this.
    SentAtMost(Run, u64),
    /// What this run sent is unknown, and no most can be said.
    SentUnbounded(Run),
    /// The newest epoch's turns are not those its publications took.
    NeedsOwner,
    /// What became of the event at this place in the history, from 0.
    Outcome(usize, Outcome),
    /// The names are the ones there were after the event at this place.
    SameNamesAsAfter(usize),
    /// The reservation of this run is closed.
    Closed(Run),
    /// No run executes, in either process.
    NothingExecutes,
    /// What is left of this run's tree, as its process holds it in memory.
    TreeLeft(Run, u64),
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
            Must::Counted(micros) => match balance.unknown {
                false => same("counted", balance.counted, *micros),
                true => Err("the books cannot say how much is counted".to_owned()),
            },
            Must::Left(micros) => same("left", balance.left.into(), *micros),
            Must::Debt(micros) => same("the debt", balance.debt, *micros),
            Must::Counts(run, micros) => match of(run)?.counted() {
                Counted::Exactly(found) => same(&format!("{run} counts"), found, *micros),
                found => Err(format!("{run} counts {found:?}, and it must be known")),
            },
            Must::CountsAtLeast(run, micros) => match of(run)?.counted() {
                Counted::AtLeast(found) => same(&format!("{run} counts at least"), found, *micros),
                found => Err(format!("{run} counts {found:?}, and it must be unknown")),
            },
            Must::Saturated(run) => match of(run)?.counts {
                u128::MAX => Ok(()),
                found => Err(format!("{run} counts {found}, and not the largest number")),
            },
            Must::CountedAtLeast(micros) => match balance.unknown {
                true => same("counted, at least", balance.counted, *micros),
                false => Err("the books say how much is counted".to_owned()),
            },
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
            Must::Sent(run, micros) => match books.run(*run) {
                Sent::Known(spend) => same(&format!("{run} sent"), spend.known(), *micros),
                found => Err(format!("{run} sent {found:?}, and it must be known")),
            },
            Must::SentAtMost(run, micros) => match books.run(*run) {
                Sent::Unknown { at_most } => same(&format!("{run} sent at most"), at_most, *micros),
                found => Err(format!("{run} sent {found:?}, and it must be unknown")),
            },
            Must::SentUnbounded(run) => match books.run(*run) {
                Sent::Unbounded => Ok(()),
                found => Err(format!("{run} sent {found:?}, and no most can be said")),
            },
            Must::NeedsOwner => match balance.needs_owner {
                true => Ok(()),
                false => Err("the store does not need its owner".to_owned()),
            },
            Must::Outcome(at, outcome) => match outcomes.get(*at) {
                Some(found) if found == outcome => Ok(()),
                found => Err(format!("event {at} came to {found:?}, not {outcome:?}")),
            },
            Must::Closed(run) => match of(run)?.closed {
                true => Ok(()),
                false => Err(format!("{run} is not closed")),
            },
            Must::TreeLeft(run, micros) => {
                let found = books.tree_left(*run).ok_or(format!("{run} runs no tree"))?;
                same(&format!("{run}'s tree has left"), found.into(), *micros)
            }
            Must::NothingExecutes => {
                let mut runs = books.executing_in(false);
                runs.extend(books.executing_in(true));
                match runs.is_empty() {
                    true => Ok(()),
                    false => Err(format!("{runs:?} execute")),
                }
            }
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
        // Everything it must give that it does not is said, not the first alone.
        let (mut after, mut wrong) = (Vec::new(), Vec::new());
        let (books, outcomes) = run_watched(&self.events, |at, books, outcomes| {
            for (_, must) in self.earlier.iter().filter(|(when, _)| *when == at) {
                if let Err(why) = must.holds(books, outcomes) {
                    wrong.push(format!("after event {at}: {why}"));
                }
            }
            after.push(at);
            Ok(())
        })
        .map_err(|why| format!("{}: {why}", self.name))?;
        if let Some((at, _)) = self.earlier.iter().find(|(at, _)| !after.contains(at)) {
            return Err(format!("{}: it has no event {at}", self.name));
        }
        let last = self
            .must
            .iter()
            .filter_map(|must| must.holds(&books, &outcomes).err());
        wrong.extend(last);
        match wrong.is_empty() {
            true => Ok(books),
            false => Err(format!("{}: {}", self.name, wrong.join("; "))),
        }
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
/// the plan for checkpoint 1B produced, in the plan's order and under its numbers;
/// 14 and 15 are worded as the plan was amended with ADR 0016.
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
        vec![state(r(1), State::Frozen), Counts(r(1), 32), Left(0)],
        vec![],
    );
    add(
        "14 a reservation's content damaged, its run deleted and pruned, no request id: \
         frozen at no less than the amount its hold's name states",
        vec![
            reset(20),
            admit(1, 10),
            Event::Crash,
            Event::Damage(Name::Reservation(r(1))),
            Event::DeletePrune { run: r(1) },
            Event::Recover,
        ],
        vec![state(r(1), State::Frozen), Counts(r(1), 20), Left(0)],
        vec![],
    );
    add(
        "15 reserve 10, every dispatch settled for 3 in all, then the reservation damaged: \
         no less than 10",
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
        vec![state(r(1), State::Frozen), Counts(r(1), 27), Left(0)],
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
            // Frozen: the amount and every bound; and for the other, the amount,
            // the charge, and the amount for each of three places with no dispatch.
            Counts(r(1), 23),
            state(r(2), State::Frozen),
            Counts(r(2), 44),
            state(r(3), State::Open),
            Counts(r(3), 5),
            Left(0),
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
            // Each closing takes a turn: the one that judged the same books is
            // asked again, and is done.
            came(8, again(Outcome::Done)),
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
    // A correction of the first reservation as other hands would make it after
    // these events: at its next place, naming this turn, sealed over its names.
    let forged = |events: &[Event], turn: u64, micros: u64| {
        let books = Books::after(events).0;
        let place = books.next_place(r(1));
        Name::Adjusted {
            run: r(1),
            place,
            turn,
            seal: books.seal_for(r(1), place),
            micros,
        }
    };
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
            Counts(r(1), 52),
            Outstanding(r(1), 10),
            Left(0),
        ],
        vec![
            (4, Counts(r(1), 17)),
            (5, state(r(1), State::Frozen)),
            (5, Counts(r(1), 42)),
        ],
    );
    let interrupted = || with(prefix(), vec![Event::Crash, Event::Recover]);
    add(
        "44 a correction: one other hands made without its place is none; the owner's \
         writes off what is outstanding; and it says nothing once a name of its \
         reservation is made or damaged",
        with(
            interrupted(),
            vec![
                Event::Plant(forged(&interrupted(), 1, 0)),
                adjust(1, 0),
                Event::Plant(Name::Closed {
                    run: r(1),
                    released: true,
                }),
                adjust(1, 0),
                Event::Damage(Name::Held {
                    epoch: 1,
                    run: r(1),
                    micros: 10,
                }),
            ],
        ),
        vec![
            came(6, Outcome::Done),
            came(8, Outcome::Done),
            came(9, Outcome::Done),
            state(r(1), State::Frozen),
            Counts(r(1), 45),
            Counted(45),
        ],
        vec![
            (4, state(r(1), State::Interrupted)),
            (5, state(r(1), State::Frozen)),
            (5, Counts(r(1), 35)),
            // A correction other hands made took no turn: the owner is needed.
            (5, Must::NeedsOwner),
            (6, state(r(1), State::Adjusted)),
            (6, Counts(r(1), 0)),
            (7, state(r(1), State::Frozen)),
            (7, Counts(r(1), 35)),
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
        vec![
            state(r(1), State::Frozen),
            Epoch(2),
            // Two holds: the books cannot tell which is the ledger's.
            Must::CountedAtLeast(15),
            // The hold other hands made in the new epoch took no turn there.
            Must::NeedsOwner,
            Left(0),
        ],
        vec![(3, Epoch(1)), (3, Must::CountedAtLeast(15)), (3, Left(0))],
    );
    add(
        "46 a second hold of more: counted at the larger, and nothing more is dispatched",
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
            came(6, refused(Why::Frozen)),
            state(r(1), State::Frozen),
            Must::CountsAtLeast(r(1), 57),
            Left(0),
        ],
        vec![],
    );
    add(
        "47 a turn other hands made far ahead: an admission that judged before a \
         settlement is still asked again, and nothing is admitted until the owner resets",
        with(
            prefix(),
            vec![
                Event::Plant(Name::Turn {
                    epoch: 1,
                    turn: 100,
                }),
                settle(1, 1, charged(12)),
                stale(3, admit(2, 10)),
                reset(20),
                admit(2, 8),
            ],
        ),
        vec![
            came(5, again(refused(Why::NeedsOwner))),
            came(7, Outcome::Done),
            Epoch(2),
            Counted(8),
            EpochOf(r(1), 1),
        ],
        vec![(3, Must::NeedsOwner), (5, Absent(r(2))), (5, Counted(12))],
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
            Counts(r(1), 15),
            Outstanding(r(1), 5),
            Counted(15),
            Must::NeedsOwner,
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
        "50 a second dispatch at one place that names another run: frozen, and what \
         its run sent is unknown, at most what the reservation counts for",
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
            Counts(r(1), 15),
            Must::SentAtMost(r(1), 15),
        ],
        vec![(2, Must::Sent(r(1), 5))],
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
            // The amount, the bound, and the amount for the place the charge took.
            Counts(r(1), 25),
            Left(0),
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
    // What the second review round found: each of its seven traces, as it was given.
    let stray = |count| Name::Stray { run: r(1), count };
    let answered = |place, micros| Name::Settled {
        run: r(1),
        place,
        stage: Stage::Answered,
        micros,
    };
    let charge = |place, micros| Event::Charge {
        root: r(1),
        place,
        micros,
    };
    add(
        "57 a correction other hands made with the right seal and no place of its own: \
         it is none, and no other name stands in for the place",
        with(
            interrupted(),
            vec![
                Event::Plant(forged(&interrupted(), 1, 0)),
                Event::Plant(stray(1)),
                admit(2, 20),
            ],
        ),
        vec![
            came(7, refused(Why::NeedsOwner)),
            state(r(1), State::Frozen),
            Counts(r(1), 35),
            Counted(35),
        ],
        vec![(5, state(r(1), State::Frozen)), (5, Counts(r(1), 35))],
    );
    add(
        "58 a correction, then a settlement, then names made and lost until as many are \
         there as before: the correction says nothing of names it was not made over",
        with(
            prefix(),
            vec![
                Event::Plant(stray(1)),
                Event::Plant(stray(2)),
                Event::Plant(stray(3)),
                second(Event::Recover),
                adjust(1, 0),
                settle(1, 1, charged(4)),
                Event::Remove(stray(1)),
                Event::Remove(stray(2)),
                Event::Plant(answered(1, 1)),
                Event::Remove(stray(3)),
            ],
        ),
        vec![
            state(r(1), State::Frozen),
            Counts(r(1), 40),
            Charged(r(1), 5),
        ],
        vec![
            (7, state(r(1), State::Adjusted)),
            (7, Counts(r(1), 0)),
            (8, state(r(1), State::Frozen)),
            (8, Counts(r(1), 39)),
            (10, state(r(1), State::Frozen)),
            (11, state(r(1), State::Frozen)),
        ],
    );
    add(
        "59 a lost place lets a second request take it: every bound and every charge \
         at the place is counted, and the run does not end with the first unsettled",
        vec![
            reset(20),
            admit(1, 10),
            dispatch(1, 3),
            charge(1, 3),
            Event::Remove(Name::Place {
                run: r(1),
                place: 1,
            }),
            stale(2, dispatch(1, 4)),
            charge(1, 8),
            settle(1, 1, charged(8)),
            end(1),
        ],
        vec![
            came(5, Outcome::Placed(1)),
            came(8, refused(Why::RequestsOutstanding)),
            state(r(1), State::Frozen),
            // The amount, 3 and 4 and 8, and the amount for the place the charge took.
            Counts(r(1), 35),
            Must::SentAtMost(r(1), 35),
        ],
        vec![(4, state(r(1), State::Frozen)), (5, Counts(r(1), 17))],
    );
    add(
        "60 a settlement under its bound that ends a correction: an admission that \
         judged before it is asked again",
        with(
            prefix(),
            vec![
                second(Event::Recover),
                adjust(1, 0),
                settle(1, 1, charged(3)),
                stale(5, admit(2, 20)),
            ],
        ),
        vec![
            came(6, again(refused(Why::OverAllowance { left: 10 }))),
            state(r(1), State::Interrupted),
            Counts(r(1), 10),
            Absent(r(2)),
        ],
        vec![(4, Left(20)), (5, Left(10))],
    );
    add(
        "61 a turn that is lost: nothing more is admitted until the owner resets; an \
         admission that judged before the loss is published, is not confirmed, and \
         is closed with everything released",
        with(
            prefix(),
            vec![
                settle(1, 1, charged(12)),
                Event::Remove(Name::Turn { epoch: 1, turn: 2 }),
                stale(3, admit(2, 10)),
                admit(3, 1),
                reset(30),
                admit(3, 1),
            ],
        ),
        vec![
            came(5, refused(Why::NotConfirmed)),
            came(6, refused(Why::NeedsOwner)),
            came(8, Outcome::Done),
            state(r(2), State::Released),
            Counts(r(2), 0),
            Epoch(2),
            Left(29),
        ],
        vec![
            (4, Must::NeedsOwner),
            (4, Left(0)),
            (5, Must::NeedsOwner),
            (6, Must::NeedsOwner),
        ],
    );
    add(
        "62 a charge over its bound without its guard takes no place: a place nothing \
         accounts for is still counted at the amount",
        with(
            prefix(),
            vec![
                dispatch(1, 5),
                charge(1, 6),
                Event::Remove(Name::Dispatched {
                    run: r(1),
                    place: 2,
                    by: r(1),
                    micros: 5,
                }),
                Event::Plant(answered(1, 6)),
            ],
        ),
        vec![state(r(1), State::Frozen), Counts(r(1), 31)],
        vec![(5, Counts(r(1), 25))],
    );
    add(
        "63 a dispatch whose name is lost, its settlement still there: what its run \
         sent is unknown, at most what the reservation counts for",
        with(
            prefix(),
            vec![
                settle(1, 1, charged(3)),
                end(1),
                Event::Remove(Name::Dispatched {
                    run: r(1),
                    place: 1,
                    by: r(1),
                    micros: 5,
                }),
            ],
        ),
        vec![
            state(r(1), State::Frozen),
            Counts(r(1), 33),
            Must::SentAtMost(r(1), 33),
        ],
        vec![(4, Must::Sent(r(1), 3)), (4, Counts(r(1), 3))],
    );
    add(
        "64 a settlement with its guard at a charge its stage does not allow: it is no \
         settlement, and recovery releases nothing",
        with(
            prefix(),
            vec![
                Event::Plant(Name::Settling {
                    run: r(1),
                    place: 1,
                }),
                Event::Plant(Name::Settled {
                    run: r(1),
                    place: 1,
                    stage: Stage::Unresolved,
                    micros: 0,
                }),
                Event::Crash,
                Event::Recover,
            ],
        ),
        vec![state(r(1), State::Frozen), Counts(r(1), 25), Counted(25)],
        vec![(4, state(r(1), State::Frozen)), (4, Counts(r(1), 15))],
    );
    // What the third review round found: each of its three traces, as it was given.
    add(
        "65 a dispatch whose name is lost, then a settlement made at its place without \
         a guard: the place counts the amount still, and nothing is admitted",
        with(
            prefix(),
            vec![
                dispatch(1, 5),
                settle(1, 1, charged(12)),
                Event::Remove(Name::Dispatched {
                    run: r(1),
                    place: 2,
                    by: r(1),
                    micros: 5,
                }),
                Event::Plant(Name::Settled {
                    run: r(1),
                    place: 2,
                    stage: Stage::NotSent,
                    micros: 0,
                }),
                admit(2, 8),
            ],
        ),
        vec![
            // Its hold states its amount, so the store goes on: and 47 are counted
            // of 20.
            came(7, refused(Why::OverAllowance { left: 0 })),
            state(r(1), State::Frozen),
            // The amount, the bound, the charge, and the amount for each of the two
            // places that hold no dispatch.
            Counts(r(1), 10 + 5 + 12 + 20),
            Must::SentAtMost(r(1), 47),
            Absent(r(2)),
        ],
        vec![(4, Counts(r(1), 17)), (5, state(r(1), State::Frozen))],
    );
    add(
        "66 a settlement with its guard at a charge its stage does not allow, over its \
         bound: it accounts for no place",
        vec![
            reset(20),
            admit(1, 10),
            dispatch(1, 0),
            dispatch(1, 5),
            dispatch(1, 5),
            settle(1, 3, charged(12)),
            Event::Remove(Name::Dispatched {
                run: r(1),
                place: 2,
                by: r(1),
                micros: 5,
            }),
            Event::Plant(Name::Settling {
                run: r(1),
                place: 1,
            }),
            Event::Plant(Name::Settled {
                run: r(1),
                place: 1,
                stage: Stage::Unresolved,
                micros: 1,
            }),
        ],
        vec![
            state(r(1), State::Frozen),
            Counts(r(1), 10 + 5 + 13 + 20),
            Must::NeedsOwner,
        ],
        vec![(5, Counts(r(1), 17))],
    );
    add(
        "67 the hold lost while holds other hands made in other epochs remain, and a \
         hold made for another run: counted in its own epoch still, and nothing is \
         admitted",
        vec![
            reset(10),
            admit(1, 10),
            dispatch(1, 10),
            Event::Plant(Name::Reset {
                epoch: 1,
                micros: 11,
            }),
            Event::Plant(Name::Held {
                epoch: 2,
                run: r(1),
                micros: 10,
            }),
            Event::Plant(Name::Held {
                epoch: 3,
                run: r(1),
                micros: 10,
            }),
            Event::Remove(Name::Held {
                epoch: 1,
                run: r(1),
                micros: 10,
            }),
            Event::Plant(Name::Held {
                epoch: 1,
                run: r(9),
                micros: 0,
            }),
            admit(2, 10),
        ],
        vec![
            came(8, refused(Why::NeedsOwner)),
            Epoch(1),
            Allowed(10),
            state(r(1), State::Frozen),
            Must::CountedAtLeast(20),
            Left(0),
            Absent(r(2)),
        ],
        vec![],
    );
    // What the hunt over other seeds found, once the generators made holds in any
    // epoch and the second suite let other hands do six things.
    add(
        "68 a reservation frozen by holds other hands made in other epochs, its own \
         lost, and then corrected by its owner: counted in its own epoch still, and \
         nothing more is admitted",
        vec![
            reset(10),
            admit(1, 10),
            dispatch(1, 10),
            Event::Crash,
            Event::Recover,
            Event::Remove(Name::Held {
                epoch: 1,
                run: r(1),
                micros: 10,
            }),
            Event::Plant(Name::Held {
                epoch: 5,
                run: r(1),
                micros: 10,
            }),
            Event::Plant(Name::Held {
                epoch: 6,
                run: r(1),
                micros: 10,
            }),
            // The turns its admission and its closing took, so that the epoch's
            // turns are those its publications account for.
            Event::Remove(Name::Turn { epoch: 1, turn: 1 }),
            Event::Remove(Name::Turn { epoch: 1, turn: 2 }),
            adjust(1, 10),
            admit(2, 10),
        ],
        vec![
            came(10, Outcome::Done),
            came(11, refused(Why::OverAllowance { left: 0 })),
            state(r(1), State::Adjusted),
            Counts(r(1), 10),
            Epoch(1),
            Allowed(10),
            Counted(10),
            Left(0),
            Absent(r(2)),
        ],
        vec![
            (7, state(r(1), State::Frozen)),
            // The amount, the bound, and the amount for the closing's place.
            (7, Must::CountsAtLeast(r(1), 30)),
            (9, Must::NeedsOwner),
        ],
    );
    // What the fourth review round found: each of its traces, as it was given.
    let held = |epoch, micros| Name::Held {
        epoch,
        run: r(1),
        micros,
    };
    let sent = |place, micros| Name::Dispatched {
        run: r(1),
        place,
        by: r(1),
        micros,
    };
    add(
        "69 two holds of nothing made, then the ledger's hold and its dispatch lost: \
         the amount is unknown and the books say so, and nothing is admitted or \
         dispatched anywhere",
        vec![
            reset(30),
            admit(1, 10),
            admit(2, 5),
            dispatch(1, 10),
            Event::Plant(held(1, 0)),
            Event::Plant(held(2, 0)),
            Event::Remove(held(1, 10)),
            Event::Remove(sent(1, 10)),
            admit(3, 1),
            dispatch(1, 0),
            dispatch(2, 1),
        ],
        vec![
            came(8, refused(Why::NeedsOwner)),
            came(9, refused(Why::Frozen)),
            came(10, refused(Why::NeedsOwner)),
            state(r(1), State::Frozen),
            // No hold states more than nothing, and no dispatch is named: the
            // names say nothing of the 10 that are outstanding.
            Must::CountsAtLeast(r(1), 0),
            Must::SentUnbounded(r(1)),
            // The other reservation's 5, and nothing that a name says of this one.
            Must::CountedAtLeast(5),
            Must::NeedsOwner,
            Absent(r(3)),
            Outstanding(r(2), 0),
        ],
        vec![
            (3, Counts(r(1), 10)),
            // With the ledger's hold still among them the amount is at least its
            // own: nothing is lost, though the books cannot tell.
            (5, state(r(1), State::Frozen)),
            (5, Must::CountsAtLeast(r(1), 20)),
            (5, Must::CountedAtLeast(25)),
            (5, Must::NeedsOwner),
            (6, Must::CountsAtLeast(r(1), 10)),
        ],
    );
    let overran = || {
        vec![
            reset(30),
            admit(1, 10),
            child(1, 2, 5),
            dispatch(1, 5),
            settle(1, 1, charged(12)),
        ]
    };
    let lost = |events: Vec<Event>| {
        with(
            events,
            vec![
                Event::Remove(answered(1, 12)),
                Event::Remove(Name::Settling {
                    run: r(1),
                    place: 1,
                }),
                Event::Remove(Name::Place {
                    run: r(1),
                    place: 2,
                }),
            ],
        )
    };
    add(
        "70 a parent's settlement over its bound lost with its guard and its place: \
         the reservation reads open with 5 left, its epoch needs its owner, and the \
         child's dispatch is refused",
        with(lost(overran()), vec![dispatch(2, 5)]),
        vec![
            came(8, refused(Why::NeedsOwner)),
            SameNamesAsAfter(7),
            state(r(1), State::Open),
            Counts(r(1), 10),
            Outstanding(r(1), 5),
            Must::NeedsOwner,
            Spendable(r(2), 5),
            Owes(r(1), 7),
        ],
        vec![(4, Counts(r(1), 12)), (5, state(r(1), State::Frozen))],
    );
    add(
        "71 a parent's settlement over its bound, the record damaged and the \
         overrun's place lost, then a child's dispatch that judged before the \
         overrun: the process's memory of the tree refuses it",
        with(
            overran(),
            vec![
                Event::Damage(Name::Reservation(r(1))),
                Event::Remove(Name::Place {
                    run: r(1),
                    place: 2,
                }),
                stale(4, dispatch(2, 5)),
            ],
        ),
        vec![
            came(7, again(refused(Why::Frozen))),
            SameNamesAsAfter(6),
            state(r(1), State::Frozen),
            // The amount, the bound and the charge.
            Counts(r(1), 27),
            Spendable(r(2), 5),
        ],
        vec![(5, Counts(r(1), 37))],
    );
    let most = u64::MAX - 1;
    add(
        "72 amounts and a place near the largest number: the frozen sum stops at the \
         largest number and is over any allowance",
        vec![
            reset(most),
            admit(1, most),
            dispatch(1, 0),
            dispatch(1, 0),
            settle(1, 1, charged(most)),
            settle(1, 2, charged(most)),
            Event::Plant(Name::Place {
                run: r(1),
                place: u64::MAX,
            }),
            Event::Plant(answered(5, most)),
            Event::Plant(answered(6, most)),
            Event::Plant(answered(7, most)),
            admit(2, 0),
        ],
        vec![
            // A call of nothing is admitted in a store over any allowance.
            came(10, Outcome::Done),
            state(r(1), State::Frozen),
            Must::Saturated(r(1)),
            Allowed(most),
            Left(0),
        ],
        vec![(5, state(r(1), State::Open)), (5, Left(0))],
    );
    // What was found while these were built.
    add(
        "73 the same loss with the turn the settlement took lost too: the names are a \
         set the ledger could have written with 5 left and no epoch needs its owner, \
         and the process's memory of the tree refuses the child's dispatch",
        with(
            lost(overran()),
            vec![
                Event::Remove(Name::Turn { epoch: 1, turn: 2 }),
                dispatch(2, 5),
                dispatch(2, 0),
            ],
        ),
        vec![
            came(9, refused(Why::OverTree { left: 0 })),
            came(10, Outcome::Placed(2)),
            state(r(1), State::Open),
            Counts(r(1), 10),
            Left(20),
            Spendable(r(2), 5),
        ],
        vec![(8, Left(20)), (9, SameNamesAsAfter(8))],
    );
    add(
        "74 a hold of nothing made in the place of the ledger's, over a dispatch of 1: \
         no reservation of nothing dispatched 1, and it is frozen",
        vec![
            reset(10),
            admit(1, 1),
            dispatch(1, 1),
            Event::Plant(held(1, 0)),
            Event::Remove(held(1, 1)),
        ],
        vec![state(r(1), State::Frozen), Counts(r(1), 1), Left(9)],
        vec![],
    );
    add(
        "75 the provider's answer says 12 over a bound of 5 and is not settled yet: \
         what the process knows was charged is taken from the tree at once, and a \
         child's dispatch is refused before any name says so",
        with(
            overran()[..4].to_vec(),
            vec![
                charge(1, 12),
                dispatch(2, 5),
                settle(1, 1, How::NotSent),
                dispatch(2, 0),
            ],
        ),
        vec![
            came(5, refused(Why::OverTree { left: 0 })),
            came(6, Outcome::Done),
            came(7, Outcome::Placed(3)),
            Counts(r(1), 12),
            Spendable(r(2), 5),
        ],
        vec![(4, Counts(r(1), 10)), (5, SameNamesAsAfter(3))],
    );
    // The fifth review round's.
    add(
        "76 a turn lost and the next one made, then an admission judged before both \
         whose turn fills the place: it is published, the books read 20 against 10, \
         and it is not confirmed: nothing runs under it and all it held is released",
        vec![
            reset(10),
            admit(1, 10),
            dispatch(1, 10),
            Event::Remove(Name::Turn { epoch: 1, turn: 1 }),
            Event::Plant(Name::Turn { epoch: 1, turn: 2 }),
            stale(1, admit(2, 10)),
            dispatch(2, 10),
        ],
        vec![
            came(5, refused(Why::NotConfirmed)),
            came(6, refused(Why::NotExecuting)),
            state(r(2), State::Released),
            Counts(r(2), 0),
            Outstanding(r(1), 10),
            Counted(10),
            Left(0),
            Debt(0),
        ],
        vec![(4, Must::NeedsOwner)],
    );
    // A tree deeper than a generated history is long: 23 calls, each from inside
    // the one before, and each keeping 2 of its share. The run at the leaf sends a
    // request of 4, and the answer says 60, over all the 50 the tree reserved. No
    // name says so yet. A run half way up then publishes a request of 2 that it
    // judged before the leaf's: the names have 46 left and the run has its 2, and
    // what the process holds of the tree refuses it. The process dies; recovery
    // closes the tree's reservation with nothing released.
    let mut deep = vec![reset(100), admit(1, 50)];
    deep.extend((1..24).map(|above| child(above, above + 1, 50 - 2 * u64::from(above))));
    deep.extend([
        dispatch(24, 4),
        charge(1, 60),
        stale(25, dispatch(12, 2)),
        dispatch(1, 2),
        Event::Crash,
        Event::Recover,
    ]);
    add(
        "77 a tree 24 runs deep, the leaf's answer over all the tree reserved, then a \
         request of a run half way up that judged before it: the process's memory of \
         the tree refuses it, and after a crash recovery closes the tree's reservation",
        deep,
        vec![
            came(24, Outcome::Done),
            came(25, Outcome::Placed(1)),
            came(27, again(refused(Why::OverTree { left: 0 }))),
            came(28, refused(Why::OverTree { left: 0 })),
            Must::Closed(r(1)),
            state(r(1), State::Interrupted),
            Counts(r(1), 50),
            Outstanding(r(1), 4),
            Must::Sent(r(24), 4),
            Must::NothingExecutes,
            Counted(50),
            Left(50),
        ],
        vec![
            (26, Spendable(r(24), 0)),
            (26, Spendable(r(12), 2)),
            (27, SameNamesAsAfter(25)),
            (28, SameNamesAsAfter(25)),
            (28, state(r(1), State::Open)),
        ],
    );
    // The sixth review round's, in its two forms. A request that was judged while
    // the store was sound is published once the store needs its owner: by the loss
    // of another reservation's hold, whose amount is then unknown, and by the loss
    // of a turn. (As the round gave it, the other reservation was damaged: since
    // the owner's ruling a damaged reservation whose hold is there stops nothing
    // else, which is history 81.)
    let publish = |run, micros| Event::Publish {
        run: r(run),
        micros,
    };
    let confirm = |run| Event::Confirm { run: r(run) };
    add(
        "78 a request judged before another reservation lost its hold: it is published \
         where the store needs its owner, within what its tree really has left, and \
         nothing that is judged afterwards is",
        vec![
            reset(20),
            publish(1, 10),
            confirm(1),
            second(admit(2, 10)),
            Event::Remove(Name::Held {
                epoch: 1,
                run: r(2),
                micros: 10,
            }),
            stale(4, dispatch(1, 10)),
            stale(4, dispatch(1, 1)),
            dispatch(1, 0),
            admit(3, 0),
        ],
        vec![
            came(5, Outcome::Placed(1)),
            came(6, refused(Why::OverRun { left: 0 })),
            came(7, refused(Why::NeedsOwner)),
            came(8, refused(Why::NeedsOwner)),
            SameNamesAsAfter(5),
            state(r(1), State::Open),
            Counts(r(1), 10),
            Outstanding(r(1), 10),
            state(r(2), State::Frozen),
            // No name states the second's amount: the books say so.
            Must::CountsAtLeast(r(2), 0),
            Must::CountedAtLeast(10),
            Left(0),
            Must::NeedsOwner,
            Must::TreeLeft(r(1), 0),
            Spendable(r(1), 0),
        ],
        vec![
            (4, Must::NeedsOwner),
            (4, Must::CountedAtLeast(10)),
            (4, Must::TreeLeft(r(1), 10)),
        ],
    );
    add(
        "79 the same with nothing but a turn lost: the request judged before the \
         loss is published, and one judged after it is refused",
        vec![
            reset(10),
            publish(1, 10),
            confirm(1),
            Event::Remove(Name::Turn { epoch: 1, turn: 1 }),
            stale(3, dispatch(1, 10)),
            stale(3, dispatch(1, 1)),
            dispatch(1, 0),
            stale(4, dispatch(1, 0)),
        ],
        vec![
            came(4, Outcome::Placed(1)),
            came(5, refused(Why::OverRun { left: 0 })),
            came(6, refused(Why::NeedsOwner)),
            came(7, refused(Why::NeedsOwner)),
            SameNamesAsAfter(4),
            state(r(1), State::Open),
            Counts(r(1), 10),
            Outstanding(r(1), 10),
            Counted(10),
            Must::NeedsOwner,
            Must::TreeLeft(r(1), 0),
        ],
        vec![(3, Must::NeedsOwner), (3, Must::TreeLeft(r(1), 10))],
    );
    // What the review of the record as the sixth round left it asked for, each by
    // the letter it was given under.
    add(
        "80 (B) a place and a dispatch made under an admission between its publication \
         and its reading: its reservation is no longer the record and one hold, and \
         it is not confirmed",
        vec![
            reset(20),
            publish(1, 10),
            Event::Plant(Name::Place {
                run: r(1),
                place: 1,
            }),
            Event::Plant(sent(1, 3)),
            confirm(1),
            dispatch(1, 1),
        ],
        vec![
            came(4, refused(Why::NotConfirmed)),
            came(5, refused(Why::NotExecuting)),
            // A dispatch is named under it and not settled: its closing releases
            // nothing.
            Must::Closed(r(1)),
            state(r(1), State::Interrupted),
            Counts(r(1), 10),
            Counted(10),
            Left(10),
            Must::NothingExecutes,
        ],
        vec![(3, state(r(1), State::Open)), (3, Counts(r(1), 10))],
    );
    add(
        "81 (J) one record that cannot be read: its reservation is frozen at the most \
         its names can mean, and stops nothing else: a call that the rest of the \
         allowance covers is admitted",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 5),
            Event::Damage(sent(1, 5)),
            Event::Crash,
            Event::Recover,
            admit(2, 5),
            dispatch(2, 5),
            admit(3, 1),
        ],
        vec![
            came(6, Outcome::Done),
            came(7, Outcome::Placed(1)),
            came(8, refused(Why::OverAllowance { left: 0 })),
            state(r(1), State::Frozen),
            // The amount, the bound, and the amount for the closing's place.
            Counts(r(1), 25),
            Counts(r(2), 5),
            Counted(30),
            Allowed(30),
            Left(0),
            Debt(0),
            Absent(r(3)),
        ],
        vec![(3, Counts(r(1), 15)), (3, Left(15)), (5, Left(5))],
    );
    // A request of 10 is in flight under a reservation of 10, which is all of the
    // allowance. A second process recovers the store, which closes the reservation
    // with nothing released though its run is alive; the owner corrects it to
    // nothing, which writes off what is outstanding; and 10 more are admitted.
    let written_off = || {
        vec![
            reset(10),
            admit(1, 10),
            dispatch(1, 10),
            second(Event::Recover),
            second(adjust(1, 0)),
            second(admit(2, 10)),
            second(dispatch(2, 10)),
            second(settle(2, 1, charged(10))),
        ]
    };
    add(
        "82 (A) a correction to nothing of a reservation whose request is still in \
         flight, 10 more admitted and spent, and the request then settled at 10: 20 \
         counted against 10",
        with(written_off(), vec![settle(1, 1, charged(10))]),
        vec![
            came(8, Outcome::Done),
            // The settlement ended the correction: the reservation counts again.
            state(r(1), State::Interrupted),
            Counts(r(1), 10),
            Counts(r(2), 10),
            Allowed(10),
            Counted(20),
            Debt(10),
            Left(0),
        ],
        vec![
            (3, state(r(1), State::Interrupted)),
            (3, Counts(r(1), 10)),
            (4, state(r(1), State::Adjusted)),
            (4, Counts(r(1), 0)),
            (4, Left(10)),
            // The second admission: 10 counted, while 20 are committed.
            (5, Counted(10)),
            (5, Debt(0)),
        ],
    );
    add(
        "83 (A2) the process that sent the request dies with the provider's answer, \
         the next recovers the store, the owner corrects to nothing and 10 more are \
         admitted and spent: 10 are counted where 20 were charged",
        vec![
            reset(10),
            admit(1, 10),
            dispatch(1, 10),
            Event::Charge {
                root: r(1),
                place: 1,
                micros: 10,
            },
            Event::Crash,
            Event::Recover,
            adjust(1, 0),
            admit(2, 10),
            dispatch(2, 10),
            settle(2, 1, charged(10)),
        ],
        vec![
            state(r(1), State::Adjusted),
            Counts(r(1), 0),
            Counts(r(2), 10),
            Allowed(10),
            Counted(10),
            Debt(0),
            Left(0),
        ],
        vec![],
    );
    add(
        "84 (G) a reset to nothing while three reservations of 10 are open: each still \
         sends all it reserved, and the new epoch counts nothing",
        vec![
            reset(30),
            admit(1, 10),
            admit(2, 10),
            admit(3, 10),
            reset(0),
            dispatch(1, 10),
            dispatch(2, 10),
            dispatch(3, 10),
            admit(4, 1),
        ],
        vec![
            came(5, Outcome::Placed(1)),
            came(6, Outcome::Placed(1)),
            came(7, Outcome::Placed(1)),
            came(8, refused(Why::OverAllowance { left: 0 })),
            Epoch(2),
            Allowed(0),
            Counted(0),
            Debt(0),
            Left(0),
            EpochOf(r(1), 1),
            Outstanding(r(1), 10),
            Outstanding(r(2), 10),
            Outstanding(r(3), 10),
        ],
        vec![(3, Counted(30)), (3, Left(0))],
    );
    add(
        "85 (H) calls of nothing in a store with no allowance: each is admitted, a \
         request of nothing is placed under a reservation over its amount and in a \
         store in debt, and every charge is debt",
        vec![
            admit(1, 0),
            dispatch(1, 0),
            settle(1, 1, charged(7)),
            dispatch(1, 0),
            dispatch(1, 1),
            admit(2, 0),
            dispatch(2, 0),
            settle(2, 1, charged(5)),
        ],
        vec![
            came(0, Outcome::Done),
            came(1, Outcome::Placed(1)),
            // The charge over its bound took the second place.
            came(3, Outcome::Placed(3)),
            came(4, refused(Why::OverRun { left: 0 })),
            came(5, Outcome::Done),
            came(6, Outcome::Placed(1)),
            Epoch(0),
            Allowed(0),
            Counts(r(1), 7),
            Counts(r(2), 5),
            Counted(12),
            Debt(12),
            Left(0),
            Owes(r(1), 7),
            Owes(r(2), 5),
        ],
        vec![(2, Debt(7)), (2, Owes(r(1), 7))],
    );
    // The first of the limits the owner accepted with that ruling: with one hold
    // made in the place of the ledger's, a frozen reservation states an amount, and
    // the store goes on beside it.
    add(
        "86 one hold of nothing made in the place of the ledger's, the dispatch lost \
         with it, and the whole allowance then admitted beside the frozen \
         reservation: 30 counted of 30 where 40 are committed",
        vec![
            reset(30),
            admit(1, 10),
            dispatch(1, 10),
            Event::Plant(held(1, 0)),
            Event::Remove(held(1, 10)),
            Event::Remove(sent(1, 10)),
            admit(2, 30),
            dispatch(2, 30),
        ],
        vec![
            came(6, Outcome::Done),
            came(7, Outcome::Placed(1)),
            state(r(1), State::Frozen),
            Counts(r(1), 0),
            Counts(r(2), 30),
            Allowed(30),
            Counted(30),
            Debt(0),
            Left(0),
        ],
        vec![(2, Counted(10)), (5, Counted(0)), (5, Left(30))],
    );
    // The second: a settlement over its bound whose name is lost. The charge was in
    // that name alone; the reservation is frozen at what its other names can mean,
    // which is less, and the store goes on beside it.
    add(
        "87 a settlement at 15 over a bound of nothing whose name is lost, and what \
         then reads as left admitted beside the frozen reservation: 20 counted of 20 \
         where 33 are committed",
        vec![
            reset(20),
            admit(1, 1),
            dispatch(1, 0),
            settle(1, 1, charged(15)),
            Event::Remove(Name::Settled {
                run: r(1),
                place: 1,
                stage: Stage::Answered,
                micros: 15,
            }),
            admit(2, 18),
            dispatch(2, 18),
        ],
        vec![
            came(5, Outcome::Done),
            came(6, Outcome::Placed(1)),
            state(r(1), State::Frozen),
            // The amount, the bound of nothing, and the amount for the place the
            // charge took.
            Counts(r(1), 2),
            Counts(r(2), 18),
            Allowed(20),
            Counted(20),
            Debt(0),
            Left(0),
        ],
        vec![
            (3, Counted(15)),
            (3, Left(5)),
            (4, Counted(2)),
            (4, Left(18)),
        ],
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

/// A history as it is generated: the books it has made so far, and what chooses.
struct Generating {
    seeded: Seeded,
    books: Books,
    events: Vec<Event>,
    /// The shares taken and not handed back: the process, the run, the amount.
    taken: Vec<(bool, Run, u64)>,
    made: u32,
    /// What every amount chosen is multiplied by: 1, or a number that puts amounts
    /// near the largest a name can state.
    scale: u64,
}

impl Generating {
    fn new(seed: u64, number: u64) -> Generating {
        Generating {
            seeded: Seeded::new(seed ^ number.wrapping_mul(0xA24B_AED4_963E_E407)),
            books: Books::new(),
            events: Vec::new(),
            taken: Vec::new(),
            made: 0,
            scale: 1,
        }
    }

    /// An amount: a number under `bound`, by the scale, and never the largest
    /// number there is, which is no amount.
    fn some(&mut self, bound: u64) -> u64 {
        let chosen = self.seeded.below(bound);
        chosen.saturating_mul(self.scale).min(u64::MAX - 1)
    }

    /// An amount from nothing to this one.
    fn up_to(&mut self, most: u64) -> u64 {
        self.seeded.below(most.saturating_add(1))
    }

    /// A place: a low one; and, where amounts are large, one time in four one of
    /// the last a name can name.
    fn a_place(&mut self) -> u64 {
        let low = 1 + self.seeded.below(4);
        match self.scale > 1 && self.seeded.below(4) == 0 {
            true => u64::MAX - (low - 1),
            false => low,
        }
    }

    fn new_run(&mut self) -> Run {
        self.made += 1;
        Run(self.made)
    }

    fn push(&mut self, event: Event) {
        let outcome = self.books.apply(&event);
        let (second, _, plain) = event.plain();
        if let (Event::Take { run, micros }, Outcome::Done) = (plain, &outcome) {
            self.taken.push((second, *run, *micros));
        }
        // A share that could not be handed back is handed back to nobody.
        self.events.push(event);
    }

    /// One thing the ledger or a process does, chosen against the books as they
    /// stand, so that most apply. `wrapped` lets it be judged late, about one time
    /// in five, and happen in the second process, about one time in four.
    fn ledger(&mut self, wrapped: bool) {
        let other = wrapped && self.seeded.below(4) == 0;
        let live = self.books.executing_in(other);
        let unsettled = self.books.unsettled_in(other);
        let folded = self.books.fold();
        let roots: Vec<Run> = folded.reservations.keys().copied().collect();
        let event = match self.seeded.below(24) {
            0 => Some(reset(self.some(40))),
            1..=3 => None,
            4 | 5 => self.seeded.pick(&live).map(|parent| Event::Child {
                parent,
                child: self.new_run(),
                micros: self
                    .seeded
                    .below(self.books.spendable(parent).unwrap_or(0).saturating_add(2)),
            }),
            6..=9 => self.seeded.pick(&live).map(|by| Event::Dispatch {
                by,
                micros: self.some(8),
            }),
            10..=13 => {
                // Mostly a request this process has not settled; at times any place.
                let at = match self.seeded.below(4) {
                    0 => self
                        .seeded
                        .pick(&roots)
                        .map(|root| (root, 1 + self.seeded.below(4))),
                    _ => self.seeded.pick(&unsettled),
                };
                at.map(|(root, place)| Event::Settle {
                    root,
                    place,
                    how: match self.seeded.below(4) {
                        0 => How::NotSent,
                        1 => How::Answered(None),
                        2 => How::Unresolved,
                        _ => How::Answered(Some(self.some(15))),
                    },
                })
            }
            14 | 15 => self.seeded.pick(&live).map(|run| Event::End { run }),
            16 => self.seeded.pick(&live).map(|run| Event::Abandon { run }),
            17 => Some(Event::Crash),
            18 => Some(Event::Recover),
            19 => {
                // Mostly a reservation that is closed, and half the time a little.
                let closed = folded.reservations.iter().filter(|(_, one)| one.closed);
                let closed: Vec<Run> = closed.map(|(run, _)| *run).collect();
                let of = match self.seeded.below(4) {
                    0 => &roots,
                    _ => &closed,
                };
                self.seeded.pick(of).map(|run| Event::Adjust {
                    run,
                    micros: self.some(15) / (1 + self.seeded.below(2) * 5),
                })
            }
            20 => match self.seeded.below(2) {
                0 => self.seeded.pick(&live).map(|run| Event::Take {
                    run,
                    micros: self.up_to(self.books.spendable(run).unwrap_or(0)),
                }),
                _ => self
                    .taken
                    .iter()
                    .rposition(|(of, _, _)| *of == other)
                    .map(|at| self.taken.remove(at))
                    .map(|(_, run, micros)| Event::GiveBack { run, micros }),
            },
            21 | 22 => self
                .seeded
                .pick(&unsettled)
                .map(|(root, place)| Event::Charge {
                    root,
                    place,
                    micros: self.some(15),
                }),
            _ => self
                .seeded
                .pick(&roots)
                .map(|run| Event::DeletePrune { run }),
        };
        // Where nothing could be chosen, a call is admitted: there is always one.
        let event = match event {
            Some(event) => event,
            None => Event::Admit {
                run: self.new_run(),
                micros: self.some(13),
            },
        };
        let late = wrapped && self.seeded.below(5) == 0;
        let event = match late {
            true => stale(
                self.seeded.below(self.events.len() as u64 + 1) as usize,
                event,
            ),
            false => event,
        };
        self.push(if other { second(event) } else { event });
    }

    /// What other hands do: a name damaged, a name lost, a name made, of every form
    /// and at any number; or, with room for two events, a whole chain as the ledger
    /// would have made it: a settlement with its guard at a place that is
    /// dispatched, or a correction with its place, sealed over the names as they are
    /// and naming a turn that is there.
    fn hands(&mut self, room: usize) {
        if self.scale > 1 && room >= 4 && self.seeded.below(6) == 0 {
            return self.flood();
        }
        let names: Vec<Name> = self.books.names().iter().cloned().collect();
        let folded = self.books.fold();
        let roots: Vec<Run> = folded.reservations.keys().copied().collect();
        let run = Run(1 + self.seeded.below(8) as u32);
        let place = self.a_place();
        let micros = self.some(15);
        let epoch = self.seeded.below(3);
        let stage = Stage::ALL[self.seeded.below(3) as usize];
        let sent = names.iter().filter_map(|name| match name {
            Name::Dispatched {
                run, place, micros, ..
            } => Some((*run, *place, *micros)),
            _ => None,
        });
        let sent: Vec<(Run, u64, u64)> = sent.collect();
        // A correction of this reservation as the command would make it now.
        let correction = |books: &Books, seeded: &mut Seeded, run: Run, micros: u64| {
            let place = books.next_place(run);
            Name::Adjusted {
                run,
                place,
                turn: 1 + seeded.below(2),
                seal: books.seal_for(run, place),
                micros,
            }
        };
        let event = match self.seeded.below(20) {
            0..=2 => self.seeded.pick(&names).map(Event::Damage),
            3..=7 => self.seeded.pick(&names).map(Event::Remove),
            8 if room >= 2 => self.seeded.pick(&sent).map(|(run, place, bound)| {
                self.push(Event::Plant(Name::Settling { run, place }));
                // Half the time at a charge its stage allows for the bound.
                let micros = match (self.seeded.below(2), stage) {
                    (0, Stage::NotSent) => 0,
                    (0, Stage::Unresolved) => bound,
                    _ => self.some(6),
                };
                Event::Plant(Name::Settled {
                    run,
                    place,
                    stage,
                    micros,
                })
            }),
            9 if room >= 2 => self.seeded.pick(&roots).map(|run| {
                let name = correction(&self.books, &mut self.seeded, run, micros);
                let place = self.books.next_place(run);
                self.push(Event::Plant(name));
                Event::Plant(Name::Place { run, place })
            }),
            _ => Some(Event::Plant(match self.seeded.below(14) {
                // A hold, in any epoch up to three past the newest; and, one time in
                // three, for a run that has no reservation and no id yet.
                0 => Name::Held {
                    epoch: self.seeded.below(folded.epoch() + 4),
                    run: match self.seeded.below(3) {
                        0 => Run(self.made + 1 + self.seeded.below(3) as u32),
                        _ => run,
                    },
                    micros,
                },
                1 => Name::Dispatched {
                    run,
                    place,
                    by: Run(1 + self.seeded.below(8) as u32),
                    micros,
                },
                2 => Name::Settled {
                    run,
                    place,
                    stage,
                    micros,
                },
                3 => Name::Closed {
                    run,
                    released: self.seeded.below(2) == 0,
                },
                4 => Name::Place { run, place },
                5 => Name::Settling { run, place },
                6 => Name::Stray { run, count: place },
                7 => Name::Turn {
                    epoch,
                    turn: folded.next_turn(epoch) + self.seeded.below(3) * 50,
                },
                8 => Name::Epoch(1 + epoch),
                9 => Name::Reset {
                    epoch: 1 + epoch,
                    micros: self.some(60),
                },
                // A correction with the seal the command would give it and no
                // place; and one of any seal.
                10 => correction(&self.books, &mut self.seeded, run, micros),
                11 => Name::Adjusted {
                    run,
                    place,
                    turn: 1 + self.seeded.below(3),
                    seal: self.seeded.below(4),
                    micros,
                },
                // The place a correction that is there names.
                12 => {
                    let named = names.iter().find_map(|name| match name {
                        Name::Adjusted { run, place, .. } => Some(Name::Place {
                            run: *run,
                            place: *place,
                        }),
                        _ => None,
                    });
                    named.unwrap_or(Name::Place { run, place })
                }
                _ => Name::Reservation(run),
            })),
        };
        self.push(event.unwrap_or(Event::Plant(Name::Stray { run, count: place })));
    }

    /// Where amounts are large, other hands make the last place a name can name
    /// under one reservation, and three settlements at the largest amount: four
    /// events, which bring what a reservation of the largest amount counts for to
    /// the largest number there is.
    fn flood(&mut self) {
        let roots: Vec<Run> = self.books.fold().reservations.keys().copied().collect();
        let run = self.seeded.pick(&roots).unwrap_or(Run(1));
        let place = u64::MAX;
        self.push(Event::Plant(Name::Place { run, place }));
        for place in 5..8 {
            self.push(Event::Plant(Name::Settled {
                run,
                place,
                stage: Stage::Answered,
                micros: u64::MAX - 1,
            }));
        }
    }

    /// A tree whose parent's request is settled over its bound while its child
    /// holds a share: five events, all the ledger's, in the first process or in
    /// the second.
    fn overrun(&mut self, other: bool) {
        let wrap = |event: Event| if other { second(event) } else { event };
        let allowance = 10 + self.seeded.below(30);
        let allowance = allowance.saturating_mul(self.scale).min(u64::MAX - 1);
        self.push(wrap(reset(allowance)));
        let amount = (4 + self.seeded.below(7)).saturating_mul(self.scale);
        let (root, amount) = (self.new_run(), amount.min(u64::MAX - 1));
        self.push(wrap(Event::Admit {
            run: root,
            micros: amount,
        }));
        let share = self.scale + self.up_to(amount / 2 - self.scale);
        let child = self.new_run();
        self.push(wrap(Event::Child {
            parent: root,
            child,
            micros: share,
        }));
        let bound = self.up_to(amount - share);
        self.push(wrap(Event::Dispatch {
            by: root,
            micros: bound,
        }));
        // Over the bound: by a little, or by more than the whole reservation.
        let over = bound.saturating_add(self.scale.saturating_add(self.up_to(amount)));
        let over = charged(over.min(u64::MAX - 1));
        self.push(wrap(settle(root.0, 1, over)));
    }

    /// Other hands take away a whole settlement: itself and its guard; where it
    /// was over its bound, the last place of its reservation when that holds no
    /// dispatch; and half the time the last turn of the newest epoch. As many of
    /// them as there is room for, in that order. Where nothing is settled, one
    /// other thing other hands do.
    fn strip(&mut self, room: usize) {
        let names = self.books.names().clone();
        let settled = names.iter().filter_map(|name| match name {
            Name::Settled {
                run, place, micros, ..
            } => Some((*run, *place, *micros, name.clone())),
            _ => None,
        });
        let settled: Vec<(Run, u64, u64, Name)> = settled.collect();
        let Some((run, place, charge, name)) = self.seeded.pick(&settled) else {
            return self.hands(room);
        };
        let mut lost = vec![name, Name::Settling { run, place }];
        let bound = names.iter().find_map(|name| match name {
            Name::Dispatched {
                run: of,
                place: at,
                micros,
                ..
            } if *of == run && *at == place => Some(*micros),
            _ => None,
        });
        let last = self.books.next_place(run).saturating_sub(1);
        let holds = |name: &Name| matches!(name, Name::Dispatched { run: of, place: at, .. } if *of == run && *at == last);
        if bound.is_some_and(|bound| charge > bound) && last >= 1 && !names.iter().any(holds) {
            lost.push(Name::Place { run, place: last });
        }
        let folded = self.books.fold();
        let epoch = folded.epoch();
        let turn = folded.next_turn(epoch).saturating_sub(1);
        if self.seeded.below(2) == 0 && turn >= 1 {
            lost.push(Name::Turn { epoch, turn });
        }
        for name in lost.into_iter().take(room) {
            self.push(Event::Remove(name));
        }
    }

    /// A run that another run called, in the first process or in the second, sends
    /// a request of anything up to its share, half the time judged late. Where no
    /// such run executes, one other thing the ledger does.
    fn child_dispatch(&mut self, other: bool) {
        let live = self.books.executing_in(other);
        let called = |run: &Run| self.books.root_of(*run) != Some(*run);
        let called: Vec<Run> = live.into_iter().filter(called).collect();
        let Some(by) = self.seeded.pick(&called) else {
            return self.ledger(true);
        };
        let micros = self.up_to(self.books.spendable(by).unwrap_or(0));
        let event = Event::Dispatch { by, micros };
        let event = match self.seeded.below(2) {
            0 => stale(self.up_to(self.events.len() as u64) as usize, event),
            _ => event,
        };
        self.push(if other { second(event) } else { event });
    }

    /// Other hands take away one turn of an epoch and make another: any turn that
    /// is there, and the next that is free, or one time in four the one after it.
    /// In either order; in the newest epoch, or one time in four in any epoch that
    /// has a turn. Two events, or one where the epoch has no turn to lose. What it
    /// returns is the epoch and the turn that was lost.
    fn turns_moved(&mut self) -> Option<(u64, u64)> {
        let folded = self.books.fold();
        let newest = folded.epoch();
        let epochs: Vec<u64> = folded.turns.keys().copied().collect();
        let epoch = match self.seeded.below(4) {
            0 => self.seeded.pick(&epochs).unwrap_or(newest),
            _ => newest,
        };
        let taken = folded.turns.get(&epoch).into_iter().flatten();
        let taken: Vec<u64> = taken.copied().collect();
        let lost = self.seeded.pick(&taken);
        let made = folded.next_turn(epoch) + u64::from(self.seeded.below(4) == 0);
        let lose = lost.map(|turn| Event::Remove(Name::Turn { epoch, turn }));
        let make = Some(Event::Plant(Name::Turn { epoch, turn: made }));
        let events = match self.seeded.below(2) {
            0 => [lose, make],
            _ => [make, lose],
        };
        for event in events.into_iter().flatten() {
            self.push(event);
        }
        lost.map(|turn| (epoch, turn))
    }

    /// An admission judged before other hands moved a turn, and what its run then
    /// asks for. Three times in four it was judged on the names as they were just
    /// before the lost turn was taken, so that it chooses that turn; half the time
    /// it is for all that was left then. It is confirmed at once, or, one time in
    /// three, published and confirmed apart, with up to two things between: what
    /// other hands do, what the ledger does, a crash and a recovery. One time in
    /// four in the second process. Nine events at most.
    fn late_admission(&mut self, moved: Option<(u64, u64)>) {
        let now = self.events.len();
        let took = moved.and_then(|(epoch, turn)| {
            let name = Name::Turn { epoch, turn };
            let there = |after: &usize| {
                let names = self.books.names_after(*after);
                names.is_some_and(|names| names.contains(&name))
            };
            (1..=now).find(there)
        });
        let judged = match (took, self.seeded.below(4)) {
            (Some(after), 1..) => after - 1,
            _ => self.seeded.below(now as u64 + 1) as usize,
        };
        let none = BTreeSet::new();
        let names = self.books.names_after(judged).unwrap_or(&none);
        let left = fold(names, &none).balance().left;
        let micros = match self.seeded.below(2) {
            0 => left,
            _ => self.some(13),
        };
        let run = self.new_run();
        let other = self.seeded.below(4) == 0;
        let wrap = |event: Event| if other { second(event) } else { event };
        if self.seeded.below(3) > 0 {
            self.push(wrap(stale(judged, Event::Admit { run, micros })));
        } else {
            self.push(wrap(stale(judged, Event::Publish { run, micros })));
            for _ in 0..self.seeded.below(3) {
                match self.seeded.below(6) {
                    0 => {
                        self.push(wrap(Event::Crash));
                        self.push(wrap(Event::Recover));
                    }
                    1 | 2 => self.hands(1),
                    _ => self.ledger(true),
                }
            }
            self.push(wrap(Event::Confirm { run }));
        }
        self.push(wrap(Event::Dispatch { by: run, micros }));
        self.push(wrap(Event::Dispatch { by: run, micros: 0 }));
    }

    /// One thing that happens while two admissions wait to be confirmed: either
    /// process dies, either process recovers the store, other hands do something,
    /// or the ledger does. One event.
    fn meanwhile(&mut self) {
        let event = match self.seeded.below(6) {
            0 => Event::Crash,
            1 => second(Event::Crash),
            2 => Event::Recover,
            3 => second(Event::Recover),
            4 => return self.hands(1),
            _ => return self.ledger(true),
        };
        self.push(event);
    }

    /// Two admissions are published, each in either process, each judged now or
    /// earlier, for all that is left or for less, and both wait. Up to two things
    /// happen meanwhile; they are confirmed in either order, half the time with one
    /// more thing between; and each run asks for all it reserved. Nine events at
    /// most.
    fn pair(&mut self) {
        let now = self.events.len();
        let mut waiting = Vec::new();
        for _ in 0..2 {
            let other = self.seeded.below(2) == 0;
            let judged = self.seeded.below(now as u64 + 1) as usize;
            let micros = match self.seeded.below(2) {
                0 => self.books.balance().left,
                _ => self.some(13),
            };
            let run = self.new_run();
            let event = Event::Publish { run, micros };
            let event = match self.seeded.below(2) {
                0 => stale(judged, event),
                _ => event,
            };
            self.push(if other { second(event) } else { event });
            waiting.push((other, run, micros));
        }
        for _ in 0..self.seeded.below(3) {
            self.meanwhile();
        }
        if self.seeded.below(2) == 0 {
            waiting.reverse();
        }
        let wrap = |other: bool, event: Event| if other { second(event) } else { event };
        for (at, (other, run, _)) in waiting.iter().enumerate() {
            self.push(wrap(*other, Event::Confirm { run: *run }));
            if at == 0 && self.seeded.below(2) == 0 {
                self.meanwhile();
            }
        }
        for (other, run, micros) in waiting {
            self.push(wrap(other, Event::Dispatch { by: run, micros }));
        }
    }

    /// The events of the first shape, up to `length` of them: about one in four is
    /// other hands'. With `tree`, about one in four is a request of a run that
    /// another run called.
    fn mixed(&mut self, length: usize, tree: Option<bool>) {
        while self.events.len() < length {
            match (self.seeded.below(4), tree) {
                (0, _) => self.hands(length - self.events.len()),
                (1, Some(other)) => self.child_dispatch(other),
                _ => self.ledger(true),
            }
        }
    }
}

/// A generated history: at most [`GENERATED_EVENTS`] events, the same for the same
/// seed and number. About one event in four is other hands': names of every form
/// made, lost and damaged, whole chains among them, and holds in any epoch up to
/// three past the newest, for runs that have a reservation and runs that have none. Of the rest, about one
/// publication in five is judged late, and about one event in four happens in a
/// second process over the same store. One in four, by its number, begins with a
/// tree whose parent's request is settled over its bound while its child holds a
/// share, half of those in the second process; in those about one event in four
/// is a request of a run that another run called.
pub fn generated(seed: u64, number: u64) -> Vec<Event> {
    first_shape(Generating::new(seed, number), number)
}

fn first_shape(mut history: Generating, number: u64) -> Vec<Event> {
    let length = 1 + history.seeded.below(GENERATED_EVENTS as u64) as usize;
    let tree = (number % 4 == 3 && length > 5).then_some(number % 8 == 7);
    if let Some(other) = tree {
        history.overrun(other);
    }
    history.mixed(length, tree);
    history.events
}

/// A generated history of another shape, at most [`GENERATED_EVENTS`] events: what
/// the ledger alone does, in one process and in order, so that the books are ones
/// it could have written; then one to six things that other hands do; then one
/// to four things the ledger and its processes do, late ones and a second
/// process's among them. One in three begins with a tree whose parent's request is
/// settled over its bound while its child holds a share; about one time in four
/// what other hands do is to take away a whole settlement, as many of its names
/// as there is room for; and about one time in three what follows is a request of
/// a run that another run called, half of those judged late.
pub fn generated_tampered(seed: u64, number: u64) -> Vec<Event> {
    tampered(Generating::new(seed ^ 0x5EED_0F7A_3BE2, number))
}

fn tampered(mut history: Generating) -> Vec<Event> {
    // 14 at most, then 6 at most, then 4 at most: 24.
    let first = 3 + history.seeded.below(12) as usize;
    if history.seeded.below(3) == 0 {
        history.overrun(false);
    }
    let first = first.max(history.events.len());
    while history.events.len() < first {
        history.ledger(false);
    }
    let hands = first + 1 + history.seeded.below(6) as usize;
    while history.events.len() < hands {
        let room = hands - history.events.len();
        match room >= 2 && history.seeded.below(4) == 0 {
            true => history.strip(room),
            false => history.hands(room),
        }
    }
    let length = hands + 1 + history.seeded.below(4) as usize;
    while history.events.len() < length {
        match history.seeded.below(3) {
            0 => history.child_dispatch(false),
            _ => history.ledger(true),
        }
    }
    history.events
}

/// A generated history of a fourth shape, at most [`GENERATED_EVENTS`] events, the
/// same for the same seed and number: the owner sets an allowance and the ledger
/// alone acts, one in three with the tree whose parent's request is settled over
/// its bound; other hands take away one turn and make another, in either order, in
/// the newest epoch or in another; an admission that was judged before they did is
/// published, mostly with the turn that was lost, confirmed at once or apart, and
/// its run asks for all it reserved; then one to four things the ledger and its
/// processes do.
pub fn generated_turned(seed: u64, number: u64) -> Vec<Event> {
    let mut history = Generating::new(seed ^ 0x7012_4E5F_11ED, number);
    // 10 at most, then 3 at most, then 9 at most, then up to 24.
    let first = 3 + history.seeded.below(8) as usize;
    let allowance = 10 + history.seeded.below(40);
    match history.seeded.below(3) {
        0 => history.overrun(false),
        _ => history.push(reset(allowance)),
    }
    let first = first.max(history.events.len());
    while history.events.len() < first {
        history.ledger(false);
    }
    if history.seeded.below(4) == 0 {
        history.hands(1);
    }
    let moved = history.turns_moved();
    history.late_admission(moved);
    let more = 1 + history.seeded.below(4) as usize;
    let length = (history.events.len() + more).min(GENERATED_EVENTS);
    while history.events.len() < length {
        match history.seeded.below(3) {
            0 => history.child_dispatch(false),
            _ => history.ledger(true),
        }
    }
    history.events
}

/// A generated history of a fifth shape, at most [`GENERATED_EVENTS`] events, the
/// same for the same seed and number: the owner sets an allowance and the ledger
/// alone acts; half the time other hands move a turn; then two admissions are
/// published and wait together, in one process or in two, and are confirmed in
/// either order, with a crash, a recovery by either process, other hands or the
/// ledger between; each run asks for all it reserved; and the ledger goes on.
pub fn generated_paired(seed: u64, number: u64) -> Vec<Event> {
    let mut history = Generating::new(seed ^ 0x9A12_ED0F_7770, number);
    // 6 at most, then 2, then 9, then 3: 20.
    let allowance = 10 + history.seeded.below(40);
    history.push(reset(allowance));
    let first = 2 + history.seeded.below(5) as usize;
    while history.events.len() < first {
        history.ledger(false);
    }
    if history.seeded.below(2) == 0 {
        history.turns_moved();
    }
    history.pair();
    let length = history.events.len() + history.seeded.below(4) as usize;
    while history.events.len() < length {
        history.ledger(true);
    }
    history.events
}

/// What every amount of [`generated_large`] is multiplied by: a sixth of the
/// largest amount a name can state. An amount chosen from six up is the largest
/// there is, so that many are, one reservation can be of all an allowance has, and
/// two are over what a number of 64 bits holds.
pub const LARGE: u64 = (u64::MAX - 1) / 6;

/// A generated history with amounts near the largest a name can state, and places
/// other hands make near the largest a name can name: of the first shape for an
/// even number and of the second for an odd one, the same for the same seed and
/// number. At most [`GENERATED_EVENTS`] events.
pub fn generated_large(seed: u64, number: u64) -> Vec<Event> {
    let mut history = Generating::new(seed ^ 0x1A56_E0F7_11AD, number);
    history.scale = LARGE;
    match number % 2 {
        0 => first_shape(history, number / 2),
        _ => tampered(history),
    }
}

/// The most events a history of [`generated_whole`] has.
pub const WHOLE_EVENTS: usize = 16;

/// A generated history of a third shape, at most [`WHOLE_EVENTS`] events, the same
/// for the same seed and number: the owner sets an allowance, and then only the
/// ledger, one process and the owner act, in order. The books it leaves are ones the
/// ledger wrote alone, for a test that then changes one name of them.
pub fn generated_whole(seed: u64, number: u64) -> Vec<Event> {
    let mut history = Generating::new(seed ^ 0x0A11_B00C_5A1D, number);
    let allowance = 10 + history.seeded.below(40);
    history.push(reset(allowance));
    let length = 3 + history.seeded.below(WHOLE_EVENTS as u64 - 5) as usize;
    while history.events.len() < length {
        history.ledger(false);
    }
    // One in four ends as a process that died does: recovered, and where that left
    // a reservation interrupted, corrected by its owner, as the command allows.
    if history.seeded.below(4) == 0 {
        history.push(Event::Crash);
        history.push(Event::Recover);
        let folded = history.books.fold();
        let interrupted = |(_, one): &(&Run, &Standing)| one.state == State::Interrupted;
        if let Some((run, one)) = folded.reservations.iter().find(interrupted) {
            let room = one.most().saturating_sub(one.settled).min(40) as u64;
            let micros = one.settled as u64 + history.seeded.below(room + 1);
            history.push(Event::Adjust { run: *run, micros });
        }
    }
    history.events
}
