//! The ledger's part in admission (`SPEC.md` section 6; ADR 0007, "The reservation").
//!
//! A call is admitted with a [`Reservation`] of what it may cost at most: a record,
//! kind `reservation/1`, made in the one publication that makes the run and the record
//! of its request id, under a name that belongs to no run. The [`Ledger`] is asked in
//! that step, before anything is published, and may decline: then nothing is published.
//!
//! What is here is what admission calls. The ledger itself, with its allowance, its
//! dispatch records and settlement, is checkpoint 1B's; until then the only ledger is
//! the test kit's fake, and an executor that was given none reserves nothing.

use serde::{Deserialize, Serialize};

use crate::artifact::Contents;
use crate::capability::Shown;
use crate::identity::Digest;
use crate::store::{self, Entry, Generation, Publication};

use super::record::RECORD_KIND;
use super::request::{NOT_AN_AMOUNT, REQUEST_RECORD_MAX_BYTES, RequestRecordError};
use super::request::{RequestId, kind_of, line_of, malformed, present};
use super::{CallError, RunId, Secrets, StoreFailure};

/// The kind of a reservation record, with its version.
pub const RESERVATION_RECORD: &str = "reservation/1";

/// What the name of every record of spend starts with, outside runs.
pub const SPEND_PREFIX: &str = "spend/";

/// What the name of every reservation starts with: the run's id follows.
pub const RESERVATIONS_PREFIX: &str = "spend/reservations/";

/// The longest name a ledger may make in an admission, in characters.
pub const LEDGER_NAME_LIMIT: usize = 200;

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
    /// The ledger could not judge: the store failed, or a record it needed is not one
    /// this build reads, or the ledger itself failed ([`CallError::Ledger`]). This is
    /// not a judgement of the call.
    Failed(CallError),
}

impl From<CallError> for Declined {
    fn from(error: CallError) -> Declined {
        Declined::Failed(error)
    }
}

/// What admission asks before a run is made: may this be reserved?
///
/// **How it takes part in the step.** [`Ledger::reserve`] is called inside admission,
/// once for every attempt at the admission's publication, after the publication was
/// begun and before it is published. It is handed the reservation and the [`Step`]:
/// the generation to judge on, and a way to add records of its own to the same
/// publication. When it returns `Ok`, the executor adds the reservation's record and
/// publishes; the run, the request id's record, the reservation and whatever the
/// ledger added become visible together or not at all. When it declines, the
/// publication is dropped: nothing was published, no run exists, and the request's id
/// is not taken by this request.
///
/// **A judgement that must hold when the step is published.** The generation a ledger
/// judges on can be an old one by the time the publication is published: another
/// admission over the same store may have reserved meanwhile. The store decides only
/// what a publication asks it to: that a name it makes is not there. So a ledger whose
/// answer depends on what was reserved makes a name ([`Step::make`]) that any
/// admission it must not overlook would make too. The store then refuses the later of
/// the two whole, and the executor asks the ledger again, on the generation as it is
/// then.
///
/// **What a call to `reserve` is not.** It is not a reservation: an attempt that the
/// store refuses reserved nothing, and the ledger is asked again for the next. What
/// was reserved is what the current generation names ([`reservations`]), so a ledger
/// keeps no count of its own.
pub trait Ledger: Send + Sync {
    fn reserve(&self, reservation: &Reservation, step: &mut Step<'_, '_>) -> Result<(), Declined>;
}

/// One attempt at an admission, as the ledger sees it.
pub struct Step<'a, 's> {
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

    /// Every reservation that generation names ([`reservations`]).
    pub fn reserved(&self) -> Result<Vec<Reservation>, CallError> {
        reservations(self.generation, |entry| self.contents.read_record(entry))
    }

    /// Adds a record of the ledger's own to the admission's publication, under a name
    /// that is made: the admission is published with it or not at all, and is refused
    /// whole when the store holds the name by then, in which case the ledger is asked
    /// again.
    ///
    /// The name starts with `spend/` and not with `spend/reservations/`, which is the
    /// executor's; it is of the ASCII letters, the digits, `_`, `-`, `.`, `:` and `/`,
    /// 200 characters at most. A name the step's generation already holds is refused
    /// here: it could never be made. A name or a content that holds a secret is
    /// refused, and not shown.
    pub fn make(&mut self, name: &str, bytes: &[u8]) -> Result<(), CallError> {
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
        let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/');
        if !name.starts_with(SPEND_PREFIX)
            || name.starts_with(RESERVATIONS_PREFIX)
            || name.len() > LEDGER_NAME_LIMIT
            || !name.chars().all(plain)
        {
            return refuse(&format!(
                "{} is no name for a record of the ledger: its names start with \
                 {SPEND_PREFIX} and not with {RESERVATIONS_PREFIX}, and are of the ASCII \
                 letters, the digits, _, -, ., : and /, {LEDGER_NAME_LIMIT} characters at most",
                Shown(name)
            ));
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
        Ok(())
    }

    /// The names the ledger made in this attempt.
    pub(super) fn into_made(self) -> Vec<String> {
        self.made
    }
}
