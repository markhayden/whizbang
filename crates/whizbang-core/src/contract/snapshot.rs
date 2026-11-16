//! One snapshot of the config (ADR 0015, "One snapshot of the config").
//!
//! The executor asks its source ([`ConfigSource`]) for a snapshot once for every call
//! it prepares and once for every admission of a call that nothing called. A snapshot
//! is the config's digest together with what it is the digest of, as far as the
//! executor uses it: the backends, as prepare asks them and as a run is handed them,
//! made from one thing; the cap of one call; the dispatch policy and the credentials
//! of its recipients; the secrets; and what makes the settings a run records. A run
//! is admitted under one snapshot and executes under that one, with every run it
//! calls, whatever the source answers afterwards.
//!
//! Nothing here reads a file. What makes a snapshot of a config file and its env file
//! is the config's (`crate::config`); a digest alone is a source of a snapshot that
//! holds nothing else.

use std::sync::{Arc, Mutex, PoisonError};

use crate::capability::{Declaration, Filler, Judge, Segmenter, Shown, under};
use crate::egress::{Credential, DispatchPolicy, NotAllowed, Recipient, Ways};
use crate::identity::Digest;

use super::record::{SETTINGS_LIMIT, Setting};
use super::{
    Backends, Capabilities, Capability, Estimate, Missing, ModelIdentity, NotJudged, PreparedCall,
    Secrets, Value,
};

/// The most secrets an executor keeps, over every snapshot it has taken. A snapshot
/// that adds a secret and would bring it over is not taken: a secret is never let go
/// to make room. Those an executor is built with are not bounded.
pub const SECRETS_KEPT_LIMIT: usize = 4096;

/// The longest reason of a source that is passed on, in characters.
pub const SOURCE_WHY_LIMIT: usize = 256;

/// What is said of a snapshot whose policy names a recipient at an origin that holds
/// a secret.
pub const SECRET_ORIGIN_APPROVED: &str = "the config approves a recipient whose origin holds a secret, so nothing could be \
     sent to it; the origin is not shown";

/// What is said in place of a source's reason that holds a secret.
pub const SOURCE_WHY_WITHHELD: &str =
    "the config's source refused, and its reason is not shown: it held a secret";

/// Where the executor reads the config. It is asked for one snapshot every time a
/// call is prepared, and once for every admission of a call that nothing called; a
/// call from inside an action is prepared and admitted under the snapshot of the run
/// that made it, and the source is not asked.
///
/// A source that cannot answer refuses ([`SourceRefused`]), and the call is not
/// looked at: nothing is prepared, nothing is published, and the next call asks
/// again. A digest is a source of a config that never changes and holds nothing but
/// its digest, and a snapshot is a source of itself.
pub trait ConfigSource: Send + Sync {
    /// What the config says now, whole, of one reading.
    fn snapshot(&self) -> Result<Snapshot, SourceRefused>;
}

impl ConfigSource for Digest {
    fn snapshot(&self) -> Result<Snapshot, SourceRefused> {
        Ok(Snapshot::of(*self))
    }
}

impl ConfigSource for Snapshot {
    fn snapshot(&self) -> Result<Snapshot, SourceRefused> {
        Ok(self.clone())
    }
}

/// Why a source answers no snapshot: a sentence of fixed words, which is all the type
/// takes. It names a file as "the config" or "the env file" and quotes nothing of
/// either: no key, no value and no path.
///
/// The type does not hold that against a source that leaks a made text, so the
/// executor replaces a reason that holds a secret it knows with fixed words
/// ([`SOURCE_WHY_WITHHELD`]) and cuts every reason at [`SOURCE_WHY_LIMIT`]
/// characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRefused {
    why: &'static str,
}

impl SourceRefused {
    /// Refused, for this reason. A reason of nothing is said as `no reason was given`.
    pub fn because(why: &'static str) -> SourceRefused {
        SourceRefused { why }
    }

    pub fn why(&self) -> &'static str {
        if self.why.trim().is_empty() {
            "no reason was given"
        } else {
            self.why
        }
    }

    /// The reason as the executor passes it on: fixed words in place of one that the
    /// search for these secrets changes, and no more than [`SOURCE_WHY_LIMIT`]
    /// characters of any other. A cut takes text away and adds none, so what is left
    /// of a reason that held no secret holds none.
    pub(super) fn said(&self, secrets: &Secrets) -> String {
        let why = self.why();
        if secrets.redact(why) != why {
            return SOURCE_WHY_WITHHELD.to_owned();
        }
        why.chars().take(SOURCE_WHY_LIMIT).collect()
    }
}

impl std::fmt::Display for SourceRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.why())
    }
}

impl std::error::Error for SourceRefused {}

/// One backend, made: what answers the requests of the runs admitted under the
/// snapshot it was made for. What `{:?}` prints is its capability.
#[derive(Clone)]
pub enum Made {
    Segment(Arc<dyn Segmenter>),
    Fill(Arc<dyn Filler>),
    Judge(Arc<dyn Judge>),
}

impl Made {
    /// The capability the backend answers.
    pub fn capability(&self) -> Capability {
        match self {
            Made::Segment(_) => Capability::Segment,
            Made::Fill(_) => Capability::Fill,
            Made::Judge(_) => Capability::Judge,
        }
    }

    /// What the backend declares: its model, its features, its price and who receives
    /// its requests. It is the backend's own, so what a roster offers of a backend
    /// that was made and what that backend is are one thing.
    pub fn declaration(&self) -> &Declaration {
        match self {
            Made::Segment(backend) => backend.declaration(),
            Made::Fill(backend) => backend.declaration(),
            Made::Judge(backend) => backend.declaration(),
        }
    }
}

impl Made {
    /// This backend as a run that resolved this model is handed it, or none when it
    /// does not answer that model: the match an executor makes among the backends
    /// it was built with, of this one backend.
    fn under(self, model: &ModelIdentity) -> Option<Made> {
        match self {
            Made::Segment(backend) => under(&[backend], model).map(Made::Segment),
            Made::Fill(backend) => under(&[backend], model).map(Made::Fill),
            Made::Judge(backend) => under(&[backend], model).map(Made::Judge),
        }
    }
}

impl std::fmt::Debug for Made {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Made({})", self.capability().name())
    }
}

/// The backends of one snapshot: what prepare asks ([`Backends`], which answers a
/// backend that is not ready with its reason), and each backend that is ready, made,
/// under the name a call chooses it by.
///
/// **A backend is picked by its name, never by its model:** two backends of one
/// capability may declare one model, and only their names tell them apart. The name
/// is the one the call chose, or else the capability's default.
pub trait Offered: Backends {
    /// The name of the backend that answers a call that names none.
    fn default_of(&self, capability: Capability) -> Option<&str>;

    /// The backend offered under this name, made. None for a name that is not
    /// offered, and for a backend that is not ready.
    fn made(&self, capability: Capability, name: &str) -> Option<Made>;
}

/// What makes the settings a run records (ADR 0013, "A setting"), of one snapshot.
pub trait Settings: Send + Sync {
    /// The settings of a run whose call resolved these backends, each by its
    /// capability and the name it is offered under, as names and values. A text may
    /// be given whole: the executor makes each pair as a record takes one. What
    /// cannot be recorded is a failure of the whole, never a setting left out.
    fn of(
        &self,
        resolved: &[(Capability, &str)],
        secrets: &Secrets,
    ) -> Result<Vec<(String, Value)>, NotJudged>;
}

/// What a config says, of one reading: its digest, and what the executor uses of it.
/// A clone is the same snapshot, with the same backends.
///
/// What `{:?}` prints is the digest, whether it holds backends, whether it holds a
/// cap, the policy, and how many secrets it brings: never a credential, a secret, a
/// setting or the cap's amount, which could spell a secret.
#[derive(Clone)]
pub struct Snapshot {
    digest: Digest,
    offered: Option<Arc<dyn Offered>>,
    cap: Option<Estimate>,
    /// The policy and the credentials, with no sender and no check: those are the
    /// executor's.
    ways: Option<Ways>,
    secrets: Secrets,
    settings: Option<Arc<dyn Settings>>,
}

impl Snapshot {
    /// A snapshot of the config with this digest, which holds nothing else: an
    /// executor under it asks the backends it was built with, sends through the ways
    /// it was given, and records no setting.
    pub fn of(digest: Digest) -> Snapshot {
        Snapshot {
            digest,
            offered: None,
            cap: None,
            ways: None,
            secrets: Secrets::none(),
            settings: None,
        }
    }

    /// The same snapshot, with these backends: prepare asks them, and a run admitted
    /// under the snapshot is handed the one its call resolved, by its name, and no
    /// backend the executor was built with.
    pub fn offering(mut self, offered: Arc<dyn Offered>) -> Snapshot {
        self.offered = Some(offered);
        self
    }

    /// The same snapshot, with what stands around its backends: `around` is handed
    /// the backends the snapshot holds and answers what holds them from then on.
    /// It is for a wrapper that changes what prepare is answered and hands a run
    /// the same backends: `spend::PerCall`, which makes the bound of a need cover
    /// every request a call sends. A snapshot that holds no backends is as it was,
    /// and `around` is not asked.
    pub fn offering_through(
        mut self,
        around: impl FnOnce(Arc<dyn Offered>) -> Arc<dyn Offered>,
    ) -> Snapshot {
        self.offered = self.offered.map(around);
        self
    }

    /// The same snapshot, with this cap on what one call may reserve. The executor
    /// holds no call to it: it is shown to the parts of the admission.
    pub fn capped_at(mut self, cap: Estimate) -> Snapshot {
        self.cap = Some(cap);
        self
    }

    /// The same snapshot, under this policy: a run admitted under the snapshot sends
    /// under it, with the credentials the snapshot holds, and not under the policy
    /// or with the credentials the executor was built with.
    pub fn sending_under(mut self, policy: DispatchPolicy) -> Snapshot {
        self.ways = Some(self.ways.unwrap_or_default().under(policy));
        self
    }

    /// The same snapshot, holding this credential for requests of this recipient's
    /// backends to the credential's origin, as the executor's ways hold one
    /// ([`Ways::with_credential`]), with the same refusals and the same bound.
    pub fn with_credential(
        mut self,
        recipient: &Recipient,
        credential: Credential,
    ) -> Result<Snapshot, NotAllowed> {
        let ways = self.ways.unwrap_or_default();
        self.ways = Some(ways.with_credential(recipient, credential)?);
        Ok(self)
    }

    /// The same snapshot, bringing these secrets: the executor takes them out of what
    /// it writes from here on, for as long as it lives.
    pub fn with_secrets(mut self, secrets: Secrets) -> Snapshot {
        self.secrets = secrets;
        self
    }

    /// The same snapshot, whose runs record the settings this makes.
    pub fn recording(mut self, settings: Arc<dyn Settings>) -> Snapshot {
        self.settings = Some(settings);
        self
    }

    /// The digest of the config.
    pub fn digest(&self) -> Digest {
        self.digest
    }

    /// The most one call may reserve, when the config caps it.
    pub fn cap(&self) -> Option<Estimate> {
        self.cap
    }

    /// The policy a run admitted under this snapshot sends under, when the snapshot
    /// holds one.
    pub fn policy(&self) -> Option<&DispatchPolicy> {
        self.ways.as_ref().map(Ways::policy)
    }

    /// Whether the snapshot holds backends of its own.
    pub fn holds_backends(&self) -> bool {
        self.offered.is_some()
    }

    pub(super) fn secrets(&self) -> &Secrets {
        &self.secrets
    }

    /// Whether the snapshot's policy names a recipient whose own origin, as it is
    /// written, holds one of these secrets. A record writes an origin whole, so
    /// every send to such a recipient would be refused where it is made (ADR 0015,
    /// "A policy before a send, and a checked send"): the config is one no run can
    /// use as it reads.
    pub(super) fn names_a_secret_origin(&self, secrets: &Secrets) -> bool {
        let Some(policy) = self.policy() else {
            return false;
        };
        policy.recipients().any(|recipient| {
            let written = recipient.origin().to_string();
            secrets.redact(&written) != written
        })
    }

    /// What prepare asks about capabilities under this snapshot, when it holds
    /// backends.
    pub(super) fn backends(&self) -> Option<&dyn Backends> {
        self.offered.as_deref().map(|offered| {
            let backends: &dyn Backends = offered;
            backends
        })
    }

    /// The ways out of a run admitted under this snapshot: the snapshot's policy and
    /// credentials behind the executor's sender and check, or the executor's own
    /// ways when the snapshot holds neither.
    pub(super) fn ways(&self, executors: &Ways) -> Ways {
        match &self.ways {
            Some(ways) => ways.clone().behind(executors),
            None => executors.clone(),
        }
    }

    /// The name each capability of this call was resolved under: the one the call
    /// chose, or else the default. As a run is handed backends, a capability with two
    /// needs is that of the first.
    fn resolved<'a>(&'a self, prepared: &'a PreparedCall) -> Vec<(Capability, &'a str)> {
        let Some(offered) = &self.offered else {
            return Vec::new();
        };
        let mut resolved: Vec<(Capability, &str)> = Vec::new();
        for need in prepared.needs() {
            let capability = need.capability();
            if need.model().is_none() || resolved.iter().any(|(of, _)| *of == capability) {
                continue;
            }
            let name = need.choice().or_else(|| offered.default_of(capability));
            if let Some(name) = name {
                resolved.push((capability, name));
            }
        }
        resolved
    }

    /// The backends a run of this call is handed: for each capability the call needs
    /// and a model answers, the backend the snapshot made under the name the call
    /// resolved. None when the snapshot holds no backends: the executor then hands
    /// over those it was built with.
    ///
    /// A snapshot that resolved a model and holds no backend under that name that
    /// declares that model is not one a run can be admitted under: the answer is
    /// what is missing, and the call is refused as not ready.
    pub(super) fn handed(
        &self,
        prepared: &PreparedCall,
    ) -> Result<Option<Capabilities>, Vec<Missing>> {
        let Some(offered) = &self.offered else {
            return Ok(None);
        };
        let resolved = self.resolved(prepared);
        let mut handed = Capabilities::none();
        let mut missing = Vec::new();
        let mut seen = Vec::new();
        for need in prepared.needs() {
            let capability = need.capability();
            let Some(model) = need.model() else { continue };
            if seen.contains(&capability) {
                continue;
            }
            seen.push(capability);
            let name = resolved.iter().find(|(of, _)| *of == capability);
            let made = name.and_then(|(_, name)| offered.made(capability, name));
            // Whether the backend under the name answers the model that was
            // resolved is the executor's own match (`capability::inventory::under`,
            // ADR 0015, "The backend a run is handed"): by all three texts, but
            // for a version that is an installation's identity, where it is a
            // local backend of that backend and model, declaring the identity.
            let made = made
                .filter(|made| made.capability() == capability)
                .and_then(|made| made.under(model));
            handed = match made {
                Some(Made::Segment(backend)) => handed.with_segment(backend),
                Some(Made::Fill(backend)) => handed.with_fill(backend),
                Some(Made::Judge(backend)) => handed.with_judge(backend),
                None => {
                    missing.push(Missing::of(
                        capability,
                        need.features(),
                        "the config resolved a model for it and holds no backend that \
                         declares that model under the name the call resolved",
                    ));
                    handed
                }
            };
        }
        if missing.is_empty() {
            Ok(Some(handed))
        } else {
            Err(missing)
        }
    }

    /// The settings a run of this call records, each as a record takes one with
    /// these secrets. Anything a record would not take is a failure of the whole,
    /// which names the setting when its name holds no secret and never its value.
    pub(super) fn settings(
        &self,
        prepared: &PreparedCall,
        secrets: &Secrets,
    ) -> Result<Vec<(String, Value)>, NotJudged> {
        let Some(settings) = &self.settings else {
            return Ok(Vec::new());
        };
        let pairs = settings.of(&self.resolved(prepared), secrets)?;
        if pairs.len() > SETTINGS_LIMIT {
            return Err(NotJudged::saying(&format!(
                "the config gives more than the {SETTINGS_LIMIT} settings a record lists"
            )));
        }
        for (at, (name, value)) in pairs.iter().enumerate() {
            // The name is looked at whole for a secret before anything of it is shown.
            if secrets.redact(name) != *name {
                return Err(NotJudged::saying(
                    "the name a setting would be recorded under holds a secret, and is not \
                     shown",
                ));
            }
            let shown: String = name.chars().take(64).collect();
            if pairs[..at].iter().any(|(earlier, _)| earlier == name) {
                return Err(NotJudged::saying(&format!(
                    "the setting {} is given twice",
                    Shown(&shown)
                )));
            }
            if Setting::new(name, value, secrets).is_none() {
                return Err(NotJudged::saying(&format!(
                    "the setting {} is not one a record takes: its name is not a \
                     setting's, or its value is a number a record would write with a \
                     secret in it; the value is not shown",
                    Shown(&shown)
                )));
            }
        }
        Ok(pairs)
    }
}

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshot")
            .field("digest", &self.digest)
            .field("backends", &self.offered.is_some())
            .field("cap", &self.cap.is_some())
            .field("policy", &self.policy())
            .field("secrets", &self.secrets)
            .field("settings", &self.settings.is_some())
            .finish()
    }
}

/// The secrets an executor knows: those it was built with, and those of every
/// snapshot it has taken. One is never let go, so a key that was rotated is still
/// taken out of what a later run writes. Every clone of an executor shares them.
#[derive(Clone, Default)]
pub(super) struct Known(Arc<Mutex<Secrets>>);

impl Known {
    pub(super) fn of(secrets: Secrets) -> Known {
        Known(Arc::new(Mutex::new(secrets)))
    }

    /// The secrets known now.
    pub(super) fn now(&self) -> Secrets {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Takes these in, and answers all that are known with them. None, and nothing
    /// is taken in, when these add a secret and all that would then be known are
    /// more than [`SECRETS_KEPT_LIMIT`]. The bound is on what is added: an executor
    /// built with more secrets than that keeps every one of them, and is answered
    /// them all when nothing is added. The look and the keeping are one step under
    /// the lock.
    pub(super) fn with(&self, more: &Secrets) -> Option<Secrets> {
        let mut known = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let joined = known.joined(more);
        if joined.count() > known.count() && joined.count() > SECRETS_KEPT_LIMIT {
            return None;
        }
        *known = joined.clone();
        Some(joined)
    }
}
