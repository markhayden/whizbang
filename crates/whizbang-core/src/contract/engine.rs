//! The executor: its interface, and the one implementation, which the binary builds and
//! hands to whatever runs actions.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use crate::artifact::{self, Artifact, Contents};
use crate::capability::{
    Around, EXCHANGES_MAX_BYTES, Place, Places, REQUESTS_LIMIT, Shown, Wrappers,
};
use crate::egress::Ways;
use crate::identity::{DecodeLimits, Digest};
use crate::store::{self, Store};

use super::journal::{Heard, JournalBounds, Kept};
use super::ledger::{Closing, Declined, Reread, Reservation, Settled, Step};
use super::live::{self, LiveRun, Told};
use super::nested::{Added, DESCENDANTS_LIMIT, Family, HeldOpen, Nested, Offer, Under};
use super::part::{self, Asking, PartDeclined, PartStep};
use super::prepare::prepare;
use super::record::{
    Diagnostic, Facts, Inside, OUTPUTS_PREFIX, RECORD_KIND, RECORD_MAX_BYTES, RECORD_NAME, Ran,
    RunRecord, Status, StoredRecordError,
};
use super::registry::MEASUREMENTS_MAX_BYTES;
use super::request::{Found, LookedUp, look_up, outcome_of};
use super::snapshot::Known;
use super::{
    ADMITTED_NAME, Action, ActionDescriptor, Admission, AdmissionPart, Allowance, Answer, Backends,
    Call, CallError, Cancel, Cancellation, Capabilities, Clock, Committed, ConfigSource, Context,
    Ending, Estimate, Failure, Findings, Finished, Ledger, MESSAGE_LIMIT, Outcome, PreparedCall,
    Produced, Readiness, Refusal, Registry, Request, RequestId, ResultKind, RunFacts, RunId,
    RunState, Secrets, Snapshot, Stale, StoreFailure, SystemClock, Value, Verdict, Watched,
    request_fingerprint,
};

/// What runs actions. A door, the evaluation harness and an action that calls another
/// action all hold one of these and nothing closer to a tool.
///
/// `submit` and `request` return when the run is over and its record is durable, or,
/// for a request that was sent before, as soon as its id was looked up. `start` returns
/// once the run is admitted, and is refused when too many started runs are live; a run
/// is then asked after by its id (`status`, `wait`), and cancelled by it (`cancel`), by
/// any caller (ADR 0008).
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
    ///
    /// Started runs are bounded (ADR 0008, "How many runs are started at once"): a
    /// request that would be one more than the executor's bound is refused, before
    /// anything is prepared or written ([`Refusal::TooManyStarted`]). And a run that
    /// was admitted and whose thread the system then refused to make is ended as an
    /// error, and the caller is handed [`CallError::NotStarted`].
    fn start(&self, request: &Request) -> Result<Answer, CallError>;

    /// Where the run is now, and, while it executes in this process, the progress
    /// events it has reported so far. It answers while the run's action is busy, and
    /// while the run's end is being published. A run the store does not hold is
    /// refused ([`Refusal::NoSuchRun`]).
    fn status(&self, run: &RunId) -> Result<Watched, CallError>;

    /// As [`Executor::status`], with only the events from the place `seen` on: what a
    /// caller that has seen the first `seen` places of the run's events has not, and
    /// how many of those places hold an event the run did not keep (plan m5, task K1,
    /// item 4). A poll copies no more than is new.
    fn status_after(&self, run: &RunId, seen: u64) -> Result<Watched, CallError> {
        self.status(run).map(|watched| watched.after(seen))
    }

    /// As [`Executor::status`], once the run is over: it returns when the run has
    /// ended, or at once when nothing in this process is working on it. So it never
    /// waits for a run that no process will end: such a run is answered as not ended.
    fn wait(&self, run: &RunId) -> Result<Watched, CallError>;

    /// Cancels the run (ADR 0008, "Cancellation"). A cancellation that arrives before
    /// the run enters its commit step is accepted: nothing the run made is committed,
    /// and the run ends as cancelled. One that arrives after is not: it waits until the
    /// executor is done with the run, the last sync of its end included, and is
    /// answered with how the run ended, or that it was abandoned. A run that had ended
    /// is answered with how, and nothing changes. A run the store does not hold is
    /// refused ([`Refusal::NoSuchRun`]).
    fn cancel(&self, run: &RunId) -> Result<Cancel, CallError>;

    /// The request id this text is. A door hands over the text a caller gave here, and
    /// not to [`RequestId::parse`], whose refusal shows the text: this looks first for
    /// the secrets the executor was told of, in the text as it was given, and refuses
    /// text that holds one without showing any of it. An id that starts
    /// [`super::RESERVED_REQUEST_PREFIX`] is refused: those are a lab's cells'.
    fn request_id(&self, text: &str) -> Result<RequestId, CallError>;

    /// What the id `id` finds for `request`, looked up as [`Executor::start`] and
    /// [`Executor::request`] look it up first, through the store's durable records of
    /// request ids and the request's fingerprint, with the same checks of secrets: no
    /// request under the id ([`LookedUp::Missing`]), this request ([`LookedUp::Repeat`],
    /// with its run), or another ([`LookedUp::Conflict`]). It admits, reserves, starts
    /// and writes nothing, whatever the executor's bound on started runs: a door asks
    /// it before it gives a request a place (plan m6, task S1, item 8). An executor
    /// that keeps no request ids refuses it.
    fn look_up(&self, id: &RequestId, request: &Request) -> Result<LookedUp, CallError> {
        let _ = (id, request);
        Err(Refusal::NotARequestId {
            why: "this executor keeps no request ids".to_owned(),
        }
        .into())
    }

    /// How many runs this executor started on threads of their own, a run called from
    /// inside an action among them, whose thread has not yet let go of everything it
    /// held of the executor and the store (plan m5, task H1): a door that drops its
    /// executor once this is 0 lets the store's writer go with it. None for an
    /// executor that starts no run.
    fn on_threads(&self) -> usize {
        0
    }
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
    /// The secrets it was built with and those of every snapshot of the config it has
    /// taken, which every clone of it shares (ADR 0015).
    secrets: Known,
    /// The ledger that takes part in admission, when this executor was given one.
    ledger: Option<Arc<dyn Ledger>>,
    /// The parts of every admission, in the order they were given (ADR 0015).
    parts: Vec<Arc<dyn AdmissionPart>>,
    /// The backends each run's capability handles are made over.
    capabilities: Capabilities,
    /// The ways out: what each handle's checked sender is made of (ADR 0015).
    ways: Ways,
    /// What stands in the three places between each handle and its backend (ADR
    /// 0015, "A wrapper around each request"). The gate's place, when nothing was
    /// put there, holds the gate of the run's policy.
    places: Places,
    /// How many runs may be admitted under one run that nothing called.
    descendants: usize,
    /// How many capability requests one run may send.
    requests: usize,
    /// How many bytes of exchanges with a judge one run may store.
    exchanges: u64,
    /// What a run's journal keeps (plan m5, task K1, item 4).
    journal: JournalBounds,
    /// How many bytes a run's record may be before it is written again without what
    /// the executor can leave out.
    record_max: u64,
    /// For tests: what is done in a wait from inside an action, between its first
    /// look at the waiter's signal and its look at the run.
    before_watch: Option<Arc<dyn Fn() + Send + Sync>>,
    /// How many started runs the store's writer may have, not over, when this executor
    /// starts one more ([`Engine::with_started_bound`]).
    started_bound: usize,
    /// What makes the thread of a started run.
    spawn: Spawn,
    /// For tests: what is done at each place where a panic of the executor's own code
    /// is caught and told apart ([`Engine::with_panic_at`]).
    panic_at: Option<PanicHook>,
}

/// What a test has done at a place in the ending of a run: the run's action, and the
/// place ([`Engine::with_panic_at`]).
pub type PanicHook = Arc<dyn Fn(&str, PanicAt) + Send + Sync>;

/// A place in the ending of a run at which a panic of the executor's own code is told
/// apart (plan m6, task S1, item 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicAt {
    /// The action has come back and its outcome is made; nothing of the run's end is
    /// visible.
    BeforeTheEnd,
    /// The run's end is visible, and was not confirmed durable.
    Visible,
    /// The run's end is durable; its parent and its waiters are not told yet.
    Durable,
}

/// A run's end once it is durable: how it finished, its status, and when it started and
/// ended, which is what is left to tell its parent.
type Durable = (Finished, Status, (u64, u64));

/// How far the ending of a run has got, which a catch of a panic goes by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reached {
    Running,
    Visible,
    Durable,
}

/// What a run's end is made of, once its action has come back.
struct Pre {
    facts: RunFacts,
    outcome: Outcome,
    outputs: Vec<(Committed, Artifact)>,
    verdict: Option<Verdict>,
    measurements: Option<serde_json::Value>,
    findings: Option<Findings>,
}

impl Pre {
    /// The end of a run whose executor's own code panicked before its end was visible:
    /// an error of the kind `panicked`, with nothing committed.
    fn panicked(live: &Arc<LiveRun>, secrets: &Secrets, text: &str) -> Pre {
        let refusing = live.clone();
        Pre {
            facts: RunFacts::of_a_run(
                secrets.clone(),
                Box::new(move |refused| refusing.nest().refused(refused)),
            ),
            outcome: Outcome::Error {
                kind: PANICKED_KIND.to_owned(),
                message: said(secrets, text),
            },
            outputs: Vec::new(),
            verdict: None,
            measurements: None,
            findings: None,
        }
    }
}

/// The kind of the error a run ends with when the executor's own code panicked before
/// its end was visible (plan m6, task S1, item 2).
pub const PANICKED_KIND: &str = "panicked";

/// How many runs an executor starts and has live at once, each on a thread of its own,
/// unless it is told otherwise ([`Engine::with_started_bound`]).
pub const STARTED_RUNS_DEFAULT: usize = 64;

/// The most an executor can be told to start and have live at once. A thread is not
/// free, and a process that makes them without end takes the machine with it.
pub const STARTED_RUNS_CEILING: usize = 256;

/// A bound on started runs that an executor does not take: it is 1 to
/// [`STARTED_RUNS_CEILING`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundRefused {
    pub asked: usize,
}

impl std::fmt::Display for BoundRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "an executor starts at least 1 run and at most {STARTED_RUNS_CEILING} at once: \
             {} is not a bound it takes",
            self.asked
        )
    }
}

impl std::error::Error for BoundRefused {}

/// What makes the thread a started run's action runs on: it is given the thread's name
/// and what the thread does, and says whether the thread was made. For tests, which
/// have it fail ([`Engine::with_spawn`]); an executor makes real threads.
#[doc(hidden)]
pub type Spawn = fn(String, Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<()>;

/// A thread of the system, with this name. This is the one place the product makes a
/// thread, and how many are live is bounded where it is called
/// ([`Engine::with_started_bound`]): hence the lint that is allowed here (`clippy.toml`).
#[allow(clippy::disallowed_methods)]
fn spawn_thread(name: String, work: Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<()> {
    std::thread::Builder::new().name(name).spawn(work).map(drop)
}

/// The version of whizbang a record is written with: this crate's, which the workspace
/// shares.
pub const WHIZBANG_VERSION: &str = env!("CARGO_PKG_VERSION");

impl Engine {
    /// An executor over these actions that reads this store, under this config, which
    /// is asked for a snapshot each time a call is prepared, at admission too (ADR 0015,
    /// "One snapshot of the config"). Its backends are those of each snapshot, made by
    /// the walk of the config that also resolves a call's models, so a model a call
    /// resolved is held or the call is refused at admission; it is given none apart
    /// from them. Images are decoded within the default limits. It runs nothing: it was
    /// given no way to write.
    pub fn reading(
        registry: Registry,
        contents: Arc<dyn Contents + Send + Sync>,
        config: Arc<dyn ConfigSource>,
    ) -> Engine {
        Engine::built(registry, Arc::new(super::NoBackends), contents, config)
    }

    /// The same executor, which runs actions: it reads this store and writes its runs
    /// to it. The store is open to write, so this process is its one writer for as
    /// long as the executor lives.
    pub fn writing(registry: Registry, store: Arc<Store>, config: Arc<dyn ConfigSource>) -> Engine {
        let mut engine = Engine::reading(registry, store.clone(), config);
        engine.store = Some(store);
        engine
    }

    /// An executor over these actions and backends, reading this store, under the config
    /// with this digest. It runs nothing: it was given no way to write.
    ///
    /// **A test's alone** (the feature `fakes`; plan m4, task K1, item 5): the backends
    /// that resolve a call are given here, apart from those the executor holds
    /// ([`Engine::with_capabilities`]), and nothing holds one to the other, so a call
    /// that resolved a model no held backend declares is admitted and its action finds
    /// no handle. The binary's executor cannot be built so ([`Engine::reading`]).
    #[cfg(feature = "fakes")]
    pub fn new(
        registry: Registry,
        backends: Arc<dyn Backends>,
        contents: Arc<dyn Contents + Send + Sync>,
        config: Digest,
    ) -> Engine {
        Engine::built(registry, backends, contents, Arc::new(config))
    }

    fn built(
        registry: Registry,
        backends: Arc<dyn Backends>,
        contents: Arc<dyn Contents + Send + Sync>,
        config: Arc<dyn ConfigSource>,
    ) -> Engine {
        Engine {
            registry,
            backends,
            contents,
            store: None,
            config,
            limits: DecodeLimits::default(),
            clock: Arc::new(SystemClock),
            secrets: Known::default(),
            ledger: None,
            parts: Vec::new(),
            capabilities: Capabilities::none(),
            ways: Ways::none(),
            places: Places::none(),
            descendants: DESCENDANTS_LIMIT,
            requests: REQUESTS_LIMIT,
            exchanges: EXCHANGES_MAX_BYTES,
            journal: JournalBounds::default(),
            record_max: RECORD_MAX_BYTES,
            before_watch: None,
            started_bound: STARTED_RUNS_DEFAULT,
            spawn: spawn_thread,
            panic_at: None,
        }
    }

    /// An executor that runs actions: it reads this store and writes its runs to it. The
    /// store is open to write, so this process is its one writer for as long as the
    /// executor lives. A test's alone, as [`Engine::new`] is.
    #[cfg(feature = "fakes")]
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

    /// The same executor, under a config that is asked for a snapshot each time a call
    /// is prepared, at admission too, instead of the one digest it was built with (ADR
    /// 0015, "One snapshot of the config"). A run is admitted under one snapshot and
    /// executes under it, with every run it calls. A snapshot that holds backends, a
    /// policy or credentials is used in place of what the executor was built with
    /// (in a test, `Engine::over` and `Engine::with_capabilities`; [`Engine::with_ways`]); the
    /// secrets of every snapshot are kept beside those it was built with.
    pub fn with_config(mut self, config: Arc<dyn ConfigSource>) -> Engine {
        self.config = config;
        self
    }

    /// The same executor, telling the time by this clock.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Engine {
        self.clock = clock;
        self
    }

    /// The same executor, taking these secrets out of everything it writes, and with
    /// them those of every snapshot of the config it takes from here on.
    pub fn with_secrets(mut self, secrets: Secrets) -> Engine {
        self.secrets = Known::of(secrets);
        self
    }

    /// The same executor, admitting every run that may cost something with a
    /// reservation that this ledger is asked for (ADR 0007, "The reservation"). A run
    /// that may cost nothing is admitted without asking it, with an allowance of
    /// nothing. Without a ledger nothing is reserved and nothing is counted.
    pub fn with_ledger(mut self, ledger: Arc<dyn Ledger>) -> Engine {
        self.ledger = Some(ledger);
        self
    }

    /// The same executor, with one more part in every admission, asked after those it
    /// already has (ADR 0015, "A part in the admission"). Without one, admission is
    /// as it was.
    pub fn with_part(mut self, part: Arc<dyn AdmissionPart>) -> Engine {
        self.parts.push(part);
        self
    }

    /// The same executor, handing each run's action a handle on each of these
    /// backends, bound to that run's cancellation and deadline. A test's alone, as
    /// [`Engine::new`] is: the binary's executor is handed its backends by the snapshot
    /// a run is admitted under.
    #[cfg(feature = "fakes")]
    pub fn with_capabilities(mut self, capabilities: Capabilities) -> Engine {
        self.capabilities = capabilities;
        self
    }

    /// The same executor, whose backends send through these ways out and no other
    /// (ADR 0015, "A policy before a send, and a checked send"): each backend of each
    /// run is handed a sender that holds every address to the recipient the backend
    /// declared, to this policy and to this check, before anything is handed over.
    /// Without this nothing is sent: there is no sender, and the policy and the
    /// check approve nothing.
    pub fn with_ways(mut self, ways: Ways) -> Engine {
        self.ways = ways;
        self
    }

    /// The same executor, with this wrapper in one of the three places between each
    /// handle and its backend, in place of what stood there (ADR 0015, "A wrapper
    /// around each request"). The lease's and the meter's places are empty until
    /// they are given one. The gate's is never empty: with none given, it holds the
    /// gate of the policy the run executes under ([`Engine::with_ways`]), which
    /// approves nothing unless it was told to.
    pub fn with_wrapper(mut self, place: Place, wrapper: Arc<dyn Around>) -> Engine {
        self.places = self.places.with(place, wrapper);
        self
    }

    /// The same executor, starting a run ([`Executor::start`]) only while the store's
    /// writer has fewer than `bound` started runs that are not over
    /// ([`Refusal::TooManyStarted`]). The count is the store's, so every executor over
    /// one store value counts the same runs. A bound is 1 to [`STARTED_RUNS_CEILING`];
    /// without this it is [`STARTED_RUNS_DEFAULT`].
    pub fn with_started_bound(mut self, bound: usize) -> Result<Engine, BoundRefused> {
        if !(1..=STARTED_RUNS_CEILING).contains(&bound) {
            return Err(BoundRefused { asked: bound });
        }
        self.started_bound = bound;
        Ok(self)
    }

    /// For tests: the same executor, making the thread of a started run with this.
    /// It is a hook for a test and nothing else: a function that answers `Ok` and never
    /// runs the work it was given leaves a run that nothing ends, and a `wait` for it
    /// blocked.
    #[doc(hidden)]
    pub fn with_spawn(mut self, spawn: Spawn) -> Engine {
        self.spawn = spawn;
        self
    }

    /// For tests: the same executor, doing this at each place in the ending of a run
    /// where a panic of its own code is told apart ([`PanicAt`]); a hook that panics
    /// there stands for such a panic.
    #[doc(hidden)]
    pub fn with_panic_at(mut self, hook: PanicHook) -> Engine {
        self.panic_at = Some(hook);
        self
    }

    /// What a test does at this place in the ending of a run of this action.
    fn panicking(&self, action: &str, at: PanicAt) {
        if let Some(hook) = &self.panic_at {
            hook(action, at);
        }
    }

    /// For tests: how many callers are waiting, now, for the run to be over.
    #[doc(hidden)]
    pub fn waiting_for(&self, run: &RunId) -> usize {
        let attached = self.store.as_ref().and_then(|s| s.attached(run.as_str()));
        attached.and_then(live::of).map_or(0, |live| live.waiting())
    }

    /// For tests: when this process keeps the run as abandoned, by its marker, whether
    /// it was its ending (`true`) or its admission alone (`false`) that became visible
    /// and was not confirmed (plan m6, task S1, item 3).
    #[doc(hidden)]
    pub fn marked_abandoned(&self, run: &RunId) -> Option<bool> {
        let attached = self.store.as_ref().and_then(|s| s.attached(run.as_str()))?;
        match live::open_of(attached)? {
            live::Open::Marked(marker) => Some(marker.of_an_ending()),
            live::Open::Live(_) => None,
        }
    }

    /// For tests: what holds the run open, now, when this process has it open.
    #[doc(hidden)]
    pub fn held_open(&self, run: &RunId) -> Option<HeldOpen> {
        let attached = self.store.as_ref().and_then(|s| s.attached(run.as_str()));
        attached
            .and_then(live::of)
            .map(|live| live.nest().held_open())
    }

    /// For tests: the same executor, doing this in every wait from inside an action,
    /// after the wait has looked at the waiter's signal and before it looks at the
    /// run: the moment at which a cancellation could be missed.
    #[doc(hidden)]
    pub fn with_pause_in_a_nested_wait(mut self, then: Arc<dyn Fn() + Send + Sync>) -> Engine {
        self.before_watch = Some(then);
        self
    }

    /// For tests: the same executor, with this many runs at most under one run that
    /// nothing called, in place of [`DESCENDANTS_LIMIT`].
    #[doc(hidden)]
    pub fn with_descendants_limit(mut self, limit: usize) -> Engine {
        self.descendants = limit;
        self
    }

    /// For tests: the same executor, whose runs each send this many capability
    /// requests at most, in place of [`REQUESTS_LIMIT`]. It is never more than that.
    #[doc(hidden)]
    pub fn with_requests_limit(mut self, limit: usize) -> Engine {
        self.requests = limit.min(REQUESTS_LIMIT);
        self
    }

    /// For tests: the same executor, whose runs each store this many bytes of
    /// exchanges with a judge at most, in place of [`EXCHANGES_MAX_BYTES`]. It is
    /// never more than that.
    #[doc(hidden)]
    pub fn with_exchanges_budget(mut self, bytes: u64) -> Engine {
        self.exchanges = bytes.min(EXCHANGES_MAX_BYTES);
        self
    }

    /// For tests: the same executor, whose runs each keep in their journals at most
    /// these many progress messages, these many bytes of their text and these many
    /// typed events, in place of [`super::MESSAGES_KEPT_LIMIT`],
    /// [`super::MESSAGES_KEPT_MAX_BYTES`] and [`super::EVENTS_KEPT_LIMIT`]. Each is
    /// never more than that: so a test shows what a run does past its bound without
    /// sending thousands of events.
    #[doc(hidden)]
    pub fn with_journal_bounds(mut self, messages: usize, bytes: usize, events: usize) -> Engine {
        self.journal = JournalBounds::at_most(messages, bytes, events);
        self
    }

    /// For tests: the same executor, which takes a record of more than this many
    /// bytes for one over the limit, in place of [`RECORD_MAX_BYTES`]. It is never
    /// more than that. What a call and a run may hold is bounded where it comes in,
    /// so that no record without outputs is over the real limit; this is how a test
    /// shows what the executor does with one that is.
    #[doc(hidden)]
    pub fn with_record_limit(mut self, bytes: u64) -> Engine {
        self.record_max = bytes.min(RECORD_MAX_BYTES);
        self
    }

    /// For tests: the same executor, reading inputs and holding what is handed back
    /// within these decode limits, each never past the default's (plan m3, task F1).
    #[doc(hidden)]
    pub fn with_decode_limits(mut self, limits: DecodeLimits) -> Engine {
        let most = DecodeLimits::default();
        self.limits = DecodeLimits {
            max_pixels: limits.max_pixels.min(most.max_pixels),
            max_bytes: limits.max_bytes.min(most.max_bytes),
            max_profile_bytes: limits.max_profile_bytes.min(most.max_profile_bytes),
        };
        self
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

/// How many times the publication that ends a run is made again because another
/// publication took the place or the turn its reservation's closing chose, before the
/// store's refusal is handed back as its failure and the run is abandoned, which
/// counts its reservation in full. Each time, the store had changed.
const MAX_SETTLINGS: u32 = 1024;

/// The kind of the error a run ends with when its admission was published and was
/// not confirmed (ADR 0016; ADR 0013, where an error's kind is a word).
pub const NOT_CONFIRMED_KIND: &str = "not_confirmed";

impl Executor for Engine {
    fn prepare(&self, call: &Call) -> Result<PreparedCall, CallError> {
        let taken = self.take()?;
        let prepared = self.prepare_from(call, self.contents.as_ref(), &taken.snapshot)?;
        // A preparation carries the time it was made, beside its fingerprint and not
        // in it (ADR 0015): the clock is read once it is made, and for no refusal.
        Ok(prepared.made_at(self.clock.now_ms()))
    }

    fn submit(&self, call: &Call) -> Result<Finished, CallError> {
        match self.admit(call, None, None, &Bounded::by_nothing(), None) {
            Ok(admitted) => self.execute(admitted),
            Err(Stopped::Refused(error)) => Err(error),
            // Only a request with an id makes the name that can be taken.
            Err(Stopped::Taken(error)) => Err(CallError::Store(StoreFailure::new(error))),
        }
    }

    fn on_threads(&self) -> usize {
        // A started run holds its place until its thread has let go of its clone of
        // the executor (`Engine::execute_and_let_go`), which is the last thing it does.
        self.store
            .as_ref()
            .map_or(0, |store| store.started_places())
    }

    fn request_id(&self, text: &str) -> Result<RequestId, CallError> {
        // Before the text is parsed: a refusal of text that is no id shows the text.
        if self.secrets.now().redact(text) != text {
            return Err(Refusal::SecretInRequestId.into());
        }
        // The ids of a lab's cells are the harness's alone (plan m3, task L2).
        if text.starts_with(super::RESERVED_REQUEST_PREFIX) {
            let why = format!(
                "an id that starts {} is reserved for the cells of a lab",
                super::RESERVED_REQUEST_PREFIX
            );
            return Err(Refusal::NotARequestId { why }.into());
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
        // A place among the started runs is taken first, before anything is prepared
        // or admitted (ADR 0008, "How many runs are started at once"): so a request
        // over the bound is refused here, with no run, nothing reserved and nothing
        // written. An executor that only reads its store starts nothing, and takes
        // no place to say so.
        let place = match &self.store {
            Some(store) => match store.start_place(self.started_bound) {
                Some(place) => Some(place),
                None => return self.over_the_bound(request),
            },
            None => None,
        };
        let admitted = match self.begin(request)? {
            Ok(admitted) => admitted,
            Err(answer) => return Ok(answer),
        };
        self.launch(admitted, place)
    }

    fn look_up(&self, id: &RequestId, request: &Request) -> Result<LookedUp, CallError> {
        if self.secrets.now().redact(id.as_str()) != id.as_str() {
            return Err(Refusal::SecretInRequestId.into());
        }
        match self.find_request(id, request.fingerprint())? {
            Found::Nothing => Ok(LookedUp::Missing),
            Found::Same(earlier) => Ok(LookedUp::Repeat(earlier)),
            // As in `begin`: the id is looked at once more before an answer names it.
            Found::Other { .. } if self.secrets.now().redact(id.as_str()) != id.as_str() => {
                Err(Refusal::SecretInRequestId.into())
            }
            Found::Other { run } => Ok(LookedUp::Conflict { run }),
        }
    }

    fn status(&self, run: &RunId) -> Result<Watched, CallError> {
        self.watch(run, false, None, 0)
    }

    fn status_after(&self, run: &RunId, seen: u64) -> Result<Watched, CallError> {
        self.watch(run, false, None, seen)
    }

    fn wait(&self, run: &RunId) -> Result<Watched, CallError> {
        self.watch(run, true, None, 0)
    }

    fn cancel(&self, run: &RunId) -> Result<Cancel, CallError> {
        let unknown = || CallError::Refused(Refusal::NoSuchRun { run: run.clone() });
        let not = |state| Ok(Cancel::NotAccepted(state));
        // The table of open runs is asked first, and not the store (ADR 0008,
        // "Cancellation"). A run this process made is there from before it is visible
        // until the executor is done with it, which is after its end's last sync. So
        // a run found there, with an executor's state, is cancelled or not under its
        // own lock, and that is the decision: no look at the store is, and an end
        // that is visible and not yet confirmed is waited for, not reported. The
        // table's lock is let go before the run's is taken.
        let attached = self.store.as_ref().and_then(|s| s.attached(run.as_str()));
        let mut open = RunState::NotEnded { here: false };
        if let Some(attached) = attached {
            open = RunState::NotEnded { here: true };
            if let Some(live) = live::open_of(attached) {
                if live.cancel() == Told::Accepted {
                    return Ok(Cancel::Accepted);
                }
                // Too late: the run's end had begun, and the executor is done with it
                // now. An end it did not confirm is not said to be one, whatever the
                // store shows: the run was abandoned, with the store's failure.
                if let Some(failure) = live.abandoned() {
                    // Unless the run was deleted meanwhile: then there is no run.
                    return match self.seen(run) {
                        Ok(Seen::Nothing) => Err(unknown()),
                        _ => not(RunState::Abandoned { failure }),
                    };
                }
            }
        }
        // Ended and confirmed, made by other code of this process, or not in the
        // table: the store says. A run it holds open that is not in the table was
        // made by a process that is gone.
        match self.seen(run)? {
            Seen::Nothing => Err(unknown()),
            Seen::Ended(outcome) => not(RunState::Ended(outcome)),
            Seen::Open => not(open),
        }
    }
}

impl Engine {
    /// The answer to a request to start a run when the started runs are at the bound.
    /// A request that was sent before starts nothing, and is answered as it always
    /// is, with the run it made; any other is refused.
    fn over_the_bound(&self, request: &Request) -> Result<Answer, CallError> {
        let refused = Refusal::TooManyStarted {
            bound: self.started_bound,
        };
        let Some(id) = request.id() else {
            return Err(refused.into());
        };
        if self.secrets.now().redact(id.as_str()) != id.as_str() {
            return Err(Refusal::SecretInRequestId.into());
        }
        match self.find_request(id, request.fingerprint())? {
            Found::Nothing => Err(refused.into()),
            Found::Same(earlier) => Ok(Answer::Repeat(earlier)),
            // As in `begin`: the id is looked at once more before an answer names it.
            Found::Other { .. } if self.secrets.now().redact(id.as_str()) != id.as_str() => {
                Err(Refusal::SecretInRequestId.into())
            }
            Found::Other { run } => Err(CallError::Refused(Refusal::Conflict {
                request: id.clone(),
                run,
            })),
        }
    }

    /// Everything before a run's action: the look for a request's id, the preparation
    /// and the admission. What comes back is the run that was admitted, to be executed,
    /// or the answer to a request that makes no run: a repeat.
    fn begin(&self, request: &Request) -> Result<Result<Admitted, Answer>, CallError> {
        let bounded = Bounded::by(request);
        // A request marked as running only under a preparation, that came with none,
        // is stale: refused before anything is prepared, admitted, reserved or sent
        // (plan m5, task K1, item 5). A request with an id is looked up first.
        let unprepared = || {
            let action = request.call().action().to_owned();
            let refused = Refusal::Stale {
                action,
                why: Stale::Missing,
            };
            (request.needs_preparation() && request.expected().is_none()).then_some(refused)
        };
        let Some(id) = request.id() else {
            if let Some(refused) = unprepared() {
                return Err(refused.into());
            }
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
        if self.secrets.now().redact(id.as_str()) != id.as_str() {
            return Err(Refusal::SecretInRequestId.into());
        }
        // The id is looked up here, before anything else: a repeated id answers with the
        // run it already has, and neither readiness nor preparation is consulted
        // (`SPEC.md` section 3, "Request identity").
        let fingerprint = request.fingerprint();
        let answer = |found: Found| match found {
            Found::Nothing => None,
            Found::Same(earlier) => Some(Ok(Err(Answer::Repeat(earlier)))),
            // The answer to a conflict names the id. A snapshot taken since the id
            // was looked at, by this admission or by another call, may have brought
            // a key the id holds (ADR 0015): it is looked for before the id is given.
            Found::Other { .. } if self.secrets.now().redact(id.as_str()) != id.as_str() => {
                Some(Err(Refusal::SecretInRequestId.into()))
            }
            Found::Other { run } => Some(Err(CallError::Refused(Refusal::Conflict {
                request: id.clone(),
                run,
            }))),
        };
        if let Some(answered) = answer(self.find_request(id, fingerprint)?) {
            return answered;
        }
        if let Some(refused) = unprepared() {
            return Err(refused.into());
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
                answer(self.find_request(id, fingerprint)?).unwrap_or(Err(error))
            }
            Err(Stopped::Refused(error)) => Err(error),
            // Another request was admitted under the id between the look and the
            // admission, and the store refused this one. Nothing of this request was
            // published and its action did not run: it is answered as the look would
            // have answered it a moment later.
            Err(Stopped::Taken(error)) => answer(self.find_request(id, fingerprint)?)
                .unwrap_or_else(|| Err(CallError::Store(StoreFailure::new(error)))),
        }
    }
}

impl Engine {
    /// A place among the started runs of the store's writer, for a run that is to be
    /// started, or the refusal when as many as this executor starts are live. An
    /// executor that only reads its store takes none.
    pub(super) fn start_place(&self) -> Result<Option<store::Place>, Refusal> {
        let Some(store) = &self.store else {
            return Ok(None);
        };
        let place = store.start_place(self.started_bound);
        place.map(Some).ok_or(Refusal::TooManyStarted {
            bound: self.started_bound,
        })
    }

    /// Starts a run that was admitted, on a thread of its own, holding this place
    /// among the started runs: for a request from outside and for a call from inside
    /// an action alike (ADR 0008, "How many runs are started at once").
    pub(super) fn launch(
        &self,
        mut admitted: Admitted,
        place: Option<store::Place>,
    ) -> Result<Answer, CallError> {
        // The run holds the place from here, and gives it back when the executor is
        // done with it, whatever happened to it: it is let go with the run.
        if let Some(over) = admitted.over.as_mut() {
            over.place = place;
        }
        let run = admitted.run.clone();
        // The action runs on a thread of its own, which holds what it needs of this
        // executor, the store among it, and lets all of it go before anyone who waits
        // for the run is answered (`Engine::execute_and_let_go`). How the run ended
        // is in the store, and is what whoever asks after the run is told; nobody is
        // handed this result.
        // The thread takes the run out of a cell that this caller holds too: a thread
        // that could not be made never ran what it was given, and the run is still
        // there to be ended, here.
        let cell = Arc::new(Mutex::new(Some(admitted)));
        let work = {
            let (engine, cell) = (self.clone(), cell.clone());
            move || {
                let admitted = cell.lock().unwrap_or_else(PoisonError::into_inner).take();
                if let Some(admitted) = admitted {
                    engine.execute_and_let_go(admitted);
                }
            }
        };
        let name = format!("run {run}");
        let Err(error) = (self.spawn)(name, Box::new(work)) else {
            return Ok(Answer::Started(run));
        };
        // No thread: the system has none to give. The run was admitted, durably, and
        // its action will never start. It is ended here, as an error that says so, by
        // the publication that ends any run: a definite end, in the store, for
        // whoever asks after the run, in this process or the next. Only when that
        // end cannot be written is the run abandoned, by the guard it carries (ADR
        // 0008, "A run with nothing behind it"). Either way nothing waits for it, and
        // the caller is told what happened, which is not a judgement of the call.
        let why = said(&self.secrets.now(), &error.to_string());
        let admitted = cell.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(admitted) = admitted {
            let message = format!(
                "the run was admitted and could not be started: no thread could be made \
                 for its action: {why}"
            );
            drop(self.end(admitted, Some(("start", &message))));
        }
        Err(CallError::NotStarted { run, why })
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
    /// What the run executes under (ADR 0015, "One snapshot of the config").
    under: Held,
    /// Says that the executor is done with the run, when this is dropped. Executing
    /// the run takes it out, to drop it after everything else the run held.
    over: Option<Over>,
}

/// What a run was admitted under and executes under, whatever the config's source
/// answers afterwards: the snapshot of the config, what was made of it for this run
/// at its admission, and the secrets known then. It is kept in memory with the run
/// and nowhere else.
struct Held {
    snapshot: Snapshot,
    /// The executor's secrets as of the admission, the snapshot's among them: what
    /// is taken out of everything the run writes.
    secrets: Secrets,
    /// The backends the snapshot made that the call resolved, by name; none for a
    /// snapshot that holds no backends, whose run is handed those the executor was
    /// built with.
    handed: Option<Capabilities>,
    /// The settings the run records.
    settings: Vec<(String, Value)>,
}

/// A snapshot the executor has taken, with every secret it knows from then on.
struct Taken {
    snapshot: Snapshot,
    secrets: Secrets,
}

/// What bounds a request from outside: the signal it is cancelled by, and its deadline
/// when it has one.
pub(super) struct Bounded {
    pub(super) signal: Cancellation,
    pub(super) deadline: Option<Instant>,
    /// The cap the request came with, which the executor shows to its parts and
    /// holds no call to (ADR 0015).
    pub(super) cap: Option<Estimate>,
}

impl Bounded {
    /// The signal and the deadline the request came with. A request that came with no
    /// signal is given one, which is its run's from the admission on.
    fn by(request: &Request) -> Bounded {
        Bounded {
            signal: request.cancellation().cloned().unwrap_or_default(),
            deadline: request.deadline(),
            cap: request.cap(),
        }
    }

    fn by_nothing() -> Bounded {
        Bounded {
            signal: Cancellation::new(),
            deadline: None,
            cap: None,
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
        let scoped = self.added.scoped(self.holding.generation().clone());
        self.added.onto(scoped, &self.store)
    }

    fn read(&self, entry: &store::Entry) -> store::Result<Vec<u8>> {
        self.store.read_output(entry)
    }

    fn read_record(&self, entry: &store::Entry) -> store::Result<Vec<u8>> {
        self.store.read(entry)
    }
}

/// What the executor is done with when it is done with a run, in the order it lets go:
/// the run's place among the started runs, when it has one; then the run's entry in the
/// store's table of open runs, when its end became visible and it was kept there for
/// this; and only then are those who wait for the run told that nothing in this process
/// works on it any more. So a caller that waited for a run finds its place free (ADR
/// 0008, "How many runs are started at once").
///
/// It is made with the run's live state, before the admission is published: an
/// admission that is given up, or that became visible and was not confirmed, drops it,
/// so nobody waits for, or is told they cancelled, a run whose action will never start.
/// However it is dropped, by a return or by a panic that unwinds, the run is over for
/// this process: ended, when the executor said so first, and otherwise abandoned, with
/// the store's failure it was told of (ADR 0008, "A run with nothing behind it").
struct Over {
    /// The run's place among the started runs, when it was started.
    place: Option<store::Place>,
    /// Once the run is admitted: what lets it go from the store's table of open runs.
    /// It holds the table and not the store's writer. A wait answers once the run has
    /// left the table, and whoever waited may then drop its writer and open the store
    /// again: nothing here may hold the writer, and its lock, at that moment (ADR
    /// 0015, "One way to a writer, with recovery before it", limits).
    kept: Option<store::Closer>,
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

impl Over {
    /// How the run's end went, for what this says when it is dropped: ended, when the
    /// end was published and confirmed; abandoned otherwise, with the store's failure
    /// when there was one.
    fn told_of(&mut self, ended: &Result<Finished, CallError>) {
        match ended {
            Ok(_) => self.ended = true,
            Err(CallError::Store(failure)) => self.failure = Some(failure.clone()),
            Err(_) => {}
        }
    }
}

impl Drop for Over {
    fn drop(&mut self) {
        drop(self.place.take());
        if let Some(parent) = self.started_under.take() {
            parent.nest().started_over();
        }
        // Only a run whose end was confirmed is let go of. One whose end is visible
        // and was not confirmed stays in the table, abandoned: a cancel or a wait
        // that begins later finds it there, and does not say it ended (ADR 0008,
        // "Visible before durable").
        if let (true, Some(kept)) = (self.ended, &self.kept) {
            kept.let_go();
        }
        let failure = self.failure.take();
        self.live.over(self.ended, failure.clone());
        // Abandoned: a marker takes the place of its live state in the table, so that
        // what a long-lived process keeps of such runs is small (plan m6, task S1, item
        // 3). Whoever waits holds the live state until it is told, which it is above.
        if let (false, Some(kept)) = (self.ended, &self.kept) {
            kept.abandon(|ending| live::marker(failure, ending));
        }
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
    fn find_request(&self, id: &RequestId, fingerprint: Digest) -> Result<Found, CallError> {
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
                live::open_of(attached?)?.abandoned()
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
        // The one snapshot of the config this admission is made under (ADR 0015): the
        // call is prepared under it, every part is shown it, and the run executes
        // under it. It is taken before anything is held, so a source that refuses, or
        // panics, leaves nothing behind. Its secrets are known from here on.
        let Taken { snapshot, secrets } = self.take()?;
        // The id was looked at for a secret before this snapshot was taken: a key the
        // snapshot brought is looked for now, before the id is written anywhere.
        if asked.is_some_and(|(id, _)| secrets.redact(id.as_str()) != id.as_str()) {
            return Err(CallError::Refused(Refusal::SecretInRequestId).into());
        }
        let added = Arc::new(Added::default());
        // A run under another reads what the runs of its tree committed for them: the
        // names of the first run of the tree (plan m3, task Q1, item 3). A run nothing
        // called is that first run, and says so once it is named.
        if let Some(first) = under.and_then(|under| under.above.first()) {
            added.under_first(first.as_str());
        }
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
            Some(view) => self.prepare_from(call, view.as_ref(), &snapshot),
            None => self.prepare_from(call, self.contents.as_ref(), &snapshot),
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
        // The run holds each input it takes that is a name outside runs, as it holds
        // each output it commits: a deletion of another run then leaves it (ADR 0008,
        // "What deleting a run removes"). An input that only the first run of this
        // tree holds for its calls is that run's, and goes with it.
        let holds: Vec<(String, store::Entry)> = (prepared.inputs().iter())
            .filter_map(|input| {
                let name = artifact::name(input.artifact(), input.id());
                let entry = view.holding.generation().get(&name)?;
                (entry.kind == store::Kind::Output).then(|| (name, entry.clone()))
            })
            .collect();
        // What the run will execute under is made of the snapshot here, before
        // anything is taken or staged for the call (ADR 0015): the backends it is
        // handed, by the name its call resolved, and the settings it records. A
        // snapshot that resolved a model it holds no backend for is one no run is
        // admitted under, and settings a record would not take are not left out: the
        // first is a refusal, the second a failure, and neither makes a run.
        let handed = snapshot.handed(&prepared).map_err(|missing| {
            CallError::Refused(Refusal::NotReady {
                action: prepared.action().to_owned(),
                missing,
            })
        })?;
        let settings = snapshot.settings(&prepared, &secrets).map_err(|failure| {
            failure.into_error(
                |why| part::sentence(&secrets, why),
                |why| CallError::Config { why },
            )
        })?;

        // Admission: one publication makes the run, with its admission among its
        // names, and, for a request with an id, makes the id's record outside runs.
        // The store decides both inside its publishing step: a run of that id is named
        // again, and an id that was admitted meanwhile stops this request with nothing
        // published. From the publication on there is a run, durably, and whatever
        // happens to its action it is in the store.
        let start_ms = self.clock.now_ms();
        // The preparation made here was made at the time of the admission: the one
        // reading of the clock an admission makes (ADR 0015).
        let prepared = prepared.made_at(start_ms);
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
        // A call that may cost nothing is admitted without asking the ledger, whatever
        // the books say: nothing is reserved for it, so no name of the ledger's is
        // made or read at its admission, and none at its end (`SPEC.md` section 6:
        // spend control errs toward letting work run). It is counted all the same,
        // with nothing to spend: its run has an allowance of nothing, which a paid
        // request or a paid call from inside it is refused against.
        let booked = reserved.filter(|micros| *micros > 0);
        let mut taken = match (under, reserved) {
            (Some(under), Some(micros)) => under.allowance.take(micros).map_err(|left_micros| {
                CallError::Refused(Refusal::OverCaller {
                    action: prepared.action().to_owned(),
                    asked_micros: micros,
                    left_micros,
                })
            })?,
            _ => None,
        };
        // The allowance of a run nothing called, and what this process holds of its
        // tree's reservation, are made only once its admission is confirmed (ADR
        // 0016): until then it has none. A run another run called has its share.
        let share = reserved.filter(|_| under.is_some());
        let mut family = Family::of(under, prepared.action(), share, self.descendants);
        let parent = family.parent.as_ref().map(|(run, _)| run.clone());
        // The reservation the ledger was asked for in the attempt that is published.
        let mut reserved_as: Option<Reservation> = None;
        // What the run keeps while it executes is made before the run is, and attached
        // to it in the store's table of open runs by the publication that makes it: so
        // whoever sees the run in a snapshot finds it there (ADR 0008, "Status").
        let kept = Arc::new(Kept::new(self.clock.clone(), secrets.clone()).bounded(self.journal));
        let live = Arc::new(LiveRun::new(kept.clone(), bounded.signal.clone(), added));
        let mut over = Over {
            place: None,
            kept: None,
            live: live.clone(),
            ended: false,
            failure: None,
            started_under: None,
        };
        let mut run = self.name_run(store, start_ms)?;
        let mut namings = 1;
        // An admission of a run nothing called that became visible and was not
        // confirmed durable: the store's failure, and the run, which is ended below.
        let mut unacknowledged: Option<StoreFailure> = None;
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
            // The executor's parts are asked first, in the order they were given, for
            // every attempt: each may refuse the call, and may add records among the
            // names of the run this attempt makes (ADR 0015). A refusal or a failure
            // drops the publication with all it staged, before the ledger is asked.
            let asking = Asking {
                call,
                id: request,
                expected,
                cap: bounded.cap,
                prepared: &prepared,
                now_ms: start_ms,
                parent: parent.as_ref(),
                snapshot: &snapshot,
            };
            let mut added = BTreeSet::new();
            for part in &self.parts {
                let name = part.name();
                let mut step = PartStep::new(&mut publication, &run, &secrets, name, &mut added);
                match part.look(&asking, &mut step) {
                    Ok(()) => {}
                    Err(PartDeclined::Refused(refusal)) => {
                        return Err(CallError::Refused(Refusal::ByPart {
                            action: prepared.action().to_owned(),
                            part: name,
                            kind: refusal.kind(),
                            why: part::sentence(&secrets, refusal.why()),
                        })
                        .into());
                    }
                    // A failure is the store's, which is as it was, or a sentence,
                    // which is cleaned as a refusal's is: a record's, or the part's,
                    // named by the part that was asked. It is never a refusal, whatever the part handed
                    // back, so it gets no second look for the id.
                    Err(PartDeclined::Failed(failure)) => {
                        let failed = failure.into_error(
                            |why| part::sentence(&secrets, why),
                            |why| CallError::Part { part: name, why },
                        );
                        return Err(failed.into());
                    }
                }
            }
            // The ledger takes part here: it judges, and adds what it must to this
            // publication, before anything is published. Declined, the publication is
            // dropped with all it staged.
            let mut reservation_name = None;
            let mut ledgers = Vec::new();
            // The ledger is asked for a run that nothing called, and for no other:
            // nothing is reserved twice (`SPEC.md` section 6).
            let ledger = self.ledger.as_ref().filter(|_| under.is_none());
            if let (Some(ledger), Some(micros)) = (ledger, booked) {
                let reservation = Reservation::new(
                    run.clone(),
                    request.cloned(),
                    Digest::of_bytes(&admission),
                    micros,
                    start_ms,
                );
                let generation = store.snapshot().map_err(failed)?;
                // Read through the store, as the confirmation and the run's end are.
                let mut step = Step::new(&mut publication, &generation, store.as_ref(), &secrets);
                match ledger.reserve(&reservation, &mut step) {
                    // What a ledger hands back is a name it made, in this step: one
                    // it kept from another attempt is published by nothing.
                    Ok(made) if step.made_here(&made) => {}
                    Ok(_) => {
                        return Err(CallError::Ledger {
                            why: "it handed back a name that it did not make in this \
                                  attempt at the admission: a ledger returns from \
                                  reserving with a name the attempt publishes"
                                .to_owned(),
                        }
                        .into());
                    }
                    Err(Declined::OverAllowance { left_micros }) => {
                        return Err(CallError::Refused(Refusal::OverAllowance {
                            action: prepared.action().to_owned(),
                            asked_micros: micros,
                            left_micros,
                        })
                        .into());
                    }
                    Err(Declined::NeedsOwner { epoch }) => {
                        return Err(CallError::Refused(Refusal::NeedsOwner {
                            action: prepared.action().to_owned(),
                            epoch,
                        })
                        .into());
                    }
                    // As a part's failure: the store's as it was, or a sentence
                    // that is cleaned, and never a refusal.
                    Err(Declined::Failed(failure)) => {
                        let failed = failure.into_error(
                            |why| part::sentence(&secrets, why),
                            |why| CallError::Ledger { why },
                        );
                        return Err(failed.into());
                    }
                }
                ledgers = step.into_made();
                let name = reservation.name();
                publication
                    .create(&name, RECORD_KIND, &reservation.to_bytes())
                    .map_err(failed)?;
                reservation_name = Some(name);
                reserved_as = Some(reservation);
            }
            publication.create_run(run.as_str()).map_err(failed)?;
            publication.attach(run.as_str(), live.clone());
            // A run that is deleted while it executes is told to stop: its end will
            // find no run, so nothing it goes on to do can be kept.
            publication.tell_when_deleted(run.as_str(), {
                let live = live.clone();
                Arc::new(move || live.stop())
            });
            publication
                .put_in(run.as_str(), ADMITTED_NAME, RECORD_KIND, &admission)
                .map_err(failed)?;
            for (name, entry) in &holds {
                publication
                    .hold_in(run.as_str(), name, entry.clone())
                    .map_err(failed)?;
            }
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
                // An admission that became visible and was not confirmed, of a run
                // nothing called: its run is in the store, and its action is never
                // run. It is ended at once, as an error, by this executor, so that
                // its request id answers a run that ended (plan m5, task K1, item 6).
                Err(error @ store::Error::Unacknowledged { .. }) if under.is_none() => {
                    unacknowledged = Some(StoreFailure::new(error));
                    break;
                }
                // Among these is an admission that became visible and was not
                // confirmed, of a run another called: it is in the store and open
                // here, and is abandoned.
                Err(error) => {
                    // The admission is in the store, naming the run it drew on and
                    // the amount: what was taken for it stays taken, or the
                    // admissions that name a run could add up to more than it has.
                    if let (store::Error::Unacknowledged { .. }, Some(taken)) =
                        (&error, taken.take())
                    {
                        taken.keep();
                    }
                    let failure = StoreFailure::new(error);
                    over.failure = Some(failure.clone());
                    return Err(CallError::Store(failure).into());
                }
            }
        }

        // A run that nothing called is the first of its tree.
        if under.is_none() {
            view.added.under_first(run.as_str());
        }
        // The run is made: what was taken from its caller's allowance stays taken.
        if let Some(taken) = taken {
            taken.keep();
        }
        // From here the run is in the store's table under this id: what its guard lets
        // go of, when its end was visible and it was kept there.
        over.kept = Some(store.closer(run.as_str()));
        // The confirmation (ADR 0016, "An admission is confirmed before anything runs
        // under it"). The publication admitted nothing by itself: the store is read
        // again, as it is now that the admission is durable, and the ledger answers
        // whether the admission stands on that reading. Nothing is done under the
        // reservation before: no thread, no allowance, no tally of its tree, no
        // action. A reading that fails is no answer, and is taken as one that says no.
        let ledger = self.ledger.as_ref().filter(|_| under.is_none());
        let stands = match (ledger, &reserved_as) {
            // Not confirmed durable: nothing stands on it, and the books are not read.
            _ if unacknowledged.is_some() => Ok(false),
            (Some(ledger), Some(reservation)) => match store.snapshot() {
                Ok(generation) => {
                    let reread = Reread::new(&generation, store.as_ref());
                    ledger.confirm(reservation, &reread).map_err(|failure| {
                        failure.into_error(
                            |why| part::sentence(&secrets, why),
                            |why| CallError::Ledger { why },
                        )
                    })
                }
                Err(error) => Err(failed(error)),
            },
            _ => Ok(true),
        };
        if let (Ok(true), None, Some(micros)) = (&stands, under, reserved) {
            family.allowance = Allowance::of(micros);
        }
        let admitted = Admitted {
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
            under: Held {
                snapshot,
                secrets,
                handed,
                settings,
            },
            over: Some(over),
        };
        if let Some(failure) = unacknowledged {
            // Ended by the publication that ends any run, which closes its reservation
            // and makes its request id's ending. Where that fails too, the run is
            // abandoned as before, and the next recovery ends it. Either way the
            // caller is told of the store's failure.
            let message = format!(
                "the run was admitted and its admission was not confirmed durable, so its \
                 action was not run: {failure}"
            );
            drop(self.end(admitted, Some((NOT_CONFIRMED_KIND, &message))));
            return Err(CallError::Store(failure).into());
        }
        let (why, unread) = match stands {
            Ok(true) => return Ok(admitted),
            Ok(false) => (
                "on the books as they were read after its admission was published, the \
                 admission does not stand"
                    .to_owned(),
                None,
            ),
            Err(error) => (
                format!("the books could not be read after its admission was published: {error}"),
                Some(error),
            ),
        };
        // It does not stand: the run is ended at once, having run nothing, by the
        // publication that ends a run, which closes its reservation and makes its
        // request id's ending. Where that publication fails the run stays admitted,
        // is never started, and is abandoned here with the failure; the next
        // recovery ends it. Either way the caller is told, with a reason of its own,
        // which is no refusal of the call: a run was made. One case is told as what
        // it is: where the books could not be read and the run could not be ended
        // either, the caller is handed that failure, the store's or the ledger's.
        let run = admitted.run.clone();
        let action = admitted.prepared.action().to_owned();
        let message = format!(
            "the run was admitted and its admission was not confirmed, so its action \
             was not run: {why}"
        );
        let ended = self
            .end(admitted, Some((NOT_CONFIRMED_KIND, &message)))
            .is_ok();
        match unread {
            Some(error) if !ended => Err(error.into()),
            _ => Err(CallError::NotConfirmed { run, action, ended }.into()),
        }
    }

    /// Asks the config's source for a snapshot, and takes its secrets in with those
    /// the executor knows (ADR 0015, "One snapshot of the config"). A source that
    /// refuses, and secrets that would be more than the executor keeps, are a
    /// failure and no judgement of a call: nothing was looked at under a config. No
    /// lock of the executor's is held while the source is asked.
    fn take(&self) -> Result<Taken, CallError> {
        let snapshot = self
            .config
            .snapshot()
            .map_err(|refused| CallError::Config {
                why: refused.said(&self.secrets.now()),
            })?;
        let Some(secrets) = self.secrets.with(snapshot.secrets()) else {
            return Err(CallError::Config {
                why: format!(
                    "the config adds secrets that would make more than the {} an executor \
                     keeps over every config it has read, and none is let go",
                    super::SECRETS_KEPT_LIMIT
                ),
            });
        };
        // A recipient whose own origin is a secret could never be sent to: said
        // here, once, and not by every send of every run. The secrets are known
        // all the same.
        if snapshot.names_a_secret_origin(&secrets) {
            return Err(CallError::Config {
                why: super::SECRET_ORIGIN_APPROVED.to_owned(),
            });
        }
        Ok(Taken { snapshot, secrets })
    }

    /// This executor under one snapshot for good: what the executor in a run's
    /// context is, so that a call from inside an action is prepared and admitted
    /// under the snapshot of the run that made it, and the source is not asked.
    fn under(&self, snapshot: &Snapshot) -> Engine {
        let mut engine = self.clone();
        engine.config = Arc::new(snapshot.clone());
        engine
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

    /// Prepares a call against this read of the store, under this snapshot of the
    /// config: its digest, and its backends when it holds some, asked as they answer
    /// prepare, so that one that is not ready is answered with its reason.
    fn prepare_from(
        &self,
        call: &Call,
        contents: &(dyn Contents + Send + Sync),
        snapshot: &Snapshot,
    ) -> Result<PreparedCall, CallError> {
        let Some(action) = self.registry.get(call.action()) else {
            return Err(CallError::Refused(Refusal::UnknownAction {
                name: call.action().to_owned(),
            }));
        };
        prepare(
            action.as_ref(),
            call,
            contents,
            snapshot.backends().unwrap_or(self.backends.as_ref()),
            snapshot.digest(),
            snapshot.cap(),
            &self.limits,
        )
    }

    /// Runs the action of a run that was admitted, and ends the run: everything from
    /// the admission on. Whatever comes back, nothing in this process is working on
    /// the run any more, and whoever waits for it is told so.
    pub(super) fn execute(&self, admitted: Admitted) -> Result<Finished, CallError> {
        self.end(admitted, None)
    }

    /// Ends a run that was admitted: after running its action, or without running it,
    /// as an error of the kind and with the sentence `unstarted` gives: a run that was
    /// to be started and has no thread, and a run whose admission was not confirmed.
    fn end(
        &self,
        mut admitted: Admitted,
        unstarted: Option<(&str, &str)>,
    ) -> Result<Finished, CallError> {
        let mut over = admitted.over.take();
        // Everything the run held is let go in here, its hold on the generation it
        // read among it, before whoever waits for the run is told that it is over.
        // A panic in there unwinds through here, and the run is abandoned.
        let ended = self.run_and_end(admitted, unstarted);
        if let Some(over) = &mut over {
            over.told_of(&ended);
        }
        drop(over);
        ended
    }

    /// Executes a run on the thread that was made for it, and lets go of this clone of
    /// the executor, which only that thread holds, before the executor is done with
    /// the run: before the run's place is given back, before the run leaves the
    /// store's table of open runs, and before whoever waits for it is told. An
    /// executor holds the store's writer, and with it the writer lock. Whoever waited
    /// for the run may drop its own writer and open the store again at once, and
    /// would be refused as held for as long as this thread still had its clone (ADR
    /// 0015, "One way to a writer, with recovery before it", limits).
    fn execute_and_let_go(self, mut admitted: Admitted) {
        // Values are dropped in the reverse of the order they are named in, on a
        // return and in a panic that unwinds alike: the guard is named first, so it
        // goes last, after the executor.
        let mut over = admitted.over.take();
        let executor = self;
        let ended = executor.run_and_end(admitted, None);
        if let Some(over) = &mut over {
            over.told_of(&ended);
        }
        drop(ended);
        drop(executor);
        drop(over);
    }

    fn run_and_end(
        &self,
        admitted: Admitted,
        unstarted: Option<(&str, &str)>,
    ) -> Result<Finished, CallError> {
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
            under: held,
            over: _,
        } = admitted;
        // The run writes without the secrets known at its admission, those of its
        // snapshot among them, from its first note to its record.
        let secrets = &held.secrets;
        let request = request.as_ref();
        let failed = |error| CallError::Store(StoreFailure::new(error));
        // The context is the run's: its signal, its deadline, the generation it was
        // admitted against, and handles that the context binds to that signal and
        // that deadline itself.
        // The executor in it admits a call under this run (ADR 0008, "Nested calls").
        // How far the run's ending has got, kept where a panic in the executor's own code
        // cannot lose it: what a catch below does depends on it (plan m6, task S1, item
        // 2). Both ways of running an admitted run come through here: `end`, which a
        // child called from inside a parent's action takes, and `execute_and_let_go`.
        let reached = Cell::new(Reached::Running);
        let action_name = prepared.action().to_owned();
        // The generation the run was admitted against stays held through its end, as
        // the context held it, whatever is let go before (ADR 0008, "The generation a
        // run reads").
        let _view = view.clone();
        let pre = catch_unwind(AssertUnwindSafe(|| -> Result<Pre, CallError> {
            let offer = Offer::of(live.clone(), secrets.clone(), family.offer.clone());
            let offer = Arc::new(offer);
            let under = Under {
                run: run.clone(),
                live: live.clone(),
                stack: family.stack.clone(),
                above: family.above_with(&run),
                tree: family.tree.clone(),
                allowance: family.allowance.clone(),
                deadline,
                inputs: Arc::new(prepared.inputs().iter().map(|input| input.id()).collect()),
                offer: offer.clone(),
            };
            // The run's place for facts (ADR 0013): what another module notes of the run
            // while it executes. A refusal of a request goes with the run's other
            // refusals, in one order and within one bound.
            let facts = RunFacts::of_a_run(secrets.clone(), {
                let live = live.clone();
                Box::new(move |refused| live.nest().refused(refused))
            });
            // The settings the run ran under, which its snapshot gave at admission and
            // which were held there to what a record takes, with these same secrets. One
            // that is not taken all the same is not left out in silence: the action is
            // not run, and the run ends as an error that says so.
            let recorded = held
                .settings
                .iter()
                .all(|(n, value)| facts.setting(n, value));
            let unrecorded = "a setting the run was admitted with could not be noted for its \
                              record, so its action was not run";
            let unstarted = match (unstarted, recorded) {
                (None, false) => Some(("start", unrecorded)),
                (unstarted, _) => unstarted,
            };
            // The backends of the run's snapshot that its call resolved, and the ways out
            // under that snapshot's policy, with its credentials: those of the admission,
            // whatever the source answers now. A snapshot that holds neither leaves the
            // run what the executor was built with.
            let capabilities = match held.handed {
                Some(handed) => handed,
                None => self.capabilities.resolved_for(&prepared),
            };
            let ways = held.snapshot.ways(&self.ways);
            // What stands between each handle and its backend, for this run: the three
            // places as the executor was given them, with the gate of this run's policy
            // where the gate's place was given nothing. Each wrapper is told the run,
            // the run at the root of its tree, the run's allowance and its place for
            // facts.
            let places = match self.places.at(Place::Gate) {
                Some(_) => self.places.clone(),
                None => self.places.clone().with(Place::Gate, ways.gate()),
            };
            let above = family.above_with(&run);
            let around = Wrappers {
                run: run.clone(),
                root: above.first().cloned().unwrap_or_else(|| run.clone()),
                places,
                allowance: family.allowance.clone(),
                facts: facts.clone(),
            };
            let context = Context::new(
                live.signal().clone(),
                deadline,
                kept.clone(),
                family.allowance.clone(),
                view,
                // The backends the call resolved, and no other (ADR 0009), with the
                // executor's clock for their deadline.
                capabilities
                    .timed_by(self.clock.clone())
                    // And a way out for each, checked, which notes what it hands over in
                    // the run's place for facts (ADR 0015).
                    .sending_through(ways, facts.clone())
                    // And the wrappers, between each handle's tally and its backend.
                    .wrapped(around),
                // The executor in the context is under the run's snapshot: what the run
                // calls is prepared and admitted under it.
                Arc::new(Nested::new(
                    self.under(&held.snapshot),
                    under,
                    secrets.clone(),
                )),
            )
            .with_diagnostics(kept.clone())
            .recording(kept.clone(), self.requests, self.exchanges)
            .noting(facts.clone())
            .offering(offer)
            .placed(above.clone());
            live.nest().watch(context.requests().clone());
            // A panic in the action is caught: its run ends as any other does, with its
            // record and what it wrote, and the executor goes on (ADR 0008). The context
            // and what it shares are not left broken by one: what a run keeps is behind
            // locks that are taken whatever a panic did to them.
            // A run that was cancelled before its action started does not run it. Neither
            // does a run that has no thread to run it on: it is handed the failure.
            // A run whose admission was not confirmed ends as that whether or not it was
            // cancelled meanwhile (ADR 0016; ADR 0013): it never was a run that could
            // execute, and its caller is told so, so its record says the same.
            let not_confirmed = unstarted.is_some_and(|(kind, _)| kind == NOT_CONFIRMED_KIND);
            let handed =
                (not_confirmed || !live.signal().is_cancelled()).then(|| match unstarted {
                    None => catch_unwind(AssertUnwindSafe(|| action.run(&prepared, &context))),
                    Some((kind, why)) => Ok(Err(Failure::Error {
                        kind: kind.to_owned(),
                        message: why.to_owned(),
                    })),
                });
            // The action has come back. No call and no capability request is taken from
            // here on, and the run waits for those there are: every run it called is over,
            // and every request its handles sent is accounted for, before it ends (ADR
            // 0008, "Nested calls"). A cancellation is still accepted while it waits, and
            // reaches them.
            live.nest().settle();
            // A request of the run that could not be accounted for: a wrapper failed
            // once its backend had been called, or its backend panicked under a wrapper
            // (ADR 0015). Whatever the action handed back, the run is not ended: no end
            // is published, its reservation is not closed and nothing of it is
            // released. It is abandoned here, with the failure (ADR 0008, "A run with
            // nothing behind it"), and the next recovery ends it.
            if let Some(why) = context.requests().failed() {
                return Err(CallError::Abandoned {
                    run: run.clone(),
                    why: said(secrets, &why),
                });
            }
            // The entry to the commit step (ADR 0008, "Cancellation"): the action has come
            // back, with whatever it came back with, and nothing it started is left. From
            // here on a cancellation is too late. One that was accepted before this was
            // set under the lock this reads under: the run ends as cancelled, and nothing
            // it handed back is committed.
            let cancelled = live.begin_end();
            // What is given back to the run's allowance from here on is dropped: its
            // reservation is closed by its own settlements (ADR 0016).
            family.allowance.close();
            // The outcome is made here, from what the action handed back, with the secrets
            // taken out: what is reported holds none, as what is written holds none. And
            // each of its texts is cut to a length a record can hold ([`MESSAGE_LIMIT`]).
            let said = |text: &str| said(secrets, text);
            let error = |kind: &str, message: &str| Outcome::Error {
                kind: said(kind),
                message: said(message),
            };
            let mut outputs = Vec::new();
            let mut verdict = None;
            let mut measurements = None;
            let mut findings = None;
            let mut outcome = match handed {
                None => Outcome::Cancelled,
                Some(Ok(Ok(produced))) => {
                    verdict = produced.verdict().cloned();
                    measurements = produced.measurements().cloned();
                    // What is handed back beside the artifacts is held to the kinds the
                    // descriptor declares (ADR 0035), and findings to their schema.
                    let undeclared = held_to_results(action.descriptor(), &produced);
                    let undeclared = undeclared.map(|read| findings = read);
                    let held = undeclared
                        .and_then(|()| {
                            held_to_ports(&prepared, action.descriptor(), produced, secrets)
                        })
                        .and_then(|held| {
                            let store = context.store();
                            super::outputs::held_to_relations(
                                &prepared,
                                action.descriptor(),
                                &held,
                                store,
                                &self.limits,
                                &|id| live.nest().committed(id),
                            )
                            .map(|()| held)
                        });
                    match held {
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
            if cancelled && !not_confirmed {
                outputs.clear();
                outcome = Outcome::Cancelled;
            }
            self.panicking(&action_name, PanicAt::BeforeTheEnd);
            Ok(Pre {
                facts,
                outcome,
                outputs,
                verdict,
                measurements,
                findings,
            })
        }));
        // A panic before the end: the run's descendants are cancelled and waited for, and
        // its requests accounted, as for any failed run; it then ends as an error of the
        // kind `panicked`, with its record, and only then does its parent see it.
        let panicked = |panic: Box<dyn Any + Send>| {
            live.signal().set();
            live.signal().tell();
            live.nest().settle();
            live.begin_end();
            family.allowance.close();
            Pre::panicked(&live, secrets, &panic_text(panic))
        };
        let pre = match pre {
            Ok(Ok(pre)) => pre,
            Ok(Err(error)) => return Err(error),
            Err(panic) => panicked(panic),
        };
        let told = Cell::new(false);
        let visible: RefCell<Option<StoreFailure>> = RefCell::new(None);
        let durable: RefCell<Option<Durable>> = RefCell::new(None);
        let end = |pre: Pre| -> Result<Finished, CallError> {
            let Pre {
                facts,
                mut outcome,
                mut outputs,
                verdict,
                measurements,
                findings,
            } = pre;
            let said = |text: &str| said(secrets, text);
            let end_ms = self.clock.now_ms().max(start_ms);

            // The end: one publication adds the journal, every diagnostic and the record to
            // the run, and, for a request with an id, makes the id's ending outside runs.
            // For a run that completed it is the commit step (ADR 0008): the same
            // publication gives every output its name, so the outputs and the completed
            // status become visible together or not at all, and `outputs` holds something
            // only then. The run must still be there: one that was deleted while it
            // executed is not made again from its last names. The run is reported after
            // this is durable.
            let (journal, written) = kept.take();
            let kept: Vec<(Diagnostic, Vec<u8>)> =
                std::iter::once(Diagnostic::whole_journal(&journal))
                    .chain(written)
                    .collect();
            // What the run's record says of the calls its action made: they are all over.
            let parent = family.parent.as_ref().map(|(run, _)| run.clone());
            // A verdict is written only in the record of a run that completed: the record
            // drops it for any other (ADR 0009).
            // The run's end begins here for what is noted of it too: no note is taken from
            // now on, and what was noted is in the record or was answered no.
            let mut noted = facts.close();
            let mut inside = live.nest().inside(parent);
            // The caller is given the verdict with the outcome, as the record keeps it.
            let handed_verdict =
                (verdict.as_ref()).map(|v| Verdict::new(v.decision(), &said(v.why())));
            inside.verdict = verdict;
            // Findings are data and hold no text: they are kept as they were read.
            inside.findings = findings.clone();
            // A record lists what a request cost only for a request it lists. What was
            // noted for a request the run did not send cannot be written, and leaving it
            // out in silence would be a record that says less was spent than was noted:
            // the run ends as an error that says so, whatever its action handed back, and
            // commits nothing.
            if let Some(request) = noted.unlisted_paid(inside.asked.len()) {
                let (_, all, _) = noted.counts();
                let left_out = noted.drop_unlisted_paid(inside.asked.len());
                outputs.clear();
                inside.verdict = None;
                outcome = Outcome::Error {
                    kind: "record".to_owned(),
                    message: format!(
                        "what a request cost was noted for request {request}, and the run sent \
                         {} capability requests: a record lists what a request cost only for \
                         a request it lists, so {left_out} of the {all} that were noted is not \
                         in this record",
                        inside.asked.len()
                    ),
                };
            }
            let record_of = |outcome: &Outcome, inside: &Inside, noted: &Facts| {
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
                        facts: noted.clone(),
                    },
                    secrets,
                )
            };
            let mut record = record_of(&outcome, &inside, &noted);
            // A record over the limit is refused on reading, so it is not published as it
            // is. What makes it too large is left out in two stages, the outputs first.
            //
            // First (ADR 0005): a run whose outputs make such a record commits none of
            // them. It ends as an error of the type `output`, with a record that lists no
            // output and keeps everything else: what the run sent, what it paid and where
            // its data went are not what was too large, and a run that paid never has a
            // record that says nothing was spent because it handed back too much.
            let is_at_most = format!("a record is at most {} bytes", self.record_max);
            let mut handed_back = None;
            let mut bytes = record.to_bytes().len() as u64;
            if bytes > self.record_max && !outputs.is_empty() {
                handed_back = Some(outputs.len());
                outcome = Outcome::Error {
                    kind: "output".to_owned(),
                    message: format!(
                        "the action handed back {} artifacts, and the record that lists \
                         them would be {bytes} bytes: {is_at_most}",
                        outputs.len()
                    ),
                };
                outputs.clear();
                inside.verdict = None;
                record = record_of(&outcome, &inside, &noted);
                bytes = record.to_bytes().len() as u64;
            }
            // Then (ADR 0009, ADR 0013): a record that is over the limit with no output in
            // it leaves out what it would have said of the run's capability requests and
            // what was noted of the run, and says so: how many there were of each, and
            // that none of them is listed. The run ends as an error of the type `record`.
            // Everything else a record lists is bounded where it comes in, and the sum of
            // it all is under the limit (ADR 0008, "What a call and a run may hold"), so
            // only a limit that a test lowered reaches this, and the record that is
            // written here is one that is read back.
            if bytes > self.record_max {
                let (settings, paid, went) = noted.counts();
                let before = handed_back.map_or_else(String::new, |count| {
                    format!("the action handed back {count} artifacts, which are not committed; ")
                });
                outcome = Outcome::Error {
                    kind: "record".to_owned(),
                    message: format!(
                        "{before}the run sent {} capability requests, and what was noted of \
                         it is {settings} settings, {paid} paid requests and {went} entries \
                         of where data went: the record that lists them would be {bytes} \
                         bytes, and {is_at_most}, so none of them is listed in this record",
                        inside.asked.len()
                    ),
                };
                inside.asked.clear();
                inside.timed.clear();
                inside.verdict = None;
                // What was noted of the run is left out with the requests it is about.
                noted = Facts::none();
                record = record_of(&outcome, &inside, &noted);
            }
            // The reservation of a run nothing called is closed in the same publication:
            // the ledger is asked inside it, with a step as at admission, and a failure
            // of it is a failure of the publication (ADR 0016, "What the second part
            // builds"). A closing takes its reservation's next place and its epoch's next
            // turn; where another publication took one meanwhile the store refuses this
            // one whole, and it is made again on the store as it is then.
            // A run that reserved nothing has no reservation to close, and the ledger is
            // not asked: nothing of the books is read for it.
            let closing = match (&self.ledger, reserved, &family.parent) {
                (Some(ledger), Some(micros), None) if micros > 0 => Some((ledger, micros)),
                _ => None,
            };
            let mut attempts = 0;
            loop {
                attempts += 1;
                let mut publication = store.begin().map_err(failed)?;
                publication.close_run(run.as_str()).map_err(failed)?;
                // The run stays in the store's table of open runs after its end is
                // visible, until this function has let go of everything: a `wait` that
                // finds the end in the store finds the run there too, and waits for that.
                publication.keep_until_let_go(run.as_str());
                let mut place = 0;
                for (at, (output, artifact)) in outputs.iter().enumerate() {
                    // The outputs of one port are together, in the order they were handed
                    // back.
                    let first = at == 0 || outputs[at - 1].0.port() != output.port();
                    place = if first { 0 } else { place + 1 };
                    let reference = format!("{OUTPUTS_PREFIX}{}/{place}", output.port());
                    let first = family.first().map(RunId::as_str);
                    artifact::commit(&mut publication, run.as_str(), first, &reference, artifact)
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
                    let ending =
                        Ending::new(run.clone(), id.clone(), record.outcome().status, end_ms);
                    publication
                        .create(&id.ending_name(), RECORD_KIND, &ending.to_bytes())
                        .map_err(failed)?;
                }
                let mut ledgers = Vec::new();
                if let Some((ledger, micros)) = closing {
                    let generation = store.snapshot().map_err(failed)?;
                    let mut step =
                        Step::new(&mut publication, &generation, store.as_ref(), secrets);
                    let asked = Closing {
                        run: &run,
                        reserved_micros: micros,
                        at_ms: end_ms,
                    };
                    let refused = |why: &str| CallError::Ledger {
                        why: part::sentence(secrets, why),
                    };
                    match ledger.settle(&asked, &mut step) {
                        Ok(Settled::Nothing) => {}
                        Ok(Settled::Closed(made)) if step.made_here(&made) => {}
                        Ok(Settled::Closed(_)) => {
                            return Err(refused(
                                "it handed back a name that it did not make in this attempt \
                                 at the run's end",
                            ));
                        }
                        Err(failure) => {
                            return Err(failure.into_error(
                                |why| part::sentence(secrets, why),
                                |why| CallError::Ledger { why },
                            ));
                        }
                    }
                    ledgers = step.into_made();
                }
                match publication.publish() {
                    Ok(_) => break,
                    Err(store::Error::NameExists { name })
                        if ledgers.contains(&name) && attempts < MAX_SETTLINGS => {}
                    Err(error @ store::Error::Unacknowledged { .. }) => {
                        // Visible, and not confirmed durable: left as such an ending is,
                        // whatever comes after, and never told as confirmed.
                        let failure = StoreFailure::new(error);
                        *visible.borrow_mut() = Some(failure.clone());
                        reached.set(Reached::Visible);
                        self.panicking(&action_name, PanicAt::Visible);
                        return Err(CallError::Store(failure));
                    }
                    Err(error) => return Err(failed(error)),
                }
            }
            // Durable: the committed outcome stands, and what is left is only to be
            // finished, once.
            let status = record.outcome().status;
            let finished = Finished::new(run.clone(), outcome).with_results(
                handed_verdict,
                measurements,
                findings,
            );
            *durable.borrow_mut() = Some((finished.clone(), status, (start_ms, end_ms)));
            reached.set(Reached::Durable);
            self.panicking(&action_name, PanicAt::Durable);
            // The run's end is durable: what it did not use of its share goes back to
            // the run that called it, while that run still executes (ADR 0016).
            if family.parent.is_some() {
                family.allowance.hand_up();
            }
            // The run that called this one is told how it ended, and from here on reads
            // what it committed: before this run is over, which is what its caller waits
            // for.
            if let Some((_, parent)) = &family.parent {
                let times = (start_ms, end_ms);
                told.set(true);
                parent
                    .nest()
                    .ended(&run, status, finished.outcome().outputs(), times, &store);
            }
            Ok(finished)
        };
        match catch_unwind(AssertUnwindSafe(|| end(pre))) {
            Ok(ended) => ended,
            Err(panic) => match reached.get() {
                // Nothing of the end is visible: it is made again, as `panicked`.
                Reached::Running => {
                    let again = panicked(panic);
                    match catch_unwind(AssertUnwindSafe(|| end(again))) {
                        Ok(ended) => ended,
                        Err(_) => Err(CallError::Abandoned {
                            run: run.clone(),
                            why: "the executor's own code panicked twice as it ended the run"
                                .to_owned(),
                        }),
                    }
                }
                // Visible and not confirmed: left so, with the store's failure.
                Reached::Visible => Err(match visible.take() {
                    Some(failure) => CallError::Store(failure),
                    None => CallError::Abandoned {
                        run: run.clone(),
                        why: "the executor's own code panicked as it ended the run".to_owned(),
                    },
                }),
                // Durable: what was left is finished, once; nothing is settled again.
                Reached::Durable => {
                    let Some((finished, status, times)) = durable.take() else {
                        return Err(CallError::Abandoned {
                            run: run.clone(),
                            why: "the executor's own code panicked as it ended the run".to_owned(),
                        });
                    };
                    if family.parent.is_some() {
                        family.allowance.hand_up();
                    }
                    if let (false, Some((_, parent))) = (told.get(), &family.parent) {
                        let outputs = finished.outcome().outputs();
                        parent.nest().ended(&run, status, outputs, times, &store);
                    }
                    Ok(finished)
                }
            },
        }
    }
}

/// What an action handed back beside its artifacts, held to the kinds of result its
/// descriptor declares (ADR 0035): a verdict only from an action that answers with one,
/// measurements only from one that measures, at most [`MEASUREMENTS_MAX_BYTES`], and
/// findings only from one that declares them, read strictly as `findings/1`: what
/// comes back is the findings as they were read, when there are any.
fn held_to_results(
    descriptor: &ActionDescriptor,
    produced: &Produced,
) -> Result<Option<Findings>, String> {
    if produced.verdict().is_some() && !descriptor.answers_with(ResultKind::Verdict) {
        return Err(
            "the action handed back a verdict, and its descriptor declares none".to_owned(),
        );
    }
    if let Some(measured) = produced.measurements() {
        if !descriptor.answers_with(ResultKind::Measurements) {
            return Err(
                "the action handed back measurements, and its descriptor declares none".to_owned(),
            );
        }
        if measured.to_string().len() > MEASUREMENTS_MAX_BYTES {
            return Err(format!(
                "the action's measurements are over {MEASUREMENTS_MAX_BYTES} bytes"
            ));
        }
    }
    let Some(found) = produced.findings() else {
        return Ok(None);
    };
    if !descriptor.answers_with(ResultKind::Findings) {
        return Err("the action handed back findings, and its descriptor declares none".to_owned());
    }
    let read = Findings::read(found);
    read.map(Some)
        .map_err(|why| format!("the action's findings are not findings/1: {why}"))
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
///
/// What comes back is never longer than [`MESSAGE_LIMIT`] characters and three dots,
/// which is what a reader of a record takes: when the dots do complete a secret, they
/// are left off, and the cut text, which holds none, is what is kept.
pub(super) fn said(secrets: &Secrets, text: &str) -> String {
    cut_to(secrets, text, MESSAGE_LIMIT, "...")
}

/// A text with the secrets taken out, and then its first `limit` characters, with
/// `more` after them when there were more and `more` completes no secret.
pub(super) fn cut_to(secrets: &Secrets, text: &str, limit: usize, more: &str) -> String {
    let whole = secrets.redact(text);
    let Some((cut, _)) = whole.char_indices().nth(limit) else {
        return whole;
    };
    let marked = format!("{}{more}", &whole[..cut]);
    if secrets.redact(&marked) == marked {
        marked
    } else {
        whole[..cut].to_owned()
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

    /// A wait from inside an action, which is given up when this signal is set.
    pub(super) fn wait_unless(
        &self,
        run: &RunId,
        signal: &Cancellation,
    ) -> Result<Watched, CallError> {
        // A run that is cancelled is told so before anything is looked at: a run that
        // is not open here, or is over, is not waited for, and would be answered.
        if signal.is_cancelled() {
            return Err(Refusal::WaitCancelled { run: run.clone() }.into());
        }
        if let Some(then) = &self.before_watch {
            then();
        }
        let watched = self.watch(run, true, Some(signal), 0)?;
        // And once more before the run is answered for: a cancellation that landed
        // after the first look, with a run that had ended and left the table by the
        // time the store was looked at, was never waited on and so never seen.
        if signal.is_cancelled() {
            return Err(Refusal::WaitCancelled { run: run.clone() }.into());
        }
        Ok(watched)
    }

    /// Where the run is now (ADR 0008, "Status"): a look at the store, then at the
    /// store's table of the runs this process has open, then, when the run is in
    /// neither state, at the store again. With `wait`, a run that this process has
    /// open is waited for first.
    /// `unless` is the signal of a run that waits from inside its action: set, the
    /// wait is given up ([`Refusal::WaitCancelled`]).
    fn watch(
        &self,
        run: &RunId,
        wait: bool,
        unless: Option<&Cancellation>,
        seen: u64,
    ) -> Result<Watched, CallError> {
        let unknown = || CallError::Refused(Refusal::NoSuchRun { run: run.clone() });
        let wait_for = |live: &live::Open| match unless {
            None => {
                live.wait_until_over();
                Ok(())
            }
            Some(signal) if live.wait_until_over_unless(signal) => Ok(()),
            Some(_) => Err(CallError::Refused(Refusal::WaitCancelled {
                run: run.clone(),
            })),
        };
        let watched = |state, events| Ok(Watched::new(run.clone(), state, events));
        // What a wait says of a run the executor abandoned, whatever the store shows
        // of an end that was not confirmed; unless the run was deleted meanwhile.
        let abandoned = |failure, events| match self.seen(run) {
            Ok(Seen::Nothing) => Err(unknown()),
            _ => watched(RunState::Abandoned { failure }, events),
        };
        match self.seen(run)? {
            Seen::Nothing => return Err(unknown()),
            Seen::Ended(outcome) => {
                // The end is visible. The executor that ran it here may not be done:
                // the publication's last sync, and the run's place among the started
                // runs. A status says what the store says. A wait is for the
                // executor: the run stays in the table until that is done with it, so
                // found there it is waited for, and not found there it is done.
                if wait {
                    let attached = self.store.as_ref().and_then(|s| s.attached(run.as_str()));
                    if let Some(live) = attached.and_then(live::open_of) {
                        wait_for(&live)?;
                        // An end the executor did not confirm is not said to be one.
                        if let Some(failure) = live.abandoned() {
                            return abandoned(failure, live.events(seen));
                        }
                    }
                }
                return watched(RunState::Ended(outcome), Heard::default());
            }
            Seen::Open => {}
        }
        // The run was admitted and had not ended when the store was looked at. A run
        // this process made is in the table from before it was visible until its end
        // is, and the executor is done with it: found there, it is open here.
        let attached = self.store.as_ref().and_then(|s| s.attached(run.as_str()));
        if let Some(attached) = attached {
            let Some(live) = live::open_of(attached) else {
                return watched(RunState::NotEnded { here: true }, Heard::default());
            };
            if wait {
                wait_for(&live)?;
            }
            let Some(failure) = live.abandoned() else {
                if !wait {
                    return watched(RunState::NotEnded { here: true }, live.events(seen));
                }
                // It ended, and its end was confirmed: the store says how.
                return match self.seen(run)? {
                    Seen::Nothing => Err(unknown()),
                    Seen::Ended(outcome) => watched(RunState::Ended(outcome), Heard::default()),
                    Seen::Open => watched(RunState::NotEnded { here: true }, live.events(seen)),
                };
            };
            // Nothing here works on it any more, and its end was not confirmed. A wait
            // says so, whatever became visible. A status says what the store says: an
            // end that became visible all the same, or the run abandoned as admitted.
            if wait {
                return abandoned(failure, live.events(seen));
            }
            return match self.seen(run)? {
                Seen::Nothing => Err(unknown()),
                Seen::Ended(outcome) => watched(RunState::Ended(outcome), Heard::default()),
                Seen::Open => watched(RunState::Abandoned { failure }, live.events(seen)),
            };
        }
        // Not in the table. Either this process never made it, or it left the table
        // since the look at the store, which it does only once its end, or its
        // deletion, is visible: a second look sees that, and is the answer.
        match self.seen(run)? {
            Seen::Nothing => Err(unknown()),
            Seen::Ended(outcome) => watched(RunState::Ended(outcome), Heard::default()),
            Seen::Open => watched(RunState::NotEnded { here: false }, Heard::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    use whizbang_testkit::barrier::Barriers;
    use whizbang_testkit::scratch::Scratch;

    use super::*;
    use crate::contract::{Kind, NoBackends};
    use crate::store::fs::RealFs;

    #[test]
    fn a_wait_for_a_run_that_is_over_by_a_waiter_that_is_cancelled_says_the_waiter_was() {
        // Both are so before the wait begins: no thread, and no race.
        let kept = Arc::new(Kept::new(Arc::new(SystemClock), Secrets::none()));
        let live = Arc::new(LiveRun::new(kept, Cancellation::new(), Arc::default()));
        live.over(true, None);
        let signal = Cancellation::new();
        assert!(live.wait_until_over_unless(&signal));
        signal.cancel();
        assert!(!live.wait_until_over_unless(&signal));
    }

    #[test]
    fn a_runs_place_is_free_before_whoever_waits_for_the_run_is_told() {
        // The order is what lets a caller that waited start the next run, and nothing
        // outside can see it: the two happen within a few instructions of each other.
        // So the lock under which a waiter is told is held here, and the executor's
        // hold on the run is dropped on another thread. That thread gives the place
        // back and then stops at the lock; were the order the other way round, it
        // would stop at the lock with the place still taken, and the loop below would
        // run out.
        let dir = Scratch::new("over");
        let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
        let kept = Arc::new(Kept::new(Arc::new(SystemClock), Secrets::none()));
        let live = Arc::new(LiveRun::new(kept, Cancellation::new(), Arc::default()));
        let over = Over {
            place: Some(store.start_place(1).unwrap()),
            kept: Some(store.closer(RunId::minted(1, 1).as_str())),
            live: live.clone(),
            ended: true,
            failure: None,
            started_under: None,
        };
        assert!(store.start_place(1).is_none());

        let not_told = live.not_told_yet();
        let letting_go = whizbang_testkit::threads::spawn(move || drop(over));
        let limit = Instant::now() + Duration::from_secs(10);
        let free = loop {
            if let Some(place) = store.start_place(1) {
                break Some(place);
            }
            if Instant::now() > limit {
                break None;
            }
            std::thread::yield_now();
        };
        assert!(
            !not_told.is_over(),
            "whoever waits was told while the lock was held"
        );
        assert!(
            free.is_some(),
            "the place was not given back before whoever waits was to be told"
        );
        drop(not_told);
        letting_go.join().unwrap();
        live.wait_until_over();
    }

    /// What a test attaches to a run: when the store's table of open runs lets the
    /// run go, this is dropped, and notes whether a place among the started runs was
    /// free at that moment.
    struct Probe {
        store: std::sync::Weak<Store>,
        free: Arc<std::sync::Mutex<Option<bool>>>,
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            let free = self.store.upgrade().map(|s| s.start_place(1).is_some());
            *self.free.lock().unwrap() = free;
        }
    }

    #[test]
    fn a_runs_place_is_free_before_the_run_leaves_the_table_of_open_runs() {
        // The other half of the order. A wait that finds a run ended and no longer in
        // the table answers without waiting to be told, so the place must be given
        // back before the table lets the run go. Nothing outside can stop the
        // executor between the two; what the table held for the run is dropped at
        // the moment it lets go, and looks then. No thread, and no race.
        let dir = Scratch::new("over");
        let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
        let run = RunId::minted(1, 1);
        let free = Arc::new(std::sync::Mutex::new(None));
        let probe = Probe {
            store: Arc::downgrade(&store),
            free: free.clone(),
        };
        let mut publication = store.begin().unwrap();
        publication.create_run(run.as_str()).unwrap();
        publication.attach(run.as_str(), Arc::new(probe));
        publication.publish().unwrap();
        let mut publication = store.begin().unwrap();
        publication.close_run(run.as_str()).unwrap();
        publication.keep_until_let_go(run.as_str());
        publication.publish().unwrap();
        assert!(store.attached(run.as_str()).is_some());

        let kept = Arc::new(Kept::new(Arc::new(SystemClock), Secrets::none()));
        let live = Arc::new(LiveRun::new(kept, Cancellation::new(), Arc::default()));
        let over = Over {
            place: Some(store.start_place(1).unwrap()),
            kept: Some(store.closer(run.as_str())),
            live: live.clone(),
            ended: true,
            failure: None,
            started_under: None,
        };
        assert_eq!(*free.lock().unwrap(), None, "the table let go too early");
        drop(over);
        assert!(store.attached(run.as_str()).is_none());
        assert_eq!(
            *free.lock().unwrap(),
            Some(true),
            "the run left the table while its place was still taken"
        );
        live.wait_until_over();
    }

    /// What a test attaches to a run: when the store's table of open runs lets the
    /// run go, this is dropped, and notes how many held the store's writer at that
    /// moment.
    struct Holders {
        store: std::sync::Weak<Store>,
        held_by: Arc<std::sync::Mutex<Option<usize>>>,
    }

    impl Drop for Holders {
        fn drop(&mut self) {
            *self.held_by.lock().unwrap() = Some(self.store.strong_count());
        }
    }

    #[test]
    fn the_executors_hold_on_a_run_holds_nothing_of_the_store_when_a_wait_can_answer() {
        // A wait answers at one of two moments: when the run has left the table of
        // open runs, or when it is told. Whoever waited may then drop its writer and
        // open the store again, and is refused as held while anything of the run
        // holds the writer. So the executor's hold on the run holds none: at both
        // moments the only writer is this test's. The first is seen by what the table
        // held for the run, which is dropped as it lets go; the second by holding the
        // lock a waiter is told under, where the thread that drops the hold stops.
        let dir = Scratch::new("over");
        let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
        let run = RunId::minted(1, 1);
        let held_by = Arc::new(std::sync::Mutex::new(None));
        let holders = Holders {
            store: Arc::downgrade(&store),
            held_by: held_by.clone(),
        };
        let mut publication = store.begin().unwrap();
        publication.create_run(run.as_str()).unwrap();
        publication.attach(run.as_str(), Arc::new(holders));
        publication.publish().unwrap();
        let mut publication = store.begin().unwrap();
        publication.close_run(run.as_str()).unwrap();
        publication.keep_until_let_go(run.as_str());
        publication.publish().unwrap();

        let kept = Arc::new(Kept::new(Arc::new(SystemClock), Secrets::none()));
        let live = Arc::new(LiveRun::new(kept, Cancellation::new(), Arc::default()));
        let over = Over {
            place: None,
            kept: Some(store.closer(run.as_str())),
            live: live.clone(),
            ended: true,
            failure: None,
            started_under: None,
        };
        let not_told = live.not_told_yet();
        let letting_go = whizbang_testkit::threads::spawn(move || drop(over));
        let limit = Instant::now() + Duration::from_secs(10);
        while store.attached(run.as_str()).is_some() {
            assert!(Instant::now() < limit, "the run never left the table");
            std::thread::yield_now();
        }
        assert_eq!(
            *held_by.lock().unwrap(),
            Some(1),
            "the run left the table while the executor's hold on it held the writer"
        );
        assert!(!not_told.is_over());
        assert_eq!(
            Arc::strong_count(&store),
            1,
            "whoever waits is about to be told while the executor's hold on the run \
             holds the writer"
        );
        drop(not_told);
        letting_go.join().unwrap();
        live.wait_until_over();
    }

    /// A clock that stops at the barrier `end` the first time it is asked the time
    /// once a test has armed it.
    struct StopsOnceArmed {
        barriers: Barriers,
        armed: AtomicBool,
    }

    impl Clock for StopsOnceArmed {
        fn now_ms(&self) -> u64 {
            if self.armed.swap(false, Ordering::SeqCst) {
                self.barriers.stop_at("end");
            }
            9000
        }
    }

    /// The action `work`, which stops at the barrier `busy`.
    struct Works(ActionDescriptor, Barriers);

    impl Action for Works {
        fn descriptor(&self) -> &ActionDescriptor {
            &self.0
        }

        fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
            self.1.stop_at("busy");
            Ok(Produced::nothing())
        }
    }

    #[test]
    fn a_started_run_that_was_admitted_leaves_the_table_with_its_thread_holding_nothing_of_the_store()
     {
        // The test above builds the executor's hold on a run by hand. This one takes
        // the hold an admission makes: a run is started, and its thread is stopped
        // where it has let the run go from the table of open runs and is about to
        // tell whoever waits. It is stopped there by the lock a waiter is told under,
        // which this test takes while the thread is held at the clock: the executor
        // asks the time once the run's end has begun, which is the last time it takes
        // that lock before it tells. At that moment the only writer is this test's.
        let dir = Scratch::new("over");
        let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
        let barriers = Barriers::new();
        let clock = Arc::new(StopsOnceArmed {
            barriers: barriers.clone(),
            armed: AtomicBool::new(false),
        });
        let draft = ActionDescriptor::draft("work", "1.0.0", Kind::Primitive, "Works.");
        let mut registry = Registry::new();
        registry
            .register(Arc::new(Works(draft.build().unwrap(), barriers.clone())))
            .unwrap();
        let executor = Engine::over(
            registry,
            Arc::new(NoBackends),
            store.clone(),
            Digest::of_bytes(b""),
        )
        .with_clock(clock.clone());
        let request = Request::new(Call::new("work")).with_id(RequestId::parse("order-1").unwrap());
        let run = match executor.start(&request) {
            Ok(Answer::Started(run)) => run,
            other => panic!("the request was not started: {other:?}"),
        };
        barriers.arrived("busy", 1);
        let live = live::of(store.attached(run.as_str()).unwrap()).unwrap();
        drop(executor);
        clock.armed.store(true, Ordering::SeqCst);
        barriers.release("busy");
        barriers.arrived("end", 1);
        let not_told = live.not_told_yet();
        barriers.release("end");
        let limit = Instant::now() + Duration::from_secs(10);
        while store.attached(run.as_str()).is_some() {
            assert!(Instant::now() < limit, "the run never left the table");
            std::thread::yield_now();
        }
        assert!(!not_told.is_over());
        assert_eq!(
            Arc::strong_count(&store),
            1,
            "the run left the table, and whoever waits is about to be told, while the \
             hold its admission made on it, or its thread, holds the writer"
        );
        drop(not_told);
        live.wait_until_over();
    }
}
