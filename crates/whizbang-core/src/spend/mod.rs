//! Spend (`SPEC.md` section 6): the ledger of a store (ADR 0016), and what holds a call
//! to what it may cost.
//!
//! The books are names under `spend/`, each made once. Here are the one parser and
//! writer of those names ([`name`]), the content under each ([`record`]), and a
//! reading of them: each entry by itself, sound or damaged, and what the names of one
//! reservation say, with the recogniser and the most they can mean ([`reading`]). The
//! fold across reservations is [`fold`]: epochs, turns, states, balances and one run's
//! spend. The real ledger ([`StoreLedger`]), which reserves at admission and closes
//! a reservation when its run ends, and the owner's reset ([`reset()`]) are here
//! (plan 1B, S2 to S11, as cut down on 2026-11-15). Nothing here recovers a
//! reservation and nothing corrects one: the tasks that were to, S6 and S12, are
//! cut. The executor's interface to a ledger is `contract::Ledger`.
//!
//! Beside the books are two parts in the admission (`contract::AdmissionPart`), which
//! an executor is given ([`crate::contract::Engine::with_part`]): [`CallCap`], the cap
//! of one call, and [`PreparationAge`], the hour a preparation holds.
//!
//! And quotes (task S4): [`request_bound`], what one request is held to before it
//! leaves. The bound of a need, which covers every request the need says a call may
//! send its backend, is made where a call is prepared.
//!
//! And the meter (task S5): [`Meter`], which stands in the meter's place between a
//! handle and its backend, and debits and records a paid request before it leaves.

mod age;
mod cap;
pub mod fold;
mod ledger;
mod meter;
pub mod name;
mod quote;
pub mod reading;
pub mod record;
mod reset;

pub use age::{AGE_PART, MADE_AFTER_NOW, PREPARATION_HOLDS_MS, PreparationAge, TOO_OLD};
pub use cap::{
    CAP_PART, CallCap, OVER_ITS_OWN_CAP, OVER_THE_CAP, OWN_CAP_ABOVE, OWN_CAP_ZERO,
    SNAPSHOT_CAP_ZERO,
};
pub use ledger::StoreLedger;
pub use meter::Meter;
pub use name::{Amount, Belongs, EpochWord, NAME_LIMIT, Name, NotAName, SPEND_PREFIX, Seal};
pub use quote::{NotQuoted, request_bound};
pub use reading::{
    AtPlace, Correction, Counted, Formless, Names, Reading, RunSpend, Sent, Sum, lowest_free,
    stage_allows,
};
pub use record::{ClosedBy, PlaceBy, Record, SPEND_RECORD_MAX_BYTES, SpendRecordError, TurnBy};
pub use reset::{NotReset, Opened, reset};
