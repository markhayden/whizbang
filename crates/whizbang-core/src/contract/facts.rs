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

use super::record::{
    Attempt, AttemptsEnded, Facts, Handed, Paid, Refused, Replayed, Setting, Started, Went,
};
use super::{RefusalKind, Secrets, Value};
use crate::egress::{Account, Classes, DataClass, Origin};

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
    /// For each entry listed as sent, how many notes of it are held, and how many of
    /// them hold each class: what is left of the entry when one is taken back.
    sent: Vec<Sent>,
}

/// The notes held of one entry that is listed as sent.
struct Sent {
    origin: String,
    account: Option<String>,
    notes: u64,
    /// By the class's place among the four.
    holding: [u64; 4],
}

/// The four classes of data, in the order a record lists them.
const CLASSES: [DataClass; 4] = [
    DataClass::SourceImages,
    DataClass::DerivedImages,
    DataClass::Prompts,
    DataClass::Evidence,
];

/// The note that data is handed to an origin, taken before anything is handed over
/// (ADR 0015): the entry is in the run's record from here on, whatever becomes of
/// the request, unless the note is taken back because nothing left.
pub(crate) struct Sending {
    facts: RunFacts,
    origin: String,
    account: Option<String>,
    classes: Classes,
}

impl Sending {
    /// Nothing was handed over after all: the note is taken back. The entry keeps
    /// what other notes of it hold, and goes when this was the only one. Once the
    /// run's end has begun the record is made, and the entry stays as it is listed.
    pub(crate) fn take_back(self) {
        let mut noted = self.facts.noted();
        if !noted.open {
            return;
        }
        let same = |sent: &Sent| sent.origin == self.origin && sent.account == self.account;
        let Some(place) = noted.sent.iter().position(same) else {
            return;
        };
        let sent = &mut noted.sent[place];
        sent.notes = sent.notes.saturating_sub(1);
        for (at, class) in CLASSES.iter().enumerate() {
            if self.classes.iter().any(|held| held == *class) {
                sent.holding[at] = sent.holding[at].saturating_sub(1);
            }
        }
        let left = (sent.notes > 0).then(|| {
            let held = CLASSES.iter().zip(sent.holding);
            let held = held.filter(|(_, notes)| *notes > 0);
            held.fold(Classes::none(), |classes, (class, _)| classes.with(*class))
        });
        if left.is_none() {
            noted.sent.remove(place);
        }
        noted
            .facts
            .sent_now(&self.origin, self.account.as_deref(), left);
    }
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
                    sent: Vec::new(),
                }),
            }),
        }
    }

    /// The place of one run of an executor with these secrets, whose refusals of a
    /// request go to `refuse`.
    pub(super) fn of_a_run(secrets: Secrets, refuse: Refuse) -> RunFacts {
        RunFacts::over(secrets, Some(refuse))
    }

    /// Whether the text holds a secret of the run's executor: those it was built with
    /// and those of the snapshot the run was admitted under.
    pub(crate) fn holds_secret(&self, text: &str) -> bool {
        self.shared.secrets.redact(text) != text
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

    /// Notes where a recipe's run started and what it was given (`run/6`). False, and
    /// nothing is noted: when one is noted already, when a record would not hold it,
    /// and when the run's end has begun.
    pub fn started(&self, started: Started) -> bool {
        self.note(|facts| facts.started(started))
    }

    /// Notes the run this run replays (`run/7`, ADR 0048). False, and nothing is noted:
    /// when one is noted already, when a record would not hold it, and when the run's
    /// end has begun.
    pub fn replayed(&self, replayed: Replayed) -> bool {
        self.note(|facts| facts.replayed(replayed))
    }

    /// Notes a loop's next attempt (`run/6`): the reason it has no verdict, when it
    /// has none, with the executor's secrets taken out. False, and nothing is noted:
    /// when it is not the next by its number, when the attempts were said to have
    /// ended, when a record would not hold it, and when the run's end has begun.
    pub fn attempt(&self, mut attempt: Attempt) -> bool {
        if let Some(why) = &mut attempt.unchecked {
            *why = self.shared.secrets.redact(why);
        }
        self.note(|facts| facts.attempt(attempt))
    }

    /// Notes which of a loop's attempts was kept, by its number, and why the attempts
    /// ended, in one line with the executor's secrets taken out (`run/6`), and as a word
    /// of a closed set when one is given (`run/8`). False, and nothing is noted: when it
    /// was said already, when a record would not hold it, and when the run's end has
    /// begun.
    pub fn attempts_ended(
        &self,
        kept: Option<u32>,
        why: &str,
        reason: Option<AttemptsEnded>,
    ) -> bool {
        let why = self.shared.secrets.redact(why);
        self.note(|facts| facts.attempts_ended(kept, why, reason))
    }

    /// Notes what one paid request of the run cost, in whatever order requests are
    /// settled: the record lists them in the order they were sent. False, and nothing
    /// is noted: when one is noted already for this request, or the record lists as
    /// many as it may; and when the run's end has begun.
    ///
    /// One that names a request the run did not send is taken here, since only the
    /// end of the run knows what it sent. It is never dropped in silence: such a run
    /// ends as an error of the type `record` that says so.
    pub fn paid(&self, paid: Paid) -> bool {
        self.note(|facts| facts.paid(paid))
    }

    /// Notes that data of these classes was handed to an origin, or that a send to it
    /// was refused. The account is written cut, and not at all when it holds a secret
    /// of the executor's ([`Went::new`]). An origin, account and outcome that is noted
    /// already gains these classes.
    ///
    /// False, and nothing is noted: when the origin, as it is written, holds a secret
    /// of the executor's, since a record writes an origin whole; when the entry is
    /// new and the record lists as many as it may, of which those of a refused send
    /// are half at most; and when the run's end has begun.
    pub fn went(
        &self,
        origin: &Origin,
        account: Option<&Account>,
        classes: Classes,
        outcome: Handed,
    ) -> bool {
        if outcome == Handed::Sent {
            // A note that is never taken back.
            return self.sending(origin, account, classes).is_some();
        }
        let Some(went) = self.entry(origin, account, classes, outcome) else {
            return false;
        };
        self.note(|facts| facts.went(went))
    }

    /// The entry for this origin, unless the origin as it is written holds a secret.
    fn entry(
        &self,
        origin: &Origin,
        account: Option<&Account>,
        classes: Classes,
        outcome: Handed,
    ) -> Option<Went> {
        let secrets = &self.shared.secrets;
        let written = origin.to_string();
        (secrets.redact(&written) == written)
            .then(|| Went::new(origin, account, classes, outcome, secrets))
    }

    /// Notes that data of these classes is about to be handed to an origin, before
    /// it is: the look at whether the run still takes notes, the look for room and
    /// the note are one step under the place's lock. None, and nothing is noted,
    /// when [`RunFacts::went`] would answer false; then nothing may be handed over.
    pub(crate) fn sending(
        &self,
        origin: &Origin,
        account: Option<&Account>,
        classes: Classes,
    ) -> Option<Sending> {
        let went = self.entry(origin, account, classes, Handed::Sent)?;
        let (origin, account) = (went.origin().to_owned(), went.account().map(str::to_owned));
        let mut noted = self.noted();
        if !noted.open || !noted.facts.went(went) {
            return None;
        }
        let same = |sent: &Sent| sent.origin == origin && sent.account == account;
        let place = match noted.sent.iter().position(same) {
            Some(place) => place,
            None => {
                noted.sent.push(Sent {
                    origin: origin.clone(),
                    account: account.clone(),
                    notes: 0,
                    holding: [0; 4],
                });
                noted.sent.len() - 1
            }
        };
        let sent = &mut noted.sent[place];
        sent.notes = sent.notes.saturating_add(1);
        for (at, class) in CLASSES.iter().enumerate() {
            if classes.iter().any(|held| held == *class) {
                sent.holding[at] = sent.holding[at].saturating_add(1);
            }
        }
        drop(noted);
        Some(Sending {
            facts: self.clone(),
            origin,
            account,
            classes,
        })
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
