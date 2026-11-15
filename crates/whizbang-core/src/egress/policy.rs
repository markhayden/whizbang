//! The policy before a send (`SPEC.md` section 10, "Egress"; ADR 0015, "A policy before
//! a send, and a checked send"): which recipients may receive which classes of data,
//! and which may be probed; and, for a recipient that may not receive, which part of
//! it is not as it was approved (ADR 0019, "The run's recipients").
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

    /// Why this recipient may not receive data of all these classes, and nothing when
    /// it may ([`DispatchPolicy::may_receive`]). The answer holds no value: not an
    /// account, not an origin and not a class.
    ///
    /// A recipient that the policy names whole is short of a class. One it does not
    /// name is compared with every approval of its origin: where one has its account,
    /// the upstreams differ; where none has, the account does; and where no approval
    /// names its origin, it is approved for no class.
    pub fn why_not(&self, recipient: &Recipient, classes: Classes) -> Option<NotApproved> {
        if self.may_receive(recipient, classes) {
            return None;
        }
        if self.of(recipient).is_some() {
            return Some(NotApproved::Classes);
        }
        let same_origin = |a: &&Approved| a.recipient.origin() == recipient.origin();
        let mut named = self.approved.iter().filter(same_origin).peekable();
        if named.peek().is_none() {
            return Some(NotApproved::Classes);
        }
        Some(
            if named.any(|a| a.recipient.account() == recipient.account()) {
                NotApproved::Upstreams
            } else {
                NotApproved::Account
            },
        )
    }

    /// Whether this recipient may be probed.
    pub fn may_probe(&self, recipient: &Recipient) -> bool {
        self.of(recipient).is_some_and(|a| a.probed)
    }
}

/// Why a recipient may not receive what a request holds: which part of it is not as
/// it was approved. It is a kind and a sentence of fixed words, and holds no value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotApproved {
    /// An approval names the recipient's origin, and none names it with the
    /// recipient's account: under another, or under none where the recipient has
    /// one, or under one where it has none.
    Account,
    /// An approval names the recipient's origin and account, with other upstreams.
    Upstreams,
    /// The recipient is not approved for every class the request holds: it is
    /// approved for others or for none, or no approval names its origin at all.
    Classes,
}

impl NotApproved {
    /// The sentence a refusal says it with. An origin or an account is said to be
    /// named by an approval, not to be approved: the approval may be for probing
    /// alone, which approves no data.
    pub fn why(self) -> &'static str {
        match self {
            NotApproved::Account => {
                "the recipient's origin is named by an approval, but not with the account \
                 the backend declares"
            }
            NotApproved::Upstreams => {
                "the recipient's origin and account are named by an approval, but not with \
                 the upstreams the backend declares"
            }
            NotApproved::Classes => {
                "the recipient is not approved for the classes of data the request holds"
            }
        }
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
