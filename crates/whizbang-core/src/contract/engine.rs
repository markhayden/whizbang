//! The executor: its interface, and the one implementation, which the binary builds and
//! hands to whatever runs actions.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::artifact::Contents;
use crate::identity::{DecodeLimits, Digest};
use crate::store::Store;

use super::journal::Kept;
use super::prepare::prepare;
use super::record::{Diagnostic, RECORD_KIND, RECORD_NAME, Ran, RunRecord};
use super::{
    Allowance, Backends, Call, CallError, Cancellation, Capabilities, Clock, Context, Failure,
    Finished, Outcome, PreparedCall, Readiness, Refusal, Registry, RunId, Secrets, StoreFailure,
    SystemClock,
};

/// What runs actions. A door, the evaluation harness and an action that calls another
/// action all hold one of these and nothing closer to a tool.
///
/// Both methods return when they have their answer: `submit` returns when the run is
/// over and its record is durable. How a run that takes long is scheduled, watched and
/// cancelled while it executes is not decided here.
pub trait Executor: Send + Sync {
    /// Can this exact call run now? Nothing is written, nothing is spent, and nothing is
    /// sent to a model. A call that is refused and a store that failed are told apart
    /// ([`CallError`]).
    fn prepare(&self, call: &Call) -> Result<PreparedCall, CallError>;

    /// The entry point for a call that is to run: the one place a call comes in, whatever
    /// door it came through. A call that is refused has no run. One that is taken runs,
    /// and what comes back is its run's id and outcome, after the run's record and
    /// diagnostics are durable in the store, whatever the outcome.
    fn submit(&self, call: &Call) -> Result<Finished, CallError>;
}

/// The executor of one build: a registry, the backends, a store and the digest of the
/// config.
///
/// Built over a read of a store ([`Engine::new`]) it prepares calls and runs none. Built
/// over a store opened to write ([`Engine::over`]) it runs them, and writes each run's
/// record and diagnostics through that store's publication.
pub struct Engine {
    registry: Registry,
    backends: Arc<dyn Backends>,
    contents: Arc<dyn Contents + Send + Sync>,
    /// The store to write runs to, when this executor may.
    store: Option<Arc<Store>>,
    config: Digest,
    limits: DecodeLimits,
    clock: Arc<dyn Clock>,
    secrets: Secrets,
    /// The runs this executor has named, so that two of one millisecond differ.
    named: AtomicU64,
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
            config,
            limits: DecodeLimits::default(),
            clock: Arc::new(SystemClock),
            secrets: Secrets::none(),
            named: AtomicU64::new(0),
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
    /// count. The count starts at this executor's count of runs and goes up until the
    /// store holds no run of that id. One process writes a store at a time, and this is
    /// that process, so an id that is free now stays free until this run is published,
    /// unless this executor names it again, which the count prevents.
    fn name_run(&self, store: &Store, now_ms: u64) -> Result<RunId, CallError> {
        let generation = store
            .snapshot()
            .map_err(|error| CallError::Store(StoreFailure::new(error)))?;
        loop {
            let count = self.named.fetch_add(1, Ordering::SeqCst) + 1;
            let id = RunId::minted(now_ms, count);
            if generation.run(id.as_str()).is_none() {
                return Ok(id);
            }
        }
    }
}

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
            self.config,
            &self.limits,
        )
    }

    fn submit(&self, call: &Call) -> Result<Finished, CallError> {
        // A caller's request id is looked up here, before anything else: a repeated id
        // answers with the run it already has, and neither readiness nor preparation is
        // consulted (`SPEC.md` section 3, "Request identity"). A call carries no request
        // id yet, and there is no record to look one up in.

        let prepared = self.prepare(call)?;
        if let Readiness::NotReady(missing) = prepared.readiness() {
            return Err(CallError::Refused(Refusal::NotReady {
                action: prepared.action().to_owned(),
                missing: missing.clone(),
            }));
        }
        let Some(store) = &self.store else {
            return Err(CallError::Refused(Refusal::ReadOnly));
        };
        // Prepare found the action a moment ago, in a registry that does not change.
        let Some(action) = self.registry.get(prepared.action()) else {
            return Err(CallError::Refused(Refusal::UnknownAction {
                name: prepared.action().to_owned(),
            }));
        };

        // Admission comes here: the preparation is revalidated, the spend is reserved
        // and the request id is recorded with the run. None of it is built. From the
        // next line on there is a run, and whatever happens it gets its record.
        let start_ms = self.clock.now_ms();
        let run = self.name_run(store, start_ms)?;

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
        let end_ms = self.clock.now_ms();

        // One publication holds the journal, every diagnostic and the record: the run is
        // in the store whole or not at all, and it is durable before it is reported.
        let failed = |error| CallError::Store(StoreFailure::new(error));
        let (lines, written) = kept.take();
        let mut publication = store.begin().map_err(failed)?;
        let mut diagnostics = Vec::new();
        for (diagnostic, bytes) in std::iter::once(Diagnostic::journal(&lines)).chain(written) {
            publication
                .put_in(run.as_str(), diagnostic.name(), RECORD_KIND, &bytes)
                .map_err(failed)?;
            diagnostics.push(diagnostic);
        }
        let record = RunRecord::of(
            Ran {
                run: run.as_str(),
                whizbang: WHIZBANG_VERSION,
                prepared: &prepared,
                outcome: &outcome,
                start_ms,
                end_ms,
                diagnostics,
            },
            &self.secrets,
        );
        publication
            .put_in(run.as_str(), RECORD_NAME, RECORD_KIND, &record.to_bytes())
            .map_err(failed)?;
        publication.publish().map_err(failed)?;
        Ok(Finished::new(run, outcome))
    }
}
