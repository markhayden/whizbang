//! What an executing action is handed (`SPEC.md` section 3, "Execute"): a cancellation
//! signal, a deadline, an event sink for progress, a spend allowance, the store,
//! capability handles and the executor.
//!
//! The executor makes one of these for each run (ADR 0005), with the run's cancellation
//! signal and its deadline (ADR 0008, "Cancellation"); the allowance is what was
//! reserved for the run, or carved for it from its caller's (ADR 0008, "Nested
//! calls"). The capability handles are the capability interfaces' (ADR 0006),
//! and a context makes its own, bound to its own signal and deadline, from the backends
//! it is given.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use crate::artifact::Contents;
use crate::capability::{FillHandle, Filler, Judge, JudgeHandle, SegmentHandle, Segmenter, Tally};

use super::Executor;

/// A cancellation signal. Every clone is the same signal: once one is cancelled, all are,
/// and none is ever uncancelled.
///
/// The signal of a run that another run called is a signal of its own that is also set
/// whenever its caller's is: cancelling a run cancels every run under it, at once and
/// with nothing to pass on, and cancelling a run does not cancel its caller.
#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<Signal>);

#[derive(Debug, Default)]
struct Signal {
    set: AtomicBool,
    /// The signals this one is under: set when any of them is.
    above: Vec<Cancellation>,
}

/// Two signals are equal when they are the same signal.
impl PartialEq for Cancellation {
    fn eq(&self, other: &Cancellation) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Cancellation {
    pub fn new() -> Cancellation {
        Cancellation::default()
    }

    /// A new signal under these: it is set when it is cancelled itself, and whenever
    /// one of these is. Cancelling it sets none of them.
    pub(super) fn under(above: impl IntoIterator<Item = Cancellation>) -> Cancellation {
        Cancellation(Arc::new(Signal {
            set: AtomicBool::new(false),
            above: above.into_iter().collect(),
        }))
    }

    pub fn cancel(&self) {
        self.0.set.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.set.load(Ordering::SeqCst) || self.0.above.iter().any(Cancellation::is_cancelled)
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

/// What a run may spend, in millionths of a US dollar: what was reserved for it when
/// it was admitted, or, for a run another run called, what was carved for it from its
/// caller's allowance (ADR 0008, "Nested calls"). Each call the run makes takes its
/// estimate from what is left, under one lock, so two calls made at once cannot both
/// take the last of it. A run that was admitted with no ledger has no amount: nothing
/// was reserved, and nothing is counted. Settlement arrives with the ledger.
///
/// A clone is the same allowance. Two are equal when they are the same one.
#[derive(Debug, Clone)]
pub struct Allowance {
    purse: Option<Arc<Purse>>,
}

#[derive(Debug)]
struct Purse {
    total: u64,
    left: Mutex<u64>,
}

impl PartialEq for Allowance {
    fn eq(&self, other: &Allowance) -> bool {
        match (&self.purse, &other.purse) {
            (None, None) => true,
            (Some(one), Some(other)) => Arc::ptr_eq(one, other),
            _ => false,
        }
    }
}

impl Eq for Allowance {}

impl Allowance {
    /// No amount: nothing was reserved, and nothing is counted.
    pub fn none() -> Allowance {
        Allowance { purse: None }
    }

    /// An allowance of this much.
    pub(super) fn of(micros: u64) -> Allowance {
        Allowance {
            purse: Some(Arc::new(Purse {
                total: micros,
                left: Mutex::new(micros),
            })),
        }
    }

    /// What the run was given, when it was given an amount.
    pub fn total_micros(&self) -> Option<u64> {
        self.purse.as_ref().map(|purse| purse.total)
    }

    /// What is left of it now: the amount, less what the calls the run made took.
    pub fn left_micros(&self) -> Option<u64> {
        let purse = self.purse.as_ref()?;
        Some(*purse.left.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Takes this much for a call the run makes, when that much is left; otherwise
    /// nothing is taken, and what is left comes back. The look and the taking are one
    /// step, under the allowance's lock. None when the allowance has no amount.
    pub(super) fn take(&self, micros: u64) -> Result<Option<Taken>, u64> {
        let Some(purse) = &self.purse else {
            return Ok(None);
        };
        let mut left = purse.left.lock().unwrap_or_else(PoisonError::into_inner);
        if micros > *left {
            return Err(*left);
        }
        *left -= micros;
        Ok(Some(Taken {
            purse: purse.clone(),
            micros,
            kept: false,
        }))
    }
}

/// What was taken from an allowance for a call. Dropped before the call's run was
/// made, it is given back.
#[derive(Debug)]
pub(super) struct Taken {
    purse: Arc<Purse>,
    micros: u64,
    kept: bool,
}

impl Taken {
    /// The call's run was made: what was taken stays taken.
    pub(super) fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for Taken {
    fn drop(&mut self) {
        if !self.kept {
            let mut left = self
                .purse
                .left
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            *left = left.saturating_add(self.micros);
        }
    }
}

/// The backends a context's capability handles are made over: for each of the three
/// capabilities, a backend, or none. It holds no handle. A handle is bound to one run,
/// and only a context makes the handles an action is offered, with its own signal and
/// its own deadline ([`Context::new`]): so an action is never handed a handle bound to
/// another run's signal (ADR 0006, ADR 0008).
///
/// What `{:?}` prints is which of the three there are.
#[derive(Clone, Default)]
pub struct Capabilities {
    segment: Option<Arc<dyn Segmenter>>,
    fill: Option<Arc<dyn Filler>>,
    judge: Option<Arc<dyn Judge>>,
}

impl Capabilities {
    /// No backend at all.
    pub fn none() -> Capabilities {
        Capabilities::default()
    }

    /// The same set, with this segmenter.
    pub fn with_segment(mut self, backend: Arc<dyn Segmenter>) -> Capabilities {
        self.segment = Some(backend);
        self
    }

    /// The same set, with this filler.
    pub fn with_fill(mut self, backend: Arc<dyn Filler>) -> Capabilities {
        self.fill = Some(backend);
        self
    }

    /// The same set, with this judge.
    pub fn with_judge(mut self, backend: Arc<dyn Judge>) -> Capabilities {
        self.judge = Some(backend);
        self
    }

    /// A handle on each backend, bound to this signal and held to this deadline, each
    /// counting the requests it sends in this tally.
    fn bound(
        &self,
        cancellation: &Cancellation,
        deadline: Option<Instant>,
        tally: &Arc<Tally>,
    ) -> Handles {
        let run = || cancellation.clone();
        let segment = self.segment.clone();
        let segment = segment.map(|b| SegmentHandle::new(b, run()).counted(tally.clone()));
        let fill = self.fill.clone();
        let fill = fill.map(|b| FillHandle::new(b, run()).counted(tally.clone()));
        let judge = self.judge.clone();
        let judge = judge.map(|b| JudgeHandle::new(b, run()).counted(tally.clone()));
        match deadline {
            None => Handles {
                segment,
                fill,
                judge,
            },
            Some(by) => Handles {
                segment: segment.map(|handle| handle.until(by)),
                fill: fill.map(|handle| handle.until(by)),
                judge: judge.map(|handle| handle.until(by)),
            },
        }
    }
}

impl std::fmt::Debug for Capabilities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Capabilities")
            .field("segment", &self.segment.is_some())
            .field("fill", &self.fill.is_some())
            .field("judge", &self.judge.is_some())
            .finish()
    }
}

/// The capability handles of one context: for each of the three, a handle, or none. An
/// action reaches a model through one of these and in no other way. Each is bound to
/// the context's cancellation signal and held to its deadline, which an action may
/// only shorten. Only a context makes one.
#[derive(Debug, Clone)]
pub struct Handles {
    segment: Option<SegmentHandle>,
    fill: Option<FillHandle>,
    judge: Option<JudgeHandle>,
}

impl Handles {
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
    capabilities: Handles,
    /// Every request the handles above were sent, and whether each was answered.
    requests: Arc<Tally>,
    executor: Arc<dyn Executor>,
    diagnostics: Arc<dyn Diagnostics>,
}

impl Context {
    /// A context with this signal and this deadline. Its capability handles are made
    /// here, one on each backend of `capabilities`, bound to that signal and held to
    /// that deadline.
    pub fn new(
        cancellation: Cancellation,
        deadline: Option<Instant>,
        events: Arc<dyn Events>,
        allowance: Allowance,
        store: Arc<dyn Contents + Send + Sync>,
        capabilities: Capabilities,
        executor: Arc<dyn Executor>,
    ) -> Context {
        let requests = Arc::new(Tally::default());
        let capabilities = capabilities.bound(&cancellation, deadline, &requests);
        Context {
            cancellation,
            deadline,
            events,
            allowance,
            store,
            capabilities,
            requests,
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

    /// The capability handles, each bound to this context's signal and deadline.
    pub fn capabilities(&self) -> &Handles {
        &self.capabilities
    }

    /// The count of the requests this context's handles were sent, which the executor
    /// closes and waits on before it ends the run.
    pub(super) fn requests(&self) -> &Arc<Tally> {
        &self.requests
    }

    /// The executor: the one way an action reaches another action.
    pub fn executor(&self) -> &dyn Executor {
        self.executor.as_ref()
    }
}
