//! What an executing action is handed (`SPEC.md` section 3, "Execute"): a cancellation
//! signal, a deadline, an event sink for progress, a spend allowance, the store,
//! capability handles and the executor.
//!
//! These are the types. Nothing executes an action yet, so nothing here is given to one:
//! what each must do when an action runs is decided with the code that runs it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::artifact::Contents;

use super::Executor;

/// A cancellation signal. Every clone is the same signal: once one is cancelled, all are,
/// and none is ever uncancelled.
#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn new() -> Cancellation {
        Cancellation::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Where an action reports progress.
pub trait Events: Send + Sync {
    /// The action got this far.
    fn progress(&self, message: &str);
}

/// What an action may spend. It holds nothing yet: amounts, reservation and settlement
/// arrive with the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowance {
    _contents: (),
}

impl Allowance {
    /// An allowance of nothing.
    pub fn none() -> Allowance {
        Allowance { _contents: () }
    }
}

/// The handle through which an action asks a segmenter. Its contents, the request and
/// the response, are the capability interfaces' to define.
#[derive(Debug, Clone)]
pub struct SegmentHandle {
    _contents: (),
}

/// The handle through which an action asks a filler. Its contents are the capability
/// interfaces' to define.
#[derive(Debug, Clone)]
pub struct FillHandle {
    _contents: (),
}

/// The handle through which an action asks a judge. Its contents are the capability
/// interfaces' to define.
#[derive(Debug, Clone)]
pub struct JudgeHandle {
    _contents: (),
}

/// The capabilities a context offers: for each of the three, a handle, or none. An action
/// reaches a model through one of these and in no other way.
#[derive(Debug, Clone)]
pub struct Capabilities {
    segment: Option<SegmentHandle>,
    fill: Option<FillHandle>,
    judge: Option<JudgeHandle>,
}

impl Capabilities {
    /// No capability at all. It is the only set there is until a handle can be made.
    pub fn none() -> Capabilities {
        Capabilities {
            segment: None,
            fill: None,
            judge: None,
        }
    }

    pub fn segment(&self) -> Option<&SegmentHandle> {
        self.segment.as_ref()
    }

    pub fn fill(&self) -> Option<&FillHandle> {
        self.fill.as_ref()
    }

    pub fn judge(&self) -> Option<&JudgeHandle> {
        self.judge.as_ref()
    }
}

/// What one execution of an action is handed.
#[derive(Clone)]
pub struct Context {
    cancellation: Cancellation,
    deadline: Option<Instant>,
    events: Arc<dyn Events>,
    allowance: Allowance,
    store: Arc<dyn Contents + Send + Sync>,
    capabilities: Capabilities,
    executor: Arc<dyn Executor>,
}

impl Context {
    pub fn new(
        cancellation: Cancellation,
        deadline: Option<Instant>,
        events: Arc<dyn Events>,
        allowance: Allowance,
        store: Arc<dyn Contents + Send + Sync>,
        capabilities: Capabilities,
        executor: Arc<dyn Executor>,
    ) -> Context {
        Context {
            cancellation,
            deadline,
            events,
            allowance,
            store,
            capabilities,
            executor,
        }
    }

    /// The signal that the run was cancelled.
    pub fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }

    /// When the run must be over, if it has a limit.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Where progress is reported.
    pub fn events(&self) -> &dyn Events {
        self.events.as_ref()
    }

    /// What the run may spend.
    pub fn allowance(&self) -> &Allowance {
        &self.allowance
    }

    /// The store, to read from. An action's outputs do not go in through this: they are
    /// committed together when it completes.
    pub fn store(&self) -> &(dyn Contents + Send + Sync) {
        self.store.as_ref()
    }

    /// The capability handles.
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// The executor: the one way an action reaches another action.
    pub fn executor(&self) -> &dyn Executor {
        self.executor.as_ref()
    }
}
