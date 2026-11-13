//! The executor: its interface, and the one implementation, which the binary builds and
//! hands to whatever runs actions.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::artifact::Contents;
use crate::identity::{DecodeLimits, Digest};
use crate::store::{self, Store};

use super::journal::Kept;
use super::prepare::prepare;
use super::record::{Diagnostic, RECORD_KIND, RECORD_NAME, Ran, RunRecord};
use super::request::{Found, look_up};
use super::{
    ADMITTED_NAME, Admission, Allowance, Answer, Backends, Call, CallError, Cancellation,
    Capabilities, Clock, Context, Ending, Failure, Finished, Outcome, PreparedCall, Readiness,
    Refusal, Registry, Request, RequestId, RunId, Secrets, Stale, StoreFailure, SystemClock,
    request_fingerprint,
};

/// What runs actions. A door, the evaluation harness and an action that calls another
/// action all hold one of these and nothing closer to a tool.
///
/// Every method returns when it has its answer: `submit` and `request` return when the
/// run is over and its record is durable, or, for a request that was sent before, as
/// soon as its id was looked up. How a run that takes long is scheduled, watched and
/// cancelled while it executes is not decided here.
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
}

/// The executor of one build: a registry, the backends, a store and the digest of the
/// config.
///
/// Built over a read of a store ([`Engine::new`]) it prepares calls, answers a request
/// that was sent before, and runs none. Built over a store opened to write
/// ([`Engine::over`]) it runs them: it makes each run in the store when it admits it,
/// and adds the run's record and diagnostics when the run ends.
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

    /// The actions this executor runs.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// A new run's id: the time in milliseconds since the Unix epoch, a hyphen, and a
    /// count. The count is one for the whole process ([`NAMED`]), so two executors never
    /// make the same id from the same time, and it goes up past any id the store
    /// already holds now. That an id is still free when its run is admitted is not
    /// taken on trust: the admission's publication makes the run, and the store refuses
    /// it if the id was taken meanwhile ([`MAX_NAMINGS`]).
    fn name_run(&self, store: &Store, now_ms: u64) -> Result<RunId, CallError> {
        let generation = store
            .snapshot()
            .map_err(|error| CallError::Store(StoreFailure::new(error)))?;
        loop {
            let count = NAMED.fetch_add(1, Ordering::SeqCst) + 1;
            let id = RunId::minted(now_ms, count);
            if generation.run(id.as_str()).is_none() {
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
        match self.run(call, None, None) {
            Ok(finished) => Ok(finished),
            Err(Stopped::Refused(error)) => Err(error),
            // Only a request with an id makes the name that can be taken.
            Err(Stopped::Taken(error)) => Err(CallError::Store(StoreFailure::new(error))),
        }
    }

    fn request(&self, request: &Request) -> Result<Answer, CallError> {
        let Some(id) = request.id() else {
            return match self.run(request.call(), None, request.expected()) {
                Ok(finished) => Ok(Answer::Ran(finished)),
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
            Found::Same(earlier) => Some(Ok(Answer::Repeat(earlier))),
            Found::Other { run } => Some(Err(CallError::Refused(Refusal::Conflict {
                request: id.clone(),
                run,
            }))),
        };
        if let Some(answered) = answer(self.look_up(id, fingerprint)?) {
            return answered;
        }
        match self.run(request.call(), Some((id, fingerprint)), request.expected()) {
            Ok(finished) => Ok(Answer::Ran(finished)),
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

/// Why a call did not run to its end.
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
        look_up(self.contents.as_ref(), id, fingerprint, made_here)
    }

    /// Prepares, admits and runs a call: everything after the look for a request id.
    /// `expected` is the preparation the request came with, when it came with one: the
    /// call is prepared again here, under the config and the backends as they are now,
    /// and a request whose preparation no longer holds is refused before anything is
    /// admitted.
    fn run(
        &self,
        call: &Call,
        asked: Option<(&RequestId, Digest)>,
        expected: Option<&PreparedCall>,
    ) -> Result<Finished, Stopped> {
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
            (Err(CallError::Refused(refusal)), Some(expected)) => {
                return Err(stale(expected.action(), Stale::Refused(Box::new(refusal))));
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
        let mut run = self.name_run(store, start_ms)?;
        let mut namings = 1;
        loop {
            let admission = Admission::new(
                run.clone(),
                request.cloned(),
                fingerprint,
                prepared.fingerprint(),
                prepared.action(),
                prepared.version(),
                start_ms,
            )
            .to_bytes();
            let mut publication = store.begin().map_err(failed)?;
            publication.create_run(run.as_str()).map_err(failed)?;
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
                Err(error @ store::Error::NameExists { .. }) if request.is_some() => {
                    return Err(Stopped::Taken(error));
                }
                Err(error) => return Err(failed(error).into()),
            }
        }

        let kept = Arc::new(Kept::new(self.clock.clone(), self.secrets.clone()));
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
        // The outcome is made here, from what the action handed back, with the secrets
        // taken out: what is reported holds none, as what is written holds none.
        let outcome = match action.run(&prepared, &context) {
            Ok(_) => Outcome::Completed {
                outputs: Vec::new(),
            },
            Err(Failure::Error { kind, message }) => Outcome::Error {
                kind: self.secrets.redact(&kind),
                message: self.secrets.redact(&message),
            },
            Err(Failure::Invalid { why }) => Outcome::Invalid {
                why: self.secrets.redact(&why),
            },
        };
        let end_ms = self.clock.now_ms().max(start_ms);

        // The end: one publication adds the journal, every diagnostic and the record to
        // the run, and, for a request with an id, makes the id's ending outside runs.
        // The run must still be there: one that was deleted while it executed is not
        // made again from its last names. The run is reported after this is durable.
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
                outcome: &outcome,
                start_ms,
                end_ms,
                diagnostics: kept.iter().map(|(d, _)| d.clone()).collect(),
            },
            &self.secrets,
        );
        let mut publication = store.begin().map_err(failed)?;
        publication.extend_run(run.as_str()).map_err(failed)?;
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
