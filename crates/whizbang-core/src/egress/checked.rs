//! The checked send (`SPEC.md` section 10, "Egress"; ADR 0015, "A policy before a send,
//! and a checked send"): the sending interface as the executor hands it to a backend.
//!
//! A backend is handed no sender of its own and no credential. What it is handed, with
//! the bounds of each request ([`Bounds::sender`](crate::capability::Bounds::sender)),
//! is made for that backend and that run: it knows the recipient the backend declared,
//! and before anything is handed to the sender behind it, every address goes through
//! the same questions, the first address and each one an answer pointed to alike.
//! **Nothing here decides who may receive what:** the policy is a value the executor
//! was given, and the check is the egress strand's. The default of each refuses.

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::{
    Address, Classes, Credential, DispatchPolicy, NotAllowed, Origin, Outgoing, Piece, Recipient,
    Reply, Sender,
};
use crate::capability::{
    Around, Bounds, Declaration, Fault, FaultKind, Nothing, Open, Stage, Stop, Wrapped,
};
use crate::contract::record::Handed;
use crate::contract::{RefusalKind, RunFacts};

/// The most credentials an executor holds for its recipients, over all of them.
pub const CREDENTIALS_LIMIT: usize = 256;

/// One address a backend asked to reach, as the check is shown it, before anything is
/// handed over. What `{:?}` prints is the recipient as one is always printed, the
/// address's origin and the size of its rest, the classes and the count: never the
/// rest of the address.
#[derive(Clone, Copy)]
pub struct Hop<'a> {
    recipient: &'a Recipient,
    to: &'a Address,
    pieces: &'a [Piece],
    fields: &'a [(&'a str, &'a str)],
    needs: Classes,
    policy: &'a DispatchPolicy,
    moves: u32,
    total: u32,
    request: u32,
}

impl<'a> Hop<'a> {
    /// The recipient the backend declared: whose origin and upstreams the address was
    /// held to, and whom the policy was asked about.
    pub fn recipient(&self) -> &'a Recipient {
        self.recipient
    }

    /// The address the request is for.
    pub fn to(&self) -> &'a Address {
        self.to
    }

    /// The origin of that address.
    pub fn origin(&self) -> &'a Origin {
        self.to.origin()
    }

    /// The pieces the request holds, for a check that looks at what would leave. What
    /// is printed of a hop shows nothing of them.
    pub fn pieces(&self) -> &'a [Piece] {
        self.pieces
    }

    /// The fields of its own the request names ([`Outgoing::with_fields`]), for a
    /// check that looks at what would leave.
    pub fn fields(&self) -> &'a [(&'a str, &'a str)] {
        self.fields
    }

    /// The classes of data the request holds: the class each piece states, and all
    /// four when any piece states none.
    pub fn needs(&self) -> Classes {
        self.needs
    }

    /// The policy the executor holds.
    pub fn policy(&self) -> &'a DispatchPolicy {
        self.policy
    }

    /// How many answers in a row, just before this request, pointed elsewhere: 0 for
    /// a first address, 1 for the address its answer pointed to, and so on. An
    /// answered request of the same backend sets it back to nothing; a request that
    /// was refused, or that failed, does not. It is counted for one backend of one
    /// run, and not for one request: a run that sends through one handle from two
    /// threads at once has one count for both.
    pub fn moves(&self) -> u32 {
        self.moves
    }

    /// How many answers pointed elsewhere, up to this request, for this backend of
    /// this run. It never goes back.
    pub fn total_moves(&self) -> u32 {
        self.total
    }

    /// How many answers have pointed elsewhere within the one request of the backend
    /// that this send is part of: 0 for its first address. It is counted with the
    /// bounds that request was handed, so it never goes back while the request
    /// lasts, is shared with no other request through the same handle, and is the
    /// same count for a clone of those bounds.
    pub fn request_moves(&self) -> u32 {
        self.request
    }
}

impl fmt::Debug for Hop<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Hop({:?} for {:?}, {:?}, after {} moves)",
            self.to, self.recipient, self.needs, self.moves
        )
    }
}

/// Why a check does not let a request go to an address: a sentence of fixed words,
/// which is all the type takes. It quotes nothing of the address or of the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Denied {
    why: &'static str,
}

impl Denied {
    /// Denied, for this reason. A reason of nothing is said as `no reason was given`.
    pub fn because(why: &'static str) -> Denied {
        Denied { why }
    }

    pub fn why(&self) -> &'static str {
        if self.why.trim().is_empty() {
            "no reason was given"
        } else {
            self.why
        }
    }
}

/// The check every address goes through before data is handed to it: the first
/// address of a request, and each address an answer pointed to. It is asked after the
/// executor has held the address to the recipient's origin and upstreams and has asked
/// the policy, and only when both said yes; what it adds is the egress strand's.
pub trait HopCheck: Send + Sync {
    /// Whether the request may be handed to this address. Anything but `Ok` hands
    /// nothing over.
    fn look(&self, hop: &Hop<'_>) -> Result<(), Denied>;
}

/// The check an executor has until it is given another: it approves no address.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHop;

impl HopCheck for NoHop {
    fn look(&self, _: &Hop<'_>) -> Result<(), Denied> {
        Err(Denied::because(
            "no check approves an address: this executor sends nothing",
        ))
    }
}

/// The ways out an executor holds: the sender behind every checked send, the check,
/// the policy, and the credentials of its recipients. With none of them given, nothing
/// leaves: there is no sender, the check approves no address and the policy approves
/// no recipient.
///
/// What `{:?}` prints is whether there is a sender, the policy, and how many
/// credentials are held: never a credential.
#[derive(Clone)]
pub struct Ways {
    sender: Option<Arc<dyn Sender>>,
    check: Arc<dyn HopCheck>,
    policy: DispatchPolicy,
    credentials: Vec<(Recipient, Credential)>,
}

impl Default for Ways {
    fn default() -> Ways {
        Ways::none()
    }
}

impl Ways {
    /// No way out at all.
    pub fn none() -> Ways {
        Ways {
            sender: None,
            check: Arc::new(NoHop),
            policy: DispatchPolicy::none(),
            credentials: Vec::new(),
        }
    }

    /// Sending through this, once the policy and a check approve: until both are
    /// given, nothing is handed to it.
    pub fn through(sender: Arc<dyn Sender>) -> Ways {
        Ways {
            sender: Some(sender),
            ..Ways::none()
        }
    }

    /// The same, with every address looked at by this check.
    pub fn checked_by(mut self, check: Arc<dyn HopCheck>) -> Ways {
        self.check = check;
        self
    }

    /// The same, under this policy.
    pub fn under(mut self, policy: DispatchPolicy) -> Ways {
        self.policy = policy;
        self
    }

    /// The same, holding this credential for requests of this recipient's backends to
    /// the credential's origin. Refused: a credential for an origin that is neither
    /// the recipient's own nor one of its upstreams; a second one for the same
    /// recipient and origin; and one more than [`CREDENTIALS_LIMIT`].
    pub fn with_credential(
        mut self,
        recipient: &Recipient,
        credential: Credential,
    ) -> Result<Ways, NotAllowed> {
        if !recipient.reaches(credential.origin()) {
            return Err(NotAllowed::Elsewhere);
        }
        let held = |(to, held): &(Recipient, Credential)| {
            to == recipient && held.origin() == credential.origin()
        };
        if self.credentials.iter().any(held) {
            return Err(NotAllowed::Twice);
        }
        if self.credentials.len() >= CREDENTIALS_LIMIT {
            return Err(NotAllowed::TooMany);
        }
        self.credentials.push((recipient.clone(), credential));
        Ok(self)
    }

    /// This policy and these credentials, behind the sender and the check of those
    /// ways: what a run admitted under a snapshot of the config sends through (ADR
    /// 0015, "One snapshot of the config"). The policy and the credentials of
    /// `executors` are not used.
    pub(crate) fn behind(mut self, executors: &Ways) -> Ways {
        self.sender = executors.sender.clone();
        self.check = executors.check.clone();
        self
    }

    /// The policy these ways are under.
    pub fn policy(&self) -> &DispatchPolicy {
        &self.policy
    }

    /// The sender one backend of one run is handed: it knows the recipient the
    /// backend declared, holds the credentials of that recipient and no other, and
    /// notes what it hands over, and what it refuses, in the run's place for facts.
    pub(crate) fn for_backend(
        &self,
        declaration: &Declaration,
        facts: RunFacts,
    ) -> Arc<dyn Sender> {
        let recipient = declaration.recipient().cloned();
        let held = self.credentials.iter();
        let credentials = held
            .filter(|(to, _)| Some(to) == recipient.as_ref())
            .map(|(_, credential)| credential.clone());
        Arc::new(Checked {
            credentials: credentials.collect(),
            recipient,
            sender: self.sender.clone(),
            check: self.check.clone(),
            policy: self.policy.clone(),
            facts,
            moves: AtomicU32::new(0),
            total: AtomicU32::new(0),
        })
    }
}

impl Ways {
    /// What stands in the gate's place for a run under these ways (ADR 0015, "A
    /// wrapper around each request"): the same policy the checked sender asks, asked
    /// once before the backend is called.
    pub(crate) fn gate(&self) -> Arc<dyn Around> {
        Arc::new(Gate {
            policy: self.policy.clone(),
        })
    }
}

/// The gate's place: a request to a backend that names a recipient goes on only when
/// the policy approves that recipient for the classes of data the request is known to
/// hold. A request it refuses reaches neither the places after it nor its backend,
/// and so no sender. A backend that names no recipient is local, and is not asked
/// about: what it tries to send is refused by its sender.
///
/// It is the first of two checks and never the only one. What a request is known to
/// hold before any piece of it is made is less than what its backend then hands
/// over, and every piece is held to the policy again by the checked sender.
struct Gate {
    policy: DispatchPolicy,
}

impl Around for Gate {
    fn before(&self, asked: &Wrapped<'_>) -> Result<Box<dyn Open>, Stop> {
        let Some(recipient) = asked.declaration.recipient() else {
            return Ok(Box::new(Nothing));
        };
        let Some(not) = self.policy.why_not(recipient, asked.classes) else {
            return Ok(Box::new(Nothing));
        };
        let (origin, why) = (recipient.origin(), not.why());
        let listed = asked
            .facts
            .went(origin, recipient.account(), asked.classes, Handed::Refused);
        // No piece of the request was made: what it holds is what was known of it
        // before one is, and where that is nothing the sentence says so.
        let sentence = not_sent_to(origin, why, asked.classes, listed, NONE_KNOWN);
        let fault = Fault::new(FaultKind::Rejected, why);
        match Stop::new(RefusalKind::RequestNotApproved, &sentence, fault) {
            Some(stop) => Err(stop),
            // The kind is one of a request: this is not reached.
            None => Ok(Box::new(Nothing)),
        }
    }
}

/// What the gate says of a request it knows no class of: its backend was not called,
/// so no piece of it was made, and what it would have held is not known.
const NONE_KNOWN: &str = "none known before a piece of it is made";

/// What the checked sender says of a request whose pieces hold no data.
const NONE: &str = "none";

/// The sentence a refusal of a request is noted with among its run's refusals.
/// `none` is what it says of a request that holds no class.
fn not_sent_to(to: &Origin, why: &str, needs: Classes, listed: bool, none: &str) -> String {
    let classes: Vec<&str> = needs.iter().map(|class| class.name()).collect();
    format!(
        "nothing was sent to {to}: {why} (the classes of data the request holds: {}{})",
        if classes.is_empty() {
            none.to_owned()
        } else {
            classes.join(", ")
        },
        if listed {
            ""
        } else {
            "; not listed in where the run's data went"
        }
    )
}

impl fmt::Debug for Ways {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ways")
            .field("sender", &self.sender.is_some())
            .field("policy", &self.policy)
            .field("credentials", &self.credentials.len())
            .finish()
    }
}

/// What a handle that no executor made hands its backend: nothing is sent.
pub(crate) struct NoWayOut;

impl Sender for NoWayOut {
    fn send(&self, _: &Outgoing, _: &Bounds) -> Result<Reply, Fault> {
        Err(Fault::new(
            FaultKind::Rejected,
            "the handle was made by no executor, and has no way to send",
        )
        .not_sent())
    }
}

/// The sender of one backend of one run.
struct Checked {
    /// The recipient the backend declared; none for a backend that declared itself
    /// local.
    recipient: Option<Recipient>,
    /// The credentials of that recipient, each bound to one origin it reaches.
    credentials: Vec<Credential>,
    sender: Option<Arc<dyn Sender>>,
    check: Arc<dyn HopCheck>,
    policy: DispatchPolicy,
    facts: RunFacts,
    /// How many answers in a row, up to now, pointed elsewhere.
    moves: AtomicU32,
    /// How many answers pointed elsewhere, up to now. It never goes back.
    total: AtomicU32,
}

/// What a send that cannot be listed is refused with.
const NOT_LISTED: &str = "what is handed over is listed in the run's record, and this could \
                          not be: the run is over, the list is full, or the origin holds a \
                          secret; so nothing was sent";

/// What a send answers of an answer that points to an address holding a secret.
pub const MOVED_SECRET: &str = "the answer points elsewhere, to an address that holds a \
                                secret, and is not shown";

/// What a sender answers once its run is over.
const RUN_OVER: &str = "the run this sender was made for is over, and nothing is sent for it";

impl Checked {
    /// Refuses a request before anything is handed over: the refusal is among the
    /// run's refusals and in its list of where data went, and the backend is answered
    /// with a fault that says nothing was sent.
    ///
    /// The refusal among the run's refusals is the note that counts: that list
    /// lists it or counts it. The entry in where data went is listed when it can
    /// be, and the sentence says so when it could not: the list keeps half its room
    /// for what was sent, and writes no origin that holds a secret. Once the run's
    /// end has begun nothing is noted, and the answer says that the run is over.
    fn refuse(&self, to: &Origin, needs: Classes, why: &'static str) -> Fault {
        let account = self.recipient.as_ref().and_then(|r| r.account());
        let listed = self.facts.went(to, account, needs, Handed::Refused);
        // The sentence is cleaned where it is noted: the executor's secrets are taken
        // out of it while it is whole, and it is then cut.
        let noted = self.facts.refused(
            RefusalKind::RequestNotApproved,
            &not_sent_to(to, why, needs, listed, NONE),
        );
        let why = if noted { why } else { RUN_OVER };
        Fault::new(FaultKind::Rejected, why).not_sent()
    }
}

impl Sender for Checked {
    fn send(&self, outgoing: &Outgoing, bounds: &Bounds) -> Result<Reply, Fault> {
        let (to, needs) = (outgoing.to(), outgoing.needs());
        let origin = to.origin();
        let Some(recipient) = &self.recipient else {
            return Err(self.refuse(
                origin,
                needs,
                "the backend names no recipient, so nothing it is asked leaves the machine",
            ));
        };
        // A backend is handed no credential, and one that brings its own is not sent:
        // the executor attaches the one bound to the origin, or none.
        if outgoing.credential().is_some() {
            return Err(self.refuse(
                origin,
                needs,
                "the request came with a credential, and a backend holds none",
            ));
        }
        if !recipient.reaches(origin) {
            return Err(self.refuse(
                origin,
                needs,
                "the origin is neither the recipient's own nor one of its upstreams",
            ));
        }
        if let Some(not) = self.policy.why_not(recipient, needs) {
            return Err(self.refuse(origin, needs, not.why()));
        }
        let hop = Hop {
            recipient,
            to,
            pieces: outgoing.pieces(),
            fields: outgoing.fields(),
            needs,
            policy: &self.policy,
            moves: self.moves.load(Ordering::SeqCst),
            total: self.total.load(Ordering::SeqCst),
            request: bounds.moved(),
        };
        if let Err(denied) = self.check.look(&hop) {
            return Err(self.refuse(origin, needs, denied.why()));
        }
        let Some(sender) = &self.sender else {
            // Noted nowhere: nothing was refused, and nothing left.
            return Err(Fault::new(
                FaultKind::Unavailable,
                "the executor was given no sender, and sends nothing",
            )
            .not_sent());
        };
        // The credential bound to this origin, and never another: a request to an
        // upstream goes without the recipient's own.
        let held = self.credentials.iter().find(|c| c.origin() == origin);
        let outgoing = match held {
            None => Cow::Borrowed(outgoing),
            Some(credential) => {
                let with = outgoing.clone().with_credential(credential.clone());
                Cow::Owned(with.map_err(|_| {
                    self.refuse(
                        origin,
                        needs,
                        "the credential is bound to another origin than the request's",
                    )
                })?)
            }
        };
        // The entry is in the run's record before anything is handed over: the look
        // at whether the run still takes notes and the note are one step. A send
        // that cannot be listed is not made, which is also what stops a sender that
        // outlived its run. From here on the entry stays whatever comes back, an
        // answer, a fault or a panic of the sender, since what a sender that unwound
        // did with the request is not known; only a fault that says nothing left
        // takes it back.
        let account = recipient.account();
        let Some(note) = self.facts.sending(origin, account, needs) else {
            return Err(self.refuse(origin, needs, NOT_LISTED));
        };
        let reply = sender.send(outgoing.as_ref(), bounds);
        // An answer that points to an address that holds a secret of the run is not
        // handed to the backend: the secrets are the run's, its snapshot's among them,
        // which the sender behind the check may not have been told.
        let secret = reply
            .as_ref()
            .ok()
            .and_then(Reply::moved_to)
            .is_some_and(|to| {
                self.facts
                    .holds_secret(&format!("{}{}", to.origin(), to.rest()))
            });
        if secret {
            bounds.moved_once_more();
            return Err(Fault::new(FaultKind::Malformed, MOVED_SECRET).unresolved());
        }
        match &reply {
            // An answer that points elsewhere is one more move, in a row and in all.
            Ok(reply) if reply.moved_to().is_some() => {
                bounds.moved_once_more();
                for count in [&self.moves, &self.total] {
                    let more = |now: u32| Some(now.saturating_add(1));
                    // The closure always answers, so the update is always made.
                    if count
                        .try_update(Ordering::SeqCst, Ordering::SeqCst, more)
                        .is_err()
                    {
                        count.store(u32::MAX, Ordering::SeqCst);
                    }
                }
            }
            // An answer sets the moves in a row back to nothing.
            Ok(_) => self.moves.store(0, Ordering::SeqCst),
            // A fault does not; one that says nothing left takes the note back.
            Err(fault) if fault.stage() == Stage::NotSent => note.take_back(),
            Err(_) => {}
        }
        reply
    }
}
