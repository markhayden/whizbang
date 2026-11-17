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
//! that the calls ahead end first. A waiting request sleeps until the earliest of its
//! deadline, that bound and its next read of the count of removals, or until the queue
//! changes; so its cancel is seen within [`READ_REMOVALS_EVERY`].
//!
//! **A restart fails everything in flight.** A step that ends the worker (it died, it was
//! killed at a limit, a deadline or a cancel it ignored, or it broke the protocol) ends
//! its request with its fault, whose sentence then says what the next request finds: a
//! worker started again, a worker started at all when the one that ended said no hello,
//! or none until the pause after three in a row; every request waiting then ends too,
//! `unavailable` and `not_sent`, with a sentence that says the worker was restarted before
//! its turn. Nothing is sent again. The next request starts another worker; one that ends
//! before it answers anything is not started again in a loop
//! ([`super::worker::STARTS_FAILED_MOST`]).
//!
//! **Removal** (task P5b; the plan's decisions D5 and D6). Every request keeps the count of
//! removals its hold was given. A removal in this process tells the supervisor each time
//! it sets its mark ([`Supervisor::removing`], through
//! [`crate::models::remove_and_tell`]): every request waiting for the model ends at once,
//! refused and never sent, so its caller lets its hold go, and the model is unloaded when
//! it is resident and no turn holds the worker. A removal in another process cannot tell
//! it: each waiting request reads its model's count every [`READ_REMOVALS_EVERY`] of the
//! clock, and again when it is taken off the queue, and ends, refused and never sent, when
//! the count has moved from its own, whether or not the mark is still there, or cannot be
//! read; when the count has not moved it asks [`crate::models::draining`] and no other
//! question of the mark. The current call is not ended: the removal waits for its hold.
//!
//! **Its end** ([`Supervisor::kill`]): every request waiting ends, the call in the worker
//! ends at its next look and the worker is killed and waited for, without waiting for the
//! call; nothing is taken after.
//!
//! **What it tells.** [`Supervisor::status`]: who waits (each with the count of removals
//! its hold was given), the call in the worker, the resident model and the worker's
//! process id; [`Supervisor::wait_for`] a state of these; and [`Supervisor::journal`], the
//! last [`JOURNAL_KEPT`] requests written to a worker, each with its request's number (0
//! for an unload a removal asked for) and the worker's process id.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use whizbang_core::capability::{Fault, FaultKind, Stage};
use whizbang_core::contract::{Clock, Secrets};

use crate::models::{InstallationId, ModelName, ModelsDir, draining, removals};

use super::proto::{Ask, Scalar};
use super::worker::{
    CALL_LIMIT, CANCEL_LIMIT, Called, END_LIMIT, Ended, Failed, HELLO_LIMIT, LOAD_LIMIT, Limits,
    Progress, RESTART_PAUSE, STARTS_FAILED_MOST, UNLOAD_LIMIT, Worker,
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
/// How often, on the clock, a waiting request reads its model's count of removals (D5).
/// It is also the longest a waiting request sleeps between two looks at its limits.
pub const READ_REMOVALS_EVERY: Duration = Duration::from_millis(250);
/// How long a wait for a state of the supervisor ([`Supervisor::wait_for`]) lets pass,
/// at most, between two looks.
const PACE: Duration = Duration::from_millis(5);

/// The sentence of a request a removal in this process ended while it waited, or one
/// whose model a live removal had marked when it was read.
const REMOVED_HERE: &str = "the model is being removed; the request was ended while it waited in the queue of the pack's worker, and was not sent";
/// The sentence of a request whose model's count of removals had moved when it was read.
const REMOVED_ELSEWHERE: &str = "a removal of the model began after the request's hold was given; the request was ended while it waited in the queue of the pack's worker, and was not sent";
/// The sentence of a request whose model's count of removals could not be read.
const COUNT_UNREAD: &str = "the model's count of removals cannot be read; the request was ended while it waited in the queue of the pack's worker, and was not sent";
/// The sentence of a request that waited, or was asked, when the supervisor was ended.
const SUPERVISOR_ENDED: &str = "the supervisor of the pack's worker was ended before this request's turn came; it was not sent";

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
    /// P5a): kept while the request waits, and compared with the model's (task P5b).
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
    /// The request's number; 0 for an unload a removal asked for.
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
    /// The number of the request whose turn it was in; 0 for an unload a removal asked
    /// for.
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
    /// Requests ended while they waited, by a restart, a removal or the supervisor's
    /// end, until their callers take them.
    ended: BTreeMap<u64, Box<Failed>>,
    calling: Option<Calling>,
    /// Whether a turn holds the worker.
    turn: bool,
    /// Whether the supervisor was ended ([`Supervisor::kill`]): nothing is taken now.
    closing: bool,
    resident: Option<Resident>,
    worker: Option<u32>,
    journal: VecDeque<Sent>,
}

impl Shared {
    /// The worker ended: nothing is resident, and every request waiting ends, never
    /// sent.
    fn worker_ended(&mut self) {
        self.resident = None;
        self.worker = None;
        let waiting: Vec<Waiting> = self.waiting.drain(..).collect();
        for waiting in waiting {
            let words =
                "the pack's worker was restarted before this request's turn came; it was not sent";
            let fault = Fault::new(FaultKind::Unavailable, words).not_sent();
            self.ended.insert(waiting.number, failure(fault));
        }
    }

    /// This request leaves the queue, if it is in it.
    fn leave(&mut self, number: u64) {
        self.waiting.retain(|waiting| waiting.number != number);
    }
}

/// The one supervisor of a process (D4): the queue, and the worker behind it.
pub struct Supervisor {
    worker: Mutex<Worker>,
    shared: Mutex<Shared>,
    changed: Condvar,
    clock: Arc<dyn Clock>,
    /// The worker's [`Worker::halt`], set when the supervisor is ended.
    halt: Arc<AtomicBool>,
    /// The models directory where the counts of removals are read.
    models: ModelsDir,
    secrets: Secrets,
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
    /// The supervisor of this worker, on its clock, reading the counts of removals in
    /// this models directory: the one the worker is told of. It starts nothing.
    pub fn new(worker: Worker, models: ModelsDir) -> Supervisor {
        let clock = worker.clock();
        let secrets = worker.secrets();
        let halt = worker.halt();
        Supervisor {
            worker: Mutex::new(worker),
            shared: Mutex::default(),
            changed: Condvar::new(),
            clock,
            halt,
            models,
            secrets,
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
            if shared.closing {
                let fault = Fault::new(FaultKind::Unavailable, SUPERVISOR_ENDED).not_sent();
                return Err(failure(fault));
            }
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
        let turn = self.wait_for_turn(number, &request.model, request.removals, limits)?;
        let (served, next) = {
            let mut worker = self.worker.lock().unwrap_or_else(PoisonError::into_inner);
            let served = self.serve(&mut worker, number, request, limits);
            (served, self.what_comes_next(&worker))
        };
        let mut shared = self.lock();
        let served = match served {
            Err(failed) if failed.ended.is_some() => {
                // The worker ended: a restart. Every request waiting ends too, never sent.
                shared.worker_ended();
                Err(worker_gone(failed, next))
            }
            other => other,
        };
        drop(shared);
        drop(turn);
        served
    }

    /// What a request whose step ended the worker is told comes next, after its own
    /// sentence: nothing when the supervisor was ended.
    fn what_comes_next(&self, worker: &Worker) -> Option<String> {
        if self.halt.load(Ordering::SeqCst) {
            None
        } else if worker.paused() {
            Some(format!(
                "; {STARTS_FAILED_MOST} workers in a row ended before they answered anything, and no worker is started until {} seconds after the last",
                RESTART_PAUSE.as_secs()
            ))
        } else if !worker.said_hello() {
            Some("; it said no hello, and the next request starts another worker".to_owned())
        } else {
            Some("; the pack's worker is restarted for the next request".to_owned())
        }
    }

    /// Waits for the request's turn: the front of the queue, and no turn holding the
    /// worker. It leaves the queue at its cancel, its deadline or [`WAIT_LIMIT`], and at
    /// a count of removals of its model that has moved from its own or cannot be read,
    /// never sent; and hands back its end when a restart, a removal in this process or
    /// the supervisor's end ended it while it waited.
    ///
    /// The count is read every [`READ_REMOVALS_EVERY`] of the clock, and when the
    /// request is taken off the queue, without the queue's lock. Between two looks the
    /// request sleeps until the earliest of its deadline, [`WAIT_LIMIT`] and its next
    /// read, or until the queue changes.
    fn wait_for_turn(
        &self,
        number: u64,
        model: &ModelName,
        own: u64,
        limits: &Limits,
    ) -> Result<Turn<'_>, Box<Failed>> {
        let begun = self.clock.now();
        let longest = begun.checked_add(WAIT_LIMIT);
        let mut next_read = begun.checked_add(READ_REMOVALS_EVERY);
        let mut shared = self.lock();
        loop {
            if let Some(failed) = shared.ended.remove(&number) {
                return Err(failed);
            }
            let front = shared.waiting.front().map(|waiting| waiting.number);
            let due = front == Some(number) && !shared.turn && !shared.closing;
            let now = self.clock.now();
            if due || next_read.is_some_and(|at| now >= at) {
                drop(shared);
                let removed = self.removed_since(model, own);
                shared = self.lock();
                next_read = now.checked_add(READ_REMOVALS_EVERY);
                if let Some(failed) = shared.ended.remove(&number) {
                    return Err(failed);
                }
                if let Some(fault) = removed {
                    shared.leave(number);
                    self.changed.notify_all();
                    return Err(failure(fault));
                }
                let front = shared.waiting.front().map(|waiting| waiting.number);
                if due && front == Some(number) && !shared.turn && !shared.closing {
                    shared.waiting.pop_front();
                    shared.turn = true;
                    self.changed.notify_all();
                    return Ok(Turn { supervisor: self });
                }
                continue;
            }
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
                shared.leave(number);
                self.changed.notify_all();
                return Err(failure(fault));
            }
            // Asleep until the first of its own times, or until the queue changes.
            let wake = [limits.deadline, longest, next_read]
                .into_iter()
                .flatten()
                .min();
            let span = wake.map_or(READ_REMOVALS_EVERY, |at: Instant| {
                at.saturating_duration_since(now)
            });
            shared = self
                .changed
                .wait_timeout(shared, span.min(READ_REMOVALS_EVERY))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// The fault of a waiting request for this model whose hold was given this count of
    /// removals, if a removal ends it: the model's count has moved from it, or cannot be
    /// read; or, the count as it was, the model is draining or its mark cannot be read
    /// under a live remover. Nothing when the request may go on.
    fn removed_since(&self, model: &ModelName, own: u64) -> Option<Fault> {
        let words = match removals(&self.models, model, &self.secrets) {
            Ok(count) if count != own => REMOVED_ELSEWHERE,
            Ok(_) => match draining(&self.models, model, &self.secrets) {
                Ok(false) => return None,
                Ok(true) | Err(_) => REMOVED_HERE,
            },
            Err(_) => COUNT_UNREAD,
        };
        Some(Fault::new(FaultKind::Rejected, words).not_sent())
    }

    /// A removal of this model in this process has set its mark (task P5b): every
    /// request waiting for the model ends at once, refused and never sent, and its
    /// caller lets its hold go; and when the model is resident and no turn holds the
    /// worker, it is unloaded, on the caller's thread, within the limit of an unload.
    /// The current call is not ended: the removal waits for its hold. A request asked
    /// after this was refused its hold (the lease), or is ended by its count.
    pub fn removing(&self, model: &ModelName) {
        let mut shared = self.lock();
        let (ending, staying): (VecDeque<Waiting>, VecDeque<Waiting>) =
            std::mem::take(&mut shared.waiting)
                .into_iter()
                .partition(|waiting| waiting.model == *model);
        shared.waiting = staying;
        for waiting in ending {
            let fault = Fault::new(FaultKind::Rejected, REMOVED_HERE).not_sent();
            shared.ended.insert(waiting.number, failure(fault));
        }
        self.changed.notify_all();
        let idle = !shared.turn
            && !shared.closing
            && shared
                .resident
                .as_ref()
                .is_some_and(|resident| resident.model == *model);
        if !idle {
            return;
        }
        shared.turn = true;
        drop(shared);
        let turn = Turn { supervisor: self };
        let mut worker = self.worker.lock().unwrap_or_else(PoisonError::into_inner);
        let resident = self.lock().resident.clone().filter(|resident| {
            resident.model == *model && resident.worker.is_some() && resident.worker == worker.pid()
        });
        if let Some(old) = resident {
            let unload = Ask::Unload {
                model: old.model,
                installation: old.installation,
            };
            let unloaded = self.step(&mut worker, 0, unload, &Limits::none());
            let mut shared = self.lock();
            match unloaded {
                Ok(_) => shared.resident = None,
                Err(failed) if failed.ended.is_some() => shared.worker_ended(),
                // The worker refused the unload and lives on: what it holds is as it was.
                Err(_) => {}
            }
        }
        drop(worker);
        drop(turn);
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

    /// Ends the supervisor, without waiting for the call in the worker: every request
    /// waiting ends, `unavailable` and never sent; the call in the worker ends at its
    /// next look, `unavailable`, and the worker is killed and waited for; a worker being
    /// started is ended too. Nothing is taken after: a request asked then is refused at
    /// once, and no worker is started. Says how the worker ended; nothing when none was
    /// running.
    pub fn kill(&self) -> Option<Ended> {
        {
            let mut shared = self.lock();
            shared.closing = true;
            let waiting: Vec<Waiting> = shared.waiting.drain(..).collect();
            for waiting in waiting {
                let fault = Fault::new(FaultKind::Unavailable, SUPERVISOR_ENDED).not_sent();
                shared.ended.insert(waiting.number, failure(fault));
            }
            self.changed.notify_all();
        }
        self.halt.store(true, Ordering::SeqCst);
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

/// The failure of a request whose step ended the worker, its sentence saying what comes
/// next, when anything does; how far it got is unchanged.
fn worker_gone(mut failed: Box<Failed>, next: Option<String>) -> Box<Failed> {
    let Some(next) = next else {
        return failed;
    };
    let words = format!("{}{next}", failed.fault.sentence());
    let fault = Fault::new(failed.fault.kind(), &words);
    failed.fault = match failed.fault.stage() {
        Stage::NotSent => fault.not_sent(),
        Stage::Answered => fault.answered(None),
        Stage::Unresolved => fault.unresolved(),
    };
    failed
}
