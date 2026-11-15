//! Spend (`SPEC.md` section 6; plan 1B, the spend strand): the ledger's records, its
//! fold, and what reserves, dispatches and settles. Each task of the strand adds its
//! module here; the names, the records and the reading of one reservation are task
//! I2's (ADR 0016).

mod age;
mod cap;
mod fold;
mod name;
mod reading;
mod record;
mod support;

use std::num::NonZeroU64;

use whizbang_core::contract::RunId;
use whizbang_core::spend::Amount;

pub(crate) fn run(text: &str) -> RunId {
    RunId::parse(text).unwrap()
}

pub(crate) fn n(number: u64) -> NonZeroU64 {
    NonZeroU64::new(number).unwrap()
}

pub(crate) fn amount(micros: u64) -> Amount {
    Amount::new(micros).unwrap()
}
