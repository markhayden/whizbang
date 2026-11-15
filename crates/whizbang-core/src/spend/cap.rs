//! The cap of one call (plan 1B, task S3): a part in the admission (ADR 0015, "A part
//! in the admission") that refuses a call whose estimate is over the most one call may
//! reserve.
//!
//! **The cap.** The snapshot the call is admitted under carries the configured cap
//! ([`Snapshot::cap`](crate::contract::Snapshot::cap), from `spend.call_cap_usd`), and
//! a request may carry a cap of its own
//! ([`Request::capped_at`](crate::contract::Request::capped_at)). The cap a call is
//! held to is the lower of the two, or the one there is. A call's own cap lowers the
//! configured one and never raises it: one above it is refused, not lowered to it.
//! With neither, the part lets every call through.
//!
//! **Zero is not a cap.** A request's own cap of zero is refused, whatever the call
//! would cost: a caller that must not spend says so another way. A snapshot whose cap
//! is zero is no judgement of the call but a failure of the part: the config's reader
//! gives no such snapshot.
//!
//! **What is compared** is the estimate of the preparation made at admission, which is
//! what the admission would reserve. An estimate equal to the cap is admitted. One
//! that has no bound ([`Estimate::is_unbounded`](crate::contract::Estimate::is_unbounded))
//! is over every cap, the largest amount there is among them: under a cap, a call
//! whose cost cannot be counted is not admitted.
//!
//! **A call from inside an action** is held to the same cap: it is shown with its
//! caller's snapshot, and with a cap of its own when it came with one.
//!
//! **No refusal quotes an amount:** not the configured cap, not the call's own, not
//! the estimate. The config's reader does not look for a secret in a number, so a cap
//! could spell one; a sentence that names the cap by its key cannot show it. The
//! caller holds the estimate in its preparation and its own cap in its request.
//!
//! **The kinds a refusal is recorded under** (`run/3` has none of the cap's own; ADR
//! 0015, "A refusal of its own kind"). Over the cap is `over_allowance`: the call may
//! cost more than it is allowed to reserve, which is what that kind says of the
//! ledger's allowance; a caller tells the two apart by the part's name. A cap that is
//! zero, or above the configured one, is `invalid`: the request is not one that can be
//! made, whatever it would cost.

use crate::contract::RefusalKind;
use crate::contract::{AdmissionPart, Asking, NotJudged, PartDeclined, PartRefusal, PartStep};

/// What the part is called in a refusal ([`crate::contract::Refusal::ByPart`]).
pub const CAP_PART: &str = "cap";

/// What a call over the configured cap is refused with.
pub const OVER_THE_CAP: &str = "its estimate is over the cap of one call, which the config \
                                sets as spend.call_cap_usd";

/// What a call over its own cap is refused with.
pub const OVER_ITS_OWN_CAP: &str = "its estimate is over the cap the request came with";

/// What a request whose own cap is zero is refused with.
pub const OWN_CAP_ZERO: &str = "the cap the request came with is zero, which is not a cap: a \
                                cap is an amount above zero";

/// What a request whose own cap is above the configured one is refused with.
pub const OWN_CAP_ABOVE: &str = "the cap the request came with is above the cap of one call, \
                                 which the config sets as spend.call_cap_usd: a request lowers \
                                 that cap, and never raises it";

/// What the part fails with under a snapshot whose cap is zero.
pub const SNAPSHOT_CAP_ZERO: &str = "the cap of one call in the snapshot of the config is \
                                     zero, which is not a cap";

/// The part that holds every call to the cap of one call. It holds nothing itself:
/// each cap is read from what the part is shown.
#[derive(Debug, Clone, Copy, Default)]
pub struct CallCap;

fn refused(kind: RefusalKind, why: &'static str) -> PartDeclined {
    match PartRefusal::new(kind, why) {
        Some(refusal) => refusal.into(),
        // Neither kind used here is of a capability request, and no sentence is empty.
        None => NotJudged::saying(why).into(),
    }
}

impl AdmissionPart for CallCap {
    fn name(&self) -> &'static str {
        CAP_PART
    }

    fn look(&self, asking: &Asking<'_>, _: &mut PartStep<'_, '_>) -> Result<(), PartDeclined> {
        let configured = asking.snapshot().cap().map(|cap| cap.micros());
        let own = asking.cap().map(|cap| cap.micros());
        if configured == Some(0) {
            return Err(NotJudged::saying(SNAPSHOT_CAP_ZERO).into());
        }
        if own == Some(0) {
            return Err(refused(RefusalKind::Invalid, OWN_CAP_ZERO));
        }
        // The cap the call is held to, and whose it is.
        let (cap, over) = match (configured, own) {
            (None, None) => return Ok(()),
            (Some(configured), None) => (configured, OVER_THE_CAP),
            (None, Some(own)) => (own, OVER_ITS_OWN_CAP),
            (Some(configured), Some(own)) if own > configured => {
                return Err(refused(RefusalKind::Invalid, OWN_CAP_ABOVE));
            }
            (Some(_), Some(own)) => (own, OVER_ITS_OWN_CAP),
        };
        // The largest amount there is, as a cap, would be equal to a cost with no
        // bound: such a cost is over it all the same.
        let estimate = asking.prepared().estimate();
        if estimate.is_unbounded() || estimate.micros() > cap {
            return Err(refused(RefusalKind::OverAllowance, over));
        }
        Ok(())
    }
}
