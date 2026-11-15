//! Estimates expire (plan 1B, task S8): a part in the admission (ADR 0015, "A part in
//! the admission") that refuses, as stale, a request that came with a preparation made
//! an hour ago or longer, or made at a time later than now.
//!
//! **What is judged.** Only the preparation a request came with
//! ([`Request::expecting`](crate::contract::Request::expecting)): what its caller was
//! shown, estimate and all, with the time it was made by the clock of the executor
//! that made it ([`PreparedCall::made_ms`](crate::contract::PreparedCall::made_ms)).
//! It is compared with the time of the admission
//! ([`Asking::now_ms`](crate::contract::Asking::now_ms)), the one reading of the clock
//! an admission makes. A request that came with no preparation was shown nothing that
//! could be old, and is not looked at.
//!
//! **An hour, and it is a constant** ([`PREPARATION_HOLDS_MS`]). A preparation one
//! millisecond younger is admitted; one of exactly the hour is refused. Nothing
//! shortens it: no variable of the environment, no feature of the build, no argument
//! of the part, which holds nothing. A test moves the executor's clock.
//!
//! **Made after now.** A preparation whose time is later than the admission's was made
//! under a clock that has since been set back, or under another clock: its age cannot
//! be told, so it is refused too.
//!
//! **A repeat is not judged.** A request under an id that was admitted before is
//! answered with its run by the look for the id, before anything is prepared and
//! before any part is asked (ADR 0007, "The look comes first"), however old the
//! preparation it comes with has become.
//!
//! **The kind a refusal is recorded under** (`run/3` has none of the part's own; ADR
//! 0015, "A refusal of its own kind") is `stale`: the kind of a request whose
//! preparation no longer holds, which its caller answers the same way, by preparing
//! again and deciding again. A caller tells this refusal from a preparation that
//! differs by the part's name.

use crate::contract::{
    AdmissionPart, Asking, NotJudged, PartDeclined, PartRefusal, PartStep, RefusalKind,
};

/// How long a preparation holds, in milliseconds: one hour. A request that comes with
/// one this old or older is refused.
pub const PREPARATION_HOLDS_MS: u64 = 60 * 60 * 1000;

/// What the part is called in a refusal ([`crate::contract::Refusal::ByPart`]).
pub const AGE_PART: &str = "age";

/// What a request whose preparation is an hour old or older is refused with.
pub const TOO_OLD: &str = "the preparation the request came with is an hour old or older: \
                           prepare the call again";

/// What a request whose preparation was made at a time later than now is refused with.
pub const MADE_AFTER_NOW: &str = "the preparation the request came with was made at a time \
                                  later than now, so its age cannot be told: prepare the call \
                                  again";

/// The part that refuses a request for the age of the preparation it came with. It
/// holds nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct PreparationAge;

impl AdmissionPart for PreparationAge {
    fn name(&self) -> &'static str {
        AGE_PART
    }

    fn look(&self, asking: &Asking<'_>, _: &mut PartStep<'_, '_>) -> Result<(), PartDeclined> {
        let Some(expected) = asking.expected() else {
            return Ok(());
        };
        let why = match asking.now_ms().checked_sub(expected.made_ms()) {
            None => MADE_AFTER_NOW,
            Some(age_ms) if age_ms >= PREPARATION_HOLDS_MS => TOO_OLD,
            Some(_) => return Ok(()),
        };
        Err(match PartRefusal::new(RefusalKind::Stale, why) {
            Some(refusal) => refusal.into(),
            // The kind is of a call, and neither sentence is empty.
            None => NotJudged::saying(why).into(),
        })
    }
}
