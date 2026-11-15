//! A reading of the names under `spend/` (ADR 0016): each entry by itself, sound or
//! damaged, and what the names of one reservation say.
//!
//! [`Reading::of`] sorts every name a generation holds under `spend/`: the names of
//! each reservation ([`Names`]), the names that belong to an epoch, and the texts
//! that are no name. No name makes it fail: a content that cannot be read, or that
//! does not say what its name says, is damage and is noted beside its name.
//!
//! [`Names`] is one reservation: the recogniser ([`Names::reachable`]), which takes
//! only a set the ledger could have written and replays what the ledger checked; the
//! sums of what its names say; and the most they can mean ([`Names::most`]), a sum
//! with no credits in which the places that hold no dispatch are counted and never
//! walked. What is across reservations (epochs and their allowances, turns, whether
//! a correction stands, a reservation's state, the store's balance) is the fold's
//! (`spend::fold`, task S1b), which is built on this and answers in the shapes
//! defined here: [`Counted`] and [`Sent`].
//!
//! Every sum is of 128 bits and stops at the largest number ([`Sum`]).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::capability::Stage;
use crate::contract::{Reservation, RunId};
use crate::store::{self, Entry, Generation};

use super::name::{Amount, Belongs, EpochWord, Name, NotAName, SPEND_PREFIX, Seal};
use super::record::{Record, SPEND_RECORD_MAX_BYTES};

/// A sum of amounts: of 128 bits, and it stops at the largest number there is, which
/// is over any allowance. Nothing here wraps and nothing fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Sum(u128);

impl Sum {
    pub const ZERO: Sum = Sum(0);

    /// Where a sum stops.
    pub const LARGEST: Sum = Sum(u128::MAX);

    pub fn new(micros: u128) -> Sum {
        Sum(micros)
    }

    pub fn of(amount: Amount) -> Sum {
        Sum(u128::from(amount.micros()))
    }

    pub fn micros(self) -> u128 {
        self.0
    }

    /// Whether the sum stopped: it is the largest number, and what it stands for may
    /// be more.
    pub fn stopped(self) -> bool {
        self == Sum::LARGEST
    }

    #[must_use]
    pub fn plus(self, other: Sum) -> Sum {
        Sum(self.0.saturating_add(other.0))
    }

    /// This sum, less another, or nothing.
    #[must_use]
    pub fn less(self, other: Sum) -> Sum {
        Sum(self.0.saturating_sub(other.0))
    }

    #[must_use]
    pub fn times(self, count: u64) -> Sum {
        Sum(self.0.saturating_mul(u128::from(count)))
    }

    /// The sum of these.
    pub fn total(amounts: impl IntoIterator<Item = Amount>) -> Sum {
        amounts
            .into_iter()
            .fold(Sum::ZERO, |all, one| all.plus(Sum::of(one)))
    }

    /// The sum these decimal digits are: no leading zero, no sign, 128 bits at most.
    pub fn parse(text: &str) -> Option<Sum> {
        let plain = !text.is_empty()
            && text.len() <= 39
            && text.bytes().all(|byte| byte.is_ascii_digit())
            && (text == "0" || !text.starts_with('0'));
        plain.then(|| text.parse().ok()).flatten().map(Sum)
    }
}

impl fmt::Display for Sum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What a reservation, or an epoch, counts for, as far as the books can say: the
/// first two of the three shapes of a reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counted {
    /// A number.
    Exactly(Sum),
    /// Unknown, and at least this: a name that stated an amount may be the one that
    /// is lost.
    AtLeast(Sum),
}

impl Counted {
    /// The number: what is counted, or the least it can be.
    pub fn least(self) -> Sum {
        match self {
            Counted::Exactly(sum) | Counted::AtLeast(sum) => sum,
        }
    }
}

/// What one run sent under its tree's reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunSpend {
    pub charged: Sum,
    pub outstanding: Sum,
    pub unresolved: Sum,
}

impl RunSpend {
    /// What it sent that is charged or outstanding.
    pub fn known(&self) -> Sum {
        self.charged.plus(self.outstanding)
    }
}

/// What the books can say of one run's spend: a number, "unknown, at most", or
/// "unknown" with no most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// Its reservation's names are a set the ledger could have written: this is what
    /// it sent.
    Known(RunSpend),
    /// Its reservation is frozen: which run sent what cannot be read, and no run
    /// under it sent more than this.
    Unknown { at_most: Sum },
    /// Its reservation is frozen and its amount cannot be read either: no most can
    /// be said.
    Unbounded,
}

/// Whether a settlement's stage allows its charge for this bound (ADR 0010): not
/// sent, nothing; unresolved, the bound; answered, any charge.
pub fn stage_allows(stage: Stage, charge: Amount, bound: Amount) -> bool {
    match stage {
        Stage::NotSent => charge == Amount::ZERO,
        Stage::Unresolved => charge == bound,
        Stage::Answered => true,
    }
}

/// The lowest number from 1 that is not among these.
pub fn lowest_free(taken: &BTreeSet<u64>) -> u64 {
    let mut free = 1u64;
    for one in taken {
        if *one == free {
            free = free.saturating_add(1);
        }
    }
    free
}

/// A correction, as its name says it, with what a fold needs to say whether it
/// stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub name: Name,
    pub place: u64,
    pub turn: u64,
    pub seal: Seal,
    pub micros: Amount,
    /// Its own content cannot be read, or does not say what its name says.
    pub damaged: bool,
    /// What its seal has to be for it to stand: the seal of every other name of the
    /// reservation as they are, the guard of its own place aside.
    pub sealed_over: Seal,
    /// The reservation has a closing; the place it names is there, not damaged, and
    /// holds nothing else: no dispatch, no settlement and no settlement's guard.
    pub at_its_place: bool,
}

impl Correction {
    /// Whether it stands, but for its turn: it is not damaged, it is at a place of
    /// its own under a closed reservation, and its seal is that of the names as they
    /// are. A fold adds the one thing a reservation's names do not hold: that the
    /// turn it names is there, in the epoch of the reservation's hold.
    pub fn stands_but_for_its_turn(&self) -> bool {
        !self.damaged && self.at_its_place && self.seal == self.sealed_over
    }
}

/// What one place under a reservation counts for, and for whom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtPlace {
    pub place: u64,
    /// Every run a dispatch at this place names: each is named for all of it.
    pub by: BTreeSet<RunId>,
    /// What the settlements named here say was charged.
    pub charged: Sum,
    /// What it counts for beyond its charges: nothing where its one dispatch is
    /// settled with its whole chain under a reachable reservation, and otherwise
    /// every bound named here.
    pub outstanding: Sum,
    /// The part of both that no answer settled.
    pub unresolved: Sum,
    /// One dispatch with its place, one settlement with its guard at a charge its
    /// stage allows, none of the four damaged.
    pub whole: bool,
}

/// The names of one reservation: every name that names its run where a reservation's
/// run is named, of a form or of none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    run: RunId,
    all: Vec<(Name, bool)>,
    formless: Vec<String>,
    record: bool,
    holds: Vec<(u64, Amount)>,
    places: BTreeSet<u64>,
    dispatched: BTreeMap<u64, Vec<(RunId, Amount)>>,
    settling: BTreeSet<u64>,
    settled: BTreeMap<u64, Vec<(Stage, Amount)>>,
    hurt: BTreeSet<u64>,
    closed: Vec<bool>,
    adjusted: Vec<(Name, bool)>,
    damaged: bool,
    reachable: bool,
    at: Vec<AtPlace>,
}

impl Names {
    /// The reservation these names are: each name of `run` with whether its content
    /// is damaged, and the texts of no form that belong to it. A name of another
    /// run, or of none, is left out.
    pub fn of(
        run: &RunId,
        names: impl IntoIterator<Item = (Name, bool)>,
        formless: impl IntoIterator<Item = String>,
    ) -> Names {
        let mut of = Names {
            run: run.clone(),
            all: Vec::new(),
            formless: formless.into_iter().collect(),
            record: false,
            holds: Vec::new(),
            places: BTreeSet::new(),
            dispatched: BTreeMap::new(),
            settling: BTreeSet::new(),
            settled: BTreeMap::new(),
            hurt: BTreeSet::new(),
            closed: Vec::new(),
            adjusted: Vec::new(),
            damaged: false,
            reachable: false,
            at: Vec::new(),
        };
        of.formless.sort();
        of.formless.dedup();
        let mut all: Vec<(Name, bool)> = names
            .into_iter()
            .filter(|(name, _)| name.run() == Some(run))
            .collect();
        all.sort();
        all.dedup_by(|later, first| {
            let same = later.0 == first.0;
            first.1 |= same && later.1;
            same
        });
        for (name, hurt) in &all {
            of.damaged |= *hurt;
            if let Some(place) = name.place().filter(|_| *hurt) {
                of.hurt.insert(place.get());
            }
            match name {
                Name::Turn { .. } | Name::Reset { .. } | Name::Epoch(_) => {}
                Name::Reservation(_) => of.record = true,
                Name::Held { epoch, micros, .. } => of.holds.push((*epoch, *micros)),
                Name::Place { place, .. } => {
                    of.places.insert(place.get());
                }
                Name::Dispatched {
                    place, by, micros, ..
                } => of
                    .dispatched
                    .entry(place.get())
                    .or_default()
                    .push((by.clone(), *micros)),
                Name::Settling { place, .. } => {
                    of.settling.insert(place.get());
                }
                Name::Settled {
                    place,
                    stage,
                    micros,
                    ..
                } => of
                    .settled
                    .entry(place.get())
                    .or_default()
                    .push((*stage, *micros)),
                Name::Closed { released, .. } => of.closed.push(*released),
                Name::Adjusted { .. } => of.adjusted.push((name.clone(), *hurt)),
            }
        }
        of.all = all;
        of.reachable = of.recognise();
        of.at = of.count_places();
        of
    }

    /// The recogniser (ADR 0016, "What the ledger can have written"): whether these
    /// names, corrections' seals aside, are exactly a set that some sequence of the
    /// ledger's own publications makes, with nothing damaged.
    fn recognise(&self) -> bool {
        // An admission made one record and one hold; no name is of another form, and
        // none is damaged.
        if self.damaged || !self.formless.is_empty() || !self.record || self.holds.len() != 1 {
            return false;
        }
        // The places taken are 1 to their count.
        let count = self.places.len() as u64;
        if !self.places.iter().copied().eq(1..=count) {
            return false;
        }
        let mut overruns = 0u64;
        for (place, sent) in &self.dispatched {
            // A dispatch is alone at its place, with the place.
            let [(_, bound)] = sent.as_slice() else {
                return false;
            };
            if !self.places.contains(place) {
                return false;
            }
            // Its settlement is one, with its guard, at a charge its stage allows.
            match self.settled.get(place).map(Vec::as_slice) {
                None => {
                    if self.settling.contains(place) {
                        return false;
                    }
                }
                Some([(stage, charge)]) => {
                    if !self.settling.contains(place) || !stage_allows(*stage, *charge, *bound) {
                        return false;
                    }
                    overruns += u64::from(charge > bound);
                }
                Some(_) => return false,
            }
        }
        // A settlement is of a dispatch.
        let mut ended = self.settled.keys().chain(&self.settling);
        if ended.any(|place| !self.dispatched.contains_key(place)) {
            return false;
        }
        // One closing at most, and one that released has every dispatch settled.
        if self.closed.len() > 1 {
            return false;
        }
        let unsettled = |place: &u64| !self.settled.contains_key(place);
        if self.closed == [true] && self.dispatched.keys().any(unsettled) {
            return false;
        }
        // A correction is of a reservation closed as held, at a place of its own.
        let mut taken = BTreeSet::new();
        for (name, _) in &self.adjusted {
            let Name::Adjusted { place, .. } = name else {
                continue;
            };
            let place = place.get();
            let own = self.places.contains(&place) && !self.dispatched.contains_key(&place);
            if self.closed != [false] || !own || !taken.insert(place) {
                return false;
            }
        }
        // What the ledger checked, replayed in the order of the places: each
        // dispatch fit what the reservation had left when it took its place. A place
        // below a dispatch that holds none was taken by a charge over its bound, of
        // a request sent before it: nothing is dispatched after a closing. What the
        // names do not say is when a request was settled under its bound, and which
        // charge over its bound took which place: there the replay takes what
        // leaves the most, a request at the least of its bound and its charge from
        // the start, and the smallest excess first.
        let amount = self
            .holds
            .first()
            .map_or(Sum::ZERO, |(_, held)| Sum::of(*held));
        let last = self.dispatched.keys().next_back().copied().unwrap_or(0);
        let mut known = Sum::ZERO;
        let mut excesses: BTreeSet<(u64, u64)> = BTreeSet::new();
        // The places are 1 to their count, and each is a name: this walks names.
        for place in self.places.range(..=last) {
            let Some([(_, bound)]) = self.dispatched.get(place).map(Vec::as_slice) else {
                let Some((excess, _)) = excesses.pop_first() else {
                    return false;
                };
                known = known.plus(Sum::new(u128::from(excess)));
                continue;
            };
            if Sum::of(*bound) > amount.less(known) {
                return false;
            }
            let charge = self.settled.get(place).and_then(|ended| ended.first());
            let charge = charge.map_or(*bound, |(_, charge)| *charge);
            known = known.plus(Sum::of(charge.min(*bound)));
            if charge > *bound {
                excesses.insert((charge.micros() - bound.micros(), *place));
            }
        }
        // A correction follows the closing, which follows every dispatch: the
        // closing's place lies between the last dispatch and the lowest place a
        // correction names.
        if taken
            .first()
            .is_some_and(|lowest| *lowest <= last.saturating_add(1))
        {
            return false;
        }
        // Every place that is not a dispatch's was taken by a charge over its bound,
        // by the closing or by a correction.
        let bare = count - self.dispatched.len() as u64;
        bare == overruns + self.closed.len() as u64 + self.adjusted.len() as u64
    }

    fn count_places(&self) -> Vec<AtPlace> {
        let at: BTreeSet<u64> = self
            .dispatched
            .keys()
            .chain(self.settled.keys())
            .copied()
            .collect();
        let mut counted = Vec::new();
        for place in at {
            let sent = self.dispatched.get(&place).map_or(&[][..], Vec::as_slice);
            let ended = self.settled.get(&place).map_or(&[][..], Vec::as_slice);
            let bounds = Sum::total(sent.iter().map(|(_, micros)| *micros));
            let charged = Sum::total(ended.iter().map(|(_, micros)| *micros));
            let open = ended
                .iter()
                .filter(|(stage, _)| *stage == Stage::Unresolved);
            let open = Sum::total(open.map(|(_, micros)| *micros));
            let whole = match (sent, ended) {
                ([(_, bound)], [(stage, charge)]) => {
                    self.places.contains(&place)
                        && self.settling.contains(&place)
                        && !self.hurt.contains(&place)
                        && stage_allows(*stage, *charge, *bound)
                }
                _ => false,
            };
            // Where the names are reachable, a place is settled or it is not:
            // nothing, or its bound. Where they are not, no settlement gives up a
            // bound: every bound named there, beside every charge.
            let outstanding = match whole && self.reachable {
                true => Sum::ZERO,
                false => bounds,
            };
            counted.push(AtPlace {
                place,
                by: sent.iter().map(|(by, _)| by.clone()).collect(),
                charged,
                outstanding,
                unresolved: open.plus(outstanding),
                whole,
            });
        }
        counted
    }

    /// The run whose reservation this is.
    pub fn run(&self) -> &RunId {
        &self.run
    }

    /// Every name of a form, in the order of the names, each with whether it is
    /// damaged.
    pub fn all(&self) -> &[(Name, bool)] {
        &self.all
    }

    /// The texts of no form that belong to this reservation.
    pub fn formless(&self) -> &[String] {
        &self.formless
    }

    /// How many names name its run, of any form and of none: two, the record and one
    /// hold, where an admission's publication made them and nothing was made under it
    /// since.
    pub fn count(&self) -> u64 {
        (self.all.len() + self.formless.len()) as u64
    }

    /// Whether the executor's record is there.
    pub fn has_record(&self) -> bool {
        self.record
    }

    /// Whether any name of it is damaged.
    pub fn damaged(&self) -> bool {
        self.damaged
    }

    /// Each hold: the epoch it names and the amount it states.
    pub fn holds(&self) -> &[(u64, Amount)] {
        &self.holds
    }

    /// The epoch it falls under: the one its one hold names. None when it has no
    /// hold, or more than one.
    pub fn epoch(&self) -> Option<u64> {
        match self.holds.as_slice() {
            [(epoch, _)] => Some(*epoch),
            _ => None,
        }
    }

    /// Every epoch a hold of it names.
    pub fn epochs(&self) -> BTreeSet<u64> {
        self.holds.iter().map(|(epoch, _)| *epoch).collect()
    }

    /// What was reserved: the largest amount a hold of it names, or nothing.
    pub fn amount(&self) -> Amount {
        let most = self.holds.iter().map(|(_, micros)| *micros).max();
        most.unwrap_or(Amount::ZERO)
    }

    /// The amount its hold's name states, when exactly one hold does.
    pub fn stated(&self) -> Option<Amount> {
        match self.holds.as_slice() {
            [(_, micros)] => Some(*micros),
            _ => None,
        }
    }

    /// How many turns its names took beside its holds': one for each settlement's
    /// guard, each closing and each correction.
    pub fn turns_beside_holds(&self) -> u64 {
        (self.settling.len() + self.closed.len() + self.adjusted.len()) as u64
    }

    /// Each closing: whether it released.
    pub fn closings(&self) -> &[bool] {
        &self.closed
    }

    /// Whether any closing names it.
    pub fn closed(&self) -> bool {
        !self.closed.is_empty()
    }

    /// Whether its one closing is the one that released.
    pub fn released(&self) -> bool {
        self.closed == [true]
    }

    /// Whether its names, corrections' seals aside, are a set the ledger could have
    /// written (the recogniser).
    pub fn reachable(&self) -> bool {
        self.reachable
    }

    /// Each place that holds a dispatch or a settlement, in order, with what it
    /// counts for and the runs its dispatches name.
    pub fn at(&self) -> &[AtPlace] {
        &self.at
    }

    /// What its settlements say was charged.
    pub fn charged(&self) -> Sum {
        let charged = self.at.iter().map(|one| one.charged);
        charged.fold(Sum::ZERO, Sum::plus)
    }

    /// The part of what was charged that settlements with their whole chain say:
    /// what a correction cannot go under.
    pub fn settled(&self) -> Sum {
        let whole = self.at.iter().filter(|one| one.whole);
        whole.fold(Sum::ZERO, |all, one| all.plus(one.charged))
    }

    /// The bounds of its dispatches that are not settled; and where its names are
    /// not reachable, the bound of every dispatch named.
    pub fn outstanding(&self) -> Sum {
        let outstanding = self.at.iter().map(|one| one.outstanding);
        outstanding.fold(Sum::ZERO, Sum::plus)
    }

    /// The part of what is charged and all of what is outstanding that no answer
    /// settled.
    pub fn unresolved(&self) -> Sum {
        let unresolved = self.at.iter().map(|one| one.unresolved);
        unresolved.fold(Sum::ZERO, Sum::plus)
    }

    /// The places that have no settlement with its whole chain.
    pub fn unsettled(&self) -> u64 {
        self.at.iter().filter(|one| !one.whole).count() as u64
    }

    /// What is known against it: its charges, and what its places count for beyond.
    pub fn known(&self) -> Sum {
        self.charged().plus(self.outstanding())
    }

    /// Where the names are not reachable: how many of the numbers from 1 to the
    /// highest place any of them names hold no dispatch. Each may be a dispatch
    /// whose name is lost. Nothing where the names are reachable.
    ///
    /// **Counted, never walked.** A name can name any place a number of 64 bits can
    /// say: this is the highest place named less the places that hold a dispatch,
    /// and nothing loops from 1 to a place.
    pub fn unexplained(&self) -> u64 {
        if self.reachable {
            return 0;
        }
        let corrected = self.corrected_places();
        let named = self
            .places
            .iter()
            .chain(self.dispatched.keys())
            .chain(self.settled.keys())
            .chain(&self.settling)
            .chain(&corrected);
        let highest = named.max().copied().unwrap_or(0);
        highest.saturating_sub(self.dispatched.len() as u64)
    }

    fn corrected_places(&self) -> Vec<u64> {
        let place = |(name, _): &(Name, bool)| match name {
            Name::Adjusted { place, .. } => Some(place.get()),
            _ => None,
        };
        self.adjusted.iter().filter_map(place).collect()
    }

    /// The largest amount a correction of it names, or nothing.
    pub fn corrections_most(&self) -> Amount {
        let micros = |(name, _): &(Name, bool)| match name {
            Name::Adjusted { micros, .. } => Some(*micros),
            _ => None,
        };
        let most = self.adjusted.iter().filter_map(micros).max();
        most.unwrap_or(Amount::ZERO)
    }

    /// The most its names can mean (ADR 0016, "What a reservation counts for"). Of a
    /// reachable reservation: its amount, what is known, or what a correction says,
    /// whichever is most. Of any other, with no name trusted to explain another: its
    /// amount, and every bound and every charge named under it, and its amount once
    /// more for each place that holds no dispatch; or what a correction says, when
    /// that is more. The sum stops at the largest number there is.
    pub fn most(&self) -> Sum {
        let amount = Sum::of(self.amount());
        let named = match self.reachable {
            true => self.known(),
            false => amount
                .plus(self.known())
                .plus(amount.times(self.unexplained())),
        };
        amount.max(Sum::of(self.corrections_most())).max(named)
    }

    /// What a reservation that is not reachable counts for, with no correction of
    /// it standing: the most its names can mean, and "unknown, at least" where it
    /// has not exactly one hold, because the ledger's own hold may be the one that
    /// is lost. None of a reachable reservation, which counts by its state: that is
    /// the fold's to say.
    pub fn frozen(&self) -> Option<Counted> {
        match (self.reachable, self.holds.len()) {
            (true, _) => None,
            (false, 1) => Some(Counted::Exactly(self.most())),
            (false, _) => Some(Counted::AtLeast(self.most())),
        }
    }

    /// What `run` sent under this reservation: a number where the names are
    /// reachable; "unknown, at most" where they are not; and "unknown" with no most
    /// where the amount cannot be read either.
    pub fn sent_by(&self, run: &RunId) -> Sent {
        if !self.reachable {
            return match self.holds.len() {
                1 => Sent::Unknown {
                    at_most: self.most(),
                },
                _ => Sent::Unbounded,
            };
        }
        let mut spend = RunSpend::default();
        for one in self.at.iter().filter(|one| one.by.contains(run)) {
            spend.charged = spend.charged.plus(one.charged);
            spend.outstanding = spend.outstanding.plus(one.outstanding);
            spend.unresolved = spend.unresolved.plus(one.unresolved);
        }
        Sent::Known(spend)
    }

    /// The next place under it: the lowest that no place, dispatch, settlement or
    /// settlement's guard of it names.
    pub fn next_place(&self) -> u64 {
        let taken: BTreeSet<u64> = self
            .places
            .iter()
            .chain(self.dispatched.keys())
            .chain(self.settled.keys())
            .chain(&self.settling)
            .copied()
            .collect();
        lowest_free(&taken)
    }

    /// The seal of every name of it as they are, these aside: what a correction made
    /// now, which will take the place `aside` guards, is sealed over.
    pub fn seal_aside(&self, aside: &[&Name]) -> Seal {
        let texts: Vec<(String, bool)> = self
            .all
            .iter()
            .filter(|(name, _)| !aside.contains(&name))
            .map(|(name, hurt)| (name.to_string(), *hurt))
            .collect();
        let formless = self.formless.iter().map(|text| (text.as_str(), false));
        Seal::over(
            texts
                .iter()
                .map(|(text, hurt)| (text.as_str(), *hurt))
                .chain(formless),
        )
    }

    /// Each correction of it, with what its seal has to be and whether it is at a
    /// place of its own.
    pub fn corrections(&self) -> Vec<Correction> {
        let mut found = Vec::new();
        for (name, damaged) in &self.adjusted {
            let Name::Adjusted {
                place,
                turn,
                seal,
                micros,
                ..
            } = name
            else {
                continue;
            };
            let guard = Name::Place {
                run: self.run.clone(),
                place: *place,
            };
            let at = place.get();
            let at_its_place = self.closed()
                && self.places.contains(&at)
                && !self.hurt.contains(&at)
                && !self.dispatched.contains_key(&at)
                && !self.settled.contains_key(&at)
                && !self.settling.contains(&at);
            found.push(Correction {
                name: name.clone(),
                place: at,
                turn: turn.get(),
                seal: *seal,
                micros: *micros,
                damaged: *damaged,
                sealed_over: self.seal_aside(&[name, &guard]),
                at_its_place,
            });
        }
        found
    }
}

/// A text under `spend/` that is no name of the ledger and belongs to an epoch or
/// to nothing ([`Belongs`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formless {
    /// The text, as the generation holds it.
    pub text: String,
    /// The epoch it names and the form it begins as, when it names one.
    pub epoch: Option<(EpochWord, u64)>,
    /// Why it is no name, with the text shown escaped and cut.
    pub why: NotAName,
}

/// Every name under `spend/` of one generation, sorted by what it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reading {
    /// Each reservation the names give, by its run.
    pub reservations: BTreeMap<RunId, Names>,
    /// The names that belong to an epoch and to no reservation, each with whether it
    /// is damaged: every turn, reset and guard.
    pub epochs: Vec<(Name, bool)>,
    /// The texts of no form that belong to no reservation: those that name an epoch,
    /// and those that belong to nothing, which change no balance.
    pub formless: Vec<Formless>,
}

impl Reading {
    /// Reads every name the generation holds under `spend/`, with `read`, which
    /// gives the content of an entry.
    ///
    /// **Each entry is answered by itself, and none makes the reading fail.** An
    /// entry longer than a record may be is not read; one that cannot be read, that
    /// is not a record read strictly, or that says something else than its name, is
    /// damaged, and so is the executor's record of a reservation that is not the
    /// reservation of the run its name says. The amounts are the names': a damaged
    /// reservation is still given, with the amount its hold's name states.
    pub fn of(generation: &Generation, read: impl Fn(&Entry) -> store::Result<Vec<u8>>) -> Reading {
        let under = generation
            .entries()
            .range(SPEND_PREFIX.to_owned()..)
            .take_while(|(text, _)| text.starts_with(SPEND_PREFIX));
        Reading::of_names(under.map(|(text, entry)| {
            // A text that is no name has no content to agree with it.
            let damaged = Name::parse(text).is_ok_and(|name| !sound(&name, entry, &read));
            (text.as_str(), damaged)
        }))
    }

    /// The same sorting, of names given as their text, each with whether its content
    /// is damaged: what [`Reading::of`] gathers a generation with, and how books that
    /// are in no store are read. A text that does not start with `spend/` is left
    /// out; of a text that is no name, whether it is damaged is not looked at.
    pub fn of_names<'a>(names: impl IntoIterator<Item = (&'a str, bool)>) -> Reading {
        let mut named: BTreeMap<RunId, Vec<(Name, bool)>> = BTreeMap::new();
        let mut strays: BTreeMap<RunId, Vec<String>> = BTreeMap::new();
        let mut reading = Reading::default();
        for (text, damaged) in names {
            if !text.starts_with(SPEND_PREFIX) {
                continue;
            }
            let name = match Name::parse(text) {
                Ok(name) => name,
                Err(why) => {
                    let epoch = match why.belongs() {
                        Belongs::Reservation(run) => {
                            strays.entry(run.clone()).or_default().push(text.to_owned());
                            continue;
                        }
                        Belongs::Epoch { word, epoch } => Some((*word, *epoch)),
                        Belongs::Unplaced => None,
                    };
                    reading.formless.push(Formless {
                        text: text.to_owned(),
                        epoch,
                        why,
                    });
                    continue;
                }
            };
            match name.run().cloned() {
                Some(run) => named.entry(run).or_default().push((name, damaged)),
                None => reading.epochs.push((name, damaged)),
            }
        }
        let runs: BTreeSet<RunId> = named.keys().chain(strays.keys()).cloned().collect();
        for run in runs {
            let names = named.remove(&run).unwrap_or_default();
            let formless = strays.remove(&run).unwrap_or_default();
            let of = Names::of(&run, names, formless);
            reading.reservations.insert(run, of);
        }
        reading
    }
}

impl Names {
    /// The names of one reservation as a generation holds them, read alone: what
    /// [`Reading::of`] gives for this run, with the content of no other
    /// reservation's names read. None where no name under `spend/` names the run.
    ///
    /// It is what the meter judges a dispatch by (task S5): its own reservation's
    /// names, and no fold of the store's books.
    pub fn of_run(
        run: &RunId,
        generation: &Generation,
        read: impl Fn(&Entry) -> store::Result<Vec<u8>>,
    ) -> Option<Names> {
        let mut named = Vec::new();
        let mut formless = Vec::new();
        let under = generation
            .entries()
            .range(SPEND_PREFIX.to_owned()..)
            .take_while(|(text, _)| text.starts_with(SPEND_PREFIX));
        for (text, entry) in under {
            match Name::parse(text) {
                Ok(name) if name.run() == Some(run) => {
                    let damaged = !sound(&name, entry, &read);
                    named.push((name, damaged));
                }
                Ok(_) => {}
                Err(why) => {
                    if matches!(why.belongs(), Belongs::Reservation(of) if of == run) {
                        formless.push(text.clone());
                    }
                }
            }
        }
        let any = !named.is_empty() || !formless.is_empty();
        any.then(|| Names::of(run, named, formless))
    }
}

/// Whether the content under a name can be read and says what the name says.
fn sound(name: &Name, entry: &Entry, read: impl Fn(&Entry) -> store::Result<Vec<u8>>) -> bool {
    if entry.bytes > SPEND_RECORD_MAX_BYTES {
        return false;
    }
    let Ok(bytes) = read(entry) else {
        return false;
    };
    match name {
        Name::Reservation(run) => {
            Reservation::from_bytes(&bytes).is_ok_and(|reservation| reservation.run() == run)
        }
        _ => Record::read(name, &bytes).is_ok(),
    }
}
