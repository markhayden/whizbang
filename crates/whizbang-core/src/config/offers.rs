//! The backends a config offers (ADR 0011, "The roster of a config"): for each
//! capability, each backend the file gives it, by the name a call chooses it by, and
//! the capability's default.
//!
//! A [`Roster`] holds the backends that are ready. A backend that is not, because the
//! build has no code for its kind, because its kind does not take its table, or because
//! the env file does not set a variable it names, is still offered: it is kept beside
//! the roster with its reason, and a call that chooses it, or that chooses none where
//! it is the default, is answered with that reason. It is never left out, and no other
//! backend ever answers in its place.

use std::collections::BTreeMap;

use crate::capability::{Resolved, Roster, RosterError, Unresolved};
use crate::contract::{Backends, Capability, Estimate, ModelIdentity, Unavailable};

use super::{CAPABILITIES, Config, Credentials, KindRefused, Kinds, Table};

/// Why a backend a config offers cannot answer. Each is fixed words, but for the name
/// of a variable, which the config's reader held to the rule of one and looked at for a
/// secret. Nothing of a value is in one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotReady {
    /// The build has no code for the backend's `kind`.
    UnknownKind,
    /// The backend's kind does not take its table.
    Refused(KindRefused),
    /// The backend names this variable for a credential, and the env file does not
    /// set it.
    Missing { variable: String },
}

impl std::fmt::Display for NotReady {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotReady::UnknownKind => f.write_str("its kind is not one this build has"),
            NotReady::Refused(why) => write!(f, "its kind does not take it: {why}"),
            NotReady::Missing { variable } => write!(
                f,
                "the env file does not set {variable}, the variable its credential is in"
            ),
        }
    }
}

/// Why nothing answers a capability for a call, of the backends a config offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unanswered {
    /// The backend the call chose, or the default when it chose none, is offered and
    /// is not ready. No other backend is used in its place.
    NotReady {
        capability: Capability,
        /// Whether the call chose it; otherwise it is the default.
        chosen: bool,
        why: NotReady,
    },
    /// As a roster says it (ADR 0006): no default, a choice that is not offered, a
    /// feature that is not declared.
    Unresolved(Unresolved),
}

impl std::fmt::Display for Unanswered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unanswered::NotReady {
                capability,
                chosen: true,
                why,
            } => write!(
                f,
                "the {} backend the call chose is not ready: {why}",
                capability.name()
            ),
            Unanswered::NotReady {
                capability,
                chosen: false,
                why,
            } => write!(
                f,
                "the default {} backend is not ready: {why}",
                capability.name()
            ),
            Unanswered::Unresolved(unresolved) => unresolved.fmt(f),
        }
    }
}

impl std::error::Error for Unanswered {}

/// What one config offers, with the kinds of one build and the credentials of one env
/// file. What `{:?}` prints is how many backends are ready and how many are not.
#[derive(Clone, PartialEq, Eq)]
pub struct Offers {
    /// The backends that are ready, and the default of each capability whose default
    /// is ready.
    roster: Roster,
    /// The backends that are not, by the capability's name and the backend's.
    not_ready: BTreeMap<(&'static str, String), NotReady>,
    /// The default the file gives a capability, ready or not, by the capability's name.
    defaults: BTreeMap<&'static str, String>,
    ready: usize,
}

impl Offers {
    /// What this config offers. Each backend is shown to the kind its table names,
    /// with the credentials that table names and no others, and is offered as that
    /// kind declares it. One whose kind is not registered, whose kind does not take
    /// its table, or that names a variable the env file does not set, is offered as
    /// not ready. The kind of a backend with a missing variable is not asked.
    ///
    /// The credentials are those of this config (`Loaded`); given another's, a backend
    /// is handed nothing and is not ready for want of its variable.
    ///
    /// The refusal is a roster's, of a name it does not take or a default that is not
    /// offered. A config that was read gives neither, and it is passed on and not
    /// dropped all the same.
    pub fn of(
        config: &Config,
        credentials: &Credentials,
        kinds: &Kinds,
    ) -> Result<Offers, RosterError> {
        let mut offers = Offers {
            roster: Roster::new(),
            not_ready: BTreeMap::new(),
            defaults: BTreeMap::new(),
            ready: 0,
        };
        for capability in CAPABILITIES {
            for (name, backend) in config.backends(capability) {
                let handed = credentials.of(capability, name);
                let missing = backend.credentials().find_map(|(key, variable)| {
                    let value = match key {
                        "key_env" => handed.key(),
                        _ => handed.token(),
                    };
                    value.is_none().then(|| variable.to_owned())
                });
                let declared = match (missing, kinds.get(backend.kind())) {
                    (Some(variable), _) => Err(NotReady::Missing { variable }),
                    (None, None) => Err(NotReady::UnknownKind),
                    (None, Some(kind)) => kind
                        .declare(&Table {
                            capability,
                            name,
                            backend,
                            handed: &handed,
                            secrets: credentials.secrets(),
                        })
                        .map_err(NotReady::Refused),
                };
                match declared {
                    Ok(declaration) => {
                        offers.roster = offers.roster.offer(capability, name, declaration)?;
                        offers.ready += 1;
                    }
                    Err(why) => {
                        offers
                            .not_ready
                            .insert((capability.name(), name.to_owned()), why);
                    }
                }
            }
            if let Some(default) = config.default_of(capability) {
                offers
                    .defaults
                    .insert(capability.name(), default.to_owned());
                if offers.not_ready(capability, default).is_none() {
                    offers.roster = offers.roster.default_for(capability, default)?;
                }
            }
        }
        Ok(offers)
    }

    /// The backends that are ready, and the default of each capability whose default
    /// is ready. Asked alone it does not know a backend that is not ready: a call that
    /// chose one is told that none of that name is offered, and a call that chose none
    /// where the default is not ready is told there is no default. Neither is ever
    /// answered by another backend; [`Offers::resolve`] gives the reason.
    pub fn roster(&self) -> &Roster {
        &self.roster
    }

    /// Why this backend is not ready, when the config offers it and it is not.
    pub fn not_ready(&self, capability: Capability, name: &str) -> Option<&NotReady> {
        self.not_ready.get(&(capability.name(), name.to_owned()))
    }

    /// Every backend that is offered and is not ready, with its capability's name, its
    /// own and the reason, in the order of those names' bytes.
    pub fn every_not_ready(&self) -> impl Iterator<Item = (&'static str, &str, &NotReady)> {
        self.not_ready
            .iter()
            .map(|((capability, name), why)| (*capability, name.as_str(), why))
    }

    /// The default the file gives the capability, ready or not.
    pub fn default_of(&self, capability: Capability) -> Option<&str> {
        self.defaults.get(capability.name()).map(String::as_str)
    }

    /// Which backend answers the capability for a call, as ADR 0006 resolves it: the
    /// one the call chose, or else the default; and it declares every feature needed.
    /// Where that one backend is offered and is not ready, the answer is its reason:
    /// nothing else is tried.
    pub fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        features: &[String],
    ) -> Result<Resolved<'_>, Unanswered> {
        let picked = choice.or_else(|| self.default_of(capability));
        if let Some(why) = picked.and_then(|name| self.not_ready(capability, name)) {
            return Err(Unanswered::NotReady {
                capability,
                chosen: choice.is_some(),
                why: why.clone(),
            });
        }
        self.roster
            .resolve(capability, choice, features)
            .map_err(Unanswered::Unresolved)
    }
}

impl std::fmt::Debug for Offers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Offers")
            .field("ready", &self.ready)
            .field("not_ready", &self.not_ready.len())
            .finish()
    }
}

impl Backends for Offers {
    fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        features: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        match Offers::resolve(self, capability, choice, features) {
            Ok(resolved) => Ok(resolved.declaration().model().clone()),
            Err(unanswered) => Err(Unavailable::new(&unanswered.to_string())),
        }
    }

    /// What the roster of the ready backends answers ([`Roster`]'s `bound`).
    fn bound(&self, capability: Capability, model: &ModelIdentity) -> Estimate {
        self.roster.bound(capability, model)
    }
}
