//! The ledger's part in admission (`SPEC.md` section 6; ADR 0007, "The reservation").
//!
//! A call is admitted with a [`Reservation`] of what it may cost at most: a record,
//! kind `reservation/1`, made in the one publication that makes the run and the record
//! of its request id, under a name that belongs to no run. The [`Ledger`] is asked in
//! that step, before anything is published, and may decline: then nothing is published.
//!
//! What is here is what the executor calls (ADR 0016, "What the second part builds"):
//! [`Ledger::reserve`] in the admission's step, [`Ledger::confirm`] on a reading made
//! after the admission is durable, and [`Ledger::settle`] in the publication that ends
//! a run nothing called. The names a ledger makes are those of `spend::Name` and no
//! other. The real ledger is the spend strand's, `spend::StoreLedger` (plan 1B, task
//! S2); the test kit has a fake, and an executor that was given none reserves
//! nothing.

use serde::{Deserialize, Serialize};

use crate::artifact::Contents;
use crate::capability::Shown;
use crate::identity::Digest;
use crate::spend::{Name, Record};
use crate::store::{self, Entry, Generation, Publication};

use super::record::RECORD_KIND;
use super::request::{NOT_AN_AMOUNT, REQUEST_RECORD_MAX_BYTES, RequestRecordError};
use super::request::{RequestId, kind_of, line_of, malformed, present};
use super::{CallError, NotJudged, RunId, Secrets, StoreFailure};

/// The kind of a reservation record, with its version.
pub const RESERVATION_RECORD: &str = "reservation/1";

/// What the name of every record of spend starts with, outside runs.
pub const SPEND_PREFIX: &str = crate::spend::SPEND_PREFIX;

/// What the name of every reservation starts with: the run's id follows.
pub const RESERVATIONS_PREFIX: &str = "spend/reservations/";

/// The longest name a ledger may make in an admission, in characters.
pub const LEDGER_NAME_LIMIT: usize = crate::spend::NAME_LIMIT;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReservationWire {
    record: String,
    run: String,
    #[serde(deserialize_with = "present")]
    request: Option<String>,
    admission: Digest,
    reserved_micros: u64,
    start_ms: u64,
}

/// What was reserved for one run when it was admitted: the most the call may cost, in
/// millionths of a US dollar. It is the estimate of the preparation made at admission,
/// as the backends stated it then: what they state later is not looked at again. It is
/// never the largest amount there is, which is no amount
/// ([`super::Estimate::is_unbounded`]).
///
/// It names its run, and the request id when the caller gave one, so that whoever
/// settles it finds the run it is of; and its own name is made from the run's id
/// ([`Reservation::name_of`]), so that it is found from the run, and from a request id
/// through the id's record, which names the run. A run's id is free again once the run
/// is deleted, so the reservation also holds the digest of the run's admission: the run
/// the store holds under that id is the reservation's only when its own `admitted`
/// entry has that digest.
///
/// It is made by the executor when it admits a call, and by reading a stored one, which
/// checks it; in no other way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservation {
    run: RunId,
    request: Option<RequestId>,
    admission: Digest,
    micros: u64,
    start_ms: u64,
}

impl Reservation {
    pub(super) fn new(
        run: RunId,
        request: Option<RequestId>,
        admission: Digest,
        micros: u64,
        start_ms: u64,
    ) -> Reservation {
        Reservation {
            run,
            request,
            admission,
            micros,
            start_ms,
        }
    }

    /// The run it was reserved for.
    pub fn run(&self) -> &RunId {
        &self.run
    }

    /// The caller's id for the request that made the run, when it gave one.
    pub fn request(&self) -> Option<&RequestId> {
        self.request.as_ref()
    }

    /// The digest of the admission the run was made with, as the run holds it under
    /// `admitted`.
    pub fn admission(&self) -> Digest {
        self.admission
    }

    /// The amount, in millionths of a US dollar.
    pub fn micros(&self) -> u64 {
        self.micros
    }

    /// When the run was admitted, in milliseconds since the Unix epoch.
    pub fn start_ms(&self) -> u64 {
        self.start_ms
    }

    /// The name, outside runs, of the reservation of this run.
    pub fn name_of(run: &RunId) -> String {
        format!("{RESERVATIONS_PREFIX}{run}")
    }

    /// The name this reservation is stored under.
    pub fn name(&self) -> String {
        Reservation::name_of(&self.run)
    }

    /// The reservation as it is stored: JSON on one line, then a newline.
    pub fn to_bytes(&self) -> Vec<u8> {
        line_of(&ReservationWire {
            record: RESERVATION_RECORD.to_owned(),
            run: self.run.as_str().to_owned(),
            request: self.request.as_ref().map(|id| id.as_str().to_owned()),
            admission: self.admission,
            reserved_micros: self.micros,
            start_ms: self.start_ms,
        })
    }

    /// Reads a stored reservation, as strictly as an admission is read: the size, the
    /// kind, the shape with no field unknown, left out or twice, a run and a request
    /// that are ids, and the bytes this code writes for what they hold.
    pub fn from_bytes(bytes: &[u8]) -> Result<Reservation, RequestRecordError> {
        kind_of(bytes, RESERVATION_RECORD)?;
        let mut reader = serde_json::Deserializer::from_slice(bytes);
        let wire = ReservationWire::deserialize(&mut reader).map_err(malformed)?;
        reader.end().map_err(malformed)?;
        let inconsistent = RequestRecordError::Inconsistent;
        let reservation = Reservation {
            run: RunId::parse(&wire.run).map_err(inconsistent)?,
            request: match &wire.request {
                Some(id) => Some(RequestId::parse(id).map_err(inconsistent)?),
                None => None,
            },
            admission: wire.admission,
            micros: wire.reserved_micros,
            start_ms: wire.start_ms,
        };
        if reservation.micros == u64::MAX {
            return Err(inconsistent(NOT_AN_AMOUNT.to_owned()));
        }
        if reservation.to_bytes() != bytes {
            return Err(RequestRecordError::NotCanonical);
        }
        Ok(reservation)
    }
}

const A_RESERVATION: &str = "a reservation";

/// Every reservation this generation names, in the order of their names, read with
/// `read`, which gives the content of an entry of either kind.
///
/// **A reservation is what a name of the generation says it is, and nothing else.** The
/// bytes of a reservation can be in the store with no name: an admission that the store
/// refused, because its run's id or its request's id was taken meanwhile, had already
/// moved its content into place. Nothing here reads content except through a name, so
/// such bytes are never counted, and a prune removes them.
///
/// What is read is not trusted: each entry is bounded before it is read, read strictly,
/// and held to be the reservation of the run its name says.
pub fn reservations(
    generation: &Generation,
    read: impl Fn(&Entry) -> store::Result<Vec<u8>>,
) -> Result<Vec<Reservation>, CallError> {
    let unread = |name: &str, why: &dyn std::fmt::Display| CallError::Record {
        what: A_RESERVATION,
        why: format!("under the name {}: {why}", Shown(name)),
    };
    let mut found = Vec::new();
    for (name, entry) in generation.entries().range(RESERVATIONS_PREFIX.to_owned()..) {
        let Some(run) = name.strip_prefix(RESERVATIONS_PREFIX) else {
            break;
        };
        if entry.bytes > REQUEST_RECORD_MAX_BYTES {
            let error = RequestRecordError::TooLarge { bytes: entry.bytes };
            return Err(unread(name, &error));
        }
        let bytes = read(entry).map_err(|error| CallError::Store(StoreFailure::new(error)))?;
        let reservation = Reservation::from_bytes(&bytes).map_err(|error| unread(name, &error))?;
        if reservation.run().as_str() != run {
            return Err(unread(name, &"it is the reservation of another run"));
        }
        found.push(reservation);
    }
    Ok(found)
}

/// Why a ledger did not let a call be admitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declined {
    /// What is left of the allowance does not cover the reservation: how much is left,
    /// in millionths of a US dollar. The call is refused
    /// ([`super::Refusal::OverAllowance`]).
    OverAllowance { left_micros: u64 },
    /// The books of this epoch, the newest, need their owner: its turns are not those
    /// its publications took, or a reservation is frozen and no name can be trusted
    /// for its amount, which stops every epoch (ADR 0016, "Epochs: what a reset
    /// bounds"). Nothing a ledger is asked for is admitted there until the owner
    /// acts: no amount that is left can be stated. The call is refused
    /// ([`super::Refusal::NeedsOwner`]). An executor does not ask for a call that may
    /// cost nothing, so such a call is admitted there all the same.
    NeedsOwner { epoch: u64 },
    /// The ledger could not judge: the store failed, or a record it needed is not one
    /// this build reads, or the ledger itself failed ([`NotJudged`]). This is not a
    /// judgement of the call. It holds a failure of the store or a sentence and
    /// nothing else: a refusal handed back here is a sentence, and the caller is
    /// given [`CallError::Ledger`].
    Failed(NotJudged),
}

impl From<NotJudged> for Declined {
    fn from(failure: NotJudged) -> Declined {
        Declined::Failed(failure)
    }
}

/// So that what a [`Step`] answers can be passed on with `?`. Any other error is kept
/// as what it says ([`NotJudged`]).
impl From<CallError> for Declined {
    fn from(error: CallError) -> Declined {
        Declined::Failed(error.into())
    }
}

/// What the executor asks of a ledger (ADR 0016).
///
/// **How it takes part in an admission.** [`Ledger::reserve`] is called inside
/// admission, once for every attempt at the admission's publication, after the
/// publication was begun and before it is published. It is handed the reservation and
/// the [`Step`]: the generation to judge on, and a way to add records of its own to
/// the same publication. When it returns `Ok`, the executor adds the reservation's
/// record and publishes; the run, the request id's record, the reservation and
/// whatever the ledger added become visible together or not at all. When it declines,
/// the publication is dropped: nothing was published, no run exists, and the request's
/// id is not taken by this request.
///
/// **A ledger cannot return from `reserve` without having made a name.** What it
/// returns, a [`MadeName`], can only be had from [`Step::make`]: a ledger that makes no
/// name has nothing to return, and does not compile.
///
/// ```compile_fail
/// use whizbang_core::contract::{
///     Closing, Declined, Ledger, MadeName, NotJudged, Reread, Reservation, Settled, Step,
/// };
/// struct Careless;
/// impl Ledger for Careless {
///     fn reserve(&self, _: &Reservation, _: &mut Step<'_, '_>) -> Result<MadeName, Declined> {
///         // There is no way to make one but `Step::make`.
///         Ok(MadeName { step: 0, name: String::new() })
///     }
///     fn confirm(&self, _: &Reservation, _: &Reread<'_>) -> Result<bool, NotJudged> {
///         Ok(true)
///     }
///     fn settle(&self, _: &Closing<'_>, _: &mut Step<'_, '_>) -> Result<Settled, NotJudged> {
///         Ok(Settled::Nothing)
///     }
/// }
/// ```
///
/// **Every ledger says what it does at each of the three.** None has a default: a
/// ledger that makes holds and left [`Ledger::confirm`] out would otherwise confirm
/// every admission, on books it never read. One that keeps no books of names, as a
/// test's may, says so in its own words: yes, and [`Settled::Nothing`].
///
/// And the executor refuses one that hands back a name it made in another step
/// ([`CallError::Ledger`]): the name that comes back is one this attempt publishes.
///
/// **A judgement that must hold when the step is published.** The generation a ledger
/// judges on can be an old one by the time the publication is published: another
/// admission over the same store may have reserved meanwhile. The store decides only
/// what a publication asks it to: that a name it makes is not there. So a ledger whose
/// answer depends on what was reserved makes a name that any admission it must not
/// overlook would make too: the next turn of its epoch (ADR 0016, "Nothing is
/// published on a stale balance"). The store then refuses the later of the two whole,
/// and the executor asks the ledger again, on the generation as it is then.
///
/// **What a call to `reserve` is not.** It is not a reservation: an attempt that the
/// store refuses reserved nothing, and the ledger is asked again for the next. Nor is
/// a publication an admission that stands: see [`Ledger::confirm`].
pub trait Ledger: Send + Sync {
    fn reserve(
        &self,
        reservation: &Reservation,
        step: &mut Step<'_, '_>,
    ) -> Result<MadeName, Declined>;

    /// Whether an admission that was published stands (ADR 0016, "An admission is
    /// confirmed before anything runs under it"). The executor calls this once the
    /// admission's publication is durable, with the store's generation read again,
    /// and makes the run's thread, its allowance and the tree's tally only when the
    /// answer is yes. The three conditions are the ledger's to hold on that reading:
    /// the reservation is the record, one hold and nothing else that names the run;
    /// neither the epoch it fell under nor the newest needs its owner; and that
    /// epoch is not over its allowance with this reservation counted in full, which
    /// a call of nothing is not held to.
    ///
    /// It has no default: a ledger that keeps no books of names says yes itself. A
    /// failure to read is no answer, and the executor backs the admission out as it
    /// does one that does not stand.
    fn confirm(&self, reservation: &Reservation, reading: &Reread<'_>) -> Result<bool, NotJudged>;

    /// Closes the reservation of a run nothing called, inside the publication that
    /// ends the run, with the same kind of step: what it makes is published with the
    /// run's record or not at all, a failure of it is a failure of that publication,
    /// and the run then stays admitted and its caller is told. A closing takes the
    /// reservation's next place and its epoch's next turn, so the store refuses the
    /// end's publication whole when either was taken meanwhile, and the executor
    /// asks again.
    ///
    /// It answers [`Settled::Nothing`] where there is nothing for it to close: a
    /// closing already names the reservation, or the ledger keeps no books. It has
    /// no default either.
    fn settle(&self, closing: &Closing<'_>, step: &mut Step<'_, '_>) -> Result<Settled, NotJudged>;
}

/// The store as it is read again once an admission is durable: what
/// [`Ledger::confirm`] folds.
pub struct Reread<'a> {
    generation: &'a Generation,
    contents: &'a dyn Contents,
}

impl<'a> Reread<'a> {
    pub(super) fn new(generation: &'a Generation, contents: &'a dyn Contents) -> Reread<'a> {
        Reread {
            generation,
            contents,
        }
    }

    /// The generation read after the admission's publication.
    pub fn generation(&self) -> &Generation {
        self.generation
    }

    /// The content of an entry of that generation.
    pub fn read(&self, entry: &Entry) -> store::Result<Vec<u8>> {
        self.contents.read_record(entry)
    }
}

/// The reservation a run's end closes.
#[derive(Debug, Clone, Copy)]
pub struct Closing<'a> {
    /// The run nothing called, whose reservation it is.
    pub run: &'a RunId,
    /// What was reserved for it.
    pub reserved_micros: u64,
    /// When the run ended, in milliseconds since the Unix epoch.
    pub at_ms: u64,
}

/// What [`Ledger::settle`] did.
#[derive(Debug)]
pub enum Settled {
    /// It closed the reservation: a name it made in this step.
    Closed(MadeName),
    /// There was nothing for it to close.
    Nothing,
}

/// Why a paid request was not dispatched (ADR 0016, "A dispatch's refusals"): each its
/// own. A request that is refused did not leave, and nothing of it is counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchRefused {
    /// The bound is the largest number there is, which is no amount.
    NoAmount,
    /// The run that sends it can spend less: how much its allowance has left.
    OverRun { left_micros: u64 },
    /// Its reservation, by its names, has less left: how much.
    OverReservation { left_micros: u64 },
    /// Its reservation is frozen: its names are not a set the ledger could have
    /// written, and nothing is dispatched under it; the owner's reset clears it.
    Frozen,
    /// A closing names its reservation: nothing is dispatched after one.
    Closed,
    /// The epoch its reservation falls under needs its owner, or a reservation whose
    /// amount is unknown makes every epoch need it: no new paid work starts.
    NeedsOwner { epoch: u64 },
    /// What the process holds of the tree's reservation does not cover it, whatever
    /// the names say: how much the tree has left in memory.
    OverTree { left_micros: u64 },
    /// It could not be judged or published: the store failed, or the ledger did.
    /// This is not a judgement of the request.
    Failed(NotJudged),
}

impl std::fmt::Display for DispatchRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let left = |f: &mut std::fmt::Formatter<'_>, what: &str, left: &u64| {
            write!(
                f,
                "the request was not sent: {what} {left} millionths of a US dollar left"
            )
        };
        match self {
            DispatchRefused::NoAmount => f.write_str(
                "the request was not sent: its bound is the largest number there is, which \
                 is no amount",
            ),
            DispatchRefused::OverRun { left_micros } => left(f, "its run has", left_micros),
            DispatchRefused::OverReservation { left_micros } => {
                left(f, "its reservation has", left_micros)
            }
            DispatchRefused::Frozen => f.write_str(
                "the request was not sent: its reservation is frozen until its owner \
                 corrects it",
            ),
            DispatchRefused::Closed => {
                f.write_str("the request was not sent: its reservation is closed")
            }
            DispatchRefused::NeedsOwner { epoch } => write!(
                f,
                "the request was not sent: the books of epoch {epoch} need their owner"
            ),
            DispatchRefused::OverTree { left_micros } => left(
                f,
                "the tree it is sent under has, as its process holds it,",
                left_micros,
            ),
            DispatchRefused::Failed(_) => {
                f.write_str("the request was not sent: it could not be judged or recorded")
            }
        }
    }
}

/// A name a ledger made in a step: what [`Step::make`] hands back, and what a ledger
/// returns from [`Ledger::reserve`]. It cannot be made any other way, and is not
/// copied.
#[derive(Debug)]
pub struct MadeName {
    step: u64,
    name: String,
}

impl MadeName {
    /// The name that was made.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// The steps this process has begun: what tells one step's names from another's.
static STEPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// One attempt at a publication, as the ledger sees it: an admission's, or the one
/// that ends a run.
pub struct Step<'a, 's> {
    id: u64,
    publication: &'a mut Publication<'s>,
    generation: &'a Generation,
    contents: &'a dyn Contents,
    secrets: &'a Secrets,
    made: Vec<String>,
}

impl<'a, 's> Step<'a, 's> {
    pub(super) fn new(
        publication: &'a mut Publication<'s>,
        generation: &'a Generation,
        contents: &'a dyn Contents,
        secrets: &'a Secrets,
    ) -> Step<'a, 's> {
        Step {
            id: STEPS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1,
            publication,
            generation,
            contents,
            secrets,
            made: Vec::new(),
        }
    }

    /// The generation to judge on: the store's current one when this attempt began.
    pub fn generation(&self) -> &Generation {
        self.generation
    }

    /// The content of an entry of that generation.
    pub fn read(&self, entry: &Entry) -> store::Result<Vec<u8>> {
        self.contents.read_record(entry)
    }

    /// Every reservation that generation names ([`reservations`]).
    pub fn reserved(&self) -> Result<Vec<Reservation>, CallError> {
        reservations(self.generation, |entry| self.contents.read_record(entry))
    }

    /// Adds a record of the ledger's own to the publication, under a name that is
    /// made: the publication is published with it or not at all, and is refused
    /// whole when the store holds the name by then, in which case the ledger is asked
    /// again.
    ///
    /// The name is one of the ledger's (ADR 0016, "The books are names"): a text that
    /// `spend::Name` reads, and not of the form `spend/reservations/<run>`, which is
    /// the executor's. So it starts with `spend/`, is 200 characters at most, and has
    /// no segment that is empty, `.` or `..`. A name the step's generation already
    /// holds is refused here: it could never be made. A name or a content that holds
    /// a secret is refused, and not shown.
    pub fn make(&mut self, name: &str, bytes: &[u8]) -> Result<MadeName, CallError> {
        let refuse = |why: &str| {
            Err(CallError::Ledger {
                why: why.to_owned(),
            })
        };
        if self.secrets.redact(name) != name || self.secrets.redact_bytes(bytes) != bytes {
            return refuse(
                "it was to write something that must not be written: a record of the \
                 ledger is stored as it is, so it cannot hold a secret",
            );
        }
        match Name::parse(name) {
            Ok(Name::Reservation(_)) => {
                return refuse(&format!(
                    "{} is no name for a record of the ledger: a name under \
                     {RESERVATIONS_PREFIX} is the executor's",
                    Shown(name)
                ));
            }
            Ok(_) => {}
            Err(not) => {
                return refuse(&format!("it was to make a name that is none: {not}"));
            }
        }
        if self.generation.get(name).is_some() {
            return refuse(&format!(
                "it was to make the name {}, which the store holds: a name is made once",
                Shown(name)
            ));
        }
        self.publication
            .create(name, RECORD_KIND, bytes)
            .map_err(|error| CallError::Store(StoreFailure::new(error)))?;
        self.made.push(name.to_owned());
        Ok(MadeName {
            step: self.id,
            name: name.to_owned(),
        })
    }

    /// Adds this record under its own name ([`Step::make`]): the content is the one
    /// way the record is written, and says what its name says.
    pub fn record(&mut self, record: &Record) -> Result<MadeName, CallError> {
        self.make(&record.name().to_string(), &record.to_bytes())
    }

    /// Whether this name was made in this step, and by it.
    pub(super) fn made_here(&self, made: &MadeName) -> bool {
        made.step == self.id && self.made.contains(&made.name)
    }

    /// The names the ledger made in this attempt.
    pub(super) fn into_made(self) -> Vec<String> {
        self.made
    }
}
