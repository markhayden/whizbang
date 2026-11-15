//! The check behind a send (`SPEC.md` section 10, "Egress"; ADR 0019; plan 1B, task
//! E2): the [`HopCheck`] an executor that sends is given.
//!
//! The executor asks four things itself before it asks a check, of every address
//! (ADR 0015): the backend declared a recipient, the request brings no credential,
//! the address is at the recipient's own origin or one of its upstreams, and the
//! policy approves the recipient for every class of data the request holds. A check
//! is asked last, and can only refuse.
//!
//! **This check approves a first address, and no address an answer pointed to.**
//! Following an answer that points elsewhere is task E3's.

use super::{Denied, Hop, HopCheck};

/// What the check answers once an answer has pointed its backend elsewhere.
const MOVED: &str = "an answer pointed this backend elsewhere in this run, and the address \
                     of no request after that is approved";

/// The check behind a send. It approves an address while no answer has pointed its
/// backend elsewhere in the run, and refuses every address after one has.
///
/// It goes by [`Hop::total_moves`], which never goes back, and not by the moves in a
/// row: a request of the same backend that is answered meanwhile sets those back to
/// nothing, and the address an answer pointed to would then pass for a first one.
/// So once one answer of a backend has pointed elsewhere, every later request of
/// that backend in that run is refused, a first address among them.
///
/// It holds nothing and counts nothing: asked for a run that is over, or twice for
/// one request, it changes nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct SendCheck;

impl SendCheck {
    pub fn new() -> SendCheck {
        SendCheck
    }
}

impl HopCheck for SendCheck {
    fn look(&self, hop: &Hop<'_>) -> Result<(), Denied> {
        if hop.total_moves() > 0 {
            return Err(Denied::because(MOVED));
        }
        Ok(())
    }
}
