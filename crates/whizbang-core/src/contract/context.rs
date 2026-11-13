//! What an executing action is handed (`SPEC.md` section 3, "Execute"): a cancellation
//! signal, a deadline, an event sink for progress, a spend allowance, the store,
//! capability handles and the executor.
//!
//! The executor makes one of these for each run (ADR 0005). The cancellation signal and
//! the deadline are carried and nothing sets them yet, and the allowance is empty until
//! there is a ledger. The capability handles are the capability interfaces' (ADR 0006);
//! the executor hands an action none of them yet.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::artifact::Contents;
use crate::capability::{FillHandle, JudgeHandle, SegmentHandle};

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

/// Where an action writes diagnostic evidence: an intermediate image, an exchange that
/// failed, a worker's error output. What is written is kept with the run whatever its
/// outcome, marked as diagnostic, and can never be used as an output.
pub trait Diagnostics: Send + Sync {
    /// Keeps `bytes` under `name` among the run's diagnostics. A name is a lower-case
    /// letter or a digit, then lower-case letters, digits, `_`, `-` or `.`, 64 characters
    /// at most, and is written once in a run. Text has the secrets taken out before it
    /// is kept; bytes that are not UTF-8 are kept as they are.
    fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String>;
}

/// Nowhere to write: what a context has until it is given a place.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoDiagnostics;

impl Diagnostics for NoDiagnostics {
    fn write(&self, _: &str, _: &[u8]) -> Result<(), String> {
        Err("this context keeps no diagnostics".to_owned())
    }
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

/// The capabilities a context offers: for each of the three, a handle, or none. An action
/// reaches a model through one of these and in no other way.
#[derive(Debug, Clone)]
pub struct Capabilities {
    segment: Option<SegmentHandle>,
    fill: Option<FillHandle>,
    judge: Option<JudgeHandle>,
}

impl Capabilities {
    /// No capability at all.
    pub fn none() -> Capabilities {
        Capabilities {
            segment: None,
            fill: None,
            judge: None,
        }
    }

    /// The same set, offering this segmenter.
    pub fn with_segment(mut self, handle: SegmentHandle) -> Capabilities {
        self.segment = Some(handle);
        self
    }

    /// The same set, offering this filler.
    pub fn with_fill(mut self, handle: FillHandle) -> Capabilities {
        self.fill = Some(handle);
        self
    }

    /// The same set, offering this judge.
    pub fn with_judge(mut self, handle: JudgeHandle) -> Capabilities {
        self.judge = Some(handle);
        self
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
    diagnostics: Arc<dyn Diagnostics>,
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
            diagnostics: Arc::new(NoDiagnostics),
        }
    }

    /// The same context, writing its diagnostics here.
    pub fn with_diagnostics(mut self, diagnostics: Arc<dyn Diagnostics>) -> Context {
        self.diagnostics = diagnostics;
        self
    }

    /// Where diagnostic evidence is written.
    pub fn diagnostics(&self) -> &dyn Diagnostics {
        self.diagnostics.as_ref()
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
