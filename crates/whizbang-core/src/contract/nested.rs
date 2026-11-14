//! Nested calls (`SPEC.md` section 3, "Nested calls"; ADR 0008, "Nested calls"): an
//! action calls another action through the executor in its context, and the call is a
//! run of its own, under the run that made it.
//!
//! What is here: the executor a context holds ([`Nested`]), which refuses a call
//! deeper than four levels and a call back into an action that is already running
//! among the calls that led to it; what a run keeps of the calls it made ([`Nest`]),
//! from which its end waits for them and its record lists them; and the names a run's
//! view of the store gains when a run it called commits outputs ([`Added`]).

use std::collections::BTreeSet;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;

use crate::artifact::{self, ArtifactId};
use crate::capability::Tally;
use crate::store::{self, Generation, Store};

use super::engine::{Admitted, Bounded, Stopped};
use super::live::LiveRun;
use super::record::{Inside, REFUSALS_LIMIT, Refused, STEPS_LIMIT, Status, Step};
use super::{
    Allowance, Answer, Call, CallError, Cancel, Cancellation, Committed, Engine, Executor,
    Finished, PreparedCall, Refusal, Request, RequestId, RunId, Secrets, StoreFailure, Watched,
};

/// How many runs that one run started, each on a thread of its own, may be executing
/// at once. With four levels, the runs under one run that nothing called hold at most
/// 4 + 16 + 64 = 84 threads between them.
pub const STARTED_LIMIT: usize = 4;

/// How many runs may be admitted, at every level together, under one run that nothing
/// called. The next call anywhere under that run is refused
/// ([`Refusal::TreeFull`]).
pub const DESCENDANTS_LIMIT: usize = 1024;

/// How many levels calls nest: a run that nothing called is at the first, and a call
/// made by a run at the fourth is refused.
pub const LEVELS_LIMIT: usize = 4;

/// The names, in the store, of the artifacts that the runs one run called have
/// committed. The run's view of the store is the generation it was admitted against
/// with these names added, each as the generation current at the look holds it: so a
/// run reads what a run it called committed, and nothing else that was published
/// since it was admitted.
#[derive(Default)]
pub(super) struct Added(Mutex<BTreeSet<String>>);

impl Added {
    fn names(&self) -> MutexGuard<'_, BTreeSet<String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// This generation, and each added name it does not hold, as the store's current
    /// generation holds it.
    pub(super) fn onto(
        &self,
        mut generation: Generation,
        store: &Store,
    ) -> store::Result<Generation> {
        let names: Vec<String> = self.names().iter().cloned().collect();
        if names.is_empty() {
            return Ok(generation);
        }
        let current = store.snapshot()?;
        for name in names {
            if generation.get(&name).is_none()
                && let Some(entry) = current.get(&name)
            {
                generation.entries.insert(name, entry.clone());
            }
        }
        Ok(generation)
    }
}

/// The actions the calls of one run are held to, once the run has said which (ADR
/// 0009). It is said once: what is frozen stays as it was frozen for as long as the
/// run lives. A run that says nothing calls any action, as before.
///
/// From the moment it is frozen, a call the run makes is also held to what the run
/// may read: each input it gives is an input of the run's own call, or an output that
/// a run it called committed.
#[derive(Default)]
pub(super) struct Offer {
    actions: OnceLock<BTreeSet<String>>,
    /// The run whose calls these are, and the secrets its record is written without:
    /// where a name the action itself held to the list, and refused, is recorded.
    run: Option<(Arc<LiveRun>, Secrets)>,
    /// What the run that called this one is held to. A run is held to all of it as
    /// well: what a run froze binds every run under it.
    above: Option<Arc<Offer>>,
}

impl Offer {
    /// The offer of this run, under the one of the run that called it: nothing of
    /// its own is frozen yet.
    pub(super) fn of(run: Arc<LiveRun>, secrets: Secrets, above: Option<Arc<Offer>>) -> Offer {
        Offer {
            actions: OnceLock::new(),
            run: Some((run, secrets)),
            above,
        }
    }

    /// Freezes the actions. False when they were frozen before: nothing changes.
    pub(super) fn freeze(&self, actions: BTreeSet<String>) -> bool {
        self.actions.set(actions).is_ok()
    }

    /// Each list that binds the run: its own, when it froze one, and that of every
    /// run above it that did.
    fn lists(&self) -> impl Iterator<Item = &BTreeSet<String>> {
        let mut next = Some(self);
        std::iter::from_fn(move || {
            let offer = next?;
            next = offer.above.as_deref();
            Some(offer.actions.get())
        })
        .flatten()
    }

    /// Why the run may not make this call, when it may not: the run, or a run above
    /// it, froze its calls, and the action is not on every such list, or an input is
    /// neither one of `inputs`, the inputs of the run's own call, nor an output that a
    /// run it called committed. The store is not looked at.
    fn refuses(&self, call: &Call, inputs: &BTreeSet<ArtifactId>, nest: &Nest) -> Option<Refusal> {
        self.lists().next()?;
        let action = call.action().to_owned();
        if !self.lists().all(|offered| offered.contains(call.action())) {
            return Some(Refusal::NotOffered { action });
        }
        for (port, ids) in call.inputs() {
            let unread = |id: &&ArtifactId| !inputs.contains(*id) && !nest.committed(**id);
            if let Some(id) = ids.iter().find(unread) {
                return Some(Refusal::NotReadable {
                    action,
                    port: port.clone(),
                    id: *id,
                });
            }
        }
        None
    }

    /// Holds a name to the frozen actions without making a call: refused when the
    /// run is bound by a list and this is not on every one, and the refusal is kept
    /// for the run's record as the executor keeps one of a call it refused.
    pub(super) fn held(&self, action: &str) -> Result<(), Refusal> {
        if self.lists().all(|offered| offered.contains(action)) {
            return Ok(());
        }
        let refusal = Refusal::NotOffered {
            action: action.to_owned(),
        };
        if let Some((run, secrets)) = &self.run {
            let refused = Refused::of(&Call::new(action), &refusal, secrets);
            run.nest().refused(refused);
        }
        Err(refusal)
    }
}

/// The runs admitted under one run that nothing called, at every level together: one
/// count, shared by every run of the tree.
pub(super) struct Tree {
    limit: usize,
    admitted: Mutex<usize>,
}

impl Tree {
    fn new(limit: usize) -> Tree {
        Tree {
            limit,
            admitted: Mutex::new(0),
        }
    }

    /// Counts one more run, unless the tree has as many as it may: one step.
    fn take(&self) -> bool {
        let mut admitted = self.admitted.lock().unwrap_or_else(PoisonError::into_inner);
        let free = *admitted < self.limit;
        *admitted += usize::from(free);
        free
    }

    /// A call that was counted made no run.
    fn give_back(&self) {
        let mut admitted = self.admitted.lock().unwrap_or_else(PoisonError::into_inner);
        *admitted = admitted.saturating_sub(1);
    }
}

/// One run that a run called.
struct Child {
    run: RunId,
    action: String,
    live: Arc<LiveRun>,
    /// How it ended, once its end was published.
    status: Option<Status>,
    /// When it started and when it ended, once its end was published.
    times: Option<(u64, u64)>,
}

#[derive(Default)]
struct Calls {
    /// The run's action has come back: no call is taken from here on.
    closed: bool,
    /// Calls that were taken and are being admitted: neither refused nor a run yet.
    opening: usize,
    /// Runs the run started that are not over, and calls to start one that are being
    /// admitted: each is, or will be, a thread.
    started: usize,
    /// The runs the calls made, in the order they were admitted.
    children: Vec<Child>,
    refusals: Vec<Refused>,
    refusals_omitted: u64,
    /// The requests the run's own capability handles were sent.
    requests: Option<Arc<Tally>>,
    /// The identity of every output that a run this run called committed.
    committed: BTreeSet<ArtifactId>,
}

/// What a run keeps of the calls its action makes. It has a lock of its own, which is
/// held to read or change what is above and never across a call to the store or to an
/// action, and never while the run's own lock is taken.
#[derive(Default)]
pub(super) struct Nest {
    calls: Mutex<Calls>,
    changed: Condvar,
    added: Arc<Added>,
}

/// What holds a run open, now: for tests.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldOpen {
    /// Whether the run's action has come back.
    pub came_back: bool,
    /// How many runs it called are not over.
    pub children: usize,
    /// How many requests its capability handles sent are not accounted for.
    pub requests: u64,
    /// Whether the run's end is waiting, now, for those requests.
    pub waits_for_requests: bool,
}

impl Nest {
    pub(super) fn new(added: Arc<Added>) -> Nest {
        Nest {
            added,
            ..Nest::default()
        }
    }

    fn calls(&self) -> MutexGuard<'_, Calls> {
        self.calls.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The run's context was made: these are the requests its handles send.
    pub(super) fn watch(&self, requests: Arc<Tally>) {
        self.calls().requests = Some(requests);
    }

    /// Takes a call, to be admitted: from here until it is refused or has its run,
    /// the run's end waits for it. Refused once the run's action has come back, and
    /// when the run has called as many as one may.
    fn begin(&self, action: &str, start: bool, tree: &Tree) -> Result<(), Refusal> {
        let mut calls = self.calls();
        let action = action.to_owned();
        if calls.closed {
            return Err(Refusal::CallerOver { action });
        }
        // A call to start a run, when as many as may be are executing.
        if start && calls.started >= STARTED_LIMIT {
            return Err(Refusal::TooManyStartedUnder {
                action,
                limit: STARTED_LIMIT,
            });
        }
        if calls.children.len() + calls.opening >= STEPS_LIMIT {
            return Err(Refusal::TooManyCalls {
                action,
                limit: STEPS_LIMIT,
            });
        }
        // Last, so that a call refused above is not counted in the tree.
        if !tree.take() {
            return Err(Refusal::TreeFull {
                action: action.to_owned(),
                limit: tree.limit,
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
        self.changed.notify_all();
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

    /// A run this run called has ended, and its end is published: how, and with which
    /// outputs. From here on this run's view of the store has them.
    pub(super) fn ended(
        &self,
        run: &RunId,
        status: Status,
        outputs: &[Committed],
        times: (u64, u64),
    ) {
        let mut names = self.added.names();
        for output in outputs {
            names.insert(artifact::name(output.artifact(), output.id()));
        }
        drop(names);
        let mut calls = self.calls();
        calls.committed.extend(outputs.iter().map(Committed::id));
        if let Some(child) = calls.children.iter_mut().find(|child| child.run == *run) {
            child.status = Some(status);
            child.times = Some(times);
        }
    }

    /// Whether a run this run called committed an output of this identity.
    fn committed(&self, id: ArtifactId) -> bool {
        self.calls().committed.contains(&id)
    }

    /// The run's action has come back. From here on no call is taken and no request
    /// is sent, and this returns once every call that was taken is refused or has a
    /// run that is over, and every request that was sent is accounted for. It has no
    /// bound: a run that never comes back, and a backend that never answers, are
    /// waited for (ADR 0008, "Nested calls").
    pub(super) fn settle(&self) {
        let (children, requests) = {
            let mut calls = self.calls();
            calls.closed = true;
            if let Some(requests) = &calls.requests {
                requests.close();
            }
            while calls.opening > 0 {
                calls = self
                    .changed
                    .wait(calls)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            let children: Vec<_> = calls.children.iter().map(|c| c.live.clone()).collect();
            (children, calls.requests.clone())
        };
        for child in children {
            child.wait_until_over();
        }
        if let Some(requests) = requests {
            requests.wait();
        }
    }

    /// What the run's record says of its calls and of the requests its capability
    /// handles sent, for a run with this parent.
    pub(super) fn inside(&self, parent: Option<RunId>) -> Inside {
        let calls = self.calls();
        let timed = calls.children.iter().enumerate();
        Inside {
            asked: calls.requests.as_ref().map_or_else(Vec::new, |r| r.asked()),
            timed: timed
                .filter_map(|(at, child)| child.times.map(|(start, end)| (at, start, end)))
                .collect(),
            verdict: None,
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

    pub(super) fn held_open(&self) -> HeldOpen {
        let calls = self.calls();
        HeldOpen {
            came_back: calls.closed,
            children: calls.opening
                + calls
                    .children
                    .iter()
                    .filter(|child| !child.live.is_over())
                    .count(),
            requests: calls.requests.as_ref().map_or(0, |r| r.outstanding()),
            waits_for_requests: calls.requests.as_ref().is_some_and(|r| r.waited_for()),
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
    /// The tree the call was counted in, until it is known to have made a run.
    tree: Option<&'a Tree>,
}

impl Opening<'_> {
    fn made(mut self, child: Child) {
        self.made = true;
        self.tree = None;
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
            if let Some(tree) = self.tree {
                tree.give_back();
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
    /// The runs from the one nothing called down to this one, this one last.
    pub(super) above: Arc<Vec<RunId>>,
    /// The count of the runs under the run nothing called, which this run is under
    /// or is.
    pub(super) tree: Arc<Tree>,
    /// What this run may spend: each call it makes takes its estimate from it.
    pub(super) allowance: Allowance,
    pub(super) deadline: Option<Instant>,
    /// The identity of every input of this run's own call.
    pub(super) inputs: Arc<BTreeSet<ArtifactId>>,
    /// The actions this run's calls are held to, once it froze them.
    pub(super) offer: Arc<Offer>,
}

/// Where a run is among the calls that led to it, from its admission on.
pub(super) struct Family {
    /// The run that called it, and that run's live state.
    pub(super) parent: Option<(RunId, Arc<LiveRun>)>,
    pub(super) stack: Arc<Vec<String>>,
    /// The runs above this one, from the one nothing called down to its parent.
    above: Arc<Vec<RunId>>,
    pub(super) tree: Arc<Tree>,
    pub(super) allowance: Allowance,
    /// What the run that called this one is held to in its calls.
    pub(super) offer: Option<Arc<Offer>>,
}

impl Family {
    /// The runs above this run and this run, which is named only once it is admitted.
    pub(super) fn above_with(&self, run: &RunId) -> Arc<Vec<RunId>> {
        let mut above = self.above.to_vec();
        above.push(run.clone());
        Arc::new(above)
    }

    /// The family of a run of this action, called under this run or by nothing, with
    /// this much reserved or taken for it. A run that nothing called begins a tree
    /// that may hold `descendants` runs under it.
    pub(super) fn of(
        under: Option<&Under>,
        action: &str,
        reserved: Option<u64>,
        descendants: usize,
    ) -> Family {
        let mut stack = under.map_or_else(Vec::new, |under| under.stack.to_vec());
        stack.push(action.to_owned());
        Family {
            parent: under.map(|under| (under.run.clone(), under.live.clone())),
            stack: Arc::new(stack),
            above: under.map_or_else(Arc::default, |under| under.above.clone()),
            tree: under.map_or_else(|| Arc::new(Tree::new(descendants)), |u| u.tree.clone()),
            allowance: reserved.map_or_else(Allowance::none, Allowance::of),
            offer: under.map(|under| under.offer.clone()),
        }
    }
}

/// The executor in the context of a running action: the one way the action reaches
/// another action. A call through it is admitted under the action's run, and is
/// refused, with no run, when it would be at a fifth level, when it names an action
/// that is already running among the calls that led to it, when the run was
/// cancelled or its action has come back, when it may cost more than is left of the
/// run's allowance, and when it carries a request id. Every such refusal is kept for
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

    /// Why the run's frozen calls refuse this call, when they do.
    fn frozen(&self, call: &Call) -> Option<Refusal> {
        let under = &self.under;
        under.offer.refuses(call, &under.inputs, under.live.nest())
    }

    fn open(&self, request: &Request, start: bool) -> Result<Admitted, CallError> {
        let (under, call) = (&self.under, request.call());
        let action = || call.action().to_owned();
        if request.id().is_some() {
            return Err(Refusal::IdInside { action: action() }.into());
        }
        // A run that froze the actions it calls calls no other, and gives a call
        // nothing but what it may read: decided here, where the call is dispatched,
        // from what the run was admitted with and what its own calls came to.
        if let Some(refusal) = self.frozen(call) {
            return Err(refusal.into());
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
        // A call made once the run's signal is set is refused by admission's own look
        // at the call's signal, which is under the run's.
        let nest = under.live.nest();
        nest.begin(call.action(), start, &under.tree)?;
        let mut opening = Opening {
            nest,
            made: false,
            start,
            tree: Some(&under.tree),
        };
        // The call's run has a signal of its own, under this run's and under the one
        // the request came with: cancelling this run, or any above it, cancels it.
        let above = [
            Some(under.live.signal().clone()),
            request.cancellation().cloned(),
        ];
        let bounded = Bounded {
            signal: Cancellation::under(above.into_iter().flatten()),
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
            });
        let admitted = match admitted {
            Ok(admitted) => admitted,
            Err(error) => {
                // An admission that is visible and was not confirmed made a run,
                // which nothing will execute: it stays counted in the tree.
                if let CallError::Store(failure) = &error
                    && matches!(failure.cause(), store::Error::Unacknowledged { .. })
                {
                    opening.tree = None;
                }
                return Err(error);
            }
        };
        opening.made(Child {
            run: admitted.run.clone(),
            action: action(),
            live: admitted.live.clone(),
            status: None,
            times: None,
        });
        Ok(admitted)
    }
}

impl Executor for Nested {
    fn prepare(&self, call: &Call) -> Result<PreparedCall, CallError> {
        // A run that froze its calls is told of a call it may not make what it would
        // be told if it made it, and nothing more: not whether the action is one of
        // the build, and not whether the store holds the artifact. As any
        // preparation, it is not a call, and is not recorded.
        match self.frozen(call) {
            Some(refusal) => Err(refusal.into()),
            None => self.engine.prepare(call),
        }
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
        // A started run is a thread. It takes a place among the started runs of the
        // store's writer, the count a request from outside is held to, before it is
        // admitted; and a run may have only so many of its own at once. A full count
        // is a refusal, never a wait. Both come back when the run is over, before
        // whoever waits for it is told.
        // What the run froze is held first: a call the run may not make takes no
        // place, and is told so whatever the count of started runs is.
        if let Some(refusal) = self.frozen(request.call()) {
            let refused = Refused::of(request.call(), &refusal, &self.secrets);
            self.under.live.nest().refused(refused);
            return Err(refusal.into());
        }
        let place = match self.engine.start_place() {
            Ok(place) => place,
            Err(refusal) => {
                let refused = Refused::of(request.call(), &refusal, &self.secrets);
                self.under.live.nest().refused(refused);
                return Err(refusal.into());
            }
        };
        let mut admitted = self.admit(request, true)?;
        admitted.started_under(self.under.live.clone());
        // The thread is made where every run's thread is made; a thread that cannot
        // be made ends the run as an error, and the caller is told.
        self.engine.launch(admitted, place)
    }

    fn status(&self, run: &RunId) -> Result<Watched, CallError> {
        self.engine.status(run)
    }

    fn wait(&self, run: &RunId) -> Result<Watched, CallError> {
        // A run is over only after the runs it called: a wait for this run, or for
        // one above it, would never end, and is refused. Any other wait is given up
        // when this run is cancelled, so two runs that wait for each other are freed
        // by cancelling either, or a run above them. Both are kept for the record.
        let under = &self.under;
        let waited = if under.above.contains(run) {
            Err(Refusal::WaitAbove { run: run.clone() }.into())
        } else {
            self.engine.wait_unless(run, under.live.signal())
        };
        if let Err(CallError::Refused(
            refusal @ (Refusal::WaitAbove { .. } | Refusal::WaitCancelled { .. }),
        )) = &waited
        {
            let refused = Refused::of_a_question(refusal, &self.secrets);
            under.live.nest().refused(refused);
        }
        waited
    }

    fn cancel(&self, run: &RunId) -> Result<Cancel, CallError> {
        self.engine.cancel(run)
    }

    fn request_id(&self, text: &str) -> Result<RequestId, CallError> {
        self.engine.request_id(text)
    }
}
