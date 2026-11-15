//! The policy a config's `egress` table makes (`SPEC.md` section 10, "Egress"; ADR
//! 0019; plan 1B, task E2).
//!
//! A config approves recipients (ADR 0011: an [`Approval`] is a recipient, the classes
//! of data it may receive, and whether it may be probed). The executor holds a
//! [`DispatchPolicy`] (ADR 0015). This is the one way from the first to the second.
//! **Approvals that cannot all be taken make no policy:** there is no policy of some
//! of them.

use super::{APPROVED_LIMIT, DispatchPolicy, NotAllowed};
use crate::config::Approval;

impl DispatchPolicy {
    /// The policy these approvals make: each recipient may receive the classes of
    /// data its approval lists, and may be probed when its approval says so. A
    /// recipient that is approved for no class and is not to be probed is approved
    /// for nothing, as one that was never named is.
    ///
    /// **A backend's recipient is the one an approval names only when its origin,
    /// its account and its upstreams are all the approval's** (ADR 0012). So two
    /// approvals of one origin under two accounts are two recipients, each with its
    /// own classes, and a backend that declares no account has the approval that
    /// names no account, and no other.
    ///
    /// Refused, with no policy made:
    ///
    /// - more approvals than a policy names recipients ([`APPROVED_LIMIT`]), as
    ///   [`NotAllowed::TooMany`]: no more than one past the limit is looked at;
    /// - an origin and an account that an earlier approval named, whatever the
    ///   upstreams of either, as [`NotAllowed::Twice`]. A config's reader refuses
    ///   such a file (ADR 0011); approvals that come from two files are held to the
    ///   same rule here, because which of the two held would be a guess.
    pub fn from_approvals<'a>(
        approvals: impl IntoIterator<Item = &'a Approval>,
    ) -> Result<DispatchPolicy, NotAllowed> {
        let mut seen: Vec<&Approval> = Vec::new();
        let mut policy = DispatchPolicy::none();
        for approval in approvals {
            if seen.len() >= APPROVED_LIMIT {
                return Err(NotAllowed::TooMany);
            }
            let named = |earlier: &&Approval| {
                earlier.origin() == approval.origin() && earlier.account() == approval.account()
            };
            if seen.iter().any(named) {
                return Err(NotAllowed::Twice);
            }
            seen.push(approval);
            let recipient = approval.recipient();
            if !approval.classes().is_empty() {
                policy = policy.approving(recipient.clone(), approval.classes())?;
            }
            if approval.may_be_probed() {
                policy = policy.approving_probes_of(recipient.clone())?;
            }
        }
        Ok(policy)
    }
}
