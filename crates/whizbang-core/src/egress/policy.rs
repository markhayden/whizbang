//! The policy before a send (`SPEC.md` section 10, "Egress"; ADR 0015, "A policy before
//! a send, and a checked send"): which recipients may receive which classes of data,
//! and which may be probed.
//!
//! It is a value, and holds no credential. The executor holds one and asks it before
//! anything is handed to a sender. **One that was told nothing approves nothing.** What
//! a config's `egress` table makes of it is the egress strand's.

use std::fmt;

use super::{Classes, NotAllowed, Recipient};

/// The most recipients a policy names.
pub const APPROVED_LIMIT: usize = 64;

/// What one recipient is approved for.
#[derive(Clone, PartialEq, Eq)]
struct Approved {
    recipient: Recipient,
    /// The classes of data it may receive: none, for one that may only be probed.
    classes: Classes,
    probed: bool,
}

/// Which recipients may receive which classes of data, and which may be probed. A
/// recipient is the one that was approved only when its origin, its account and its
/// upstreams are all the same (ADR 0012): the same endpoint under another account, or
/// forwarding to one more origin, is another recipient, and is approved for nothing.
///
/// What `{:?}` prints is each recipient as a recipient is always printed, its origin
/// and its account cut, with the classes it may receive and whether it may be probed.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct DispatchPolicy {
    approved: Vec<Approved>,
}

impl DispatchPolicy {
    /// The policy that approves nothing: no recipient may receive anything, and none
    /// may be probed. It is what an executor has until it is given another.
    pub fn none() -> DispatchPolicy {
        DispatchPolicy::default()
    }

    /// Whether the policy approves nothing at all.
    pub fn is_empty(&self) -> bool {
        self.approved.is_empty()
    }

    /// Every recipient the policy names, for a class of data or for a probe.
    pub(crate) fn recipients(&self) -> impl Iterator<Item = &Recipient> {
        self.approved.iter().map(|approved| &approved.recipient)
    }

    fn entry(&mut self, recipient: Recipient) -> Result<&mut Approved, NotAllowed> {
        let place = self.approved.iter().position(|a| a.recipient == recipient);
        let place = match place {
            Some(place) => place,
            None if self.approved.len() >= APPROVED_LIMIT => return Err(NotAllowed::TooMany),
            None => {
                self.approved.push(Approved {
                    recipient,
                    classes: Classes::none(),
                    probed: false,
                });
                self.approved.len() - 1
            }
        };
        Ok(&mut self.approved[place])
    }

    /// The same policy, in which this recipient may receive these classes of data, and
    /// those it was approved for before. Refused: no class at all, and one more
    /// recipient than [`APPROVED_LIMIT`].
    pub fn approving(
        mut self,
        recipient: Recipient,
        classes: Classes,
    ) -> Result<DispatchPolicy, NotAllowed> {
        if classes.is_empty() {
            return Err(NotAllowed::Empty);
        }
        let entry = self.entry(recipient)?;
        entry.classes = classes.iter().fold(entry.classes, Classes::with);
        Ok(self)
    }

    /// The same policy, in which this recipient may be probed. That is approved apart
    /// from any class of data: a recipient that may be probed may receive nothing for
    /// it. Refused: one more recipient than [`APPROVED_LIMIT`].
    pub fn approving_probes_of(
        mut self,
        recipient: Recipient,
    ) -> Result<DispatchPolicy, NotAllowed> {
        self.entry(recipient)?.probed = true;
        Ok(self)
    }

    fn of(&self, recipient: &Recipient) -> Option<&Approved> {
        self.approved.iter().find(|a| a.recipient == *recipient)
    }

    /// Whether this recipient may receive data of all these classes. A request that
    /// holds no data at all is still a request to the recipient: it is approved only
    /// for a recipient that may receive some class.
    pub fn may_receive(&self, recipient: &Recipient, classes: Classes) -> bool {
        self.of(recipient)
            .is_some_and(|a| !a.classes.is_empty() && a.classes.covers(classes))
    }

    /// Whether this recipient may be probed.
    pub fn may_probe(&self, recipient: &Recipient) -> bool {
        self.of(recipient).is_some_and(|a| a.probed)
    }
}

impl fmt::Debug for Approved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} may receive {:?}", self.recipient, self.classes)?;
        if self.probed {
            write!(f, " and may be probed")?;
        }
        Ok(())
    }
}

impl fmt::Debug for DispatchPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DispatchPolicy")?;
        f.debug_list().entries(&self.approved).finish()
    }
}
