//! What a request cost, and how far a failed one got (ADR 0010, "How far a request got,
//! and what it cost").
//!
//! A [`Charge`] is what a request was charged, in millionths of a US dollar. A response
//! may carry one; a response without one cost its bound, which is the ledger's to apply.
//! A [`Stage`] is how far a failed request got, which decides whether it may have been
//! charged: not sent, answered, or sent with no answer that settles it.

use super::Unfit;
use super::price::whole;

/// What a request was charged, in millionths of a US dollar. It may be over the bound
/// that was quoted: it is a fact, and nothing here compares the two. What `{:?}`
/// prints is the amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Charge {
    micros: u64,
}

impl Charge {
    /// A charge of this many millionths of a US dollar. A fraction of a millionth is
    /// rounded up, so that what is counted is never under what was charged. Refused: a
    /// number that is not finite, is less than nothing, or is 2 to the power 64 or
    /// more, so that no charge is the largest amount, which is no amount.
    pub fn of_micros(micros: f64) -> Result<Charge, Unfit> {
        Ok(Charge {
            micros: whole("a charge", micros)?,
        })
    }

    /// The charge, in millionths of a US dollar.
    pub fn micros(&self) -> u64 {
        self.micros
    }
}

/// How far a request that failed got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// The request did not leave whizbang: nothing was handed to anyone. Nothing was
    /// charged.
    NotSent,
    /// An answer came that settles the request, and may say what it cost.
    Answered,
    /// The request left, or may have, and no answer settles it: it may have been
    /// charged anything up to its bound.
    Unresolved,
}

impl Stage {
    /// Every stage, in the order a request passes them.
    pub const ALL: [Stage; 3] = [Stage::NotSent, Stage::Answered, Stage::Unresolved];

    /// The stage's name, as a record spells it.
    pub fn name(self) -> &'static str {
        match self {
            Stage::NotSent => "not_sent",
            Stage::Answered => "answered",
            Stage::Unresolved => "unresolved",
        }
    }
}
