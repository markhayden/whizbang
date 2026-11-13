//! A run that this process has open: what its executor shares, while the run executes,
//! with whoever asks after it or cancels it (ADR 0008, "Status", "Cancellation").
//!
//! It is attached to the run in the store's table of open runs
//! ([`crate::store::Publication::attach`]) by the publication that admits the run, so
//! it is found from just before the run is visible until the run's end is visible and
//! the executor is done with the run, and by any executor over the same store in this
//! process.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use crate::store::Attached;

use super::journal::Kept;
use super::nested::Nest;
use super::{Cancellation, JournalLine, StoreFailure};

/// Where the executor is with an open run.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    /// The run was admitted and its end has not begun: its action has not started, or
    /// has not come back.
    Executing,
    /// The action has come back, and the executor is ending the run: for a run that
    /// completed, this is its commit step.
    Ending,
    /// The run's end was published, and confirmed.
    Ended,
    /// The executor is done with the run and did not end it, or was not told that it
    /// had: with the store's failure, when one was why. Whether the run ended all the
    /// same, visibly and unconfirmed, is the store's to say.
    Abandoned(Option<StoreFailure>),
}

impl Phase {
    fn is_over(&self) -> bool {
        matches!(self, Phase::Ended | Phase::Abandoned(_))
    }
}

/// What a cancellation of an open run came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Told {
    /// It arrived before the run's end began: the run's signal is set.
    Accepted,
    /// It arrived after: the run is over now, and the store says how.
    TooLate,
}

/// One open run.
pub(super) struct LiveRun {
    kept: Arc<Kept>,
    /// The run's cancellation signal: the one in its action's context, and the one its
    /// capability handles are bound to.
    signal: Cancellation,
    /// The run's own lock. A cancellation and the beginning of the run's end are each
    /// decided under it, so one of them is first. It is held only to read or change
    /// the phase and to set or read the signal: never across a call to the file system,
    /// to the store or to an action, and never while another lock is taken.
    phase: Mutex<Phase>,
    changed: Condvar,
    /// How many callers are waiting, now, for the run to be over.
    waiting: AtomicUsize,
    /// The calls the run's action makes of other actions (ADR 0008, "Nested calls").
    nest: Nest,
}

impl LiveRun {
    pub(super) fn new(kept: Arc<Kept>, signal: Cancellation) -> LiveRun {
        LiveRun {
            kept,
            signal,
            phase: Mutex::new(Phase::Executing),
            changed: Condvar::new(),
            waiting: AtomicUsize::new(0),
            nest: Nest::default(),
        }
    }

    /// The calls the run's action makes of other actions.
    pub(super) fn nest(&self) -> &Nest {
        &self.nest
    }

    fn phase(&self) -> MutexGuard<'_, Phase> {
        self.phase.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The run's cancellation signal.
    pub(super) fn signal(&self) -> &Cancellation {
        &self.signal
    }

    /// The progress events the run has reported so far, the secrets taken out.
    pub(super) fn events(&self) -> Vec<JournalLine> {
        self.kept.lines()
    }

    /// Cancels the run, if its end has not begun: the rule of ADR 0008. Accepted, the
    /// signal is set under the run's lock, so the executor sees it when it begins the
    /// run's end. Too late, this waits until the run is over, holding nothing.
    pub(super) fn cancel(&self) -> Told {
        let phase = self.phase();
        if *phase == Phase::Executing {
            self.signal.cancel();
            return Told::Accepted;
        }
        self.until_over(phase);
        Told::TooLate
    }

    /// Sets the run's signal if its end has not begun, and waits for nothing: for the
    /// thread that deleted the run from the store, which the run's end would wait for.
    pub(super) fn stop(&self) {
        if *self.phase() == Phase::Executing {
            self.signal.cancel();
        }
    }

    /// The executor begins the run's end: from here on a cancellation is too late.
    /// What comes back is whether the run was cancelled before this, which is read
    /// under the same lock a cancellation is accepted under.
    pub(super) fn begin_end(&self) -> bool {
        let mut phase = self.phase();
        *phase = Phase::Ending;
        self.signal.is_cancelled()
    }

    /// Nothing in this process works on the run any more. `ended` says that its end
    /// was published and confirmed; otherwise the run was abandoned, with this
    /// failure of the store when there was one.
    pub(super) fn over(&self, ended: bool, failure: Option<StoreFailure>) {
        *self.phase() = match ended {
            true => Phase::Ended,
            false => Phase::Abandoned(failure),
        };
        self.changed.notify_all();
    }

    /// Whether the executor abandoned the run, and with what failure of the store.
    pub(super) fn abandoned(&self) -> Option<Option<StoreFailure>> {
        match &*self.phase() {
            Phase::Abandoned(failure) => Some(failure.clone()),
            _ => None,
        }
    }

    /// Returns once nothing in this process works on the run any more.
    pub(super) fn wait_until_over(&self) {
        self.until_over(self.phase());
    }

    fn until_over(&self, mut phase: MutexGuard<'_, Phase>) {
        // Counted under the lock, before the first wait: whoever reads the count as
        // one or more has a caller that is waiting, or about to with the lock let go.
        self.waiting.fetch_add(1, Ordering::SeqCst);
        while !phase.is_over() {
            phase = self
                .changed
                .wait(phase)
                .unwrap_or_else(PoisonError::into_inner);
        }
        self.waiting.fetch_sub(1, Ordering::SeqCst);
    }

    /// How many callers are waiting, now, for the run to be over.
    pub(super) fn waiting(&self) -> usize {
        self.waiting.load(Ordering::SeqCst)
    }

    /// For a test: the lock whoever waits is told under, held. While it is held nobody
    /// is told that the run is over.
    #[cfg(test)]
    pub(super) fn not_told_yet(&self) -> NotTold<'_> {
        NotTold(self.phase())
    }
}

/// For a test: the lock of a run, held ([`LiveRun::not_told_yet`]).
#[cfg(test)]
pub(super) struct NotTold<'a>(MutexGuard<'a, Phase>);

#[cfg(test)]
impl NotTold<'_> {
    /// Whether the run is over for this process.
    pub(super) fn is_over(&self) -> bool {
        self.0.is_over()
    }
}

/// The open run that is attached to a run of the store, when an executor attached one.
pub(super) fn of(attached: Attached) -> Option<Arc<LiveRun>> {
    attached.downcast::<LiveRun>().ok()
}
