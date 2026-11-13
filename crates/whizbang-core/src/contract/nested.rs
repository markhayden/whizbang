//! Nested calls (`SPEC.md` section 3, "Nested calls"; ADR 0008, "Nested calls"): an
//! action calls another action through the executor in its context, and the call is a
//! run of its own, under the run that made it.
//!
//! What is here: the executor a context holds ([`Nested`]), which refuses a call
//! deeper than four levels and a call back into an action that is already running
//! among the calls that led to it; and what a run keeps of the calls it made
//! ([`Nest`]), from which its record lists them.
//!
//! What is not here yet, and is the next part's: a cancellation does not reach the
//! runs a run called, a run does not wait for them or for its capability requests
//! before it ends, and it does not read what they committed.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use super::engine::{Admitted, Bounded, Stopped};
use super::live::LiveRun;
use super::record::{Inside, REFUSALS_LIMIT, Refused, STEPS_LIMIT, Status, Step};
use super::{
    Allowance, Answer, Call, CallError, Cancel, Engine, Executor, Finished, PreparedCall, Refusal,
    Request, RequestId, RunId, Secrets, StoreFailure, Watched,
};

/// How many runs that one run started, each on a thread of its own, may be executing
/// at once. With four levels, the runs under one run that nothing called hold at most
/// 4 + 16 + 64 = 84 threads between them.
pub const STARTED_LIMIT: usize = 4;

/// How many levels calls nest: a run that nothing called is at the first, and a call
/// made by a run at the fourth is refused.
pub const LEVELS_LIMIT: usize = 4;

/// One run that a run called.
struct Child {
    run: RunId,
    action: String,
    /// How it ended, once its end was published.
    status: Option<Status>,
}

#[derive(Default)]
struct Calls {
    /// Calls that were taken and are being admitted: neither refused nor a run yet.
    opening: usize,
    /// Runs the run started that are not over, and calls to start one that are being
    /// admitted: each is, or will be, a thread.
    started: usize,
    /// The runs the calls made, in the order they were admitted.
    children: Vec<Child>,
    refusals: Vec<Refused>,
    refusals_omitted: u64,
}

/// What a run keeps of the calls its action makes. It has a lock of its own, which is
/// held to read or change what is above and never across a call to the store or to an
/// action, and never while the run's own lock is taken.
#[derive(Default)]
pub(super) struct Nest {
    calls: Mutex<Calls>,
}

impl Nest {
    fn calls(&self) -> MutexGuard<'_, Calls> {
        self.calls.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes a call, to be admitted. Refused when the run has called as many as one
    /// may: a call that is being admitted counts as one. And a call to start a run,
    /// when as many as may be are executing: the look and the count are one step.
    fn begin(&self, action: &str, start: bool) -> Result<(), Refusal> {
        let mut calls = self.calls();
        if start && calls.started >= STARTED_LIMIT {
            return Err(Refusal::TooManyStarted {
                action: action.to_owned(),
                limit: STARTED_LIMIT,
            });
        }
        if calls.children.len() + calls.opening >= STEPS_LIMIT {
            return Err(Refusal::TooManyCalls {
                action: action.to_owned(),
                limit: STEPS_LIMIT,
            });
        }
        calls.opening += 1;
        calls.started += usize::from(start);
        Ok(())
    }

    /// A call that was taken has come to something: a run, when one is given.
    fn done(&self, made: Option<Child>) {
        let mut calls = self.calls();
        calls.opening -= 1;
        calls.children.extend(made);
    }

    /// A run the run started is over, or a call to start one made none.
    pub(super) fn started_over(&self) {
        let mut calls = self.calls();
        calls.started = calls.started.saturating_sub(1);
    }

    /// A call the run's action made was refused: kept for the run's record, the
    /// first of them listed and the rest counted. One that arrives after the record
    /// was written is kept nowhere.
    fn refused(&self, refused: Refused) {
        let mut calls = self.calls();
        if calls.refusals.len() < REFUSALS_LIMIT {
            calls.refusals.push(refused);
        } else {
            calls.refusals_omitted += 1;
        }
    }

    /// A run this run called has ended, and its end is published: how.
    pub(super) fn ended(&self, run: &RunId, status: Status) {
        let mut calls = self.calls();
        if let Some(child) = calls.children.iter_mut().find(|child| child.run == *run) {
            child.status = Some(status);
        }
    }

    /// What the run's record says of its calls, for a run with this parent.
    pub(super) fn inside(&self, parent: Option<RunId>) -> Inside {
        let calls = self.calls();
        Inside {
            parent,
            steps: calls
                .children
                .iter()
                .map(|child| Step::call(&child.run, &child.action, child.status))
                .collect(),
            refusals: calls.refusals.clone(),
            refusals_omitted: calls.refusals_omitted,
        }
    }
}

/// A call that was taken and is being admitted. Dropped without a run, it came to
/// nothing.
struct Opening<'a> {
    nest: &'a Nest,
    made: bool,
    /// Whether the call is to start a run, and was counted as one.
    start: bool,
}

impl Opening<'_> {
    fn made(mut self, child: Child) {
        self.made = true;
        self.nest.done(Some(child));
    }
}

impl Drop for Opening<'_> {
    fn drop(&mut self) {
        if !self.made {
            self.nest.done(None);
            if self.start {
                self.nest.started_over();
            }
        }
    }
}

/// The run a call is made under: the run whose action makes it.
#[derive(Clone)]
pub(super) struct Under {
    pub(super) run: RunId,
    pub(super) live: Arc<LiveRun>,
    /// The names of the actions that are running, from the run nothing called down
    /// to this one. Its length is the level this run is at.
    pub(super) stack: Arc<Vec<String>>,
    /// What this run may spend: each call it makes takes its estimate from it.
    pub(super) allowance: Allowance,
    pub(super) deadline: Option<Instant>,
}

/// Where a run is among the calls that led to it, from its admission on.
pub(super) struct Family {
    /// The run that called it, and that run's live state.
    pub(super) parent: Option<(RunId, Arc<LiveRun>)>,
    pub(super) stack: Arc<Vec<String>>,
    pub(super) allowance: Allowance,
}

impl Family {
    /// The family of a run of this action, called under this run or by nothing, with
    /// this much reserved or taken for it.
    pub(super) fn of(under: Option<&Under>, action: &str, reserved: Option<u64>) -> Family {
        let mut stack = under.map_or_else(Vec::new, |under| under.stack.to_vec());
        stack.push(action.to_owned());
        Family {
            parent: under.map(|under| (under.run.clone(), under.live.clone())),
            stack: Arc::new(stack),
            allowance: reserved.map_or_else(Allowance::none, Allowance::of),
        }
    }
}

/// The executor in the context of a running action: the one way the action reaches
/// another action. A call through it is admitted under the action's run, and is
/// refused, with no run, when it would be at a fifth level, when it names an action
/// that is already running among the calls that led to it, when it may cost more than
/// is left of the run's allowance, and when it carries a request id. Every such refusal is kept for
/// the run's record. Asking after a run, and cancelling one, are the engine's.
pub(super) struct Nested {
    engine: Engine,
    under: Under,
    secrets: Secrets,
}

impl Nested {
    pub(super) fn new(engine: Engine, under: Under, secrets: Secrets) -> Nested {
        Nested {
            engine,
            under,
            secrets,
        }
    }

    /// Admits the call under this run, or refuses it and keeps the refusal.
    fn admit(&self, request: &Request, start: bool) -> Result<Admitted, CallError> {
        let admitted = self.open(request, start);
        if let Err(CallError::Refused(refusal)) = &admitted {
            let refused = Refused::of(request.call(), refusal, &self.secrets);
            self.under.live.nest().refused(refused);
        }
        admitted
    }

    fn open(&self, request: &Request, start: bool) -> Result<Admitted, CallError> {
        let (under, call) = (&self.under, request.call());
        let action = || call.action().to_owned();
        if request.id().is_some() {
            return Err(Refusal::IdInside { action: action() }.into());
        }
        if under.stack.len() >= LEVELS_LIMIT {
            return Err(Refusal::TooDeep {
                action: action(),
                levels: LEVELS_LIMIT,
            }
            .into());
        }
        // What is on the stack is compared by the action's name.
        if under.stack.iter().any(|running| running == call.action()) {
            return Err(Refusal::CallBack { action: action() }.into());
        }
        let nest = under.live.nest();
        nest.begin(call.action(), start)?;
        let opening = Opening {
            nest,
            made: false,
            start,
        };
        // The call's run has the signal its request came with, or one of its own. A
        // cancellation of this run does not reach it yet.
        let bounded = Bounded {
            signal: request.cancellation().cloned().unwrap_or_default(),
            deadline: match (under.deadline, request.deadline()) {
                (Some(one), Some(other)) => Some(one.min(other)),
                (one, other) => one.or(other),
            },
        };
        let admitted = self
            .engine
            .admit(call, None, request.expected(), &bounded, Some(under))
            .map_err(|stopped| match stopped {
                Stopped::Refused(error) => error,
                // Only a request with an id makes the name that can be taken.
                Stopped::Taken(error) => CallError::Store(StoreFailure::new(error)),
            })?;
        opening.made(Child {
            run: admitted.run.clone(),
            action: action(),
            status: None,
        });
        Ok(admitted)
    }
}

impl Executor for Nested {
    fn prepare(&self, call: &Call) -> Result<PreparedCall, CallError> {
        self.engine.prepare(call)
    }

    fn submit(&self, call: &Call) -> Result<Finished, CallError> {
        let admitted = self.admit(&Request::new(call.clone()), false)?;
        self.engine.execute(admitted)
    }

    fn request(&self, request: &Request) -> Result<Answer, CallError> {
        let admitted = self.admit(request, false)?;
        Ok(Answer::Ran(self.engine.execute(admitted)?))
    }

    fn start(&self, request: &Request) -> Result<Answer, CallError> {
        // A started run is a thread, and a run may have only so many at once: the
        // count goes down when the run is over, before whoever waits for it is told.
        let mut admitted = self.admit(request, true)?;
        admitted.started_under(self.under.live.clone());
        let run = admitted.run.clone();
        let engine = self.engine.clone();
        std::thread::spawn(move || drop(engine.execute(admitted)));
        Ok(Answer::Started(run))
    }

    fn status(&self, run: &RunId) -> Result<Watched, CallError> {
        self.engine.status(run)
    }

    fn wait(&self, run: &RunId) -> Result<Watched, CallError> {
        self.engine.wait(run)
    }

    fn cancel(&self, run: &RunId) -> Result<Cancel, CallError> {
        self.engine.cancel(run)
    }

    fn request_id(&self, text: &str) -> Result<RequestId, CallError> {
        self.engine.request_id(text)
    }
}
