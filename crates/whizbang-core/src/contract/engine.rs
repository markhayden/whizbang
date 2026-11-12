//! The executor: its interface, and the one implementation, which the binary builds and
//! hands to whatever runs actions.

use std::sync::Arc;

use crate::artifact::Contents;
use crate::identity::{DecodeLimits, Digest};

use super::prepare::prepare;
use super::{Backends, Call, CallError, Finished, PreparedCall, Readiness, Refusal, Registry};

/// What runs actions. A door, the evaluation harness and an action that calls another
/// action all hold one of these and nothing closer to a tool.
///
/// Both methods return when they have their answer. How a run that takes long is
/// scheduled, watched and cancelled while it executes is not decided here.
pub trait Executor: Send + Sync {
    /// Can this exact call run now? Nothing is written, nothing is spent, and nothing is
    /// sent to a model. A call that is refused and a store that failed are told apart
    /// ([`CallError`]).
    fn prepare(&self, call: &Call) -> Result<PreparedCall, CallError>;

    /// The entry point for a call that is to run: the one place a call comes in, whatever
    /// door it came through.
    fn submit(&self, call: &Call) -> Result<Finished, CallError>;
}

/// The executor of one build: a registry, the backends, a read of the store and the
/// digest of the config.
pub struct Engine {
    registry: Registry,
    backends: Arc<dyn Backends>,
    contents: Arc<dyn Contents + Send + Sync>,
    config: Digest,
    limits: DecodeLimits,
}

impl Engine {
    /// An executor over these actions and backends, reading this store, under the config
    /// with this digest. Images are decoded within the default limits.
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
            config,
            limits: DecodeLimits::default(),
        }
    }

    /// The actions this executor runs.
    pub fn registry(&self) -> &Registry {
        &self.registry
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

        // Admission and execution come here: the preparation is revalidated, the run is
        // assigned and the action is executed. Neither is built.
        Err(CallError::Refused(Refusal::NothingRunsYet))
    }
}
