//! Spend (`SPEC.md` section 6): the ledger of a store, and what holds a call to what
//! it may cost.
//!
//! The plan of checkpoint 1B (`docs/plans/`) builds it here: the ledger's records and
//! the names they are published under, the fold that works out what is allowed,
//! reserved, spent and unresolved, and what reserves at admission, records a dispatch
//! before a request leaves and settles each attempt. Until those tasks merge, the
//! executor's interface to a ledger is `contract::Ledger` and nothing implements it
//! outside the test kit.
//!
//! What is here so far are two parts in the admission (`contract::AdmissionPart`),
//! which an executor is given ([`crate::contract::Engine::with_part`]): [`CallCap`],
//! the cap of one call, and [`PreparationAge`], the hour a preparation holds.

mod age;
mod cap;

pub use age::{AGE_PART, MADE_AFTER_NOW, PREPARATION_HOLDS_MS, PreparationAge, TOO_OLD};
pub use cap::{
    CAP_PART, CallCap, OVER_ITS_OWN_CAP, OVER_THE_CAP, OWN_CAP_ABOVE, OWN_CAP_ZERO,
    SNAPSHOT_CAP_ZERO,
};
