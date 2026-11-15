//! Quotes (plan 1B, task S4; `SPEC.md` section 6): a paid request has an enforceable
//! bound, or it is not sent.
//!
//! **At admission.** A roster answers [`Backends::bound`](crate::contract::Backends::bound)
//! with the quote of one request, from the price a backend declares (ADR 0010), and
//! does not know how many requests a call sends. The need does: an action states it
//! ([`Requirement::requests`](crate::contract::Requirement::requests), one unless it
//! says more), and preparing a call makes the bound of a need the bound of one request
//! times that number, so what an admission reserves covers each of them. A backend
//! that offers an estimate only has the amount that is no bound, and a call that
//! needs one is refused where an executor reserves
//! ([`Refusal::NoBound`](crate::contract::Refusal::NoBound)).
//!
//! **Before a request leaves.** [`request_bound`] is what one request is held to
//! against what its run has left: what the meter asks first (task S5). It reads the
//! declaration of the request's own backend, as a wrapper is told it
//! ([`Wrapped::declaration`](crate::capability::Wrapped)). A request to a local
//! backend that states no price, and one to a backend that says it is free, are not
//! paid: nothing is held for them. It debits nothing and records nothing; the debit,
//! which is one step with the look under the allowance's lock, is
//! [`Allowance::dispatch`](crate::contract::Allowance::dispatch).

use std::fmt;

use crate::capability::{Declaration, QuoteKind};
use crate::contract::RefusalKind;

use super::name::Amount;

/// Why a request is not sent, by what its backend quotes and what its run has left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotQuoted {
    /// Its backend states no enforceable bound: an estimate only, with what it
    /// estimates, or no price at all though it names a recipient.
    NoBound { estimated_micros: Option<u64> },
    /// Its bound is over what its run has left.
    OverRun { bound_micros: u64, left_micros: u64 },
}

impl NotQuoted {
    /// The kind a run's record lists the refusal under (ADR 0013).
    pub fn kind(&self) -> RefusalKind {
        match self {
            NotQuoted::NoBound { .. } => RefusalKind::RequestNoBound,
            NotQuoted::OverRun { .. } => RefusalKind::RequestOverAllowance,
        }
    }
}

impl fmt::Display for NotQuoted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotQuoted::NoBound {
                estimated_micros: Some(about),
            } => write!(
                f,
                "the request was not sent: its backend states an estimate only, of {about} \
                 millionths of a US dollar, and no bound that can be enforced"
            ),
            NotQuoted::NoBound {
                estimated_micros: None,
            } => f.write_str(
                "the request was not sent: its backend names a recipient and states no \
                 price, so nothing bounds what the request may cost",
            ),
            NotQuoted::OverRun {
                bound_micros,
                left_micros,
            } => write!(
                f,
                "the request was not sent: it may cost {bound_micros} millionths of a US \
                 dollar, and its run has {left_micros} left"
            ),
        }
    }
}

/// What a request to this backend is held to, against what its run has left
/// ([`Allowance::left_micros`](crate::contract::Allowance::left_micros)): its bound,
/// which is what is debited and recorded before it leaves; or none, for a request
/// that is not paid: one to a local backend that states no price, and one to a
/// backend that says it is free.
///
/// Refused: a request to a backend that states an estimate only, or that names a
/// recipient and states no price; and one whose bound is over what its run has left.
/// A run with no amount, which nothing was reserved for, is held to no amount.
pub fn request_bound(
    declaration: &Declaration,
    left_micros: Option<u64>,
) -> Result<Option<Amount>, NotQuoted> {
    let Some(quote) = declaration.quote() else {
        return match declaration.is_local() {
            true => Ok(None),
            false => Err(NotQuoted::NoBound {
                estimated_micros: None,
            }),
        };
    };
    let bound_micros = match quote.kind() {
        QuoteKind::Free => return Ok(None),
        QuoteKind::EstimateOnly => {
            return Err(NotQuoted::NoBound {
                estimated_micros: quote.estimated_micros(),
            });
        }
        QuoteKind::Bound => quote.bound_micros().unwrap_or(u64::MAX),
    };
    // A quote is under the largest number, which is no amount: one that were not
    // is no bound.
    let Some(bound) = Amount::new(bound_micros) else {
        return Err(NotQuoted::NoBound {
            estimated_micros: None,
        });
    };
    match left_micros {
        Some(left_micros) if bound_micros > left_micros => Err(NotQuoted::OverRun {
            bound_micros,
            left_micros,
        }),
        _ => Ok(Some(bound)),
    }
}
