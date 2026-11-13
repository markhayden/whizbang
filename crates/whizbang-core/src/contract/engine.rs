//! The executor: its interface, and the one implementation, which the binary builds and
//! hands to whatever runs actions.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::artifact::{self, Artifact, Contents};
use crate::capability::Shown;
use crate::identity::{DecodeLimits, Digest};
use crate::store::{self, Store};

use super::journal::Kept;
use super::ledger::{Declined, Reservation, Step};
use super::live::{self, LiveRun, Told};
use super::nested::{Added, Family, HeldOpen, Nested, Offer, Under};
use super::prepare::prepare;
use super::record::{
    Diagnostic, OUTPUTS_PREFIX, RECORD_KIND, RECORD_MAX_BYTES, RECORD_NAME, Ran, RunRecord,
    StoredRecordError,
};
use super::request::{Found, look_up, outcome_of};
use super::{
    ADMITTED_NAME, Action, ActionDescriptor, Admission, Answer, Backends, Call, CallError, Cancel,
    Cancellation, Capabilities, Clock, Committed, Context, Ending, Failure, Finished, Ledger,
    MESSAGE_LIMIT, Outcome, PreparedCall, Produced, Readiness, Refusal, Registry, Request,
    RequestId, RunId, RunState, Secrets, Stale, StoreFailure, SystemClock, Watched,
    request_fingerprint,
};

/// What runs actions. A door, the evaluation harness and an action that calls another
/// action all hold one of these and nothing closer to a tool.
///
/// `submit` and `request` return when the run is over and its record is durable, or,
/// for a request that was sent before, as soon as its id was looked up. `start` returns
/// once the run is admitted; a run is then asked after by its id (`status`, `wait`),
/// and cancelled by it (`cancel`), by any caller (ADR 0008).
pub trait Executor: Send + Sync {
    /// Can this exact call run now? Nothing is written, nothing is spent, and nothing is
    /// sent to a model. A call that is refused and a store that failed are told apart
    /// ([`CallError`]).
    fn prepare(&self, call: &Call) -> Result<PreparedCall, CallError>;

    /// The entry point for a call that is to run: the one place a call comes in, whatever
    /// door it came through. A call that is refused has no run. One that is taken is
    /// admitted, which makes its run in the store, and then runs; what comes back is
    /// its run's id and outcome, after the run's record and diagnostics are durable in
    /// the store, whatever the outcome (ADR 0007).
    fn submit(&self, call: &Call) -> Result<Finished, CallError>;

    /// The same entry point, for a request that may carry the caller's id. Without an
    /// id it is [`Executor::submit`]. With one, the id is looked up before anything
    /// else: a request that was admitted before under the id is answered with the run
    /// it made then, wherever that run is now, and nothing is prepared and nothing
    /// runs; another request under the id is refused as a conflict; and an id that was
    /// never admitted is recorded, durably, in the one step that makes the run, before
    /// the action runs. A request that comes with a preparation
    /// ([`Request::expecting`]) is refused as stale, with no run, when its call no
    /// longer prepares the same.
    fn request(&self, request: &Request) -> Result<Answer, CallError>;

    /// The same entry point, for a caller that does not wait: the request is looked
    /// up, prepared and admitted as [`Executor::request`] does it, and what comes back,
    /// once the admission is durable, is [`Answer::Started`] with the run, while the
    /// action runs on a thread of its own. The run is then found by its id, by this
    /// caller or any other ([`Executor::status`], [`Executor::wait`]). A request that
    /// was sent before is answered as a repeat, and a refusal is a refusal, as there.
    fn start(&self, request: &Request) -> Result<Answer, CallError>;

    /// Where the run is now, and, while it executes in this process, the progress
    /// events it has reported so far. It answers while the run's action is busy, and
    /// while the run's end is being published. A run the store does not hold is
    /// refused ([`Refusal::NoSuchRun`]).
    fn status(&self, run: &RunId) -> Result<Watched, CallError>;

    /// As [`Executor::status`], once the run is over: it returns when the run has
    /// ended, or at once when nothing in this process is working on it. So it never
    /// waits for a run that no process will end: such a run is answered as not ended.
    fn wait(&self, run: &RunId) -> Result<Watched, CallError>;

    /// Cancels the run (ADR 0008, "Cancellation"). A cancellation that arrives before
    /// the run enters its commit step is accepted: nothing the run made is committed,
    /// and the run ends as cancelled. One that arrives after is not: it waits for the
    /// commit, and is answered with how the run ended. A run that had ended is
    /// answered with how, and nothing changes. A run the store does not hold is
    /// refused ([`Refusal::NoSuchRun`]).
    fn cancel(&self, run: &RunId) -> Result<Cancel, CallError>;

    /// The request id this text is. A door hands over the text a caller gave here, and
    /// not to [`RequestId::parse`], whose refusal shows the text: this looks first for
    /// the secrets the executor was told of, in the text as it was given, and refuses
    /// text that holds one without showing any of it.
    fn request_id(&self, text: &str) -> Result<RequestId, CallError>;
}

/// The executor of one build: a registry, the backends, a store and the digest of the
/// config.
///
/// Built over a read of a store ([`Engine::new`]) it prepares calls, answers a request
/// that was sent before, and runs none. Built over a store opened to write
/// ([`Engine::over`]) it runs them: it makes each run in the store when it admits it,
/// and adds the run's record and diagnostics when the run ends.
#[derive(Clone)]
pub struct Engine {
    registry: Registry,
    backends: Arc<dyn Backends>,
    contents: Arc<dyn Contents + Send + Sync>,
    /// The store to write runs to, when this executor may.
    store: Option<Arc<Store>>,
    config: Arc<dyn ConfigSource>,
    limits: DecodeLimits,
    clock: Arc<dyn Clock>,
    secrets: Secrets,
    /// The ledger that takes part in admission, when this executor was given one.
    ledger: Option<Arc<dyn Ledger>>,
    /// The backends each run's capability handles are made over.
    capabilities: Capabilities,
}

/// Where the executor reads the digest of the config. It is asked every time a call is
/// prepared, so a config that changed between a preparation and its admission is seen
/// at admission. A digest is a config that never changes.
pub trait ConfigSource: Send + Sync {
    fn digest(&self) -> Digest;
}

impl ConfigSource for Digest {
    fn digest(&self) -> Digest {
        *self
    }
}

/// The version of whizbang a record is written with: this crate's, which the workspace
/// shares.
pub const WHIZBANG_VERSION: &str = env!("CARGO_PKG_VERSION");

impl Engine {
    /// An executor over these actions and backends, reading this store, under the config
    /// with this digest. Images are decoded within the default limits. It runs nothing:
    /// it was given no way to write.
    pub fn new(
        registry: Registry,
        backends: Arc<dyn Backends>,
        contents: Arc<dyn Contents + Send + Sync>,
        config: Digest,
    ) -> Engine {
        Engine {
            registry,
            backends,
            contents,
            store: None,
            config: Arc::new(config),
            limits: DecodeLimits::default(),
            clock: Arc::new(SystemClock),
            secrets: Secrets::none(),
            ledger: None,
            capabilities: Capabilities::none(),
        }
    }

    /// An executor that runs actions: it reads this store and writes its runs to it. The
    /// store is open to write, so this process is its one writer for as long as the
    /// executor lives.
    pub fn over(
        registry: Registry,
        backends: Arc<dyn Backends>,
        store: Arc<Store>,
        config: Digest,
    ) -> Engine {
        let mut engine = Engine::new(registry, backends, store.clone(), config);
        engine.store = Some(store);
        engine
    }

    /// The same executor, under a config that is read each time a call is prepared, at
    /// admission too, instead of the one digest it was built with.
    pub fn with_config(mut self, config: Arc<dyn ConfigSource>) -> Engine {
        self.config = config;
        self
    }

    /// The same executor, telling the time by this clock.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Engine {
        self.clock = clock;
        self
    }

    /// The same executor, taking these secrets out of everything it writes.
    pub fn with_secrets(mut self, secrets: Secrets) -> Engine {
        self.secrets = secrets;
        self
    }

    /// The same executor, admitting every run with a reservation that this ledger is
    /// asked for (ADR 0007, "The reservation"). Without one nothing is reserved.
    pub fn with_ledger(mut self, ledger: Arc<dyn Ledger>) -> Engine {
        self.ledger = Some(ledger);
        self
    }

    /// The same executor, handing each run's action a handle on each of these
    /// backends, bound to that run's cancellation and deadline.
    pub fn with_capabilities(mut self, capabilities: Capabilities) -> Engine {
        self.capabilities = capabilities;
        self
    }

    /// For tests: how many callers are waiting, now, for the run to be over.
    #[doc(hidden)]
    pub fn waiting_for(&self, run: &RunId) -> usize {
        let attached = self.store.as_ref().and_then(|s| s.attached(run.as_str()));
        attached.and_then(live::of).map_or(0, |live| live.waiting())
    }

    /// For tests: what holds the run open, now, when this process has it open.
    #[doc(hidden)]
    pub fn held_open(&self, run: &RunId) -> Option<HeldOpen> {
        let attached = self.store.as_ref().and_then(|s| s.attached(run.as_str()));
        attached
            .and_then(live::of)
            .map(|live| live.nest().held_open())
    }

    /// The actions this executor runs.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// A new run's id: the time in milliseconds since the Unix epoch, a hyphen, and a
    /// count. The count is one for the whole process ([`NAMED`]), so two executors never
    /// make the same id from the same time, and it goes up past any id the store
    /// already holds now, and past any id whose reservation the store still holds: a
    /// run that was deleted leaves its reservation, and a run that reserves cannot
    /// have that id. That an id is still free when its run is admitted is not taken on
    /// trust: the admission's publication makes the run, and the store refuses it if
    /// the id, or its reservation's name, was taken meanwhile ([`MAX_NAMINGS`]).
    fn name_run(&self, store: &Store, now_ms: u64) -> Result<RunId, CallError> {
        let generation = store
            .snapshot()
            .map_err(|error| CallError::Store(StoreFailure::new(error)))?;
        loop {
            let count = NAMED.fetch_add(1, Ordering::SeqCst) + 1;
            let id = RunId::minted(now_ms, count);
            if generation.run(id.as_str()).is_none()
                && generation.get(&Reservation::name_of(&id)).is_none()
            {
                return Ok(id);
            }
        }
    }
}

/// The runs this process has named, whichever executor named them.
static NAMED: AtomicU64 = AtomicU64::new(0);

/// How many times a run is named and its admission published before the store's refusal
/// of its id is handed back as the store's failure.
const MAX_NAMINGS: u32 = 8;

impl Executor for Engine {
    fn prepare(&self, call: &Call) -> Result<PreparedCall, CallError> {
        self.prepare_from(call, self.contents.as_ref())
    }

    fn submit(&self, call: &Call) -> Result<Finished, CallError> {
        match self.admit(call, None, None, &Bounded::by_nothing(), None) {
            Ok(admitted) => self.execute(admitted),
            Err(Stopped::Refused(error)) => Err(error),
            // Only a request with an id makes the name that can be taken.
            Err(Stopped::Taken(error)) => Err(CallError::Store(StoreFailure::new(error))),
        }
    }

    fn request_id(&self, text: &str) -> Result<RequestId, CallError> {
        // Before the text is parsed: a refusal of text that is no id shows the text.
        if self.secrets.redact(text) != text {
            return Err(Refusal::SecretInRequestId.into());
        }
        RequestId::parse(text).map_err(|why| Refusal::NotARequestId { why }.into())
    }

    fn request(&self, request: &Request) -> Result<Answer, CallError> {
        match self.begin(request)? {
            Ok(admitted) => Ok(Answer::Ran(self.execute(admitted)?)),
            Err(answer) => Ok(answer),
        }
    }

    fn start(&self, request: &Request) -> Result<Answer, CallError> {
        let admitted = match self.begin(request)? {
            Ok(admitted) => admitted,
            Err(answer) => return Ok(answer),
        };
        let run = admitted.run.clone();
        // The action runs on a thread of its own, which holds what it needs of this
        // executor, the store among it. How the run ended is in the store, and is
        // what whoever asks after the run is told; nobody is handed this result.
        let engine = self.clone();
        std::thread::spawn(move || drop(engine.execute(admitted)));
        Ok(Answer::Started(run))
    }

    fn status(&self, run: &RunId) -> Result<Watched, CallError> {
        self.watch(run, false)
    }

    fn wait(&self, run: &RunId) -> Result<Watched, CallError> {
        self.watch(run, true)
    }

    fn cancel(&self, run: &RunId) -> Result<Cancel, CallError> {
        let unknown = || CallError::Refused(Refusal::NoSuchRun { run: run.clone() });
        let not = |state| Ok(Cancel::NotAccepted(state));
        match self.seen(run)? {
            Seen::Nothing => return Err(unknown()),
            Seen::Ended(outcome) => return not(RunState::Ended(outcome)),
            Seen::Open => {}
        }
        // The run was admitted and had not ended when the store was looked at. Found
        // in the table of open runs, with an executor's state, it is cancelled or not
        // under its own lock (ADR 0008, "Cancellation"): that is the decision, and no
        // look at the store is. The table's lock is let go before the run's is taken.
        let attached = self.store.as_ref().and_then(|s| s.attached(run.as_str()));
        let mut unended = RunState::NotEnded { here: false };
        if let Some(attached) = attached {
            let Some(live) = live::of(attached) else {
                return not(RunState::NotEnded { here: true });
            };
            if live.cancel() == Told::Accepted {
                return Ok(Cancel::Accepted);
            }
            // Too late: the run's end had begun, and it is over now. If the store
            // still holds it open, it was abandoned.
            let failure = live.abandoned().flatten();
            unended = RunState::Abandoned { failure };
        }
        // Over, or not in the table: as for a status, a second look at the store is
        // the answer. A run that is still open there was abandoned by this process,
        // or will not be ended by any.
        match self.seen(run)? {
            Seen::Nothing => Err(unknown()),
            Seen::Ended(outcome) => not(RunState::Ended(outcome)),
            Seen::Open => not(unended),
        }
    }
}

impl Engine {
    /// Everything before a run's action: the look for a request's id, the preparation
    /// and the admission. What comes back is the run that was admitted, to be executed,
    /// or the answer to a request that makes no run: a repeat.
    fn begin(&self, request: &Request) -> Result<Result<Admitted, Answer>, CallError> {
        let bounded = Bounded::by(request);
        let Some(id) = request.id() else {
            return match self.admit(request.call(), None, request.expected(), &bounded, None) {
                Ok(admitted) => Ok(Ok(admitted)),
                Err(Stopped::Refused(error)) => Err(error),
                // Only a request with an id makes the name that can be taken.
                Err(Stopped::Taken(error)) => Err(CallError::Store(StoreFailure::new(error))),
            };
        };
        // An id is written as it is, in a name of the store and in two records. One
        // that redaction would change holds a secret, and is refused without being
        // shown.
        if self.secrets.redact(id.as_str()) != id.as_str() {
            return Err(Refusal::SecretInRequestId.into());
        }
        // The id is looked up here, before anything else: a repeated id answers with the
        // run it already has, and neither readiness nor preparation is consulted
        // (`SPEC.md` section 3, "Request identity").
        let fingerprint = request.fingerprint();
        let answer = |found: Found| match found {
            Found::Nothing => None,
            Found::Same(earlier) => Some(Ok(Err(Answer::Repeat(earlier)))),
            Found::Other { run } => Some(Err(CallError::Refused(Refusal::Conflict {
                request: id.clone(),
                run,
            }))),
        };
        if let Some(answered) = answer(self.look_up(id, fingerprint)?) {
            return answered;
        }
        let asked = Some((id, fingerprint));
        match self.admit(request.call(), asked, request.expected(), &bounded, None) {
            Ok(admitted) => Ok(Ok(admitted)),
            // A refusal that comes after the look is of a request whose id may have
            // been taken since: by then the call may no longer prepare the same, be
            // ready, or fit the allowance, because the request under its id was
            // admitted. The id is looked up once more, and what it says is the
            // answer; a refusal stands only when the id is still not taken.
            Err(Stopped::Refused(error @ CallError::Refused(_))) => {
                answer(self.look_up(id, fingerprint)?).unwrap_or(Err(error))
            }
            Err(Stopped::Refused(error)) => Err(error),
            // Another request was admitted under the id between the look and the
            // admission, and the store refused this one. Nothing of this request was
            // published and its action did not run: it is answered as the look would
            // have answered it a moment later.
            Err(Stopped::Taken(error)) => answer(self.look_up(id, fingerprint)?)
                .unwrap_or_else(|| Err(CallError::Store(StoreFailure::new(error)))),
        }
    }
}

/// A run that was admitted and whose action has not run yet: what executing it needs.
pub(super) struct Admitted {
    action: Arc<dyn Action>,
    prepared: PreparedCall,
    pub(super) run: RunId,
    request: Option<RequestId>,
    reserved: Option<u64>,
    start_ms: u64,
    kept: Arc<Kept>,
    pub(super) live: Arc<LiveRun>,
    /// The run that called this one, the actions running above it, and its allowance.
    family: Family,
    store: Arc<Store>,
    /// The generation the run was admitted against, held until this is dropped.
    view: Arc<Pinned>,
    deadline: Option<Instant>,
    /// Says that the executor is done with the run, when this is dropped. Executing
    /// the run takes it out, to drop it after everything else the run held.
    over: Option<Over>,
}

/// What bounds a request from outside: the signal it is cancelled by, and its deadline
/// when it has one.
pub(super) struct Bounded {
    pub(super) signal: Cancellation,
    pub(super) deadline: Option<Instant>,
}

impl Bounded {
    /// The signal and the deadline the request came with. A request that came with no
    /// signal is given one, which is its run's from the admission on.
    fn by(request: &Request) -> Bounded {
        Bounded {
            signal: request.cancellation().cloned().unwrap_or_default(),
            deadline: request.deadline(),
        }
    }

    fn by_nothing() -> Bounded {
        Bounded {
            signal: Cancellation::new(),
            deadline: None,
        }
    }
}

/// The generation a run was admitted against, as the store its call is prepared from
/// and its action reads (ADR 0008, "The generation a run reads"): every snapshot is
/// that generation, and for as long as this lives a prune leaves it and what it names.
/// One thing is added to it: the name of each artifact that a run this run called has
/// committed, as the current generation holds it (ADR 0008, "Nested calls").
struct Pinned {
    store: Arc<Store>,
    holding: store::Holding,
    added: Arc<Added>,
}

impl Contents for Pinned {
    fn snapshot(&self) -> store::Result<store::Generation> {
        self.added
            .onto(self.holding.generation().clone(), &self.store)
    }

    fn read(&self, entry: &store::Entry) -> store::Result<Vec<u8>> {
        self.store.read_output(entry)
    }

    fn read_record(&self, entry: &store::Entry) -> store::Result<Vec<u8>> {
        self.store.read(entry)
    }
}

/// Says, when it is dropped, that nothing in this process works on the run any more.
/// It is made with the run's live state, before the admission is published: an
/// admission that is given up, or that became visible and was not confirmed, drops it,
/// so nobody waits for, or is told they cancelled, a run whose action will never start.
///
/// However it is dropped, by a return or by a panic that unwinds, the run is over for
/// this process: ended, when the executor said so first, and otherwise abandoned, with
/// the store's failure it was told of (ADR 0008, "A run with nothing behind it").
struct Over {
    live: Arc<LiveRun>,
    ended: bool,
    failure: Option<StoreFailure>,
    /// The run that started this one from inside its action, which counts the runs it
    /// started that are not over (ADR 0008, "Nested calls").
    started_under: Option<Arc<LiveRun>>,
}

impl Admitted {
    /// The run was started by this run's action, on a thread of its own.
    pub(super) fn started_under(&mut self, parent: Arc<LiveRun>) {
        if let Some(over) = &mut self.over {
            over.started_under = Some(parent);
        }
    }
}

impl Drop for Over {
    fn drop(&mut self) {
        if let Some(parent) = self.started_under.take() {
            parent.nest().started_over();
        }
        self.live.over(self.ended, self.failure.take());
    }
}

/// Why a call was not admitted.
pub(super) enum Stopped {
    /// It was refused, or the store failed.
    Refused(CallError),
    /// The store refused the admission because the request id's name was made
    /// meanwhile: another request was admitted under the id.
    Taken(store::Error),
}

impl From<CallError> for Stopped {
    fn from(error: CallError) -> Stopped {
        Stopped::Refused(error)
    }
}

impl Engine {
    fn look_up(&self, id: &RequestId, fingerprint: Digest) -> Result<Found, CallError> {
        let made_here = |run: &str| self.store.as_ref().is_some_and(|s| s.made_here(run));
        let found = look_up(self.contents.as_ref(), id, fingerprint, made_here)?;
        // The snapshot is taken before the store is asked whether it made the run. A
        // run that ended or was deleted in between has no record in that snapshot and
        // is already forgotten by the store, which would read as a run that no process
        // will end. The store forgets a run only once the generation that closes or
        // deletes it is visible, so a second look sees it, and is the answer.
        let unsure = matches!(
            &found,
            Found::Same(earlier) if earlier.state() == &RunState::NotEnded { here: false }
        );
        if unsure && self.store.is_some() {
            return look_up(self.contents.as_ref(), id, fingerprint, made_here);
        }
        // A run that is open here and that the executor abandoned is said to be: after
        // one more look, since an end that became visible and was not confirmed
        // abandons the run too, and is the answer.
        let open_here = |found: &Found| match found {
            Found::Same(earlier) if earlier.state() == &RunState::NotEnded { here: true } => {
                let attached = self.store.as_ref()?.attached(earlier.run().as_str());
                live::of(attached?)?.abandoned()
            }
            _ => None,
        };
        if open_here(&found).is_some() {
            let found = look_up(self.contents.as_ref(), id, fingerprint, made_here)?;
            return Ok(match (open_here(&found), found) {
                (Some(failure), Found::Same(earlier)) => {
                    Found::Same(earlier.in_state(RunState::Abandoned { failure }))
                }
                (_, found) => found,
            });
        }
        Ok(found)
    }

    /// Prepares and admits a call: everything after the look for a request id and
    /// before the action. `expected` is the preparation the request came with, when it came with one: the
    /// call is prepared again here, under the config and the backends as they are now,
    /// and a request whose preparation no longer holds is refused before anything is
    /// admitted. `under` is the run whose action made the call, for a call from inside
    /// an action (ADR 0008, "Nested calls").
    pub(super) fn admit(
        &self,
        call: &Call,
        asked: Option<(&RequestId, Digest)>,
        expected: Option<&PreparedCall>,
        bounded: &Bounded,
        under: Option<&Under>,
    ) -> Result<Admitted, Stopped> {
        self.still_wanted(call.action(), bounded)?;
        let added = Arc::new(Added::default());
        // The generation the run is admitted against is held from here, before the
        // call is prepared, until the executor is done with the run: the inputs are
        // read from it, and so is everything the action reads.
        let view = match &self.store {
            Some(store) => {
                let holding = store
                    .hold()
                    .map_err(|error| CallError::Store(StoreFailure::new(error)))?;
                Some(Arc::new(Pinned {
                    store: store.clone(),
                    holding,
                    added: added.clone(),
                }))
            }
            None => None,
        };
        let stale = |action: &str, why: Stale| {
            Stopped::Refused(CallError::Refused(Refusal::Stale {
                action: action.to_owned(),
                why,
            }))
        };
        let prepared = match &view {
            Some(view) => self.prepare_from(call, view.as_ref()),
            None => self.prepare(call),
        };
        let prepared = match (prepared, expected) {
            (Ok(prepared), _) => prepared,
            // The call prepared once and is refused now: the preparation is stale. A
            // failure of the store is not a judgement of the preparation.
            (Err(CallError::Refused(refusal)), Some(_)) => {
                return Err(stale(call.action(), Stale::Refused(Box::new(refusal))));
            }
            (Err(error), _) => return Err(error.into()),
        };
        // Staleness is said before readiness: a call whose model was taken away is not
        // ready, and what the caller needs to hear is that what it was shown is gone.
        if let Some(why) = expected.and_then(|expected| expected.stale_against(&prepared)) {
            return Err(stale(prepared.action(), why));
        }
        if let Readiness::NotReady(missing) = prepared.readiness() {
            return Err(CallError::Refused(Refusal::NotReady {
                action: prepared.action().to_owned(),
                missing: missing.clone(),
            })
            .into());
        }
        let Some((store, view)) = self.store.as_ref().zip(view) else {
            return Err(CallError::Refused(Refusal::ReadOnly).into());
        };
        // Prepare found the action a moment ago, in a registry that does not change.
        let Some(action) = self.registry.get(prepared.action()) else {
            return Err(CallError::Refused(Refusal::UnknownAction {
                name: prepared.action().to_owned(),
            })
            .into());
        };
        let failed = |error| CallError::Store(StoreFailure::new(error));

        // Admission: one publication makes the run, with its admission among its
        // names, and, for a request with an id, makes the id's record outside runs.
        // The store decides both inside its publishing step: a run of that id is named
        // again, and an id that was admitted meanwhile stops this request with nothing
        // published. From the publication on there is a run, durably, and whatever
        // happens to its action it is in the store.
        let start_ms = self.clock.now_ms();
        let request = asked.map(|(id, _)| id);
        let fingerprint = match asked {
            Some((_, fingerprint)) => fingerprint,
            None => request_fingerprint(call),
        };
        // What is reserved is the estimate of the preparation made a moment ago, here,
        // and never of the one the request came with, which was only compared. A cost
        // without a bound is not an amount to reserve: the ledger is not asked.
        // A call from inside an action draws on its caller's allowance, and the
        // ledger is not asked: the estimate is taken from what the caller has left,
        // in one step under the allowance's lock, and is given back if no run is made
        // (ADR 0008, "Nested calls"). A caller that was admitted with no ledger has
        // no amount, and nothing is counted.
        let counted = match under {
            Some(under) => under.allowance.total_micros().is_some(),
            None => self.ledger.is_some(),
        };
        if counted && prepared.estimate().is_unbounded() {
            return Err(CallError::Refused(Refusal::NoBound {
                action: prepared.action().to_owned(),
            })
            .into());
        }
        let reserved = counted.then(|| prepared.estimate().micros());
        let taken = match (under, reserved) {
            (Some(under), Some(micros)) => under.allowance.take(micros).map_err(|left_micros| {
                CallError::Refused(Refusal::OverCaller {
                    action: prepared.action().to_owned(),
                    asked_micros: micros,
                    left_micros,
                })
            })?,
            _ => None,
        };
        let family = Family::of(under, prepared.action(), reserved);
        let parent = family.parent.as_ref().map(|(run, _)| run.clone());
        // What the run keeps while it executes is made before the run is, and attached
        // to it in the store's table of open runs by the publication that makes it: so
        // whoever sees the run in a snapshot finds it there (ADR 0008, "Status").
        let kept = Arc::new(Kept::new(self.clock.clone(), self.secrets.clone()));
        let live = Arc::new(LiveRun::new(kept.clone(), bounded.signal.clone(), added));
        let mut over = Over {
            live: live.clone(),
            ended: false,
            failure: None,
            started_under: None,
        };
        let mut run = self.name_run(store, start_ms)?;
        let mut namings = 1;
        loop {
            // Before each attempt, which is each time the ledger is asked: a request
            // that was cancelled meanwhile, or whose deadline passed, is given up with
            // nothing published (ADR 0008, "Cancelling an admission").
            self.still_wanted(prepared.action(), bounded)?;
            // Everything that names the run is made again for every naming: the
            // admission, which is published twice as one content, and the reservation,
            // which holds the admission's digest.
            let admission = Admission::new(
                run.clone(),
                request.cloned(),
                fingerprint,
                prepared.fingerprint(),
                reserved,
                prepared.action(),
                prepared.version(),
                start_ms,
            )
            .under(parent.clone())
            .to_bytes();
            let mut publication = store.begin().map_err(failed)?;
            // The ledger takes part here: it judges, and adds what it must to this
            // publication, before anything is published. Declined, the publication is
            // dropped with all it staged.
            let mut reservation_name = None;
            let mut ledgers = Vec::new();
            // The ledger is asked for a run that nothing called, and for no other:
            // nothing is reserved twice (`SPEC.md` section 6).
            let ledger = self.ledger.as_ref().filter(|_| under.is_none());
            if let (Some(ledger), Some(micros)) = (ledger, reserved) {
                let reservation = Reservation::new(
                    run.clone(),
                    request.cloned(),
                    Digest::of_bytes(&admission),
                    micros,
                    start_ms,
                );
                let generation = store.snapshot().map_err(failed)?;
                let mut step = Step::new(
                    &mut publication,
                    &generation,
                    self.contents.as_ref(),
                    &self.secrets,
                );
                match ledger.reserve(&reservation, &mut step) {
                    Ok(()) => {}
                    Err(Declined::OverAllowance { left_micros }) => {
                        return Err(CallError::Refused(Refusal::OverAllowance {
                            action: prepared.action().to_owned(),
                            asked_micros: micros,
                            left_micros,
                        })
                        .into());
                    }
                    Err(Declined::Failed(error)) => return Err(error.into()),
                }
                ledgers = step.into_made();
                let name = reservation.name();
                publication
                    .create(&name, RECORD_KIND, &reservation.to_bytes())
                    .map_err(failed)?;
                reservation_name = Some(name);
            }
            publication.create_run(run.as_str()).map_err(failed)?;
            publication.attach(run.as_str(), live.clone());
            publication
                .put_in(run.as_str(), ADMITTED_NAME, RECORD_KIND, &admission)
                .map_err(failed)?;
            if let Some(id) = request {
                publication
                    .create(&id.admission_name(), RECORD_KIND, &admission)
                    .map_err(failed)?;
            }
            match publication.publish() {
                Ok(_) => break,
                Err(store::Error::RunExists { .. }) if namings < MAX_NAMINGS => {
                    namings += 1;
                    run = self.name_run(store, start_ms)?;
                }
                // The reservation of a run that had this id, and was deleted, is still
                // there: the id is not free for a run that reserves.
                Err(store::Error::NameExists { name })
                    if reservation_name.as_ref() == Some(&name) && namings < MAX_NAMINGS =>
                {
                    namings += 1;
                    run = self.name_run(store, start_ms)?;
                }
                // A name the ledger made was made meanwhile: another admission was
                // published since the ledger judged. It is asked again, on the
                // generation as it is now. The store changed each time this happens.
                Err(store::Error::NameExists { name }) if ledgers.contains(&name) => {}
                Err(error @ store::Error::NameExists { .. })
                    if request.is_some_and(|id| {
                        matches!(&error, store::Error::NameExists { name }
                            if *name == id.admission_name())
                    }) =>
                {
                    return Err(Stopped::Taken(error));
                }
                // Among these is an admission that became visible and was not
                // confirmed: its run is in the store and open here, and is abandoned.
                Err(error) => {
                    let failure = StoreFailure::new(error);
                    over.failure = Some(failure.clone());
                    return Err(CallError::Store(failure).into());
                }
            }
        }

        // The run is made: what was taken from its caller's allowance stays taken.
        if let Some(taken) = taken {
            taken.keep();
        }
        Ok(Admitted {
            action: action.clone(),
            prepared,
            run,
            request: request.cloned(),
            reserved,
            start_ms,
            kept,
            live,
            family,
            store: store.clone(),
            view,
            deadline: bounded.deadline,
            over: Some(over),
        })
    }

    /// Refuses a request whose signal is set or whose deadline has passed, by this
    /// executor's clock: it is not admitted.
    fn still_wanted(&self, action: &str, bounded: &Bounded) -> Result<(), CallError> {
        let action = action.to_owned();
        if bounded.signal.is_cancelled() {
            return Err(Refusal::Cancelled { action }.into());
        }
        if bounded.deadline.is_some_and(|by| self.clock.now() >= by) {
            return Err(Refusal::DeadlinePassed { action }.into());
        }
        Ok(())
    }

    /// Prepares a call against this read of the store.
    fn prepare_from(
        &self,
        call: &Call,
        contents: &(dyn Contents + Send + Sync),
    ) -> Result<PreparedCall, CallError> {
        let Some(action) = self.registry.get(call.action()) else {
            return Err(CallError::Refused(Refusal::UnknownAction {
                name: call.action().to_owned(),
            }));
        };
        prepare(
            action.descriptor(),
            call,
            contents,
            self.backends.as_ref(),
            self.config.digest(),
            &self.limits,
        )
    }

    /// Runs the action of a run that was admitted, and ends the run: everything from
    /// the admission on. Whatever comes back, nothing in this process is working on
    /// the run any more, and whoever waits for it is told so.
    pub(super) fn execute(&self, mut admitted: Admitted) -> Result<Finished, CallError> {
        let mut over = admitted.over.take();
        // Everything the run held is let go in here, its hold on the generation it
        // read among it, before whoever waits for the run is told that it is over.
        // A panic in there unwinds through here, and the run is abandoned.
        let ended = self.run_and_end(admitted);
        if let Some(over) = &mut over {
            match &ended {
                Ok(_) => over.ended = true,
                Err(CallError::Store(failure)) => over.failure = Some(failure.clone()),
                Err(_) => {}
            }
        }
        drop(over);
        ended
    }

    fn run_and_end(&self, admitted: Admitted) -> Result<Finished, CallError> {
        let Admitted {
            action,
            prepared,
            run,
            request,
            reserved,
            start_ms,
            kept,
            live,
            family,
            store,
            view,
            deadline,
            over: _,
        } = admitted;
        let request = request.as_ref();
        let failed = |error| CallError::Store(StoreFailure::new(error));
        // The context is the run's: its signal, its deadline, the generation it was
        // admitted against, and handles that the context binds to that signal and
        // that deadline itself.
        // The executor in it admits a call under this run (ADR 0008, "Nested calls").
        let offer = Arc::new(Offer::default());
        let under = Under {
            run: run.clone(),
            live: live.clone(),
            stack: family.stack.clone(),
            allowance: family.allowance.clone(),
            deadline,
            inputs: Arc::new(prepared.inputs().iter().map(|input| input.id()).collect()),
            offer: offer.clone(),
        };
        let context = Context::new(
            live.signal().clone(),
            deadline,
            kept.clone(),
            family.allowance.clone(),
            view,
            // The backends the call resolved, and no other (ADR 0009).
            self.capabilities.resolved_for(&prepared),
            Arc::new(Nested::new(self.clone(), under, self.secrets.clone())),
        )
        .with_diagnostics(kept.clone())
        .recording(kept.clone())
        .offering(offer);
        live.nest().watch(context.requests().clone());
        // A panic in the action is caught: its run ends as any other does, with its
        // record and what it wrote, and the executor goes on (ADR 0008). The context
        // and what it shares are not left broken by one: what a run keeps is behind
        // locks that are taken whatever a panic did to them.
        // A run that was cancelled before its action started does not run it.
        let handed = (!live.signal().is_cancelled())
            .then(|| catch_unwind(AssertUnwindSafe(|| action.run(&prepared, &context))));
        // The action has come back. No call and no capability request is taken from
        // here on, and the run waits for those there are: every run it called is over,
        // and every request its handles sent is accounted for, before it ends (ADR
        // 0008, "Nested calls"). A cancellation is still accepted while it waits, and
        // reaches them.
        live.nest().settle();
        // The entry to the commit step (ADR 0008, "Cancellation"): the action has come
        // back, with whatever it came back with, and nothing it started is left. From
        // here on a cancellation is too late. One that was accepted before this was
        // set under the lock this reads under: the run ends as cancelled, and nothing
        // it handed back is committed.
        let cancelled = live.begin_end();
        // The outcome is made here, from what the action handed back, with the secrets
        // taken out: what is reported holds none, as what is written holds none. And
        // each of its texts is cut to a length a record can hold ([`MESSAGE_LIMIT`]).
        let said = |text: &str| said(&self.secrets, text);
        let error = |kind: &str, message: &str| Outcome::Error {
            kind: said(kind),
            message: said(message),
        };
        let mut outputs = Vec::new();
        let mut verdict = None;
        let mut outcome = match handed {
            None => Outcome::Cancelled,
            Some(Ok(Ok(produced))) => {
                verdict = produced.verdict().cloned();
                match held_to_ports(&prepared, action.descriptor(), produced, &self.secrets) {
                    Ok(held) => {
                        outputs = held;
                        Outcome::Completed {
                            outputs: outputs.iter().map(|(output, _)| output.clone()).collect(),
                        }
                    }
                    Err(why) => error("output", &why),
                }
            }
            Some(Ok(Err(Failure::Error { kind, message }))) => error(&kind, &message),
            Some(Ok(Err(Failure::Invalid { why }))) => Outcome::Invalid { why: said(&why) },
            Some(Err(panic)) => error("panic", &panic_text(panic)),
        };
        if cancelled {
            outputs.clear();
            outcome = Outcome::Cancelled;
        }
        let end_ms = self.clock.now_ms().max(start_ms);

        // The end: one publication adds the journal, every diagnostic and the record to
        // the run, and, for a request with an id, makes the id's ending outside runs.
        // For a run that completed it is the commit step (ADR 0008): the same
        // publication gives every output its name, so the outputs and the completed
        // status become visible together or not at all, and `outputs` holds something
        // only then. The run must still be there: one that was deleted while it
        // executed is not made again from its last names. The run is reported after
        // this is durable.
        let (lines, written) = kept.take();
        let kept: Vec<(Diagnostic, Vec<u8>)> = std::iter::once(Diagnostic::journal(&lines))
            .chain(written)
            .collect();
        // What the run's record says of the calls its action made: they are all over.
        let parent = family.parent.as_ref().map(|(run, _)| run.clone());
        // A verdict is written only in the record of a run that completed: the record
        // drops it for any other (ADR 0009).
        let mut inside = live.nest().inside(parent);
        inside.verdict = verdict;
        let record_of = |outcome: &Outcome| {
            RunRecord::of(
                Ran {
                    run: run.as_str(),
                    request,
                    whizbang: WHIZBANG_VERSION,
                    prepared: &prepared,
                    reserved_micros: reserved,
                    outcome,
                    start_ms,
                    end_ms,
                    diagnostics: kept.iter().map(|(d, _)| d.clone()).collect(),
                    inside: inside.clone(),
                },
                &self.secrets,
            )
        };
        let mut record = record_of(&outcome);
        // A record lists every output, and one over the limit is refused on reading
        // (ADR 0005). A run whose outputs make such a record commits none of them: it
        // ends as an error, with a record that lists none.
        let bytes = record.to_bytes().len() as u64;
        if bytes > RECORD_MAX_BYTES && !outputs.is_empty() {
            outcome = Outcome::Error {
                kind: "output".to_owned(),
                message: format!(
                    "the action handed back {} artifacts, and the record that lists them \
                     would be {bytes} bytes: a record is at most {RECORD_MAX_BYTES} bytes",
                    outputs.len()
                ),
            };
            outputs.clear();
            record = record_of(&outcome);
        }
        let mut publication = store.begin().map_err(failed)?;
        publication.close_run(run.as_str()).map_err(failed)?;
        let mut place = 0;
        for (at, (output, artifact)) in outputs.iter().enumerate() {
            // The outputs of one port are together, in the order they were handed back.
            let first = at == 0 || outputs[at - 1].0.port() != output.port();
            place = if first { 0 } else { place + 1 };
            let reference = format!("{OUTPUTS_PREFIX}{}/{place}", output.port());
            artifact::commit(&mut publication, run.as_str(), &reference, artifact)
                .map_err(failed)?;
        }
        for (diagnostic, bytes) in &kept {
            publication
                .put_in(run.as_str(), diagnostic.name(), RECORD_KIND, bytes)
                .map_err(failed)?;
        }
        publication
            .put_in(run.as_str(), RECORD_NAME, RECORD_KIND, &record.to_bytes())
            .map_err(failed)?;
        if let Some(id) = request {
            let ending = Ending::new(run.clone(), id.clone(), record.outcome().status, end_ms);
            publication
                .create(&id.ending_name(), RECORD_KIND, &ending.to_bytes())
                .map_err(failed)?;
        }
        publication.publish().map_err(failed)?;
        // The run that called this one is told how it ended, and from here on reads
        // what it committed: before this run is over, which is what its caller waits
        // for.
        if let Some((_, parent)) = &family.parent {
            let times = (start_ms, end_ms);
            parent
                .nest()
                .ended(&run, record.outcome().status, outcome.outputs(), times);
        }
        Ok(Finished::new(run, outcome))
    }
}

/// What an action handed back, held to the output ports its call returns (ADR 0008):
/// every artifact at one of those ports and of the port's type, and every port with as
/// many as its count says. What comes back is each output with the artifact to commit,
/// in the order of the descriptor's ports, and within a port in the order handed back;
/// or why nothing of it is committed, in fixed words, with a port's name as the action
/// wrote it shown escaped and cut. The secrets are taken out of such a name first,
/// while it is whole: one that the cut or the escaping had changed would no longer be
/// found, and what was left of it would be written.
fn held_to_ports(
    prepared: &PreparedCall,
    descriptor: &ActionDescriptor,
    produced: Produced,
    secrets: &Secrets,
) -> Result<Vec<(Committed, Artifact)>, String> {
    let returned: Vec<_> = descriptor
        .outputs()
        .iter()
        .filter(|port| prepared.outputs().iter().any(|name| name == port.name()))
        .collect();
    let mut at_port: Vec<Vec<Artifact>> = returned.iter().map(|_| Vec::new()).collect();
    for (name, artifact) in produced.into_outputs() {
        let Some(place) = returned.iter().position(|port| port.name() == name) else {
            return Err(format!(
                "the action handed back an artifact at {}, which is no output port this \
                 call returns",
                Shown(&secrets.redact(&name))
            ));
        };
        let port = returned[place];
        if artifact.artifact_type() != port.artifact() {
            return Err(format!(
                "the action handed back {} at the output port {}, which returns {}",
                artifact.artifact_type().with_article(),
                Shown(port.name()),
                port.artifact().with_article()
            ));
        }
        at_port[place].push(artifact);
    }
    let mut outputs = Vec::new();
    for (port, artifacts) in returned.into_iter().zip(at_port) {
        let count = artifacts.len() as u64;
        let said = |bound: u32, which: &str| {
            format!(
                "the action handed back {count} artifacts at the output port {}, which \
                 returns {bound} at {which}",
                Shown(port.name())
            )
        };
        if let Some(max) = port.count().max().filter(|max| count > u64::from(*max)) {
            return Err(said(max, "most"));
        }
        if count < u64::from(port.count().min()) {
            return Err(said(port.count().min(), "least"));
        }
        for artifact in artifacts {
            let output = Committed::new(port.name(), port.artifact(), artifact.id());
            outputs.push((output, artifact));
        }
    }
    Ok(outputs)
}

/// A text an action or a panic gave, as an outcome and a record keep it: with the
/// secrets taken out, and then its first [`MESSAGE_LIMIT`] characters, and three dots
/// when there were more. The secrets are taken out first, while the text is whole: a
/// secret that the cut went through would no longer be found. They are looked for once
/// more after the cut, in case the three dots completed one.
pub(super) fn said(secrets: &Secrets, text: &str) -> String {
    let whole = secrets.redact(text);
    match whole.char_indices().nth(MESSAGE_LIMIT) {
        Some((cut, _)) => secrets.redact(&format!("{}...", &whole[..cut])),
        None => whole,
    }
}

/// What a caught panic says, as the message of the error its run ends with: the text
/// the panic carried, when it carried one.
///
/// What the panic carried is dropped here, and a value can panic when it is dropped:
/// that panic is caught too, so it does not leave the executor in place of the run's
/// end. What that second panic carried is not dropped at all.
fn panic_text(panic: Box<dyn Any + Send>) -> String {
    let text = panic
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned());
    if let Err(second) = catch_unwind(AssertUnwindSafe(move || drop(panic))) {
        std::mem::forget(second);
    }
    match text {
        Some(text) => format!("the action panicked: {text}"),
        None => "the action panicked, and its panic carried no text".to_owned(),
    }
}

/// What a look at the store says of a run.
enum Seen {
    /// The store holds no such run.
    Nothing,
    /// The run has its record: how it ended.
    Ended(Outcome),
    /// The run was admitted and has no record.
    Open,
}

impl Engine {
    /// What the current generation says of the run. A record that is there is read as
    /// any stored record is: bounded, strictly, and held to name the run.
    fn seen(&self, run: &RunId) -> Result<Seen, CallError> {
        let generation = self
            .contents
            .snapshot()
            .map_err(|error| CallError::Store(StoreFailure::new(error)))?;
        let read = |entry: &store::Entry| self.contents.read_record(entry);
        match super::record::stored(&generation, run.as_str(), read) {
            None => Ok(Seen::Nothing),
            Some(Err(StoredRecordError::NoRecord)) => Ok(Seen::Open),
            Some(Ok(record)) => Ok(Seen::Ended(outcome_of(&record))),
            Some(Err(StoredRecordError::Store(failure))) => Err(CallError::Store(failure)),
            Some(Err(error)) => Err(CallError::Record {
                what: "the run's record",
                why: error.to_string(),
            }),
        }
    }

    /// Where the run is now (ADR 0008, "Status"): a look at the store, then at the
    /// store's table of the runs this process has open, then, when the run is in
    /// neither state, at the store again. With `wait`, a run that this process has
    /// open is waited for first.
    fn watch(&self, run: &RunId, wait: bool) -> Result<Watched, CallError> {
        let unknown = || CallError::Refused(Refusal::NoSuchRun { run: run.clone() });
        let watched = |state, events| Ok(Watched::new(run.clone(), state, events));
        match self.seen(run)? {
            Seen::Nothing => return Err(unknown()),
            Seen::Ended(outcome) => return watched(RunState::Ended(outcome), Vec::new()),
            Seen::Open => {}
        }
        // The run was admitted and had not ended when the store was looked at. A run
        // this process made is in the table from before it was visible until its end
        // is: found there, it is open here.
        let attached = self.store.as_ref().and_then(|s| s.attached(run.as_str()));
        if let Some(attached) = attached {
            let Some(live) = live::of(attached) else {
                return watched(RunState::NotEnded { here: true }, Vec::new());
            };
            if wait {
                live.wait_until_over();
            }
            let Some(failure) = live.abandoned() else {
                if !wait {
                    return watched(RunState::NotEnded { here: true }, live.events());
                }
                // It ended, and its end was confirmed: the store says how.
                return match self.seen(run)? {
                    Seen::Nothing => Err(unknown()),
                    Seen::Ended(outcome) => watched(RunState::Ended(outcome), Vec::new()),
                    Seen::Open => watched(RunState::NotEnded { here: true }, live.events()),
                };
            };
            // Nothing here works on it any more, and its end was not confirmed. An end
            // that became visible all the same is in the store, and is the answer;
            // otherwise the run was abandoned, as it was admitted.
            return match self.seen(run)? {
                Seen::Nothing => Err(unknown()),
                Seen::Ended(outcome) => watched(RunState::Ended(outcome), Vec::new()),
                Seen::Open => watched(RunState::Abandoned { failure }, live.events()),
            };
        }
        // Not in the table. Either this process never made it, or it left the table
        // since the look at the store, which it does only once its end, or its
        // deletion, is visible: a second look sees that, and is the answer.
        match self.seen(run)? {
            Seen::Nothing => Err(unknown()),
            Seen::Ended(outcome) => watched(RunState::Ended(outcome), Vec::new()),
            Seen::Open => watched(RunState::NotEnded { here: false }, Vec::new()),
        }
    }
}
