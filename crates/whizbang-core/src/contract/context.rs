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
    Wrappers,
};
use crate::egress::{Sender, Ways};
use crate::store;

use crate::spend::Amount;

use super::ledger::DispatchRefused;
use super::nested::Offer;
use super::{Capability, Clock, Executor, PreparedCall, Refusal, RunFacts};

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

    /// Keeps `bytes` under `name` as [`Diagnostics::write`] does, for bytes that are a
    /// file of a format of its own: a picture, an artifact's canonical file. Taking a
    /// secret out of such a file would leave another file under its name, and one that
    /// may no longer be read. So the bytes are kept exactly as they were given, or not
    /// at all: when they hold a secret the write is refused, in words that do not show
    /// it, and nothing is kept (ADR 0009, "Evidence").
    fn write_exact(&self, name: &str, bytes: &[u8]) -> Result<(), String>;
}

/// Nowhere to write: what a context has until it is given a place.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoDiagnostics;

impl Diagnostics for NoDiagnostics {
    fn write(&self, _: &str, _: &[u8]) -> Result<(), String> {
        Err("this context keeps no diagnostics".to_owned())
    }

    fn write_exact(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.write(name, bytes)
    }
}

/// What a run may spend, in millionths of a US dollar (ADR 0016, "An allowance holds a
/// signed balance"): what was reserved for it when it was admitted, or, for a run
/// another run called, the share that was taken for it from its caller's allowance
/// (ADR 0008, "Nested calls"). A run that was admitted with no ledger has no amount:
/// nothing was reserved, and nothing is counted.
///
/// **The balance is signed.** It is debited before a request, by the request's bound
/// ([`Allowance::dispatch`]), and by a nested call's share; it is given back to, by
/// what a bound held beyond its charge, by a share whose call was not admitted, and
/// by a child's remainder; and a charge over its bound takes the excess from the
/// allowance of the run that sent the request and of each run above it, below
/// nothing if need be. What a run can spend is its balance or nothing, never less,
/// and every return is added to the signed balance, so it pays the debt first.
///
/// **Beside it, what the process holds of the tree's reservation:** its amount and
/// what the requests sent under it took, by any run of the tree, each at its bound
/// until it is settled and at its charge from then on, a charge over its bound from
/// the moment the process holds the answer that says it. A dispatch of more than the
/// amount less that is refused whatever its run's own balance is, and whatever the
/// names say.
///
/// An allowance is not the books: it is lost with its process, and recovery closes
/// the reservation.
///
/// A clone is the same allowance. Two are equal when they are the same one.
#[derive(Debug, Clone)]
pub struct Allowance {
    purse: Option<Arc<Purse>>,
}

#[derive(Debug)]
struct Purse {
    total: u64,
    balance: Mutex<Balance>,
    /// The allowance of the run that called this one.
    above: Option<Arc<Purse>>,
    /// What the process holds of the reservation of the tree this run is in.
    tree: Arc<TreeHeld>,
}

#[derive(Debug)]
struct Balance {
    micros: i128,
    /// The run has entered its commit step: what is given back from here on is
    /// dropped, because its reservation is closed by its own settlements.
    closed: bool,
}

/// What the process holds of one tree's reservation, in memory.
#[derive(Debug)]
struct TreeHeld {
    amount: u64,
    /// What the requests sent under it took, together.
    taken: Mutex<u128>,
}

impl TreeHeld {
    fn left(&self, taken: u128) -> u64 {
        let left = u128::from(self.amount).saturating_sub(taken);
        u64::try_from(left).unwrap_or(u64::MAX)
    }
}

impl Purse {
    fn balance(&self) -> std::sync::MutexGuard<'_, Balance> {
        self.balance.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds to the signed balance, which pays a debt first. Dropped once the run has
    /// entered its commit step, when `late` returns are.
    fn give(&self, micros: u128, even_closed: bool) {
        let mut balance = self.balance();
        if balance.closed && !even_closed {
            return;
        }
        let micros = i128::try_from(micros).unwrap_or(i128::MAX);
        balance.micros = balance.micros.saturating_add(micros);
    }

    /// Takes from the signed balance, below nothing if need be.
    fn owe(&self, micros: u128) {
        let mut balance = self.balance();
        let micros = i128::try_from(micros).unwrap_or(i128::MAX);
        balance.micros = balance.micros.saturating_sub(micros);
    }
}

/// What a run can spend of a signed balance: the balance, or nothing.
fn spendable(micros: i128) -> u64 {
    u64::try_from(micros.max(0)).unwrap_or(u64::MAX)
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

    /// The allowance of a run nothing called, with this much reserved: the tree's
    /// reservation is of the same amount, and nothing is taken of it yet.
    pub(super) fn of(micros: u64) -> Allowance {
        let tree = Arc::new(TreeHeld {
            amount: micros,
            taken: Mutex::new(0),
        });
        Allowance::with(micros, None, tree)
    }

    fn with(micros: u64, above: Option<Arc<Purse>>, tree: Arc<TreeHeld>) -> Allowance {
        Allowance {
            purse: Some(Arc::new(Purse {
                total: micros,
                balance: Mutex::new(Balance {
                    micros: i128::from(micros),
                    closed: false,
                }),
                above,
                tree,
            })),
        }
    }

    /// The allowance of a run this run called, whose share of this much was taken
    /// from this one already ([`Allowance::take`]): it is under this one, in the
    /// same tree. No amount where this has none.
    pub(super) fn child(&self, share: u64) -> Allowance {
        match &self.purse {
            Some(purse) => Allowance::with(share, Some(purse.clone()), purse.tree.clone()),
            None => Allowance::none(),
        }
    }

    /// What the run was given, when it was given an amount.
    pub fn total_micros(&self) -> Option<u64> {
        self.purse.as_ref().map(|purse| purse.total)
    }

    /// What the run can spend now: its balance, or nothing when it is in debt.
    pub fn left_micros(&self) -> Option<u64> {
        let purse = self.purse.as_ref()?;
        Some(spendable(purse.balance().micros))
    }

    /// By how much the run's balance is below nothing: what the next returns to it
    /// pay first.
    pub fn owes_micros(&self) -> Option<u64> {
        let purse = self.purse.as_ref()?;
        let owed = purse.balance().micros.min(0).unsigned_abs();
        Some(u64::try_from(owed).unwrap_or(u64::MAX))
    }

    /// What the process holds as left of the reservation of the tree this run is
    /// in: its amount less what the requests sent under it took.
    pub fn tree_left_micros(&self) -> Option<u64> {
        let purse = self.purse.as_ref()?;
        let taken = purse
            .tree
            .taken
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Some(purse.tree.left(*taken))
    }

    /// Takes this much for a call the run makes, when the run can spend that much;
    /// otherwise nothing is taken, and what it can spend comes back. The look and the
    /// taking are one step, under the allowance's lock. None when the allowance has no
    /// amount.
    pub(super) fn take(&self, micros: u64) -> Result<Option<Taken>, u64> {
        let Some(purse) = &self.purse else {
            return Ok(None);
        };
        let mut balance = purse.balance();
        let left = spendable(balance.micros);
        if micros > left {
            return Err(left);
        }
        balance.micros -= i128::from(micros);
        Ok(Some(Taken {
            purse: purse.clone(),
            micros,
            kept: false,
        }))
    }

    /// The run has entered its commit step: an amount given back to it from here on
    /// is dropped.
    pub(super) fn close(&self) {
        if let Some(purse) = &self.purse {
            purse.balance().closed = true;
        }
    }

    /// The run's end is durable: its remainder, which is its own balance when that
    /// is over nothing, is handed to the run that called it, when that run still
    /// executes. It is handed once: nothing is left here afterwards.
    pub(super) fn hand_up(&self) {
        let Some(purse) = &self.purse else {
            return;
        };
        let remainder = {
            let mut balance = purse.balance();
            let remainder = spendable(balance.micros);
            balance.micros -= i128::from(remainder);
            balance.closed = true;
            remainder
        };
        if let Some(above) = &purse.above {
            above.give(u128::from(remainder), false);
        }
    }

    /// Debits a paid request's bound before the request leaves, and publishes its
    /// dispatch: `publish` is what makes the dispatch durable, and is the caller's
    /// (the meter's, task S5), which judges by the names and makes them.
    ///
    /// The run's allowance and what the process holds of the tree are asked and
    /// debited in one step, under the tree's lock, **before** the dispatch is
    /// published; where `publish` refuses, or the store refuses it, the debit is
    /// undone. So of two runs of one tree that are each about to publish a dispatch,
    /// the second is asked after the first was debited, and both are never let by on
    /// what only one of them fits (ADR 0016, "Notes for whoever builds the second
    /// part"). No lock is held while `publish` runs.
    ///
    /// A request of nothing is debited nothing and is not refused here: by a run in
    /// debt, and under a tree that has nothing left. An allowance with no amount
    /// counts nothing: `publish` is run and nothing is debited.
    pub fn dispatch<T>(
        &self,
        bound: Amount,
        publish: impl FnOnce() -> Result<T, DispatchRefused>,
    ) -> Result<(Dispatched, T), DispatchRefused> {
        let Some(purse) = &self.purse else {
            let published = publish()?;
            return Ok((Dispatched { held: None }, published));
        };
        let micros = bound.micros();
        {
            let mut taken = purse
                .tree
                .taken
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let mut balance = purse.balance();
            let left = spendable(balance.micros);
            if micros > left {
                return Err(DispatchRefused::OverRun { left_micros: left });
            }
            let left = purse.tree.left(*taken);
            if micros > left {
                return Err(DispatchRefused::OverTree { left_micros: left });
            }
            balance.micros -= i128::from(micros);
            *taken = taken.saturating_add(u128::from(micros));
        }
        // Whatever ends `publish` without a dispatch, a panic among it, the debit is
        // undone.
        let mut undo = Undo {
            purse,
            micros,
            armed: true,
        };
        let published = publish()?;
        undo.armed = false;
        Ok((
            Dispatched {
                held: Some(Held {
                    purse: purse.clone(),
                    bound: micros,
                    counted: micros,
                }),
            },
            published,
        ))
    }
}

/// Gives a dispatch's debit back when its publication did not happen.
struct Undo<'a> {
    purse: &'a Arc<Purse>,
    micros: u64,
    armed: bool,
}

impl Drop for Undo<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let tree = &self.purse.tree;
        {
            let mut taken = tree.taken.lock().unwrap_or_else(PoisonError::into_inner);
            *taken = taken.saturating_sub(u128::from(self.micros));
        }
        // The run is executing: it is about to send a request.
        self.purse.give(u128::from(self.micros), true);
    }
}

/// A paid request that was dispatched: what its run's allowance and the tree's
/// reservation hold for it in memory, until it is settled. Dropped without being
/// settled, it stays counted as it is: nothing of it is given back.
#[derive(Debug)]
pub struct Dispatched {
    held: Option<Held>,
}

#[derive(Debug)]
struct Held {
    purse: Arc<Purse>,
    bound: u64,
    /// What the tree counts for this request now: its bound, or the charge an
    /// answer gave where that is more.
    counted: u64,
}

impl Dispatched {
    /// The process holds the answer, and it says this charge. A charge over the
    /// bound counts against the tree from now, before its settlement is durable: a
    /// dispatch by any run of the tree is refused what it took.
    pub fn answered(&mut self, charge: Amount) {
        let Some(held) = &mut self.held else {
            return;
        };
        let charge = charge.micros();
        if charge > held.counted {
            let tree = &held.purse.tree;
            let mut taken = tree.taken.lock().unwrap_or_else(PoisonError::into_inner);
            *taken = taken.saturating_add(u128::from(charge - held.counted));
            held.counted = charge;
        }
    }

    /// The request is settled at this charge, durably. Under its bound, what the
    /// bound held beyond the charge is given back to the run that sent it, and the
    /// tree counts the charge. Over it, the excess is taken from the allowance of
    /// the run that sent it and of each run above it, below nothing if need be.
    pub fn settled(mut self, charge: Amount) {
        let Some(held) = self.held.take() else {
            return;
        };
        let charge = charge.micros();
        {
            let tree = &held.purse.tree;
            let mut taken = tree.taken.lock().unwrap_or_else(PoisonError::into_inner);
            let without = taken.saturating_sub(u128::from(held.counted));
            *taken = without.saturating_add(u128::from(charge));
        }
        if charge <= held.bound {
            held.purse.give(u128::from(held.bound - charge), false);
            return;
        }
        let excess = u128::from(charge - held.bound);
        let mut purse = Some(&held.purse);
        while let Some(one) = purse {
            one.owe(excess);
            purse = one.above.as_ref();
        }
    }
}

/// What was taken from an allowance for a call. Dropped before the call's run was
/// made, it is given back: added to the signed balance, so it pays a debt first.
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
            self.purse.give(u128::from(self.micros), true);
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
    /// The executor's ways out and the run's place for facts, which each handle's
    /// checked sender is made of (ADR 0015). Without them a handle sends nothing.
    ways: Option<(Ways, RunFacts)>,
    /// What stands between each handle and its backend for the run (ADR 0015, "A
    /// wrapper around each request"). Without it nothing does.
    around: Option<Arc<Wrappers>>,
}

impl Capabilities {
    /// The same set, whose handles each take their requests through these wrappers.
    pub(super) fn wrapped(mut self, around: Wrappers) -> Capabilities {
        self.around = Some(Arc::new(around));
        self
    }

    /// The same set, whose handles each hand their backend a sender made of these
    /// ways for that backend, noting in this place what it hands over and refuses.
    pub(super) fn sending_through(mut self, ways: Ways, facts: RunFacts) -> Capabilities {
        self.ways = Some((ways, facts));
        self
    }

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
            ways: self.ways.clone(),
            around: self.around.clone(),
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
        // Each handle hands its backend a sender made for that backend's recipient
        // (ADR 0015). A context that no executor made has no ways out: its handles
        // send nothing.
        if let Some((ways, facts)) = &self.ways {
            let made = |declaration: &Declaration| -> Arc<dyn Sender> {
                ways.for_backend(declaration, facts.clone())
            };
            handles = Handles {
                segment: handles.segment.map(|h| {
                    let sender = made(h.declaration());
                    h.sending_through(sender)
                }),
                fill: handles.fill.map(|h| {
                    let sender = made(h.declaration());
                    h.sending_through(sender)
                }),
                judge: handles.judge.map(|h| {
                    let sender = made(h.declaration());
                    h.sending_through(sender)
                }),
            };
        }
        // And the wrappers, last: they stand inside the tally and outside the
        // backend, which is where a handle puts them (`Bounds::send`). This is the
        // order of a request (ADR 0015): the handle's own checks, the tally, the
        // gate, the lease, the meter, the backend.
        if let Some(around) = &self.around {
            handles = Handles {
                segment: handles.segment.map(|h| h.wrapped(around.clone())),
                fill: handles.fill.map(|h| h.wrapped(around.clone())),
                judge: handles.judge.map(|h| h.wrapped(around.clone())),
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

/// The store as an action reads it: the same content, and every failure in the words
/// that hold no path ([`store::Error::pathless`]). An action's messages are kept in its
/// run's record and handed to callers who are not on this machine, and the store's own
/// error names files by where they are. So no action is handed one: whatever it does
/// with a failure of the store, or with an artifact's error that holds one, it writes
/// no path (ADR 0005, "Redaction").
struct NoPaths(Arc<dyn Contents + Send + Sync>);

impl Contents for NoPaths {
    fn snapshot(&self) -> store::Result<store::Generation> {
        self.0.snapshot().map_err(|error| error.pathless())
    }

    fn read(&self, entry: &store::Entry) -> store::Result<Vec<u8>> {
        self.0.read(entry).map_err(|error| error.pathless())
    }

    fn read_record(&self, entry: &store::Entry) -> store::Result<Vec<u8>> {
        self.0.read_record(entry).map_err(|error| error.pathless())
    }
}

/// What one execution of an action is handed.
#[derive(Clone)]
pub struct Context {
    cancellation: Cancellation,
    deadline: Option<Instant>,
    /// The clock the deadline is held to, when it is not the machine's.
    clock: Option<Arc<dyn Clock>>,
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
    /// The run's place for facts.
    facts: RunFacts,
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
        let clock = capabilities.clock.clone();
        let capabilities = capabilities.bound(&cancellation, deadline, &requests);
        Context {
            cancellation,
            deadline,
            clock,
            events,
            allowance,
            store: Arc::new(NoPaths(store)),
            capabilities,
            requests,
            executor,
            diagnostics: Arc::new(NoDiagnostics),
            offer: Arc::new(Offer::default()),
            facts: RunFacts::default(),
        }
    }

    /// The same context, noting its run's facts here.
    pub(super) fn noting(mut self, facts: RunFacts) -> Context {
        self.facts = facts;
        self
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
    /// They send `limit` requests at most, and store `budget` bytes of exchanges.
    pub(super) fn recording(
        self,
        recorder: Arc<dyn Recorder>,
        limit: usize,
        budget: u64,
    ) -> Context {
        self.requests.record_with(recorder, limit, budget);
        self
    }

    /// The same context, writing its diagnostics here.
    pub fn with_diagnostics(mut self, diagnostics: Arc<dyn Diagnostics>) -> Context {
        self.diagnostics = diagnostics;
        self
    }

    /// The run's place for facts (ADR 0013): where a setting, a paid request, a
    /// refusal of a request and where data went are noted while the run executes.
    pub fn facts(&self) -> &RunFacts {
        &self.facts
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

    /// Whether the run's deadline has passed, by the clock its capability handles are
    /// held to: the executor's (ADR 0008, "A deadline in a run"). An action that works
    /// for long asks this from time to time, and not the machine's clock, so that it
    /// and its handles stop at the same moment. Never true of a run with no deadline.
    pub fn deadline_passed(&self) -> bool {
        let now = || self.clock.as_ref().map_or_else(Instant::now, |c| c.now());
        self.deadline.is_some_and(|deadline| now() >= deadline)
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
    /// committed together when it completes. A failure it answers with holds no path,
    /// as its text and as `{:?}`: what an action copies into a message is kept in its
    /// run's record.
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

#[cfg(test)]
mod allowance_tests {
    //! An allowance and what the process holds of a tree (ADR 0016, "An allowance holds
    //! a signed balance"). What makes one is the executor's alone, so these are here
    //! and not with the other tests.
    #![allow(clippy::unwrap_used)]

    use std::sync::mpsc;

    use super::*;
    use crate::contract::NotJudged;

    fn amount(micros: u64) -> Amount {
        Amount::new(micros).unwrap()
    }

    fn sent(allowance: &Allowance, micros: u64) -> Dispatched {
        let (dispatched, ()) = allowance.dispatch(amount(micros), || Ok(())).unwrap();
        dispatched
    }

    fn state(allowance: &Allowance) -> (u64, u64, u64) {
        (
            allowance.left_micros().unwrap(),
            allowance.owes_micros().unwrap(),
            allowance.tree_left_micros().unwrap(),
        )
    }

    #[test]
    fn an_allowance_in_debt_reports_nothing_to_spend_and_a_return_pays_the_debt_first() {
        // The model's history 8: reserve 10, two dispatches of 5, the first settles
        // at 12 and the second at nothing, which gives back 5.
        let root = Allowance::of(10);
        let first = sent(&root, 5);
        let second = sent(&root, 5);
        assert_eq!(state(&root), (0, 0, 0));
        first.settled(amount(12));
        assert_eq!(state(&root), (0, 7, 0), "in debt: nothing to spend");
        second.settled(amount(0));
        // The 5 that come back pay the debt first: 2 are still owed.
        assert_eq!(state(&root), (0, 2, 0));
        // A request of nothing is placed all the same, and one of more is refused.
        let nothing = sent(&root, 0);
        nothing.settled(amount(0));
        assert_eq!(
            root.dispatch(amount(1), || Ok(())).unwrap_err(),
            DispatchRefused::OverRun { left_micros: 0 }
        );
        // A share handed back after a call was not admitted pays a debt first too
        // (history 20): a parent in debt by 2 that is given back 5 has 3.
        let parent = Allowance::of(10);
        let share = parent.take(5).unwrap().unwrap();
        let own = sent(&parent, 5);
        own.settled(amount(7));
        assert_eq!(
            (parent.left_micros(), parent.owes_micros()),
            (Some(0), Some(2))
        );
        drop(share);
        assert_eq!(
            (parent.left_micros(), parent.owes_micros()),
            (Some(3), Some(0))
        );
    }

    #[test]
    fn a_charge_over_its_bound_takes_the_excess_from_the_run_and_from_each_run_above() {
        // History 18: a child given 5 whose request settles at 12 gives nothing
        // back, and each run above has 7 less.
        let root = Allowance::of(20);
        let taken = root.take(10).unwrap().unwrap();
        let parent = root.child(10);
        taken.keep();
        let taken = parent.take(5).unwrap().unwrap();
        let child = parent.child(5);
        taken.keep();
        let request = sent(&child, 5);
        request.settled(amount(12));
        assert_eq!(
            (child.left_micros(), child.owes_micros()),
            (Some(0), Some(7))
        );
        assert_eq!(
            (parent.left_micros(), parent.owes_micros()),
            (Some(0), Some(2))
        );
        assert_eq!((root.left_micros(), root.owes_micros()), (Some(3), Some(0)));
        assert_eq!(root.tree_left_micros(), Some(8));
        child.hand_up();
        assert_eq!(
            parent.owes_micros(),
            Some(2),
            "a child in debt hands nothing up"
        );
        // History 19: one child overruns and a second then returns its unused share:
        // the debt is paid first.
        let parent = Allowance::of(10);
        let (first, second) = (parent.take(5), parent.take(5));
        let (one, other) = (parent.child(5), parent.child(5));
        first.unwrap().unwrap().keep();
        second.unwrap().unwrap().keep();
        sent(&one, 5).settled(amount(12));
        assert_eq!(
            (parent.left_micros(), parent.owes_micros()),
            (Some(0), Some(7))
        );
        other.hand_up();
        assert_eq!(
            (parent.left_micros(), parent.owes_micros()),
            (Some(0), Some(2))
        );
    }

    #[test]
    fn a_parent_that_calls_twenty_children_in_turn_each_using_nothing_has_all_of_it_at_the_end() {
        // History 21.
        let parent = Allowance::of(1000);
        for _ in 0..20 {
            let half = parent.left_micros().unwrap() / 2;
            let taken = parent.take(half).unwrap().unwrap();
            let child = parent.child(half);
            taken.keep();
            assert_eq!(child.left_micros(), Some(half));
            child.hand_up();
            // It is handed up once.
            child.hand_up();
        }
        assert_eq!(state(&parent), (1000, 0, 1000));
    }

    #[test]
    fn an_amount_given_back_to_an_allowance_whose_run_has_entered_its_commit_step_is_dropped() {
        let parent = Allowance::of(10);
        let taken = parent.take(4).unwrap().unwrap();
        let child = parent.child(4);
        taken.keep();
        parent.close();
        child.hand_up();
        assert_eq!(parent.left_micros(), Some(6));
        // Before that step it is not: the same, with the child ending first.
        let parent = Allowance::of(10);
        let taken = parent.take(4).unwrap().unwrap();
        let child = parent.child(4);
        taken.keep();
        child.hand_up();
        parent.close();
        assert_eq!(parent.left_micros(), Some(10));
    }

    #[test]
    fn a_dispatch_is_debited_before_it_is_published_and_the_debit_is_undone_when_it_is_refused() {
        let root = Allowance::of(10);
        // What `publish` sees is the allowance and the tree already debited.
        let seen = root
            .dispatch(amount(4), || Ok(state(&root)))
            .map(|(_, seen)| seen)
            .unwrap();
        assert_eq!(seen, (6, 0, 6));
        // A refusal by the names, and a failure, each give the amount back.
        for refusal in [
            DispatchRefused::OverReservation { left_micros: 1 },
            DispatchRefused::Frozen,
            DispatchRefused::Closed,
            DispatchRefused::NeedsOwner { epoch: 3 },
            DispatchRefused::Failed(NotJudged::saying("the store failed")),
        ] {
            let refused = root.dispatch(amount(6), || Err::<(), _>(refusal.clone()));
            assert_eq!(refused.unwrap_err(), refusal);
            assert_eq!(state(&root), (6, 0, 6), "{refusal:?}");
        }
        // And so does a panic inside it.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            drop(root.dispatch(amount(6), || -> Result<(), DispatchRefused> {
                panic!("the store panicked")
            }));
        }));
        assert!(panicked.is_err());
        assert_eq!(state(&root), (6, 0, 6));
        // The run's own allowance is asked first, then the tree as the process
        // holds it; neither calls `publish`.
        let never = || -> Result<(), DispatchRefused> { panic!("it was published") };
        assert_eq!(
            root.dispatch(amount(7), never).unwrap_err(),
            DispatchRefused::OverRun { left_micros: 6 }
        );
    }

    #[test]
    fn a_dispatch_is_held_to_what_the_process_holds_of_the_tree_whatever_its_runs_share_is() {
        // The model's history 75: a parent's request of 5 is answered at 12, and no
        // name says so yet. A child that holds a share of 5 is refused a request of
        // 5: the tree of 10 has nothing left, though the child's own balance covers
        // it.
        let root = Allowance::of(10);
        let taken = root.take(5).unwrap().unwrap();
        let child = root.child(5);
        taken.keep();
        let mut request = sent(&root, 5);
        assert_eq!(child.tree_left_micros(), Some(5));
        request.answered(amount(12));
        assert_eq!(child.left_micros(), Some(5));
        let never = || -> Result<(), DispatchRefused> { panic!("it was published") };
        assert_eq!(
            child.dispatch(amount(5), never).unwrap_err(),
            DispatchRefused::OverTree { left_micros: 0 }
        );
        // A request of nothing is placed where nothing is left.
        sent(&child, 0).settled(amount(0));
        // Settled, the tree counts the charge and no more; an answer under what is
        // counted changes nothing.
        request.answered(amount(3));
        request.settled(amount(12));
        assert_eq!(root.tree_left_micros(), Some(0));
        assert_eq!(root.owes_micros(), Some(7));
        // A request settled under its bound gives the difference back to the tree.
        let other = Allowance::of(10);
        let request = sent(&other, 8);
        assert_eq!(other.tree_left_micros(), Some(2));
        request.settled(amount(3));
        assert_eq!(state(&other), (7, 0, 7));
        // One that is never settled stays counted at its bound.
        drop(sent(&other, 7));
        assert_eq!(state(&other), (0, 0, 0));
    }

    #[test]
    fn two_runs_of_one_tree_that_publish_at_once_are_not_both_let_by() {
        // A run's own balance does not bound its request by itself: the tree does.
        // The tree of 10 has had 4 taken by an answer over its bound, so that 6 are
        // left of it, while two runs of it hold 5 each. The first is stopped inside
        // `publish`, which is after its debit and before its dispatch is durable;
        // the second then asks. Were the tree debited only once a dispatch is
        // published, the second would be let by on the 6 the first is about to
        // take. Two show a race.
        let root = Allowance::of(10);
        let mut over = sent(&root, 0);
        over.answered(amount(4));
        let tree = &root.purse.as_ref().unwrap().tree;
        let run = || Allowance::with(5, None, tree.clone());
        for first_is_published in [true, false] {
            let (first, second) = (run(), run());
            let (publishing, is_publishing) = mpsc::channel();
            let (asked, was_asked) = mpsc::channel();
            let (one, other) = whizbang_testkit::threads::scope(|scope| {
                let (first, second) = (&first, &second);
                let one = scope.spawn(move || {
                    first
                        .dispatch(amount(5), || {
                            publishing.send(()).unwrap();
                            was_asked.recv().unwrap();
                            match first_is_published {
                                true => Ok(()),
                                false => Err(DispatchRefused::Closed),
                            }
                        })
                        .map(drop)
                });
                let other = scope.spawn(move || {
                    is_publishing.recv().unwrap();
                    let answer = second.dispatch(amount(5), || Ok(())).map(drop);
                    asked.send(()).unwrap();
                    answer
                });
                (one.join().unwrap(), other.join().unwrap())
            });
            assert_eq!(other, Err(DispatchRefused::OverTree { left_micros: 1 }));
            assert_eq!(second.left_micros(), Some(5));
            match first_is_published {
                true => {
                    assert_eq!(one, Ok(()));
                    assert_eq!(root.tree_left_micros(), Some(1));
                    // What the first took is given back for the second round.
                    *tree.taken.lock().unwrap() -= 5;
                }
                // The first was refused after all: its debit is undone.
                false => {
                    assert_eq!(one, Err(DispatchRefused::Closed));
                    assert_eq!(root.tree_left_micros(), Some(6));
                    assert_eq!(first.left_micros(), Some(5));
                }
            }
        }
    }

    #[test]
    fn an_allowance_with_no_amount_counts_nothing() {
        let none = Allowance::none();
        let (dispatched, published) = none.dispatch(amount(5), || Ok(7)).unwrap();
        assert_eq!(published, 7);
        dispatched.settled(amount(9));
        assert_eq!(
            (
                none.left_micros(),
                none.owes_micros(),
                none.tree_left_micros()
            ),
            (None, None, None)
        );
        assert_eq!(none.child(5), Allowance::none());
        none.close();
        none.hand_up();
    }
}
