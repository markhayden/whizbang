//! What stands between a handle and its backend (ADR 0015, "A wrapper around each
//! request"): three named places, the gate, the lease and the meter, each of which may
//! hold one [`Around`].
//!
//! The order is fixed, and is the handle's: its own checks (ADR 0006), then the tally,
//! which counts the request, then the gate, the lease and the meter, then the backend.
//! Coming back it is the other way round: the meter, the lease, the gate, and only
//! then is the request accounted for in the tally. So a request is outstanding, and
//! its run and every run above it not terminal, until the last wrapper has returned.
//!
//! Each place is empty unless an executor fills it. This module holds the interface
//! and what runs it; what a gate, a lease and a meter do is theirs.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;

use crate::contract::record::RefusalKind;
use crate::contract::{Allowance, Capability, RunFacts, RunId};
use crate::egress::Classes;

use super::{Charge, Declaration, Fault, FaultKind, Quote};

/// One request, as a wrapper is told of it before the request leaves and again when
/// it is over.
pub struct Wrapped<'a> {
    /// The run whose action sends the request.
    pub run: &'a RunId,
    /// The run at the root of its tree: the one nothing called, whose reservation
    /// the tree draws on. It is `run` itself for a run nothing called.
    pub root: &'a RunId,
    /// The request's place among the requests the run sent, from 0: where the run's
    /// record lists it (`models`), and what a paid request's line names it by (ADR
    /// 0013). None for a handle whose run does not count its requests, which no run
    /// of an executor is.
    pub request: Option<usize>,
    /// Which capability is asked.
    pub capability: Capability,
    /// What the backend declares: its model, its features, its price and, for a
    /// backend that is not local, its recipient.
    pub declaration: &'a Declaration,
    /// What one request to it costs at most, as it quotes it; none for a backend
    /// that states no price.
    pub quote: Option<Quote>,
    /// The classes of data the request is known to hold, before any piece of it is
    /// made (ADR 0015, "What a request is said to hold").
    pub classes: Classes,
    /// The run's allowance, with what its process holds of its tree.
    pub allowance: &'a Allowance,
    /// The run's place for facts.
    pub facts: &'a RunFacts,
}

/// A wrapper's refusal of a request, before the request leaves: the fault its action
/// is handed, and the refusal that is noted among its run's refusals.
#[derive(Debug, Clone)]
pub struct Stop {
    fault: Fault,
    kind: RefusalKind,
    why: String,
}

impl Stop {
    /// A refusal of this kind, noted with this sentence, that answers the action
    /// with this fault. The kind is one of a capability request
    /// ([`RefusalKind::is_of_a_request`]): any other is none, because a record
    /// holds no other for a request. The fault is marked as not sent, whatever it
    /// said: a request a wrapper refuses did not leave.
    pub fn new(kind: RefusalKind, why: &str, fault: Fault) -> Option<Stop> {
        kind.is_of_a_request().then(|| Stop {
            fault: fault.not_sent(),
            kind,
            why: why.to_owned(),
        })
    }

    /// The fault the action is handed.
    pub fn fault(&self) -> &Fault {
        &self.fault
    }
}

/// How a request ended, as a wrapper is told once it is over.
#[derive(Debug, Clone, Copy)]
pub enum Ended<'a> {
    /// The backend answered and the handle took the answer: what the answer said
    /// it cost, when it said.
    Answered(Option<Charge>),
    /// It ended in a fault, which says how far the request got and what it was
    /// charged when that is known (ADR 0010): the backend's, the handle's refusal
    /// of what the backend answered, or the refusal of a wrapper further in, after
    /// which the backend was never called.
    Faulted(&'a Fault),
}

/// What may stand in one place between a handle and its backend.
///
/// [`Around::before`] is called for every request, before the request leaves. It may
/// refuse ([`Stop`]): the request then goes no further, the places after this one and
/// the backend never see it, and the fault reaches the action. Otherwise it answers
/// with what it holds open for the request ([`Open`]), which is told how the request
/// ended.
pub trait Around: Send + Sync {
    fn before(&self, asked: &Wrapped<'_>) -> Result<Box<dyn Open>, Stop>;
}

/// What a wrapper holds for one request between its two calls. It is handed how the
/// request ended, once: by an answer, by a fault, or by the refusal of a place after
/// its own. When the backend panics it is dropped instead, without being told: what
/// it must let go of whatever happens, it lets go of when it is dropped.
///
/// A failure of [`Open::after`] is not a judgement of the request, and does not
/// account for it as free: the run is abandoned with the failure (ADR 0008), and
/// nothing of it is released. A panic of it comes to the same, and then goes on to
/// the action once the other places were told.
pub trait Open: Send {
    fn after(self: Box<Self>, asked: &Wrapped<'_>, ended: &Ended<'_>) -> Result<(), String>;

    /// Whether it holds anything for the request that being told how the request
    /// ended would account for: a hold, a debit, a record. Where it does and the
    /// backend panics, so that it is never told, the request is not accounted for
    /// and its run is abandoned. A wrapper whose work was all before the request
    /// holds nothing ([`Nothing`]), and a backend's panic under it alone is what it
    /// is without it: the fault of one request.
    fn holds(&self) -> bool {
        true
    }

    /// The model's count of removals that the hold it took for the request read
    /// (ADR 0017, "Holds"; ADR 0015, "A wrapper around each request"): what the
    /// lease's place hands on to the backend, so that a request to a local model
    /// enters the worker's queue with it (plan 1C, task P6b). None for a wrapper that
    /// took no hold; only what stands in the lease's place is asked.
    fn removals(&self) -> Option<u64> {
        None
    }
}

/// What the places handed on to the backend of one request, beside the request
/// (ADR 0015, "A wrapper around each request"; plan 1C, task P6b): the count of
/// removals the lease's hold read, when the lease's place took a hold for it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Given {
    pub(crate) removals: Option<u64>,
}

/// A wrapper that holds nothing open: for one whose work is all before the request.
pub struct Nothing;

impl Open for Nothing {
    fn after(self: Box<Self>, _: &Wrapped<'_>, _: &Ended<'_>) -> Result<(), String> {
        Ok(())
    }

    fn holds(&self) -> bool {
        false
    }
}

/// The three places, in the order a request passes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Whether the request may go to its recipient at all (ADR 0015, ADR 0019).
    Gate,
    /// A hold on the installation of a local model, for as long as the request
    /// runs (ADR 0015, "An inventory").
    Lease,
    /// The request's bound, debited and made durable before it leaves, and its
    /// settlement when it is over (ADR 0016).
    Meter,
}

impl Place {
    /// The places, in the order a request passes them.
    pub const ALL: [Place; 3] = [Place::Gate, Place::Lease, Place::Meter];

    /// The place's name.
    pub fn name(self) -> &'static str {
        match self {
            Place::Gate => "gate",
            Place::Lease => "lease",
            Place::Meter => "meter",
        }
    }
}

/// What stands in each of the three places: nothing, until a place is given a
/// wrapper.
#[derive(Clone, Default)]
pub struct Places {
    gate: Option<Arc<dyn Around>>,
    lease: Option<Arc<dyn Around>>,
    meter: Option<Arc<dyn Around>>,
}

impl Places {
    /// Every place empty.
    pub fn none() -> Places {
        Places::default()
    }

    /// The same places, with this wrapper in this one, in place of what it held.
    pub fn with(mut self, place: Place, wrapper: Arc<dyn Around>) -> Places {
        *self.at_mut(place) = Some(wrapper);
        self
    }

    /// What stands in this place.
    pub fn at(&self, place: Place) -> Option<&Arc<dyn Around>> {
        match place {
            Place::Gate => self.gate.as_ref(),
            Place::Lease => self.lease.as_ref(),
            Place::Meter => self.meter.as_ref(),
        }
    }

    fn at_mut(&mut self, place: Place) -> &mut Option<Arc<dyn Around>> {
        match place {
            Place::Gate => &mut self.gate,
            Place::Lease => &mut self.lease,
            Place::Meter => &mut self.meter,
        }
    }

    /// The wrappers there are, in the order a request passes them.
    fn in_order(&self) -> Vec<(Place, &Arc<dyn Around>)> {
        let all = Place::ALL.into_iter();
        all.filter_map(|place| self.at(place).map(|wrapper| (place, wrapper)))
            .collect()
    }
}

impl std::fmt::Debug for Places {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let filled: Vec<&str> = self.in_order().iter().map(|(at, _)| at.name()).collect();
        f.debug_tuple("Places").field(&filled).finish()
    }
}

/// The wrappers of one run's handles, with what each is told of the run.
#[derive(Clone)]
pub(crate) struct Wrappers {
    pub(crate) run: RunId,
    pub(crate) root: RunId,
    pub(crate) places: Places,
    pub(crate) allowance: Allowance,
    pub(crate) facts: RunFacts,
}

/// One request, as its handle hands it to the wrappers: its place among the requests
/// its run sent, when the run counts them; which capability is asked, of which
/// backend; and the classes of data it is known to hold.
pub(crate) struct Leaving<'a> {
    pub(crate) request: Option<usize>,
    pub(crate) capability: Capability,
    pub(crate) declaration: &'a Declaration,
    pub(crate) classes: Classes,
}

/// What the action is told when a wrapper failed after its request's backend was
/// called.
const NOT_ACCOUNTED: &str = "the request ended and could not be accounted for, so its run \
                             is abandoned and nothing more is sent for it";

impl Wrappers {
    /// Sends one request through the places, to `ask`, which calls the backend with
    /// what the places handed on ([`Given`]), and back. `fail` is told when the run is
    /// to be abandoned: a wrapper failed or panicked once the request was over, or the
    /// backend panicked under a wrapper.
    pub(crate) fn send<T>(
        &self,
        leaving: Leaving<'_>,
        charge_of: impl Fn(&T) -> Option<Charge>,
        ask: impl FnOnce(Given) -> Result<T, Fault>,
        fail: impl Fn(String),
    ) -> Result<T, Fault> {
        let places = self.places.in_order();
        if places.is_empty() {
            return ask(Given::default());
        }
        let Leaving {
            request,
            capability,
            declaration,
            classes,
        } = leaving;
        let asked = Wrapped {
            run: &self.run,
            root: &self.root,
            request,
            capability,
            declaration,
            quote: declaration.quote(),
            classes,
            allowance: &self.allowance,
            facts: &self.facts,
        };
        // What is open is closed in the other order, each told how the request
        // ended. A failure of one does not keep the others from being told, and
        // neither does a panic of one: it is caught, the run is marked as it is for
        // a failure, since what that wrapper held is not accounted for, and the
        // first such panic is handed back to go on once every other was told.
        let close = |open: Vec<(Place, Box<dyn Open>)>, ended: &Ended<'_>| {
            let mut closed = Closed::default();
            for (place, held) in open.into_iter().rev() {
                match catch_unwind(AssertUnwindSafe(|| held.after(&asked, ended))) {
                    Ok(Ok(())) => {}
                    Ok(Err(why)) => {
                        fail(format!("the {} failed: {why}", place.name()));
                        closed.failed = true;
                    }
                    Err(panic) => {
                        fail(format!(
                            "the {} panicked once its request was over",
                            place.name()
                        ));
                        closed.failed = true;
                        closed.keep(panic);
                    }
                }
            }
            closed
        };
        let mut open: Vec<(Place, Box<dyn Open>)> = Vec::new();
        for (place, wrapper) in places {
            match wrapper.before(&asked) {
                Ok(held) => open.push((place, held)),
                Err(stop) => {
                    // The refusal goes with the run's others. The places before
                    // this one are told that the request did not leave.
                    self.facts.refused(stop.kind, &stop.why);
                    close(open, &Ended::Faulted(&stop.fault)).go_on();
                    return Err(stop.fault);
                }
            }
        }
        // What the lease's place hands on to the backend: the count its hold read.
        let given = Given {
            removals: open
                .iter()
                .find(|(place, _)| *place == Place::Lease)
                .and_then(|(_, held)| held.removals()),
        };
        // The backend, under the wrappers. A panic of it unwinds through here, as it
        // does through a handle with no wrapper. Where a wrapper holds something for
        // the request, it was told the request left and is never told how it ended:
        // the request is not accounted for as free, and the run is abandoned.
        let result = match catch_unwind(AssertUnwindSafe(|| ask(given))) {
            Ok(result) => result,
            Err(panic) => {
                if let Some((place, _)) = open.iter().find(|(_, held)| held.holds()) {
                    fail(format!(
                        "a backend panicked under the {}, which held its request",
                        place.name()
                    ));
                }
                drop(open);
                resume_unwind(panic);
            }
        };
        let ended = match &result {
            Ok(answer) => Ended::Answered(charge_of(answer)),
            Err(fault) => Ended::Faulted(fault),
        };
        match close(open, &ended).go_on() {
            false => result,
            // What the request cost is not known to the run any more: no answer of
            // it is handed on.
            true => Err(Fault::new(FaultKind::Unavailable, NOT_ACCOUNTED).unresolved()),
        }
    }
}

/// What telling the wrappers of a request's end came to: whether one of them failed
/// or panicked, and the first panic, which goes on once all were told.
#[derive(Default)]
struct Closed {
    failed: bool,
    panic: Option<Box<dyn Any + Send>>,
}

impl Closed {
    /// Keeps the first panic. What a later one carried is dropped here, and a value
    /// can panic when it is dropped: that is caught, and what it carried is not
    /// dropped at all.
    fn keep(&mut self, panic: Box<dyn Any + Send>) {
        if self.panic.is_none() {
            self.panic = Some(panic);
        } else if let Err(second) = catch_unwind(AssertUnwindSafe(move || drop(panic))) {
            std::mem::forget(second);
        }
    }

    /// Whether a wrapper failed. A panic of one goes on from here, as a backend's
    /// does: it unwinds through the handle to the action, whose run is abandoned
    /// whatever the action makes of it.
    fn go_on(self) -> bool {
        if let Some(panic) = self.panic {
            resume_unwind(panic);
        }
        self.failed
    }
}
