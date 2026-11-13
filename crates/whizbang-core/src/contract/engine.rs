//! The executor: its interface, and the one implementation, which the binary builds and
//! hands to whatever runs actions.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::artifact::{self, Artifact, Contents};
use crate::capability::Shown;
use crate::identity::{DecodeLimits, Digest};
use crate::store::{self, Store};

use super::journal::Kept;
use super::ledger::{Declined, Reservation, Step};
use super::live::{self, LiveRun};
use super::prepare::prepare;
use super::record::{
    Diagnostic, OUTPUTS_PREFIX, RECORD_KIND, RECORD_NAME, Ran, RunRecord, StoredRecordError,
};
use super::request::{Found, look_up, outcome_of};
use super::{
    ADMITTED_NAME, Action, ActionDescriptor, Admission, Allowance, Answer, Backends, Call,
    CallError, Cancellation, Capabilities, Clock, Committed, Context, Ending, Failure, Finished,
    Ledger, Outcome, PreparedCall, Produced, Readiness, Refusal, Registry, Request, RequestId,
    RunId, RunState, Secrets, Stale, StoreFailure, SystemClock, Watched, request_fingerprint,
};

/// What runs actions. A door, the evaluation harness and an action that calls another
/// action all hold one of these and nothing closer to a tool.
///
/// `submit` and `request` return when the run is over and its record is durable, or,
/// for a request that was sent before, as soon as its id was looked up. `start` returns
/// once the run is admitted; a run is then asked after by its id (`status`, `wait`),
/// by any caller (ADR 0008). Nothing cancels a run yet.
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

/// What a running action is given in place of the executor: a call from inside an
/// action is refused until calls can nest.
struct NoNesting;

impl Executor for NoNesting {
    fn prepare(&self, _: &Call) -> Result<PreparedCall, CallError> {
        Err(Refusal::NoNestedCalls.into())
    }

    fn submit(&self, _: &Call) -> Result<Finished, CallError> {
        Err(Refusal::NoNestedCalls.into())
    }

    fn request(&self, _: &Request) -> Result<Answer, CallError> {
        Err(Refusal::NoNestedCalls.into())
    }

    fn request_id(&self, _: &str) -> Result<RequestId, CallError> {
        Err(Refusal::NoNestedCalls.into())
    }

    fn start(&self, _: &Request) -> Result<Answer, CallError> {
        Err(Refusal::NoNestedCalls.into())
    }

    fn status(&self, _: &RunId) -> Result<Watched, CallError> {
        Err(Refusal::NoNestedCalls.into())
    }

    fn wait(&self, _: &RunId) -> Result<Watched, CallError> {
        Err(Refusal::NoNestedCalls.into())
    }
}

impl Executor for Engine {
    fn prepare(&self, call: &Call) -> Result<PreparedCall, CallError> {
        let Some(action) = self.registry.get(call.action()) else {
            return Err(CallError::Refused(Refusal::UnknownAction {
                name: call.action().to_owned(),
            }));
        };
        prepare(
            action.descriptor(),
            call,
            self.contents.as_ref(),
            self.backends.as_ref(),
            self.config.digest(),
            &self.limits,
        )
    }

    fn submit(&self, call: &Call) -> Result<Finished, CallError> {
        match self.admit(call, None, None) {
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
}

impl Engine {
    /// Everything before a run's action: the look for a request's id, the preparation
    /// and the admission. What comes back is the run that was admitted, to be executed,
    /// or the answer to a request that makes no run: a repeat.
    fn begin(&self, request: &Request) -> Result<Result<Admitted, Answer>, CallError> {
        let Some(id) = request.id() else {
            return match self.admit(request.call(), None, request.expected()) {
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
        match self.admit(request.call(), Some((id, fingerprint)), request.expected()) {
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
struct Admitted {
    action: Arc<dyn Action>,
    prepared: PreparedCall,
    run: RunId,
    request: Option<RequestId>,
    reserved: Option<u64>,
    start_ms: u64,
    kept: Arc<Kept>,
    live: Arc<LiveRun>,
    store: Arc<Store>,
}

/// Says, when it is dropped, that nothing in this process works on the run any more.
struct Over(Arc<LiveRun>);

impl Drop for Over {
    fn drop(&mut self) {
        self.0.over();
    }
}

/// Why a call was not admitted.
enum Stopped {
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
        Ok(found)
    }

    /// Prepares and admits a call: everything after the look for a request id and
    /// before the action. `expected` is the preparation the request came with, when it came with one: the
    /// call is prepared again here, under the config and the backends as they are now,
    /// and a request whose preparation no longer holds is refused before anything is
    /// admitted.
    fn admit(
        &self,
        call: &Call,
        asked: Option<(&RequestId, Digest)>,
        expected: Option<&PreparedCall>,
    ) -> Result<Admitted, Stopped> {
        let stale = |action: &str, why: Stale| {
            Stopped::Refused(CallError::Refused(Refusal::Stale {
                action: action.to_owned(),
                why,
            }))
        };
        let prepared = match (self.prepare(call), expected) {
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
        let Some(store) = &self.store else {
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
        let reserved = match &self.ledger {
            Some(_) if prepared.estimate().is_unbounded() => {
                return Err(CallError::Refused(Refusal::NoBound {
                    action: prepared.action().to_owned(),
                })
                .into());
            }
            Some(_) => Some(prepared.estimate().micros()),
            None => None,
        };
        // What the run keeps while it executes is made before the run is, and attached
        // to it in the store's table of open runs by the publication that makes it: so
        // whoever sees the run in a snapshot finds it there (ADR 0008, "Status").
        let kept = Arc::new(Kept::new(self.clock.clone(), self.secrets.clone()));
        let live = Arc::new(LiveRun::new(kept.clone()));
        let mut run = self.name_run(store, start_ms)?;
        let mut namings = 1;
        loop {
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
            .to_bytes();
            let mut publication = store.begin().map_err(failed)?;
            // The ledger takes part here: it judges, and adds what it must to this
            // publication, before anything is published. Declined, the publication is
            // dropped with all it staged.
            let mut reservation_name = None;
            let mut ledgers = Vec::new();
            if let (Some(ledger), Some(micros)) = (&self.ledger, reserved) {
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
                Err(error) => return Err(failed(error).into()),
            }
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
            store: store.clone(),
        })
    }

    /// Runs the action of a run that was admitted, and ends the run: everything from
    /// the admission on. Whatever comes back, nothing in this process is working on
    /// the run any more, and whoever waits for it is told so.
    fn execute(&self, admitted: Admitted) -> Result<Finished, CallError> {
        let Admitted {
            action,
            prepared,
            run,
            request,
            reserved,
            start_ms,
            kept,
            live,
            store,
        } = admitted;
        let _over = Over(live);
        let request = request.as_ref();
        let failed = |error| CallError::Store(StoreFailure::new(error));
        let context = Context::new(
            Cancellation::new(),
            None,
            kept.clone(),
            Allowance::none(),
            self.contents.clone(),
            Capabilities::none(),
            Arc::new(NoNesting),
        )
        .with_diagnostics(kept.clone());
        // A panic in the action is caught: its run ends as any other does, with its
        // record and what it wrote, and the executor goes on (ADR 0008). The context
        // and what it shares are not left broken by one: what a run keeps is behind
        // locks that are taken whatever a panic did to them.
        let handed = catch_unwind(AssertUnwindSafe(|| action.run(&prepared, &context)));
        // The outcome is made here, from what the action handed back, with the secrets
        // taken out: what is reported holds none, as what is written holds none.
        let error = |kind: &str, message: &str| Outcome::Error {
            kind: self.secrets.redact(kind),
            message: self.secrets.redact(message),
        };
        let mut outputs = Vec::new();
        let outcome = match handed {
            Ok(Ok(produced)) => match held_to_ports(&prepared, action.descriptor(), produced) {
                Ok(held) => {
                    outputs = held;
                    Outcome::Completed {
                        outputs: outputs.iter().map(|(output, _)| output.clone()).collect(),
                    }
                }
                Err(why) => error("output", &why),
            },
            Ok(Err(Failure::Error { kind, message })) => error(&kind, &message),
            Ok(Err(Failure::Invalid { why })) => Outcome::Invalid {
                why: self.secrets.redact(&why),
            },
            Err(panic) => error("panic", &panic_text(panic.as_ref())),
        };
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
        let record = RunRecord::of(
            Ran {
                run: run.as_str(),
                request,
                whizbang: WHIZBANG_VERSION,
                prepared: &prepared,
                reserved_micros: reserved,
                outcome: &outcome,
                start_ms,
                end_ms,
                diagnostics: kept.iter().map(|(d, _)| d.clone()).collect(),
            },
            &self.secrets,
        );
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
        Ok(Finished::new(run, outcome))
    }
}

/// What an action handed back, held to the output ports its call returns (ADR 0008):
/// every artifact at one of those ports and of the port's type, and every port with as
/// many as its count says. What comes back is each output with the artifact to commit,
/// in the order of the descriptor's ports, and within a port in the order handed back;
/// or why nothing of it is committed, in fixed words, with a port's name as the action
/// wrote it shown escaped and cut.
fn held_to_ports(
    prepared: &PreparedCall,
    descriptor: &ActionDescriptor,
    produced: Produced,
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
                Shown(&name)
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

/// What a caught panic says, as the message of the error its run ends with: the text
/// the panic carried, when it carried one.
fn panic_text(panic: &(dyn Any + Send)) -> String {
    let text = panic
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned());
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
            if !wait {
                return watched(RunState::NotEnded { here: true }, live.events());
            }
            live.wait_until_over();
            // Nothing here works on it any more: it ended, and the store says how, or
            // its end could not be written, and it is still as it was admitted.
            return match self.seen(run)? {
                Seen::Nothing => Err(unknown()),
                Seen::Ended(outcome) => watched(RunState::Ended(outcome), Vec::new()),
                Seen::Open => watched(RunState::NotEnded { here: true }, live.events()),
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
