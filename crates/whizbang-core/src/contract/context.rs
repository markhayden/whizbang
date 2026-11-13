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
use crate::capability::{
    Declaration, FillHandle, Filler, Judge, JudgeHandle, Recorder, SegmentHandle, Segmenter, Tally,
};

use super::nested::Offer;
use super::{Capability, Clock, Executor, PreparedCall, Refusal};

/// A cancellation signal. Every clone is the same signal: once one is cancelled, all are,
/// and none is ever uncancelled.
///
/// The signal of a run that another run called is a signal of its own that is also set
/// whenever its caller's is: cancelling a run cancels every run under it, at once and
/// with nothing to pass on, and cancelling a run does not cancel its caller.
#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<Signal>);

#[derive(Default)]
struct Signal {
    set: AtomicBool,
    /// The signals this one is under: set when any of them is.
    above: Vec<Cancellation>,
    /// What is told when this signal is set, by cancelling it: those who wait for
    /// something else and give up when this is set.
    told: Mutex<Vec<(u64, Waker)>>,
}

/// What a signal tells when it is set.
type Waker = Arc<dyn Fn() + Send + Sync>;

/// The count that tells one registration from another.
static TOLD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl std::fmt::Debug for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signal")
            .field("set", &self.set)
            .field("above", &self.above)
            .finish()
    }
}

/// A registration with a signal and every signal above it. Dropped, it is taken out
/// of all of them.
pub(super) struct Telling {
    id: u64,
    signals: Vec<Cancellation>,
}

impl Drop for Telling {
    fn drop(&mut self) {
        for signal in &self.signals {
            let mut told = signal.0.told.lock().unwrap_or_else(PoisonError::into_inner);
            told.retain(|(id, _)| *id != self.id);
        }
    }
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
            told: Mutex::new(Vec::new()),
        }))
    }

    pub fn cancel(&self) {
        self.set();
        self.tell();
    }

    /// Sets the signal and tells nobody: for a caller that holds a lock, and tells
    /// once it has let the lock go.
    pub(super) fn set(&self) {
        self.0.set.store(true, Ordering::SeqCst);
    }

    /// Tells whoever asked to be told that this signal is set. No lock of the signal
    /// is held while they are told.
    pub(super) fn tell(&self) {
        let told: Vec<Waker> = {
            let told = self.0.told.lock().unwrap_or_else(PoisonError::into_inner);
            told.iter().map(|(_, waker)| waker.clone()).collect()
        };
        for waker in told {
            waker();
        }
    }

    /// Asks to be told when this signal is set by a cancellation, of it or of any
    /// signal it is under, for as long as what comes back lives.
    pub(super) fn telling(&self, waker: Waker) -> Telling {
        fn all(signal: &Cancellation, into: &mut Vec<Cancellation>) {
            into.push(signal.clone());
            for above in &signal.0.above {
                all(above, into);
            }
        }
        let mut signals = Vec::new();
        all(self, &mut signals);
        let id = TOLD.fetch_add(1, Ordering::SeqCst);
        for signal in &signals {
            let mut told = signal.0.told.lock().unwrap_or_else(PoisonError::into_inner);
            told.push((id, waker.clone()));
        }
        Telling { id, signals }
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
/// capabilities, the backends there are, in the order they were given. It holds no
/// handle. A handle is bound to one run, and only a context makes the handles an action
/// is offered, with its own signal and its own deadline ([`Context::new`]): so an
/// action is never handed a handle bound to another run's signal (ADR 0006, ADR 0008).
///
/// An executor holds every backend of its build, and hands a run those its call
/// resolved ([`Capabilities::resolved_for`], ADR 0009). A context is handed at most one
/// backend of each capability: it makes its handle over the first.
///
/// What `{:?}` prints is which of the three there are.
#[derive(Clone, Default)]
pub struct Capabilities {
    segment: Vec<Arc<dyn Segmenter>>,
    fill: Vec<Arc<dyn Filler>>,
    judge: Vec<Arc<dyn Judge>>,
    /// The clock the handles hold a deadline to, when it is not the machine's.
    clock: Option<Arc<dyn Clock>>,
}

impl Capabilities {
    /// The same set, whose handles hold a run's deadline to this clock: the executor's,
    /// which its admission goes by too.
    pub fn timed_by(mut self, clock: Arc<dyn Clock>) -> Capabilities {
        self.clock = Some(clock);
        self
    }

    /// No backend at all.
    pub fn none() -> Capabilities {
        Capabilities::default()
    }

    /// The same set, with this segmenter after those it holds.
    pub fn with_segment(mut self, backend: Arc<dyn Segmenter>) -> Capabilities {
        self.segment.push(backend);
        self
    }

    /// The same set, with this filler after those it holds.
    pub fn with_fill(mut self, backend: Arc<dyn Filler>) -> Capabilities {
        self.fill.push(backend);
        self
    }

    /// The same set, with this judge after those it holds.
    pub fn with_judge(mut self, backend: Arc<dyn Judge>) -> Capabilities {
        self.judge.push(backend);
        self
    }

    /// Of these backends, the ones this call resolved (ADR 0009): for each capability
    /// the call needs and a model answers, the first backend here that declares that
    /// model, by its backend, its model and its version; and none for a capability the
    /// call does not need, or whose model no backend here declares. A call with two
    /// needs of one capability is handed the backend of the first.
    pub(super) fn resolved_for(&self, prepared: &PreparedCall) -> Capabilities {
        let model_of = |capability: Capability| {
            let mut needs = prepared.needs().iter();
            let need = needs.find(|need| need.capability() == capability)?;
            need.model()
        };
        fn pick<B: ?Sized>(
            backends: &[Arc<B>],
            model: Option<&super::ModelIdentity>,
            declared: impl Fn(&B) -> &Declaration,
        ) -> Vec<Arc<B>> {
            let found = model.and_then(|model| {
                let mut backends = backends.iter();
                backends.find(|backend| declared(backend.as_ref()).model() == model)
            });
            found.cloned().into_iter().collect()
        }
        Capabilities {
            segment: pick(&self.segment, model_of(Capability::Segment), |b| {
                b.declaration()
            }),
            fill: pick(&self.fill, model_of(Capability::Fill), |b| b.declaration()),
            judge: pick(&self.judge, model_of(Capability::Judge), |b| {
                b.declaration()
            }), // The clock the handles hold a deadline to goes with them.
            clock: self.clock.clone(),
        }
    }

    /// A handle on the first backend of each capability, bound to this signal and held
    /// to this deadline, each counting the requests it sends in this tally.
    fn bound(
        &self,
        cancellation: &Cancellation,
        deadline: Option<Instant>,
        tally: &Arc<Tally>,
    ) -> Handles {
        let run = || cancellation.clone();
        let segment = self
            .segment
            .first()
            .cloned()
            .map(|b| SegmentHandle::new(b, run()).counted(tally.clone()));
        let fill = self
            .fill
            .first()
            .cloned()
            .map(|b| FillHandle::new(b, run()).counted(tally.clone()));
        let judge = self
            .judge
            .first()
            .cloned()
            .map(|b| JudgeHandle::new(b, run()).counted(tally.clone()));
        let mut handles = Handles {
            segment,
            fill,
            judge,
        };
        if let Some(by) = deadline {
            handles = Handles {
                segment: handles.segment.map(|handle| handle.until(by)),
                fill: handles.fill.map(|handle| handle.until(by)),
                judge: handles.judge.map(|handle| handle.until(by)),
            };
        }
        if let Some(clock) = &self.clock {
            handles = Handles {
                segment: handles.segment.map(|h| h.timed_by(clock.clone())),
                fill: handles.fill.map(|h| h.timed_by(clock.clone())),
                judge: handles.judge.map(|h| h.timed_by(clock.clone())),
            };
        }
        handles
    }
}

impl std::fmt::Debug for Capabilities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Capabilities")
            .field("segment", &!self.segment.is_empty())
            .field("fill", &!self.fill.is_empty())
            .field("judge", &!self.judge.is_empty())
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
    /// The actions the run's calls are held to, once it froze them.
    offer: Arc<Offer>,
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
            offer: Arc::new(Offer::default()),
        }
    }

    /// The same context, whose executor holds its calls to what is frozen here.
    pub(super) fn offering(mut self, offer: Arc<Offer>) -> Context {
        self.offer = offer;
        self
    }

    /// Freezes the actions this run may call, by name (ADR 0009): from here on a call
    /// through [`Context::executor`] of any other action is refused, and so is a call
    /// that gives as an input an artifact that is neither an input of this run's own
    /// call nor an output that a run it called committed. Each refusal is in the
    /// run's record. It is said once: false when the run froze them before, and then
    /// nothing changes. A context that no executor made freezes nothing its executor
    /// reads.
    pub fn freeze_calls<'a>(&self, actions: impl IntoIterator<Item = &'a str>) -> bool {
        let actions = actions.into_iter().map(str::to_owned).collect();
        self.offer.freeze(actions)
    }

    /// Holds the name of an action to what this run froze, before any call is made
    /// of it (ADR 0009): refused when the run froze its calls and this is none of
    /// them, with the refusal the executor would give such a call
    /// ([`Refusal::NotOffered`]), which is kept in the run's record as that one is.
    /// Nothing is prepared, admitted or run. A loop that dispatches what a model
    /// names asks this first, and makes no call of a name that is refused; the
    /// executor's own refusal of such a call stands behind it. A run that froze
    /// nothing is refused nothing.
    pub fn offered(&self, action: &str) -> Result<(), Refusal> {
        self.offer.held(action)
    }

    /// The same context, whose handles keep what they send with this recorder: each
    /// request timed by its clock, and each exchange with a judge kept by it.
    /// They send `limit` requests at most.
    pub(super) fn recording(self, recorder: Arc<dyn Recorder>, limit: usize) -> Context {
        self.requests.record_with(recorder, limit);
        self
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{Cancellation, PoisonError, Waker};

    fn registered(signal: &Cancellation) -> usize {
        let told = signal.0.told.lock().unwrap_or_else(PoisonError::into_inner);
        told.len()
    }

    #[test]
    fn a_waiters_registration_is_taken_out_of_every_signal_when_it_is_dropped() {
        // A signal under another. A waiter registers with the lower one, which
        // registers it with both. While the registration lives, either tells it;
        // once it is dropped neither holds it, and neither tells it again.
        let above = Cancellation::new();
        let signal = Cancellation::under([above.clone()]);
        let told = Arc::new(AtomicUsize::new(0));
        let waker: Waker = {
            let told = told.clone();
            Arc::new(move || {
                told.fetch_add(1, Ordering::SeqCst);
            })
        };
        let other: Waker = Arc::new(|| {});
        let kept = signal.telling(other);
        let telling = signal.telling(waker);
        assert_eq!((registered(&signal), registered(&above)), (2, 2));
        above.tell();
        signal.tell();
        assert_eq!(told.load(Ordering::SeqCst), 2);
        drop(telling);
        // The other registration stays: only the one that was dropped is taken out.
        assert_eq!((registered(&signal), registered(&above)), (1, 1));
        above.tell();
        signal.tell();
        assert_eq!(told.load(Ordering::SeqCst), 2);
        drop(kept);
        assert_eq!((registered(&signal), registered(&above)), (0, 0));
    }
}
