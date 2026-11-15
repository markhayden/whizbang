//! The names under `spend/` (ADR 0016, "The books are names"): one writer and one
//! parser of the grammar, which every reader of such a name uses.
//!
//! Every amount the fold needs is in a name, so a name is read as untrusted and
//! strictly: each form is written one way, and a text that is not exactly what
//! [`Name`]'s `Display` writes for some name is no name ([`Name::parse`]). The types
//! hold what the grammar holds: an amount is never the largest number there is
//! ([`Amount`]), and a place, a turn and the epoch of a reset are from 1.

use std::fmt;
use std::num::NonZeroU64;

use sha2::{Digest as _, Sha256};

use crate::capability::{Shown, Stage};
use crate::contract::RunId;

/// What every name of the ledger starts with.
pub const SPEND_PREFIX: &str = "spend/";

/// The longest name under `spend/`, in characters (ADR 0007).
pub const NAME_LIMIT: usize = 200;

/// The most decimal digits a number in a name has.
pub const NUMBER_DIGITS: usize = 20;

/// How many digits of base sixteen a seal is: the first 32 of a SHA-256.
pub const SEAL_DIGITS: usize = 32;

/// An amount, in millionths of a US dollar. It is never the largest number of 64
/// bits, which is no amount (ADR 0007): that one is refused wherever an amount comes
/// in, here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Amount(u64);

impl Amount {
    /// Nothing.
    pub const ZERO: Amount = Amount(0);

    /// The largest amount there is: one under the largest number.
    pub const MOST: Amount = Amount(u64::MAX - 1);

    /// This many millionths, unless it is the largest number there is.
    pub fn new(micros: u64) -> Option<Amount> {
        (micros != u64::MAX).then_some(Amount(micros))
    }

    pub fn micros(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What a correction is sealed over (ADR 0016, "A person's correction"): the first
/// 32 digits, of base sixteen and in lower case, of the SHA-256 of the names it was
/// made over ([`Seal::over`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Seal(u128);

impl Seal {
    /// The seal of these names, each given as its text with whether its content is
    /// damaged; a name of no form that belongs to the reservation is among them, as
    /// its text. The names are taken in the order of their text, whatever order they
    /// are given in; each is hashed as its text, then `!` when it is damaged, then a
    /// newline.
    pub fn over<'a>(names: impl IntoIterator<Item = (&'a str, bool)>) -> Seal {
        let mut lines: Vec<String> = names
            .into_iter()
            .map(|(name, damaged)| format!("{name}{}\n", if damaged { "!" } else { "" }))
            .collect();
        lines.sort();
        let mut hasher = Sha256::new();
        for line in &lines {
            hasher.update(line.as_bytes());
        }
        let digest: [u8; 32] = hasher.finalize().into();
        let mut first = [0u8; 16];
        first.copy_from_slice(&digest[..16]);
        Seal(u128::from_be_bytes(first))
    }

    /// The seal these 32 digits are: of base sixteen, in lower case, no more and no
    /// fewer.
    pub fn parse(text: &str) -> Option<Seal> {
        let plain = text.len() == SEAL_DIGITS
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        plain
            .then(|| u128::from_str_radix(text, 16).ok())
            .flatten()
            .map(Seal)
    }
}

impl fmt::Display for Seal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// A name under `spend/`: every form the grammar has, and no other.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Name {
    /// `spend/reservations/<run>`: the executor's record of a reservation.
    Reservation(RunId),
    /// `spend/held/<epoch>/<run>/<micros>`: a reservation's amount, and its epoch.
    Held {
        epoch: u64,
        run: RunId,
        micros: Amount,
    },
    /// `spend/turn/<epoch>/<n>`: the turn `n` of that epoch is taken.
    Turn { epoch: u64, turn: NonZeroU64 },
    /// `spend/reset/<epoch>/<micros>`: the allowance of an epoch.
    Reset { epoch: NonZeroU64, micros: Amount },
    /// `spend/epoch/<epoch>`: the epoch is opened, once: a reset's guard.
    Epoch(NonZeroU64),
    /// `spend/place/<run>/<n>`: the place `n` under a reservation is taken.
    Place { run: RunId, place: NonZeroU64 },
    /// `spend/dispatched/<run>/<n>/<by>/<micros>`: a request's bound, before it left.
    Dispatched {
        run: RunId,
        place: NonZeroU64,
        by: RunId,
        micros: Amount,
    },
    /// `spend/settling/<run>/<n>`: the place `n` is settled, once.
    Settling { run: RunId, place: NonZeroU64 },
    /// `spend/settled/<run>/<n>/<stage>/<micros>`: what the request was charged.
    Settled {
        run: RunId,
        place: NonZeroU64,
        stage: Stage,
        micros: Amount,
    },
    /// `spend/closed/<run>/released` or `spend/closed/<run>/held`: the reservation is
    /// closed, and whether its closing released what it held beyond its charges.
    Closed { run: RunId, released: bool },
    /// `spend/adjusted/<run>/<place>/<turn>/<seal>/<micros>`: a person's correction.
    Adjusted {
        run: RunId,
        place: NonZeroU64,
        turn: NonZeroU64,
        seal: Seal,
        micros: Amount,
    },
}

/// What a text under `spend/` that is no name of the ledger still says, by the place
/// of its segments and by nothing else (ADR 0016, "A name of no form").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Belongs {
    /// Its first word is one of the eight forms of a reservation's names
    /// (`reservations`, `held`, `place`, `dispatched`, `settling`, `settled`,
    /// `closed`, `adjusted`) and the segment where that form names its reservation's
    /// run, the third of `held` and the second of the others, is a run id: it is a
    /// name of that run's reservation that the ledger could not have written, and
    /// freezes it.
    Reservation(RunId),
    /// Its first word is `reset`, `epoch` or `turn` and its second segment is a
    /// number: it names that epoch and says nothing that can be trusted of it. A
    /// reset or a guard of no form leaves the epoch there with an allowance that is
    /// not whole, which allows nothing; a turn of no form is a turn that no
    /// publication took, and the epoch needs its owner.
    Epoch { word: EpochWord, epoch: u64 },
    /// Anything else: it belongs to no reservation and to no epoch, and changes no
    /// balance. A fold reports it.
    Unplaced,
}

/// Which of the three forms that belong to an epoch a name of no form begins as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpochWord {
    Reset,
    Guard,
    Turn,
}

/// A text under `spend/` that is no name of the ledger: why, with the text shown
/// escaped and cut, and what it belongs to all the same ([`Belongs`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAName {
    shown: String,
    why: &'static str,
    belongs: Belongs,
}

impl NotAName {
    /// The reservation or the epoch it belongs to, by the place of its segments.
    pub fn belongs(&self) -> &Belongs {
        &self.belongs
    }

    /// Which rule of the grammar the text breaks, in fixed words.
    pub fn why(&self) -> &'static str {
        self.why
    }
}

/// What a text that is no name belongs to.
fn belongs(text: &str) -> Belongs {
    let Some(rest) = text.strip_prefix(SPEND_PREFIX) else {
        return Belongs::Unplaced;
    };
    let mut parts = rest.split('/');
    let (word, second, third) = (parts.next(), parts.next(), parts.next());
    let run_at = |part: Option<&str>| match part.map(RunId::parse) {
        Some(Ok(run)) => Belongs::Reservation(run),
        _ => Belongs::Unplaced,
    };
    let epoch_of = |word: EpochWord| match second.map(number) {
        Some(Ok(epoch)) => Belongs::Epoch { word, epoch },
        _ => Belongs::Unplaced,
    };
    match word {
        Some("held") => run_at(third),
        Some(
            "reservations" | "place" | "dispatched" | "settling" | "settled" | "closed"
            | "adjusted",
        ) => run_at(second),
        Some("reset") => epoch_of(EpochWord::Reset),
        Some("epoch") => epoch_of(EpochWord::Guard),
        Some("turn") => epoch_of(EpochWord::Turn),
        _ => Belongs::Unplaced,
    }
}

impl fmt::Display for NotAName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} is no name of the ledger: {}", self.shown, self.why)
    }
}

impl std::error::Error for NotAName {}

const NOT_UNDER: &str = "its names start with spend/";
const TOO_LONG: &str = "a name is 200 characters at most";
const BAD_SEGMENT: &str = "no segment of a name is empty, . or ..";
const NO_FORM: &str = "it is of none of the forms a name has, or has a segment too many or too few";
const BAD_NUMBER: &str =
    "a number is 1 to 20 decimal digits with no leading zero, and fits in 64 bits";
const BAD_AMOUNT: &str = "an amount is a number under the largest there is, which is no amount";
const FROM_ONE: &str = "a place, a turn and the epoch of a reset are from 1";
const BAD_RUN: &str = "a run is named by a run id";
const BAD_STAGE: &str = "a stage is not_sent, answered or unresolved";
const BAD_CLOSING: &str = "a closing is released or held";
const BAD_SEAL: &str = "a seal is 32 digits of base sixteen, in lower case";

fn number(part: &str) -> Result<u64, &'static str> {
    let plain = !part.is_empty()
        && part.len() <= NUMBER_DIGITS
        && part.bytes().all(|byte| byte.is_ascii_digit())
        && (part == "0" || !part.starts_with('0'));
    if !plain {
        return Err(BAD_NUMBER);
    }
    part.parse().map_err(|_| BAD_NUMBER)
}

fn amount(part: &str) -> Result<Amount, &'static str> {
    Amount::new(number(part)?).ok_or(BAD_AMOUNT)
}

fn from_one(part: &str) -> Result<NonZeroU64, &'static str> {
    NonZeroU64::new(number(part)?).ok_or(FROM_ONE)
}

fn run(part: &str) -> Result<RunId, &'static str> {
    RunId::parse(part).map_err(|_| BAD_RUN)
}

/// The name these segments after `spend/` are.
fn form(parts: &[&str]) -> Result<Name, &'static str> {
    Ok(match parts {
        ["reservations", id] => Name::Reservation(run(id)?),
        ["held", epoch, id, micros] => Name::Held {
            epoch: number(epoch)?,
            run: run(id)?,
            micros: amount(micros)?,
        },
        ["turn", epoch, turn] => Name::Turn {
            epoch: number(epoch)?,
            turn: from_one(turn)?,
        },
        ["reset", epoch, micros] => Name::Reset {
            epoch: from_one(epoch)?,
            micros: amount(micros)?,
        },
        ["epoch", epoch] => Name::Epoch(from_one(epoch)?),
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
            stage: Stage::ALL
                .into_iter()
                .find(|one| one.name() == *stage)
                .ok_or(BAD_STAGE)?,
            micros: amount(micros)?,
        },
        ["closed", id, how] => Name::Closed {
            run: run(id)?,
            released: match *how {
                "released" => true,
                "held" => false,
                _ => return Err(BAD_CLOSING),
            },
        },
        ["adjusted", id, place, turn, seal, micros] => Name::Adjusted {
            run: run(id)?,
            place: from_one(place)?,
            turn: from_one(turn)?,
            seal: Seal::parse(seal).ok_or(BAD_SEAL)?,
            micros: amount(micros)?,
        },
        _ => return Err(NO_FORM),
    })
}

impl Name {
    /// The name this text is, read strictly, or why it is none.
    pub fn parse(text: &str) -> Result<Name, NotAName> {
        let refuse = |why: &'static str| NotAName {
            shown: Shown(text).to_string(),
            why,
            belongs: belongs(text),
        };
        let Some(rest) = text.strip_prefix(SPEND_PREFIX) else {
            return Err(refuse(NOT_UNDER));
        };
        if text.len() > NAME_LIMIT {
            return Err(refuse(TOO_LONG));
        }
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.iter().any(|part| matches!(*part, "" | "." | "..")) {
            return Err(refuse(BAD_SEGMENT));
        }
        form(&parts).map_err(refuse)
    }

    /// The run whose reservation this name belongs to, when it belongs to one: a
    /// turn, a reset and its guard belong to none.
    pub fn run(&self) -> Option<&RunId> {
        match self {
            Name::Turn { .. } | Name::Reset { .. } | Name::Epoch(_) => None,
            Name::Reservation(run)
            | Name::Held { run, .. }
            | Name::Place { run, .. }
            | Name::Dispatched { run, .. }
            | Name::Settling { run, .. }
            | Name::Settled { run, .. }
            | Name::Closed { run, .. }
            | Name::Adjusted { run, .. } => Some(run),
        }
    }

    /// The place under its reservation that this name is at, when it is at one: a
    /// place's guard, a dispatch, a settlement and its guard. A correction names a
    /// place and is not at it.
    pub fn place(&self) -> Option<NonZeroU64> {
        match self {
            Name::Place { place, .. }
            | Name::Dispatched { place, .. }
            | Name::Settling { place, .. }
            | Name::Settled { place, .. } => Some(*place),
            _ => None,
        }
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(SPEND_PREFIX)?;
        match self {
            Name::Reservation(run) => write!(f, "reservations/{run}"),
            Name::Held { epoch, run, micros } => write!(f, "held/{epoch}/{run}/{micros}"),
            Name::Turn { epoch, turn } => write!(f, "turn/{epoch}/{turn}"),
            Name::Reset { epoch, micros } => write!(f, "reset/{epoch}/{micros}"),
            Name::Epoch(epoch) => write!(f, "epoch/{epoch}"),
            Name::Place { run, place } => write!(f, "place/{run}/{place}"),
            Name::Dispatched {
                run,
                place,
                by,
                micros,
            } => write!(f, "dispatched/{run}/{place}/{by}/{micros}"),
            Name::Settling { run, place } => write!(f, "settling/{run}/{place}"),
            Name::Settled {
                run,
                place,
                stage,
                micros,
            } => write!(f, "settled/{run}/{place}/{}/{micros}", stage.name()),
            Name::Closed { run, released } => {
                let how = if *released { "released" } else { "held" };
                write!(f, "closed/{run}/{how}")
            }
            Name::Adjusted {
                run,
                place,
                turn,
                seal,
                micros,
            } => write!(f, "adjusted/{run}/{place}/{turn}/{seal}/{micros}"),
        }
    }
}
