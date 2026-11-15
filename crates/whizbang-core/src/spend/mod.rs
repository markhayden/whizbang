//! Spend (`SPEC.md` section 6): the ledger of a store (ADR 0016), and what holds a
//! call to what it may cost.
//!
//! The books are names under `spend/`, each made once. Here are the one parser and
//! writer of those names ([`name`]), the content under each ([`record`]), and a
//! reading of them: each entry by itself, sound or damaged, and what the names of one
//! reservation say, with the recogniser and the most they can mean ([`reading`]). The
//! fold across reservations, the real ledger, the meter, recovery and the commands
//! are the spend strand's tasks (plan 1B, S1b to S12), each of which adds its module
//! here. The executor's interface to a ledger is `contract::Ledger`.
//!
//! And a part in the admission (`contract::AdmissionPart`), which an executor is
//! given ([`crate::contract::Engine::with_part`]): [`CallCap`], the cap of one call.

mod cap;
pub mod name;
pub mod reading;
pub mod record;

pub use cap::{
    CAP_PART, CallCap, OVER_ITS_OWN_CAP, OVER_THE_CAP, OWN_CAP_ABOVE, OWN_CAP_ZERO,
    SNAPSHOT_CAP_ZERO,
};
pub use name::{Amount, Belongs, EpochWord, NAME_LIMIT, Name, NotAName, SPEND_PREFIX, Seal};
pub use reading::{
    AtPlace, Correction, Counted, Formless, Names, Reading, RunSpend, Sent, Sum, lowest_free,
    stage_allows,
};
pub use record::{ClosedBy, PlaceBy, Record, SPEND_RECORD_MAX_BYTES, SpendRecordError, TurnBy};
