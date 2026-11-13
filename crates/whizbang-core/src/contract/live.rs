//! A run that this process has open: what its executor shares, while the run executes,
//! with whoever asks after it (ADR 0008, "Status").
//!
//! It is attached to the run in the store's table of open runs
//! ([`crate::store::Publication::attach`]) by the publication that admits the run, so
//! it is found from just before the run is visible until the run's end is visible, and
//! by any executor over the same store in this process.

use std::sync::{Arc, Condvar, Mutex, PoisonError};

use crate::store::Attached;

use super::JournalLine;
use super::journal::Kept;

/// One open run.
pub(super) struct LiveRun {
    kept: Arc<Kept>,
    /// Whether the executor is done with the run: it ended, or its end could not be
    /// written. Held only to read or to set the flag, never across anything else.
    over: Mutex<bool>,
    changed: Condvar,
}

impl LiveRun {
    pub(super) fn new(kept: Arc<Kept>) -> LiveRun {
        LiveRun {
            kept,
            over: Mutex::new(false),
            changed: Condvar::new(),
        }
    }

    /// The progress events the run has reported so far, the secrets taken out.
    pub(super) fn events(&self) -> Vec<JournalLine> {
        self.kept.lines()
    }

    /// Nothing in this process works on the run any more.
    pub(super) fn over(&self) {
        *self.over.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.changed.notify_all();
    }

    /// Returns once nothing in this process works on the run any more.
    pub(super) fn wait_until_over(&self) {
        let mut over = self.over.lock().unwrap_or_else(PoisonError::into_inner);
        while !*over {
            over = self
                .changed
                .wait(over)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// The open run that is attached to a run of the store, when an executor attached one.
pub(super) fn of(attached: Attached) -> Option<Arc<LiveRun>> {
    attached.downcast::<LiveRun>().ok()
}
