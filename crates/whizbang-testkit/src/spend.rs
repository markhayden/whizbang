//! A model of the ledger's books (ADR 0016): pure, with no store.
//!
//! The books are a set of names, as the store's generation holds them under `spend/`,
//! and a note of which of them have content that cannot be read. Every amount is in a
//! name. [`Books::apply`] takes one [`Event`] of a history and does what the ledger's
//! code is to do with it: it judges on the names, and publishes the names the event
//! makes, all of them or, when one of them is there already, none. [`fold`] answers
//! what is allowed, counted, charged and left, for the store, for one reservation and
//! for one run, from the names alone. A reservation counts by the ledger's arithmetic
//! only when its names are a set the ledger could have written, which a recogniser
//! decides; any other is frozen at the most its names can mean.
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
    /// Every epoch a hold of it names: it counts in each. With no hold it counts in
    /// every epoch there is.
    pub epochs: BTreeSet<u64>,
    /// What was reserved: the largest amount a hold of it names.
    pub amount: u64,
    /// What its settlements say was charged, unresolved ones at their bounds.
    pub charged: u128,
    /// The part of `charged` that settlements with their whole chain say: what a
    /// correction cannot go under.
    pub settled: u128,
    /// What its places count for beyond their charges.
    pub outstanding: u128,
    /// The part of `charged` and all of `outstanding` that no answer settled.
    pub unresolved: u128,
    /// The places under it that have no settlement with its whole chain.
    pub unsettled: u64,
    /// The places that nothing accounts for, and the numbers missing among its
    /// places: each may be a dispatch whose name is lost, of any bound up to the
    /// amount. None where the names are reachable.
    pub unexplained: u64,
    /// The largest amount a correction of it names.
    pub corrections: u64,
    /// Whether its names, corrections aside, are a set the ledger could have written.
    pub reachable: bool,
    /// What it counts for against the allowance of each epoch it counts in.
    pub counts: u128,
    pub state: State,
    pub closed: bool,
}

impl Standing {
    /// What is known against it: its charges, and what its places count for beyond.
    pub fn known(&self) -> u128 {
        self.charged + self.outstanding
    }

    /// The most its names can mean: its amount, or what is known and the amount once
    /// more for each place nothing accounts for, or a correction, whichever is most.
    pub fn most(&self) -> u128 {
        let exposed = self.known() + u128::from(self.unexplained) * u128::from(self.amount);
        u128::from(self.amount.max(self.corrections)).max(exposed)
    }

    /// What a dispatch under it may still take: nothing unless it is open.
    pub fn left(&self) -> u64 {
        if self.state != State::Open {
            return 0;
        }
        u64::try_from(u128::from(self.amount).saturating_sub(self.known())).unwrap_or(u64::MAX)
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

/// What the books can say of one run's spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// Its reservation's names are whole: this is what it sent.
    Known(RunSpend),
    /// Its reservation is frozen: which run sent what cannot be read, and no run
    /// under it sent more than this.
    Unknown { at_most: u128 },
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

    /// Whether an epoch's turns are other than exactly those its publications took:
    /// not 1 to their count, or more or fewer than the names account for.
    pub fn needs_owner(&self, epoch: u64) -> bool {
        let none = BTreeSet::new();
        let turns = self.turns.get(&epoch).unwrap_or(&none);
        let count = turns.len() as u64;
        let due = self.due.get(&epoch).copied().unwrap_or(0);
        !turns.iter().copied().eq(1..=count) || count != due
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
        let needs_owner = self.needs_owner(epoch);
        let left = match needs_owner {
            true => 0,
            false => u128::from(allowed).saturating_sub(counted),
        };
        Balance {
            epoch,
            allowed,
            counted,
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
        if of.closed.is_empty() || !own || !taken.insert(one.place) {
            return false;
        }
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
    for epoch in folded.resets.keys().filter(|epoch| **epoch > 0) {
        *folded.due.entry(*epoch - 1).or_default() += 1;
    }
    for (run, of) in found {
        // The amount and the epochs: the holds'.
        let amount = of.holds.iter().map(|(_, micros)| *micros).max();
        let amount = amount.unwrap_or(0);
        let epochs: BTreeSet<u64> = of.holds.iter().map(|(epoch, _)| *epoch).collect();
        for (epoch, _) in &of.holds {
            *folded.due.entry(*epoch).or_default() += 1;
        }
        let turns = (of.settling.len() + of.closed.len() + of.adjusted.len()) as u64;
        for epoch in &epochs {
            *folded.due.entry(*epoch).or_default() += turns;
        }
        let base = reachable(&of);
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
            let bounds: u128 = sent.iter().map(|(_, micros)| u128::from(*micros)).sum();
            let charge: u128 = ended.iter().map(|(_, micros)| u128::from(*micros)).sum();
            let open = ended
                .iter()
                .filter(|(stage, _)| *stage == Stage::Unresolved);
            let open: u128 = open.map(|(_, micros)| u128::from(*micros)).sum();
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
            // What the place counts for: with its whole chain, the charge; with
            // one dispatch, the larger of its bound and every charge named there;
            // with more, every bound and every charge, because each may be a
            // request of its own.
            let value = match sent.len() {
                _ if whole && base => charge,
                0 => charge,
                1 => bounds.max(charge),
                _ => bounds + charge,
            };
            let rest = value - charge;
            if whole {
                settled += charge;
            } else {
                unsettled += 1;
            }
            // A guarded charge over its one dispatch's bound took a place.
            if sent.len() == 1 && ended.len() == 1 && guarded && charge > bounds {
                overruns += 1;
            }
            charged += charge;
            outstanding += rest;
            unresolved += open + rest;
            // Every run a dispatch at this place names is named for all of it.
            let by: BTreeSet<Run> = sent.iter().map(|(by, _)| *by).collect();
            for by in by {
                let spend = folded.runs.entry(by).or_default();
                spend.charged += charge;
                spend.outstanding += rest;
                spend.unresolved += open + rest;
            }
        }
        // Where the names are not reachable: each place that no dispatch,
        // settlement, guarded overrun, closing or correction accounts for, and each
        // number missing below the highest place named, may be a dispatch whose
        // name is lost.
        let unexplained = if base {
            0
        } else {
            let bare = of.places.iter().filter(|place| !at.contains(place));
            let own =
                |one: &&Correction| of.places.contains(&one.place) && !at.contains(&one.place);
            let due = overruns
                + of.closed.len().min(1) as u64
                + of.adjusted.iter().filter(own).count() as u64;
            let highest = of.places.last().copied().unwrap_or(0);

            let missing = (1..=highest).filter(|place| !of.places.contains(place));
            (bare.count() as u64).saturating_sub(due) + missing.count() as u64
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
            counts: 0,
            state: State::Open,
            closed,
        };
        (one.state, one.counts) = match stands {
            // A correction never counts under what settlements with their whole
            // chain say.
            Some(micros) => (State::Adjusted, u128::from(micros).max(settled)),
            None if !base => (State::Frozen, one.most()),
            // A correction that no longer stands lowers nothing, and its
            // reservation is never released.
            None if !of.adjusted.is_empty() && of.closed == [true] => (State::Frozen, one.most()),
            None if of.closed == [true] => (State::Released, one.known()),
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
    /// The epoch's turns are not those its publications took: the owner resets.
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

/// What one process holds in memory: a crash loses it.
#[derive(Debug, Clone, Default)]
struct Memory {
    live: BTreeMap<Run, Live>,
    sent: Vec<Request>,
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
/// from a name. Every request is kept, two at one place among them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Truth {
    epoch: u64,
    amount: u64,
    asked: Vec<Asked>,
    /// The names and the damage of the reservation as the owner's last correction
    /// left them.
    corrected: Option<(BTreeSet<Name>, BTreeSet<Name>)>,
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
    /// The memory of the process that is not acting.
    other: Memory,
    /// Every run id that was given, with the run at the root of its tree.
    ids: BTreeMap<Run, Run>,
    /// The truth of each reservation the ledger made, and of each epoch's allowance.
    truth: BTreeMap<Run, Truth>,
    allowed: BTreeMap<u64, u64>,
    /// The names and the damage after each event, for what is judged late.
    past: Vec<(BTreeSet<Name>, BTreeSet<Name>)>,
    /// The names other hands made, and the names of the ledger's that were lost,
    /// each with the place of its event in the history; and the place of every
    /// event in which other hands did anything.
    planted: BTreeMap<Name, usize>,
    removed: BTreeMap<Name, usize>,
    hands: Vec<usize>,
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
        let mut turns = Turns {
            folded: &folded,
            taken: BTreeMap::new(),
        };
        let newest = folded.epoch();
        match event {
            Event::Reset { micros } => {
                // A reset closes the newest epoch with a turn in it, so that an
                // admission judged before it is asked again.
                let epoch = newest + 1;
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
            Event::Admit { run, micros } => {
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
                self.ids.insert(*run, *run);
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
                self.hands.push(self.past.len());
                Outcome::Done
            }
            Event::Plant(name) => {
                if self.names.insert(name.clone()) {
                    self.planted.insert(name.clone(), self.past.len());
                    // A name of the ledger's that was lost and is made again as it
                    // was is no longer lost.
                    self.removed.remove(name);
                }
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
                    if let Name::Settled {
                        run, place, micros, ..
                    } = name
                    {
                        let asked = self.truth.get_mut(run).map(|truth| truth.asked.iter_mut());
                        let said = |asked: &&mut Asked| {
                            asked.place == *place
                                && asked.recorded
                                && asked.settled == Some(*micros)
                        };
                        if let Some(asked) = asked.and_then(|mut asked| asked.find(said)) {
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
            Event::Admit { run, .. } => !self.ids.contains_key(run),
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
                    count(&closing) == 1
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
        damaged.is_empty()
            && count(&|name| matches!(name, Name::Reservation(_))) == 1
            && count(&|name| matches!(name, Name::Held { .. })) == 1
            && places.iter().copied().eq(1..=places.len() as u64)
            && places.len() == count(&dispatch) + overruns + count(&closing) + count(&correction)
    }

    /// Whether no name states an amount for this request: every hold of its
    /// reservation is lost, and its dispatch is lost too.
    fn unnamed(&self, root: Run, asked: &Asked) -> bool {
        let hold = |name: &Name| matches!(name, Name::Held { run, .. } if *run == root);
        let dispatch = |name: &Name| matches!(name, Name::Dispatched { run, place, .. } if *run == root && *place == asked.place);
        !self.names.iter().any(hold) && self.removed.keys().any(dispatch)
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
    /// hands touched and that is believed counts as the fold has it. None where the
    /// allowance itself is other hands' or is lost.
    fn really_left(&self, folded: &Folded) -> Option<u128> {
        let newest = self.allowed.keys().next_back().copied().unwrap_or(0);
        let of_allowance = |name: &Name| match name {
            Name::Reset { epoch, .. } | Name::Epoch(epoch) => *epoch >= newest,
            _ => false,
        };
        if folded.epoch() != newest || self.planted.keys().any(of_allowance) {
            return None;
        }
        let mut counted = 0u128;
        for (root, truth) in self.truth.iter().filter(|(_, truth)| truth.epoch == newest) {
            counted += match self.held_to(*root, truth, folded) {
                Ok(Some(floor)) => floor,
                _ => folded.reservations.get(root).map_or(0, |one| one.counts),
            };
        }
        let allowed = self.allowed.get(&newest).copied().unwrap_or(0);
        Some(u128::from(allowed).saturating_sub(counted))
    }

    /// What one reservation's count is held to, from the truth: the floor of every
    /// request really sent under it. None where the checker believes the names
    /// instead, which is only where a stated limit says so (ADR 0016, "Limits").
    fn held_to(&self, root: Run, truth: &Truth, folded: &Folded) -> Result<Option<u128>, String> {
        let one = folded.reservations.get(&root);
        let (own, damaged) = self.own(root);
        // The owner's correction stands while the names are those it left: it
        // writes off what is outstanding, and what a place says that other hands
        // touched. That is its meaning, and no limit.
        let corrected = truth.corrected.as_ref() == Some(&(own.clone(), damaged));
        let hurt = |place: u64| {
            let here = |name: &Name| name.of() == Some(root) && name.place() == Some(place);
            let touched = self.planted.keys().chain(self.removed.keys()).any(here);
            touched || self.damaged.iter().any(here)
        };
        // With every hold lost, a request whose dispatch is lost too has no name
        // that states an amount.
        let lost = |asked: &Asked| self.unnamed(root, asked);
        let whole = |asked: &Asked| asked.settled.is_some() && asked.recorded && !hurt(asked.place);
        let counted = |asked: &&Asked| {
            if corrected {
                whole(asked)
            } else {
                !lost(asked)
            }
        };
        let floor: u128 = truth.asked.iter().filter(counted).map(Asked::floor).sum();
        if corrected {
            return Ok(Some(floor));
        }
        if !self.touched(root) {
            return Ok(Some(floor));
        }
        match one {
            // Every name of it is lost: a loss that leaves a reachable set.
            None if own.is_empty() => Ok(None),
            None => Err(format!("{root}: no reservation is folded for it")),
            Some(one) if one.state == State::Frozen => Ok(Some(floor)),
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
    /// of two states is held where a history is run ([`run_watched`]).
    ///
    /// What it leaves out is the four limits of ADR 0016 and nothing else.
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
            // The epoch is in the hold: with the ledger's hold lost it is not held.
            let hold = |name: &Name| matches!(name, Name::Held { run, .. } if run == root);
            if !one.counts_in(truth.epoch) && !self.removed.keys().any(hold) {
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
                .filter(|asked| !self.unnamed(*root, asked))
            {
                *sent.entry(asked.by).or_default() += asked.floor();
            }
            for (run, really) in sent {
                let found = match folded.sent_by(run, *root) {
                    Sent::Known(spend) => spend.charged + spend.outstanding,
                    Sent::Unknown { at_most } => at_most,
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
            // released later, unless other hands touched its names.
            let before = self.was.insert(*run, one.state);
            let later = before == Some(State::Interrupted) && one.state == State::Released;
            if later && !self.touched(*run) {
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
        let over = u128::from(balance.left) + balance.counted > u128::from(balance.allowed);
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
        let before = books.fold();
        let really_left = books.really_left(&before);
        let allowances: BTreeMap<Run, Live> = books
            .all_open()
            .map(|(run, live)| (*run, live.clone()))
            .collect();
        let (_, judged, plain) = event.plain();
        // The reservation a dispatch draws on: what is really left of it, whether a
        // closing names it, and whether other hands touched it.
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
        let believed = drawn.is_some_and(|root| books.touched(root));
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
        // names it makes.
        let unseen = judged.filter(|_| landed).is_some_and(|judged| {
            let since = |when: &usize| *when >= judged && *when < at;
            books.hands.iter().any(since)
        });
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
        // written: a reservation that lost one, and that no other hands touched, is
        // frozen and counts for no less than it did. Two names are the only place
        // of their amount: a settlement over its bound leaves no less than the
        // amount; and the hold leaves what is known.
        if let (Event::Remove(name), Outcome::Done) = (plain, &outcome)
            && books.removed.contains_key(name)
        {
            let own = |name: &&Name| name.of() == plain_run(plain);
            let alone = books.removed.keys().filter(own).count() == 1
                && books.planted.keys().filter(own).count() == 0;
            let was = name.of().and_then(|run| before.reservations.get(&run));
            let is = name.of().and_then(|run| after.reservations.get(&run));
            if let (Some(was), Some(is), true) = (was, is, alone) {
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
                    _ if was.state == State::Adjusted => 0,
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
            && !unseen
        {
            // No dispatch is published once a closing names its reservation,
            // whenever it was judged.
            if let Some(closing) = closing {
                return Err(broke(format!("dispatched after {closing}")));
            }
            // And none that what was really reserved does not really cover, unless
            // the reservation's names are other hands' and are believed.
            let room = room.unwrap_or(0);
            if u128::from(*micros) > room && !believed {
                return Err(broke(format!("dispatched, and really {room} was left")));
            }
        }
        // 6. No admission is published that what is really left of the allowance
        // does not cover; and none as it was judged once the ledger has taken a
        // turn in its epoch since.
        if let Event::Admit { micros, .. } = plain
            && done
            && !unseen
        {
            if let Some(really) = really_left.filter(|really| u128::from(*micros) > *really) {
                return Err(broke(format!("admitted, and really {really} was left")));
            }
            let epoch = before.epoch();
            let since = |(when, of): &(usize, u64)| {
                judged.is_some_and(|judged| *when >= judged && *when < at) && *of == epoch
            };
            if landed && books.turned.iter().any(since) {
                return Err(broke("admitted on a stale balance".into()));
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
        outcomes.push(outcome);
        watch(at, &books, &outcomes).map_err(broke)?;
    }
    Ok((books, outcomes))
}

/// The run whose reservation the name of a loss names.
fn plain_run(event: &Event) -> Option<Run> {
    match event {
        Event::Remove(name) => name.of(),
        _ => None,
    }
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
    /// What this run sent is unknown, and at most this.
    SentAtMost(Run, u64),
    /// The newest epoch's turns are not those its publications took.
    NeedsOwner,
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
            Must::Sent(run, micros) => match books.run(*run) {
                Sent::Known(spend) => {
                    let found = spend.charged + spend.outstanding;
                    same(&format!("{run} sent"), found, *micros)
                }
                found => Err(format!("{run} sent {found:?}, and it must be known")),
            },
            Must::SentAtMost(run, micros) => match books.run(*run) {
                Sent::Unknown { at_most } => same(&format!("{run} sent at most"), at_most, *micros),
                found => Err(format!("{run} sent {found:?}, and it must be unknown")),
            },
            Must::NeedsOwner => match balance.needs_owner {
                true => Ok(()),
                false => Err("the store does not need its owner".to_owned()),
            },
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
            // Two dispatches at one place may each be a request: 5 and 8.
            Counts(r(1), 13),
            state(r(2), State::Frozen),
            Counts(r(2), 10),
            state(r(3), State::Open),
            Counts(r(3), 5),
            Left(2),
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
            Counts(r(1), 10),
            Counted(10),
        ],
        vec![
            (4, state(r(1), State::Interrupted)),
            (5, state(r(1), State::Frozen)),
            (5, Counts(r(1), 10)),
            // A correction other hands made took no turn: the owner is needed.
            (5, Must::NeedsOwner),
            (6, state(r(1), State::Adjusted)),
            (6, Counts(r(1), 0)),
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
        vec![
            state(r(1), State::Frozen),
            Epoch(2),
            Counted(10),
            // The hold other hands made in the new epoch took no turn there.
            Must::NeedsOwner,
            Left(0),
        ],
        vec![(3, Epoch(1)), (3, Counted(10)), (3, Left(10))],
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
            Counts(r(1), 20),
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
            Counts(r(1), 10),
            Outstanding(r(1), 5),
            Counted(10),
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
            Counts(r(1), 10),
            Must::SentAtMost(r(1), 10),
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
            Counts(r(1), 10),
            Counted(10),
        ],
        vec![(5, state(r(1), State::Frozen)), (5, Counts(r(1), 10))],
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
            Counts(r(1), 10),
            Charged(r(1), 5),
        ],
        vec![
            (7, state(r(1), State::Adjusted)),
            (7, Counts(r(1), 0)),
            (8, state(r(1), State::Frozen)),
            (8, Counts(r(1), 10)),
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
            // 3 and 4 and 8, and the amount for the place the charge took.
            Counts(r(1), 25),
            Must::SentAtMost(r(1), 25),
        ],
        vec![(4, state(r(1), State::Frozen)), (5, Counts(r(1), 10))],
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
         admission that judged before the loss is published",
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
            came(5, Outcome::Done),
            came(6, refused(Why::NeedsOwner)),
            came(8, Outcome::Done),
            Epoch(2),
            Left(29),
        ],
        vec![(4, Must::NeedsOwner), (4, Left(0)), (6, Must::NeedsOwner)],
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
        vec![state(r(1), State::Frozen), Counts(r(1), 16)],
        vec![(5, Counts(r(1), 15))],
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
            Counts(r(1), 10),
            Must::SentAtMost(r(1), 10),
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
        vec![state(r(1), State::Frozen), Counts(r(1), 10), Counted(10)],
        vec![(4, state(r(1), State::Frozen)), (4, Counts(r(1), 10))],
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
}

impl Generating {
    fn new(seed: u64, number: u64) -> Generating {
        Generating {
            seeded: Seeded::new(seed ^ number.wrapping_mul(0xA24B_AED4_963E_E407)),
            books: Books::new(),
            events: Vec::new(),
            taken: Vec::new(),
            made: 0,
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
            0 => Some(reset(self.seeded.below(40))),
            1..=3 => None,
            4 | 5 => self.seeded.pick(&live).map(|parent| Event::Child {
                parent,
                child: self.new_run(),
                micros: self
                    .seeded
                    .below(self.books.spendable(parent).unwrap_or(0) + 2),
            }),
            6..=9 => self.seeded.pick(&live).map(|by| Event::Dispatch {
                by,
                micros: self.seeded.below(8),
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
                        _ => How::Answered(Some(self.seeded.below(15))),
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
                    micros: self.seeded.below(15) / (1 + self.seeded.below(2) * 5),
                })
            }
            20 => match self.seeded.below(2) {
                0 => self.seeded.pick(&live).map(|run| Event::Take {
                    run,
                    micros: self
                        .seeded
                        .below(self.books.spendable(run).unwrap_or(0) + 1),
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
                    micros: self.seeded.below(15),
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
                micros: self.seeded.below(13),
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
        let names: Vec<Name> = self.books.names().iter().cloned().collect();
        let folded = self.books.fold();
        let roots: Vec<Run> = folded.reservations.keys().copied().collect();
        let run = Run(1 + self.seeded.below(8) as u32);
        let place = 1 + self.seeded.below(4);
        let micros = self.seeded.below(15);
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
                    _ => self.seeded.below(6),
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
                0 => Name::Held { epoch, run, micros },
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
                    micros: self.seeded.below(60),
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
}

/// A generated history: at most [`GENERATED_EVENTS`] events, the same for the same
/// seed and number. About one event in four is other hands': names of every form
/// made, lost and damaged, whole chains among them. Of the rest, about one
/// publication in five is judged late, and about one event in four happens in a
/// second process over the same store.
pub fn generated(seed: u64, number: u64) -> Vec<Event> {
    let mut history = Generating::new(seed, number);
    let length = 1 + history.seeded.below(GENERATED_EVENTS as u64) as usize;
    while history.events.len() < length {
        match history.seeded.below(4) {
            0 => history.hands(length - history.events.len()),
            _ => history.ledger(true),
        }
    }
    history.events
}

/// A generated history of another shape, at most [`GENERATED_EVENTS`] events: what
/// the ledger alone does, in one process and in order, so that the books are ones
/// it could have written; then one to three things that other hands do; then one
/// to four things the ledger and its processes do, late ones and a second
/// process's among them.
pub fn generated_tampered(seed: u64, number: u64) -> Vec<Event> {
    let mut history = Generating::new(seed ^ 0x5EED_0F7A_3BE2, number);
    let first = 3 + history.seeded.below(13) as usize;
    while history.events.len() < first {
        history.ledger(false);
    }
    let hands = first + 1 + history.seeded.below(3) as usize;
    while history.events.len() < hands {
        history.hands(hands - history.events.len());
    }
    let length = hands + 1 + history.seeded.below(4) as usize;
    while history.events.len() < length {
        history.ledger(true);
    }
    history.events
}
