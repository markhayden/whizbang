//! The queue in front of the one worker (plan 1C, D4 and D7; task P4): the supervisor a
//! process makes once, which every request for a local model goes through.
//!
//! **One call at a time, in the order the requests came.** A request takes a number and
//! a place at the back of the queue, and waits for its turn: the front of the queue, with
//! no call in the worker. At most [`QUEUED_MOST`] wait; the one past them is refused at
//! once, `unavailable` and `not_sent`, and is not held. A request's turn is served on its
//! caller's own thread, so the queue makes no thread: the worker's three are all
//! (`super::threads`).
//!
//! **One resident model.** A turn whose model, installation and settings are not the
//! resident's first unloads the resident one, then loads its own, then asks its call; one
//! that finds its own resident asks its call alone. So a request for another model waits
//! for the current call, and the old model is then unloaded.
//!
//! **Its limits, on the clock** ([`Limits`]): a request cancelled, or past its deadline,
//! while it waits leaves the queue and is never sent (`cancelled` or `timed_out`,
//! `not_sent`). In its turn each step (unload, load, call) is one call of the worker's,
//! within its kind's limit, the request's deadline and its cancel
//! ([`Worker::call_within`]). A request with no deadline waits in the queue at most
//! [`WAIT_LIMIT`], the most the calls ahead of it can take; it is the bound of a wait
//! that the calls ahead end first.
//!
//! **A restart fails everything in flight.** A step that ends the worker (it died, it was
//! killed at a limit, a deadline or a cancel it ignored, or it broke the protocol) ends
//! its request with its fault, whose sentence then says that the worker is restarted;
//! every request waiting then ends too, `unavailable` and `not_sent`, with a sentence that
//! says the worker was restarted before its turn. Nothing is sent again. The next request
//! starts another worker; one that ends before it answers anything is not started again
//! in a loop ([`super::worker::STARTS_FAILED_MOST`]).
//!
//! **What it tells.** [`Supervisor::status`]: who waits (each with the count of removals
//! its hold was given, D5, which task P5b compares with the model's), the call in the
//! worker, the resident model and the worker's process id; [`Supervisor::wait_for`] a
//! state of these; and [`Supervisor::journal`], the last [`JOURNAL_KEPT`] requests written
//! to a worker, each with its request's number and the worker's process id.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use whizbang_core::capability::{Fault, FaultKind, Stage};
use whizbang_core::contract::Clock;

use crate::models::{InstallationId, ModelName};

use super::proto::{Ask, Scalar};
use super::worker::{
    CALL_LIMIT, CANCEL_LIMIT, Called, END_LIMIT, Ended, Failed, HELLO_LIMIT, LOAD_LIMIT, Limits,
    Progress, UNLOAD_LIMIT, Worker,
};

/// How many requests may wait for the worker (D4): the one past them is refused.
pub const QUEUED_MOST: usize = 16;
/// How many requests written to a worker the journal keeps: the last.
pub const JOURNAL_KEPT: usize = 64;
/// The longest one turn can take: a start and its hello, an unload, a load and a call,
/// a cancel's limit, and the worker's ends and the words told it.
pub const TURN_MOST: Duration = Duration::from_secs(
    HELLO_LIMIT.as_secs()
        + UNLOAD_LIMIT.as_secs()
        + LOAD_LIMIT.as_secs()
        + CALL_LIMIT.as_secs()
        + CANCEL_LIMIT.as_secs()
        + 6 * END_LIMIT.as_secs(),
);
/// How long a request with no deadline waits in the queue at most: the turns of every
/// request that can be ahead of it.
pub const WAIT_LIMIT: Duration = Duration::from_secs(TURN_MOST.as_secs() * QUEUED_MOST as u64);
/// How long a waiting request lets pass, at most, between two looks at its turn and its
/// limits.
const PACE: Duration = Duration::from_millis(5);

/// One request for a local model: the `load` that makes it resident, and its call.
#[derive(Debug, Clone)]
pub struct Request {
    /// The model.
    pub model: ModelName,
    /// The installation of it.
    pub installation: InstallationId,
    /// Its settings, which travel in the `load`.
    pub settings: BTreeMap<String, Scalar>,
    /// The call: a `segment` of that model and that installation.
    pub ask: Ask,
    /// The model's count of removals as the request's hold read it (D5; the plan's task
    /// P5a): kept while the request waits. Task P5b compares it with the model's.
    pub removals: u64,
}

/// A request in the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// Its number: requests are numbered from 1 in the order they came.
    pub number: u64,
    /// The model it is for.
    pub model: ModelName,
    /// The count of removals its hold was given.
    pub removals: u64,
}

/// The call in the worker: the step of a request's turn that is in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calling {
    /// The request's number.
    pub number: u64,
    /// `unload`, `load` or `segment`.
    pub kind: &'static str,
    /// The model it names.
    pub model: ModelName,
    /// How many events it has said.
    pub events: u32,
    /// Whether it was written whole.
    pub written: bool,
    /// Whether its `cancel` was handed to the writer.
    pub cancel_sent: bool,
}

/// A request written to a worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    /// The number of the request whose turn it was in.
    pub number: u64,
    /// `unload`, `load` or `segment`.
    pub kind: &'static str,
    /// The model it names.
    pub model: ModelName,
    /// The process id of the worker it was written to.
    pub worker: u32,
}

/// What the supervisor is doing now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Status {
    /// The requests in the queue, front first.
    pub waiting: Vec<Waiting>,
    /// The call in the worker, if one is.
    pub calling: Option<Calling>,
    /// The model resident in the worker, if one is.
    pub resident: Option<ModelName>,
    /// The worker's process id, if one runs.
    pub worker: Option<u32>,
}

/// The model resident in a worker: what was loaded, in which worker.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Resident {
    model: ModelName,
    installation: InstallationId,
    settings: BTreeMap<String, Scalar>,
    worker: Option<u32>,
}

#[derive(Default)]
struct Shared {
    next: u64,
    waiting: VecDeque<Waiting>,
    /// Requests ended while they waited, by a restart, until their callers take them.
    ended: BTreeMap<u64, Box<Failed>>,
    calling: Option<Calling>,
    /// Whether a turn holds the worker.
    turn: bool,
    resident: Option<Resident>,
    worker: Option<u32>,
    journal: VecDeque<Sent>,
}

/// The one supervisor of a process (D4): the queue, and the worker behind it.
pub struct Supervisor {
    worker: Mutex<Worker>,
    shared: Mutex<Shared>,
    changed: Condvar,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for Supervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Supervisor")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

/// A turn: the worker is the turn's until it is dropped, on every path, a panic among
/// them; then the queue moves on.
struct Turn<'a> {
    supervisor: &'a Supervisor,
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        let mut shared = self.supervisor.lock();
        shared.turn = false;
        shared.calling = None;
        if std::thread::panicking() {
            // A panic inside a call ends its worker (`Worker`'s own drop of what it
            // runs), so nothing is known to be resident.
            shared.resident = None;
        }
        self.supervisor.changed.notify_all();
    }
}

impl Supervisor {
    /// The supervisor of this worker, on its clock. It starts nothing.
    pub fn new(worker: Worker) -> Supervisor {
        let clock = worker.clock();
        Supervisor {
            worker: Mutex::new(worker),
            shared: Mutex::default(),
            changed: Condvar::new(),
            clock,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// One request, in its turn: see the module's words. Its call's result, or a failure
    /// whose fault says how far the request got.
    pub fn call(&self, request: Request, limits: &Limits) -> Result<Called, Box<Failed>> {
        let fitting = matches!(
            &request.ask,
            Ask::Segment { model, installation, .. }
                if *model == request.model && *installation == request.installation
        );
        if !fitting {
            let words = "the request's call names another model or installation than its load; it was not sent";
            return Err(failure(Fault::new(FaultKind::Rejected, words).not_sent()));
        }
        let number = {
            let mut shared = self.lock();
            if shared.waiting.len() >= QUEUED_MOST {
                let words = format!(
                    "the queue of the pack's worker is full, {QUEUED_MOST} requests wait; this one was not held"
                );
                return Err(failure(
                    Fault::new(FaultKind::Unavailable, &words).not_sent(),
                ));
            }
            shared.next += 1;
            let number = shared.next;
            shared.waiting.push_back(Waiting {
                number,
                model: request.model.clone(),
                removals: request.removals,
            });
            self.changed.notify_all();
            number
        };
        let turn = self.wait_for_turn(number, limits)?;
        let served = {
            let mut worker = self.worker.lock().unwrap_or_else(PoisonError::into_inner);
            self.serve(&mut worker, number, request, limits)
        };
        let mut shared = self.lock();
        let served = match served {
            Err(failed) if failed.ended.is_some() => {
                // The worker ended: a restart. Every request waiting ends too, never sent.
                shared.resident = None;
                shared.worker = None;
                let waiting: Vec<Waiting> = shared.waiting.drain(..).collect();
                for waiting in waiting {
                    let words = "the pack's worker was restarted before this request's turn came; it was not sent";
                    let fault = Fault::new(FaultKind::Unavailable, words).not_sent();
                    shared.ended.insert(waiting.number, failure(fault));
                }
                Err(restarted(failed))
            }
            other => other,
        };
        drop(shared);
        drop(turn);
        served
    }

    /// Waits for the request's turn: the front of the queue, and no turn holding the
    /// worker. It leaves the queue at its cancel, its deadline or [`WAIT_LIMIT`], never
    /// sent; and hands back its end when a restart ended it while it waited.
    fn wait_for_turn(&self, number: u64, limits: &Limits) -> Result<Turn<'_>, Box<Failed>> {
        let begun = self.clock.now();
        let longest = begun.checked_add(WAIT_LIMIT);
        let mut shared = self.lock();
        loop {
            if let Some(failed) = shared.ended.remove(&number) {
                return Err(failed);
            }
            let front = shared.waiting.front().map(|waiting| waiting.number);
            if front == Some(number) && !shared.turn {
                shared.waiting.pop_front();
                shared.turn = true;
                self.changed.notify_all();
                return Ok(Turn { supervisor: self });
            }
            let now = self.clock.now();
            let place = "while it waited in the queue of the pack's worker";
            let left = limits.ended_before_sending(now, place).or_else(|| {
                longest.is_none_or(|longest| now >= longest).then(|| {
                    let words = format!(
                        "the request waited {} seconds in the queue of the pack's worker, the most the calls ahead of it can take; it was not sent",
                        WAIT_LIMIT.as_secs()
                    );
                    Fault::new(FaultKind::TimedOut, &words).not_sent()
                })
            });
            if let Some(fault) = left {
                shared.waiting.retain(|waiting| waiting.number != number);
                self.changed.notify_all();
                return Err(failure(fault));
            }
            shared = self
                .changed
                .wait_timeout(shared, PACE)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// A request's turn: the unload of another resident model and the load of its own,
    /// when its own is not resident, then its call. The first step that fails ends it.
    fn serve(
        &self,
        worker: &mut Worker,
        number: u64,
        request: Request,
        limits: &Limits,
    ) -> Result<Called, Box<Failed>> {
        let wanted = Resident {
            model: request.model.clone(),
            installation: request.installation,
            settings: request.settings.clone(),
            worker: worker.pid(),
        };
        let resident = self.lock().resident.clone();
        // What a worker that has ended, or another worker, held is not resident.
        let resident = resident.filter(|resident| resident.worker == worker.pid());
        if resident.as_ref() != Some(&wanted) || wanted.worker.is_none() {
            if let Some(old) = resident {
                let unload = Ask::Unload {
                    model: old.model,
                    installation: old.installation,
                };
                self.step(worker, number, unload, limits)?;
                self.lock().resident = None;
            }
            let load = Ask::Load {
                model: request.model,
                installation: request.installation,
                settings: request.settings,
            };
            self.step(worker, number, load, limits)?;
            self.lock().resident = Some(Resident {
                worker: worker.pid(),
                ..wanted
            });
        }
        self.step(worker, number, request.ask, limits)
    }

    /// One call of the worker's in a request's turn, told to the status and the journal
    /// as it goes.
    fn step(
        &self,
        worker: &mut Worker,
        number: u64,
        ask: Ask,
        limits: &Limits,
    ) -> Result<Called, Box<Failed>> {
        let (kind, model) = match &ask {
            Ask::Unload { model, .. } => ("unload", model.clone()),
            Ask::Load { model, .. } => ("load", model.clone()),
            Ask::Segment { model, .. } => ("segment", model.clone()),
        };
        {
            let mut shared = self.lock();
            shared.calling = Some(Calling {
                number,
                kind,
                model: model.clone(),
                events: 0,
                written: false,
                cancel_sent: false,
            });
            self.changed.notify_all();
        }
        let mut watch = |progress: Progress| {
            let mut shared = self.lock();
            if let Progress::Written { worker } = progress {
                if shared.journal.len() == JOURNAL_KEPT {
                    shared.journal.pop_front();
                }
                shared.journal.push_back(Sent {
                    number,
                    kind,
                    model: model.clone(),
                    worker,
                });
                shared.worker = Some(worker);
            }
            if let Some(calling) = shared.calling.as_mut() {
                match progress {
                    Progress::Written { .. } => calling.written = true,
                    Progress::Event => calling.events = calling.events.saturating_add(1),
                    Progress::CancelSent => calling.cancel_sent = true,
                }
            }
            self.changed.notify_all();
        };
        let result = worker.call_within(ask, limits, &mut watch);
        let mut shared = self.lock();
        shared.worker = worker.pid();
        self.changed.notify_all();
        result
    }

    /// What the supervisor is doing now.
    pub fn status(&self) -> Status {
        let shared = self.lock();
        Status {
            waiting: shared.waiting.iter().cloned().collect(),
            calling: shared.calling.clone(),
            resident: shared
                .resident
                .as_ref()
                .map(|resident| resident.model.clone()),
            worker: shared.worker,
        }
    }

    /// The last [`JOURNAL_KEPT`] requests written to a worker, oldest first.
    pub fn journal(&self) -> Vec<Sent> {
        self.lock().journal.iter().cloned().collect()
    }

    /// Waits until the status is as `until` says, or `within` of the clock has passed,
    /// and at most a bounded count of looks. Says whether it was.
    pub fn wait_for(&self, within: Duration, until: &dyn Fn(&Status) -> bool) -> bool {
        let end = self.clock.now().checked_add(within);
        let most = 2 * (within.as_millis() / PACE.as_millis()).max(1);
        for _ in 0..most {
            if until(&self.status()) {
                return true;
            }
            if end.is_none_or(|end| self.clock.now() >= end) {
                break;
            }
            let shared = self.lock();
            drop(
                self.changed
                    .wait_timeout(shared, PACE)
                    .unwrap_or_else(PoisonError::into_inner),
            );
        }
        until(&self.status())
    }

    /// Kills the worker, once the call in it has ended, and waits for it. Says how it
    /// ended; nothing when none was running.
    pub fn kill(&self) -> Option<Ended> {
        let ended = self
            .worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .kill();
        let mut shared = self.lock();
        shared.worker = None;
        shared.resident = None;
        ended
    }
}

/// A failure the queue makes: its fault, and nothing of a worker's.
fn failure(fault: Fault) -> Box<Failed> {
    Box::new(Failed {
        fault,
        said: None,
        ended: None,
        error_output: Vec::new(),
    })
}

/// The failure of a request whose step ended the worker, its sentence saying that the
/// worker is restarted; how far it got is unchanged.
fn restarted(mut failed: Box<Failed>) -> Box<Failed> {
    let words = format!(
        "{}; the pack's worker is restarted for the next request",
        failed.fault.sentence()
    );
    let fault = Fault::new(failed.fault.kind(), &words);
    failed.fault = match failed.fault.stage() {
        Stage::NotSent => fault.not_sent(),
        Stage::Answered => fault.answered(None),
        Stage::Unresolved => fault.unresolved(),
    };
    failed
}
