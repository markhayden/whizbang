//! The content under each of the ledger's names (ADR 0016, "The content of each
//! record"): JSON on one line with a newline, written one way and read strictly, at
//! most [`SPEND_RECORD_MAX_BYTES`] bytes.
//!
//! A record repeats what its name says, and says a little more. The fold reads names;
//! of content it asks only whether it can be read and agrees with its name
//! ([`Record::read`]): one that does not is damage. The content under
//! `spend/reservations/<run>` is the executor's `reservation/1`
//! ([`crate::contract::Reservation`]) and is not here.

use std::fmt;
use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::capability::Stage;
use crate::contract::RunId;

use super::name::{Amount, Name, Seal};
use super::reading::Sum;

/// The most bytes a record of the ledger is. An entry that says it is longer is not
/// read.
pub const SPEND_RECORD_MAX_BYTES: u64 = 1024;

/// How many characters of what was read a refusal quotes.
const QUOTED: usize = 64;

/// What took a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnBy {
    Admission,
    Settlement,
    Closing,
    Reset,
    Adjustment,
}

/// What took a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceBy {
    Dispatch,
    Settlement,
    Closing,
    Adjustment,
}

/// What closed a reservation: the end of its run, which a confirmation that backs an
/// admission out is too, or recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosedBy {
    End,
    Recovery,
}

macro_rules! words {
    ($of:ident, $what:literal, $($one:ident => $word:literal),+) => {
        impl $of {
            /// The word a record spells it with.
            pub fn word(self) -> &'static str {
                match self { $($of::$one => $word),+ }
            }

            fn read(word: &str) -> Result<$of, SpendRecordError> {
                match word {
                    $($word => Ok($of::$one),)+
                    _ => Err(SpendRecordError::Inconsistent($what)),
                }
            }
        }
    };
}

words!(TurnBy, "what took the turn is not one of the five things that take one",
    Admission => "admission", Settlement => "settlement", Closing => "closing",
    Reset => "reset", Adjustment => "adjustment");
words!(PlaceBy, "what took the place is not one of the four things that take one",
    Dispatch => "dispatch", Settlement => "settlement", Closing => "closing",
    Adjustment => "adjustment");
words!(ClosedBy, "what closed the reservation is neither an end nor a recovery",
    End => "end", Recovery => "recovery");

/// One record of the ledger, of any of its ten kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    /// `spend-hold/1`, under `held`.
    Hold {
        epoch: u64,
        run: RunId,
        reserved: Amount,
    },
    /// `spend-turn/1`, under `turn`: what took it, and for which run when it was
    /// taken for one.
    Turn {
        epoch: u64,
        turn: NonZeroU64,
        by: TurnBy,
        run: Option<RunId>,
    },
    /// `spend-reset/1`, under `reset`.
    Reset {
        epoch: NonZeroU64,
        allowance: Amount,
        at_ms: u64,
    },
    /// `spend-epoch/1`, under `epoch`: a reset's guard.
    Epoch { epoch: NonZeroU64 },
    /// `spend-place/1`, under `place`.
    Place {
        run: RunId,
        place: NonZeroU64,
        by: PlaceBy,
    },
    /// `spend-dispatch/1`, under `dispatched`: `request` is the request's place among
    /// those of the run that sent it.
    Dispatch {
        run: RunId,
        place: NonZeroU64,
        by: RunId,
        request: u64,
        bound: Amount,
        at_ms: u64,
    },
    /// `spend-settling/1`, under `settling`.
    Settling { run: RunId, place: NonZeroU64 },
    /// `spend-settlement/1`, under `settled`.
    Settlement {
        run: RunId,
        place: NonZeroU64,
        stage: Stage,
        charged: Amount,
        over_bound: bool,
        at_ms: u64,
    },
    /// `spend-closing/1`, under `closed`. What was charged and what is unresolved
    /// are sums over the reservation's settlements, each of which can state nearly
    /// the largest number of 64 bits: they are sums of 128 bits that stop at the
    /// largest ([`Sum`]), written as decimal digits in a string.
    Closing {
        run: RunId,
        released: bool,
        by: ClosedBy,
        charged: Sum,
        unresolved: Sum,
        at_ms: u64,
    },
    /// `spend-adjustment/1`, under `adjusted`.
    Adjustment {
        run: RunId,
        place: NonZeroU64,
        turn: NonZeroU64,
        seal: Seal,
        charged: Amount,
        at_ms: u64,
    },
}

/// Why stored bytes are not a record of the ledger. It quotes what was read escaped
/// and cut, and holds no path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpendRecordError {
    /// More bytes than a record may be. They were not read.
    TooLarge { bytes: u64 },
    /// A record of another kind or version than its name's.
    OtherKind { found: String },
    /// Not a record: not JSON, or not of the kind's shape, a field unknown or left
    /// out among the reasons.
    Malformed(String),
    /// A record of the right shape that says what none can.
    Inconsistent(&'static str),
    /// A record that is not written the one way a record is written.
    NotCanonical,
    /// A record that is whole and says something else than the name it is under.
    Disagrees,
}

impl fmt::Display for SpendRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpendRecordError::TooLarge { bytes } => write!(
                f,
                "{bytes} bytes, and a record of the ledger is at most {SPEND_RECORD_MAX_BYTES}"
            ),
            SpendRecordError::OtherKind { found } => {
                write!(f, "a record of another kind or version: {found}")
            }
            SpendRecordError::Malformed(why) => write!(f, "not a record of the ledger: {why}"),
            SpendRecordError::Inconsistent(why) => f.write_str(why),
            SpendRecordError::NotCanonical => {
                f.write_str("it is not written the one way a record is written")
            }
            SpendRecordError::Disagrees => {
                f.write_str("it says something else than the name it is under")
            }
        }
    }
}

impl std::error::Error for SpendRecordError {}

/// A text that was read, as a refusal shows it: escaped, and its first characters.
fn quoted(text: &str) -> String {
    let start: String = text.chars().take(QUOTED).collect();
    let more = if text.chars().count() > QUOTED {
        "..."
    } else {
        ""
    };
    format!("{}{more}", start.escape_debug())
}

fn malformed(error: serde_json::Error) -> SpendRecordError {
    SpendRecordError::Malformed(quoted(&error.to_string()))
}

mod wire {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Hold {
        pub record: String,
        pub epoch: u64,
        pub run: String,
        pub reserved_micros: u64,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Turn {
        pub record: String,
        pub epoch: u64,
        pub turn: u64,
        pub by: String,
        #[serde(deserialize_with = "super::present")]
        pub run: Option<String>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Reset {
        pub record: String,
        pub epoch: u64,
        pub allowance_micros: u64,
        pub at_ms: u64,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Epoch {
        pub record: String,
        pub epoch: u64,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Place {
        pub record: String,
        pub run: String,
        pub place: u64,
        pub by: String,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Dispatch {
        pub record: String,
        pub run: String,
        pub place: u64,
        pub by: String,
        pub request: u64,
        pub bound_micros: u64,
        pub at_ms: u64,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Settling {
        pub record: String,
        pub run: String,
        pub place: u64,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Settlement {
        pub record: String,
        pub run: String,
        pub place: u64,
        pub stage: String,
        pub charged_micros: u64,
        pub over_bound: bool,
        pub at_ms: u64,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Closing {
        pub record: String,
        pub run: String,
        pub released: bool,
        pub by: String,
        pub charged_micros: String,
        pub unresolved_micros: String,
        pub at_ms: u64,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Adjustment {
        pub record: String,
        pub run: String,
        pub place: u64,
        pub turn: u64,
        pub seal: String,
        pub charged_micros: u64,
        pub at_ms: u64,
    }
}

/// A field that may be null and may not be left out.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

fn line_of<T: Serialize>(record: &T) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(record).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

fn strictly<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, SpendRecordError> {
    let mut reader = serde_json::Deserializer::from_slice(bytes);
    let wire = T::deserialize(&mut reader).map_err(malformed)?;
    reader.end().map_err(malformed)?;
    Ok(wire)
}

fn run_of(text: &str) -> Result<RunId, SpendRecordError> {
    RunId::parse(text).map_err(|_| SpendRecordError::Inconsistent("its run is no run id"))
}

fn from_one(number: u64) -> Result<NonZeroU64, SpendRecordError> {
    NonZeroU64::new(number).ok_or(SpendRecordError::Inconsistent(
        "a place, a turn and the epoch of a reset are from 1",
    ))
}

fn amount_of(micros: u64) -> Result<Amount, SpendRecordError> {
    Amount::new(micros).ok_or(SpendRecordError::Inconsistent(
        "an amount is under the largest number there is, which is no amount",
    ))
}

fn sum_of(text: &str) -> Result<Sum, SpendRecordError> {
    Sum::parse(text).ok_or(SpendRecordError::Inconsistent(
        "a sum is decimal digits with no leading zero, of 128 bits at most",
    ))
}

impl Record {
    /// The kind of this record, with its version.
    pub fn kind(&self) -> &'static str {
        match self {
            Record::Hold { .. } => "spend-hold/1",
            Record::Turn { .. } => "spend-turn/1",
            Record::Reset { .. } => "spend-reset/1",
            Record::Epoch { .. } => "spend-epoch/1",
            Record::Place { .. } => "spend-place/1",
            Record::Dispatch { .. } => "spend-dispatch/1",
            Record::Settling { .. } => "spend-settling/1",
            Record::Settlement { .. } => "spend-settlement/1",
            Record::Closing { .. } => "spend-closing/1",
            Record::Adjustment { .. } => "spend-adjustment/1",
        }
    }

    /// Every kind there is, in the order of the ADR's table.
    pub const KINDS: [&'static str; 10] = [
        "spend-hold/1",
        "spend-turn/1",
        "spend-reset/1",
        "spend-epoch/1",
        "spend-place/1",
        "spend-dispatch/1",
        "spend-settling/1",
        "spend-settlement/1",
        "spend-closing/1",
        "spend-adjustment/1",
    ];

    /// The name this record is published under: what it repeats.
    pub fn name(&self) -> Name {
        match self.clone() {
            Record::Hold {
                epoch,
                run,
                reserved,
            } => Name::Held {
                epoch,
                run,
                micros: reserved,
            },
            Record::Turn { epoch, turn, .. } => Name::Turn { epoch, turn },
            Record::Reset {
                epoch, allowance, ..
            } => Name::Reset {
                epoch,
                micros: allowance,
            },
            Record::Epoch { epoch } => Name::Epoch(epoch),
            Record::Place { run, place, .. } => Name::Place { run, place },
            Record::Dispatch {
                run,
                place,
                by,
                bound,
                ..
            } => Name::Dispatched {
                run,
                place,
                by,
                micros: bound,
            },
            Record::Settling { run, place } => Name::Settling { run, place },
            Record::Settlement {
                run,
                place,
                stage,
                charged,
                ..
            } => Name::Settled {
                run,
                place,
                stage,
                micros: charged,
            },
            Record::Closing { run, released, .. } => Name::Closed { run, released },
            Record::Adjustment {
                run,
                place,
                turn,
                seal,
                charged,
                ..
            } => Name::Adjusted {
                run,
                place,
                turn,
                seal,
                micros: charged,
            },
        }
    }

    /// The record as it is stored: JSON on one line, then a newline.
    pub fn to_bytes(&self) -> Vec<u8> {
        let record = self.kind().to_owned();
        let id = |run: &RunId| run.as_str().to_owned();
        match self {
            Record::Hold {
                epoch,
                run,
                reserved,
            } => line_of(&wire::Hold {
                record,
                epoch: *epoch,
                run: id(run),
                reserved_micros: reserved.micros(),
            }),
            Record::Turn {
                epoch,
                turn,
                by,
                run,
            } => line_of(&wire::Turn {
                record,
                epoch: *epoch,
                turn: turn.get(),
                by: by.word().to_owned(),
                run: run.as_ref().map(id),
            }),
            Record::Reset {
                epoch,
                allowance,
                at_ms,
            } => line_of(&wire::Reset {
                record,
                epoch: epoch.get(),
                allowance_micros: allowance.micros(),
                at_ms: *at_ms,
            }),
            Record::Epoch { epoch } => line_of(&wire::Epoch {
                record,
                epoch: epoch.get(),
            }),
            Record::Place { run, place, by } => line_of(&wire::Place {
                record,
                run: id(run),
                place: place.get(),
                by: by.word().to_owned(),
            }),
            Record::Dispatch {
                run,
                place,
                by,
                request,
                bound,
                at_ms,
            } => line_of(&wire::Dispatch {
                record,
                run: id(run),
                place: place.get(),
                by: id(by),
                request: *request,
                bound_micros: bound.micros(),
                at_ms: *at_ms,
            }),
            Record::Settling { run, place } => line_of(&wire::Settling {
                record,
                run: id(run),
                place: place.get(),
            }),
            Record::Settlement {
                run,
                place,
                stage,
                charged,
                over_bound,
                at_ms,
            } => line_of(&wire::Settlement {
                record,
                run: id(run),
                place: place.get(),
                stage: stage.name().to_owned(),
                charged_micros: charged.micros(),
                over_bound: *over_bound,
                at_ms: *at_ms,
            }),
            Record::Closing {
                run,
                released,
                by,
                charged,
                unresolved,
                at_ms,
            } => line_of(&wire::Closing {
                record,
                run: id(run),
                released: *released,
                by: by.word().to_owned(),
                charged_micros: charged.to_string(),
                unresolved_micros: unresolved.to_string(),
                at_ms: *at_ms,
            }),
            Record::Adjustment {
                run,
                place,
                turn,
                seal,
                charged,
                at_ms,
            } => line_of(&wire::Adjustment {
                record,
                run: id(run),
                place: place.get(),
                turn: turn.get(),
                seal: seal.to_string(),
                charged_micros: charged.micros(),
                at_ms: *at_ms,
            }),
        }
    }

    /// Reads a stored record of any kind, strictly: the size, the kind, the shape
    /// with no field unknown, left out or twice, values a record can hold, and the
    /// bytes this code writes for what they say.
    pub fn from_bytes(bytes: &[u8]) -> Result<Record, SpendRecordError> {
        if bytes.len() as u64 > SPEND_RECORD_MAX_BYTES {
            return Err(SpendRecordError::TooLarge {
                bytes: bytes.len() as u64,
            });
        }
        #[derive(Deserialize)]
        struct Kind {
            record: String,
        }
        let kind = serde_json::from_slice::<Kind>(bytes).map_err(malformed)?;
        let record = match kind.record.as_str() {
            "spend-hold/1" => {
                let wire: wire::Hold = strictly(bytes)?;
                Record::Hold {
                    epoch: wire.epoch,
                    run: run_of(&wire.run)?,
                    reserved: amount_of(wire.reserved_micros)?,
                }
            }
            "spend-turn/1" => {
                let wire: wire::Turn = strictly(bytes)?;
                Record::Turn {
                    epoch: wire.epoch,
                    turn: from_one(wire.turn)?,
                    by: TurnBy::read(&wire.by)?,
                    run: wire.run.as_deref().map(run_of).transpose()?,
                }
            }
            "spend-reset/1" => {
                let wire: wire::Reset = strictly(bytes)?;
                Record::Reset {
                    epoch: from_one(wire.epoch)?,
                    allowance: amount_of(wire.allowance_micros)?,
                    at_ms: wire.at_ms,
                }
            }
            "spend-epoch/1" => {
                let wire: wire::Epoch = strictly(bytes)?;
                Record::Epoch {
                    epoch: from_one(wire.epoch)?,
                }
            }
            "spend-place/1" => {
                let wire: wire::Place = strictly(bytes)?;
                Record::Place {
                    run: run_of(&wire.run)?,
                    place: from_one(wire.place)?,
                    by: PlaceBy::read(&wire.by)?,
                }
            }
            "spend-dispatch/1" => {
                let wire: wire::Dispatch = strictly(bytes)?;
                Record::Dispatch {
                    run: run_of(&wire.run)?,
                    place: from_one(wire.place)?,
                    by: run_of(&wire.by)?,
                    request: wire.request,
                    bound: amount_of(wire.bound_micros)?,
                    at_ms: wire.at_ms,
                }
            }
            "spend-settling/1" => {
                let wire: wire::Settling = strictly(bytes)?;
                Record::Settling {
                    run: run_of(&wire.run)?,
                    place: from_one(wire.place)?,
                }
            }
            "spend-settlement/1" => {
                let wire: wire::Settlement = strictly(bytes)?;
                let stage = Stage::ALL.into_iter().find(|one| one.name() == wire.stage);
                Record::Settlement {
                    run: run_of(&wire.run)?,
                    place: from_one(wire.place)?,
                    stage: stage.ok_or(SpendRecordError::Inconsistent(
                        "its stage is not one a request reaches",
                    ))?,
                    charged: amount_of(wire.charged_micros)?,
                    over_bound: wire.over_bound,
                    at_ms: wire.at_ms,
                }
            }
            "spend-closing/1" => {
                let wire: wire::Closing = strictly(bytes)?;
                Record::Closing {
                    run: run_of(&wire.run)?,
                    released: wire.released,
                    by: ClosedBy::read(&wire.by)?,
                    charged: sum_of(&wire.charged_micros)?,
                    unresolved: sum_of(&wire.unresolved_micros)?,
                    at_ms: wire.at_ms,
                }
            }
            "spend-adjustment/1" => {
                let wire: wire::Adjustment = strictly(bytes)?;
                Record::Adjustment {
                    run: run_of(&wire.run)?,
                    place: from_one(wire.place)?,
                    turn: from_one(wire.turn)?,
                    seal: Seal::parse(&wire.seal).ok_or(SpendRecordError::Inconsistent(
                        "its seal is not 32 digits of base sixteen in lower case",
                    ))?,
                    charged: amount_of(wire.charged_micros)?,
                    at_ms: wire.at_ms,
                }
            }
            other => {
                return Err(SpendRecordError::OtherKind {
                    found: quoted(other),
                });
            }
        };
        if record.to_bytes() != bytes {
            return Err(SpendRecordError::NotCanonical);
        }
        Ok(record)
    }

    /// Reads the content under a name of the ledger: a record, read strictly, that
    /// says what the name says. Anything else is damage. A name of the form
    /// `spend/reservations/<run>` holds the executor's record and is not read here.
    pub fn read(name: &Name, bytes: &[u8]) -> Result<Record, SpendRecordError> {
        let record = Record::from_bytes(bytes)?;
        if record.name() != *name {
            return Err(SpendRecordError::Disagrees);
        }
        Ok(record)
    }
}
