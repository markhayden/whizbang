//! A run's place for facts (ADR 0013, "A run's place for facts").
//!
//! The executor knows what a run was asked, what it called and what it sent. It does
//! not know what a setting resolved to, what a request cost or where data went: the
//! config, the ledger and the gate do, and each is another module. A [`RunFacts`] is
//! where they say so while the run executes. The executor gives each run one, and
//! writes what was noted in the run's record when the run ends.
//!
//! **Nothing in it is durable before the run's end.** What was noted is in memory
//! until the one publication that ends the run. A process that dies first leaves a
//! run whose record names these groups as lost.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::record::{Facts, Handed, Paid, Refused, Setting, Went};
use super::{RefusalKind, Secrets, Value};
use crate::egress::{Account, Classes, Origin};

/// Where a refusal of a request goes: among the refusals of the run, which the
/// executor keeps with the calls the run made that were refused, in one order and
/// within one bound.
type Refuse = Box<dyn Fn(Refused) + Send + Sync>;

/// One run's place for facts. A clone is the same place: whatever holds one notes into
/// the same run. What `{:?}` prints is how many facts of each kind it holds, never one
/// of them.
#[derive(Clone)]
pub struct RunFacts {
    shared: Arc<Shared>,
}

struct Shared {
    /// The executor's secrets: a text is redacted when it is noted.
    secrets: Secrets,
    /// What takes a refusal, for a run of an executor.
    refuse: Option<Refuse>,
    noted: Mutex<Noted>,
}

struct Noted {
    facts: Facts,
    /// Whether the run still takes a note: until its end began.
    open: bool,
}

impl Default for RunFacts {
    /// A place that belongs to no run of an executor, as a context that other code
    /// made has: what is noted in it is kept nowhere, and a refusal is not taken.
    fn default() -> RunFacts {
        RunFacts::over(Secrets::none(), None)
    }
}

impl std::fmt::Debug for RunFacts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let noted = self.noted();
        let (settings, paid, went) = noted.facts.counts();
        write!(
            f,
            "RunFacts({settings} settings, {paid} paid requests, {went} entries of where \
             data went, {})",
            if noted.open { "open" } else { "closed" }
        )
    }
}

impl RunFacts {
    fn over(secrets: Secrets, refuse: Option<Refuse>) -> RunFacts {
        RunFacts {
            shared: Arc::new(Shared {
                secrets,
                refuse,
                noted: Mutex::new(Noted {
                    facts: Facts::none(),
                    open: true,
                }),
            }),
        }
    }

    /// The place of one run of an executor with these secrets, whose refusals of a
    /// request go to `refuse`.
    pub(super) fn of_a_run(secrets: Secrets, refuse: Refuse) -> RunFacts {
        RunFacts::over(secrets, Some(refuse))
    }

    fn noted(&self) -> MutexGuard<'_, Noted> {
        let noted = self.shared.noted.lock();
        noted.unwrap_or_else(PoisonError::into_inner)
    }

    /// Notes one fact while the run still takes notes. The look and the note are one
    /// step under the place's lock, so a note is either in the run's record or was
    /// answered no: none is taken and then left out.
    fn note(&self, take: impl FnOnce(&mut Facts) -> bool) -> bool {
        let mut noted = self.noted();
        noted.open && take(&mut noted.facts)
    }

    /// Notes a resolved setting the run ran under. A text has the executor's secrets
    /// taken out here, while it is whole, and is then cut ([`Setting::new`]).
    ///
    /// False, and nothing is noted: when the name is not one or holds a secret; when a
    /// setting of this name is noted already, or the record lists as many as it may;
    /// and when the run's end has begun.
    pub fn setting(&self, name: &str, value: &Value) -> bool {
        let Some(setting) = Setting::new(name, value, &self.shared.secrets) else {
            return false;
        };
        self.note(|facts| facts.setting(setting))
    }

    /// Notes what one paid request of the run cost. False, and nothing is noted: when
    /// one is noted already for this request or a later one, or the record lists as
    /// many as it may; and when the run's end has begun. One that names a request the
    /// run did not send is taken here and left out of the record.
    pub fn paid(&self, paid: Paid) -> bool {
        self.note(|facts| facts.paid(paid))
    }

    /// Notes that data of these classes was handed to an origin, or that a send to it
    /// was refused. The account is written cut, and not at all when it holds a secret
    /// of the executor's ([`Went::new`]). An origin, account and outcome that is noted
    /// already gains these classes.
    ///
    /// False, and nothing is noted: when the entry is new and the record lists as many
    /// as it may; and when the run's end has begun.
    pub fn went(
        &self,
        origin: &Origin,
        account: Option<&Account>,
        classes: Classes,
        outcome: Handed,
    ) -> bool {
        let went = Went::new(origin, account, classes, outcome, &self.shared.secrets);
        self.note(|facts| facts.went(went))
    }

    /// Notes that a capability request of the run was refused and not sent. It is
    /// listed among the run's refusals (`outcome.refusals`), in the order they were
    /// refused and within their bound: past it, it is counted and not listed. The
    /// sentence has the executor's secrets taken out, and is cut.
    ///
    /// False, and nothing is noted: when the kind is not one of a request, or the
    /// sentence is empty; when the place belongs to no run of an executor; and when
    /// the run's end has begun.
    pub fn refused(&self, kind: RefusalKind, why: &str) -> bool {
        let refused = Refused::of_a_request(kind, why, &self.shared.secrets);
        let (Some(refused), Some(refuse)) = (refused, &self.shared.refuse) else {
            return false;
        };
        // Under the same lock as the look: the refusal is with the run's others
        // before the run's end can begin.
        let noted = self.noted();
        if noted.open {
            refuse(refused);
        }
        noted.open
    }

    /// The run's end begins: from here on no note is taken. What was noted is handed
    /// over, once.
    pub(super) fn close(&self) -> Facts {
        let mut noted = self.noted();
        noted.open = false;
        std::mem::take(&mut noted.facts)
    }
}
