//! A fake of the ledger that takes part in admission (ADR 0007, "The reservation"). It
//! spends nothing and knows no price: it has an allowance a test sets, declines what
//! the allowance does not cover, and counts.
//!
//! It counts two things, and they are not the same. [`FakeLedger::asked`] is how many
//! times admission asked it: once for every attempt at an admission's publication, the
//! attempts the store refused among them. [`FakeLedger::reservations`] is what was
//! reserved: the reservations the store's current generation names, read from the
//! store each time. The fake keeps no list of its own, so what it says was reserved is
//! what a ledger opened after a crash would find.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use whizbang_core::artifact::Contents;
use whizbang_core::contract::{CallError, Declined, Ledger, Reservation, Step, reservations};

/// What the name of every record the fake adds to an admission starts with: a count
/// follows.
pub const SEQUENCE_PREFIX: &str = "spend/fake/";

/// A fake ledger. It may be shared across threads and across executors.
#[derive(Debug, Default)]
pub struct FakeLedger {
    /// What may be reserved in all, in millionths of a US dollar; none is no limit.
    allowance: Mutex<Option<u64>>,
    asked: AtomicU64,
}

impl FakeLedger {
    /// A ledger that declines nothing.
    pub fn new() -> FakeLedger {
        FakeLedger::default()
    }

    /// A ledger that lets this much be reserved in all, in millionths of a US dollar.
    pub fn with_allowance(micros: u64) -> FakeLedger {
        let ledger = FakeLedger::new();
        ledger.allow(Some(micros));
        ledger
    }

    /// Sets the allowance, or takes the limit away.
    pub fn allow(&self, micros: Option<u64>) {
        *self
            .allowance
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = micros;
    }

    /// How many times admission asked this ledger for a reservation. An attempt the
    /// store refused, or the ledger declined, was asked for too.
    pub fn asked(&self) -> u64 {
        self.asked.load(Ordering::SeqCst)
    }

    /// What was reserved: every reservation the current generation of this store
    /// names. Content that no name leads to is not looked at.
    pub fn reservations(&self, contents: &dyn Contents) -> Result<Vec<Reservation>, CallError> {
        let generation = contents.snapshot().map_err(|error| CallError::Ledger {
            why: error.without_paths(),
        })?;
        reservations(&generation, |entry| contents.read_record(entry))
    }
}

impl Ledger for FakeLedger {
    fn reserve(&self, reservation: &Reservation, step: &mut Step<'_, '_>) -> Result<(), Declined> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        let held = step.reserved()?;
        let allowance = *self
            .allowance
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(allowance) = allowance {
            let reserved = held
                .iter()
                .fold(0u64, |sum, one| sum.saturating_add(one.micros()));
            let left = allowance.saturating_sub(reserved);
            if reservation.micros() > left {
                return Err(Declined::OverAllowance { left_micros: left });
            }
        }
        // The judgement above is of the generation the step shows, and another
        // admission may reserve before this one is published. Every admission through
        // this ledger makes the name numbered one past the reservations it saw: of two
        // that saw the same, the store refuses the later whole, and it is asked again.
        let name = format!("{SEQUENCE_PREFIX}{}", held.len() + 1);
        step.make(&name, format!("{}\n", reservation.run()).as_bytes())?;
        Ok(())
    }
}
