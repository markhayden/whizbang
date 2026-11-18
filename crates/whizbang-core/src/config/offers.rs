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
use std::sync::Arc;

use crate::capability::{InstalledRoster, Inventory, Resolved, Roster, RosterError, Unresolved};
use crate::contract::{
    Backends, Capability, Estimate, Made, ModelIdentity, Offered, Secrets, Unavailable,
};

use super::{CAPABILITIES, Config, Credentials, KindRefused, Kinds, Table};
use crate::egress::{Credential, NotAllowed, Recipient};

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
    /// The backend's kind made a backend of another capability than the one its
    /// table is under.
    OtherCapability,
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
            NotReady::OtherCapability => {
                f.write_str("its kind made a backend of another capability")
            }
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
///
/// It holds each backend that is ready, made by its kind in the one walk that offers
/// it: the roster's entry is what that backend declares. Two are equal when they
/// offer the same; the backends they made are not compared, and are never the same
/// ones.
#[derive(Clone)]
pub struct Offers {
    /// The backends that are ready, and the default of each capability whose default
    /// is ready.
    roster: Roster,
    /// The backends that are not, by the capability's name and the backend's.
    not_ready: BTreeMap<(&'static str, String), NotReady>,
    /// The default the file gives a capability, ready or not, by the capability's name.
    defaults: BTreeMap<&'static str, String>,
    ready: usize,
    /// The backends that are ready, made, by the capability's name and the backend's.
    made: BTreeMap<(&'static str, String), Made>,
    /// The credential each ready backend's kind made for it, with the recipient it
    /// declares, in the order the backends were made: what the snapshot holds for the
    /// executor to attach.
    credentials: Vec<(Recipient, Credential)>,
    /// The roster over an inventory, when these offers were put over one
    /// ([`Offers::over`]): what prepare is then answered by. Two offers over two
    /// inventories that offer the same are equal: what is installed is not what a
    /// config offers.
    installed: Option<InstalledRoster>,
}

impl PartialEq for Offers {
    fn eq(&self, other: &Offers) -> bool {
        (&self.roster, &self.not_ready, &self.defaults, self.ready)
            == (
                &other.roster,
                &other.not_ready,
                &other.defaults,
                other.ready,
            )
    }
}

impl Eq for Offers {}

impl Offers {
    /// What this config offers. Each backend is shown to the kind its table names,
    /// with the credentials that table names and no others; the kind makes it, and
    /// it is offered as it declares itself. One whose kind is not registered, whose
    /// kind does not take its table or makes a backend of another capability, or
    /// that names a variable the env file does not set, is offered as not ready. The
    /// kind of a backend with a missing variable is not asked. A kind's sentence is
    /// kept cleaned ([`KindRefused`]): with these credentials' secrets, which hold
    /// the backend's own key.
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
        // The credentials' own set is given as what is known already, so that the
        // set a kind is handed is that set as it is and no other is made.
        Offers::knowing(config, credentials, kinds, credentials.secrets())
    }

    /// [`Offers::of`], by a source that has read the files before and kept the
    /// credentials of those readings: a kind is handed these secrets beside the
    /// credentials', and its sentence is looked at with them all, so one that holds
    /// a key the kind was shown at an earlier reading is replaced too.
    pub fn knowing(
        config: &Config,
        credentials: &Credentials,
        kinds: &Kinds,
        earlier: &Secrets,
    ) -> Result<Offers, RosterError> {
        // The kept set first: a source's holds these credentials already, and is
        // then what comes back as it is, with the search it has
        // (`Secrets::joined`), where the credentials joined by the kept set made a
        // set and its search again at every ask after a key was rotated.
        let secrets = &earlier.joined(credentials.secrets());
        let mut offers = Offers {
            roster: Roster::new(),
            not_ready: BTreeMap::new(),
            defaults: BTreeMap::new(),
            ready: 0,
            made: BTreeMap::new(),
            credentials: Vec::new(),
            installed: None,
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
                let table = Table {
                    capability,
                    name,
                    backend,
                    handed: &handed,
                    secrets,
                };
                let made = match (missing, kinds.get(backend.kind())) {
                    (Some(variable), _) => Err(NotReady::Missing { variable }),
                    (None, None) => Err(NotReady::UnknownKind),
                    (None, Some(kind)) => kind
                        .make(&table)
                        .map_err(|refused| NotReady::Refused(refused.kept(secrets)))
                        .and_then(|made| match made.capability() == capability {
                            true => Ok(made),
                            false => Err(NotReady::OtherCapability),
                        })
                        .and_then(|made| {
                            let carried = kind
                                .credential(&table)
                                .map_err(|refused| NotReady::Refused(refused.kept(secrets)))?;
                            Ok((made, carried))
                        }),
                };
                let made = made.and_then(|(made, carried)| {
                    let Some(credential) = carried else {
                        return Ok(made);
                    };
                    // Bound to the origin of the recipient the backend declares, and to
                    // no upstream of it: one origin, the recipient's own.
                    let elsewhere = KindRefused::Credential(NotAllowed::Elsewhere);
                    match made.declaration().recipient() {
                        Some(recipient) if recipient.origin() == credential.origin() => {
                            offers.credentials.push((recipient.clone(), credential));
                            Ok(made)
                        }
                        _ => Err(NotReady::Refused(elsewhere)),
                    }
                });
                match made {
                    Ok(made) => {
                        // The roster's entry is what the backend that was made
                        // declares: the two are one thing.
                        let declaration = made.declaration().clone();
                        offers.roster = offers.roster.offer(capability, name, declaration)?;
                        offers.ready += 1;
                        offers
                            .made
                            .insert((capability.name(), name.to_owned()), made);
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

    /// The credential each ready backend's kind made for it ([`Kind::credential`](super::Kind::credential)),
    /// with the recipient that backend declares, bound to that recipient's own origin.
    pub fn credentials(&self) -> &[(Recipient, Credential)] {
        &self.credentials
    }

    /// The same offers, with the models of their local backends looked up in this
    /// inventory, as a roster over an inventory looks them up ([`InstalledRoster`];
    /// ADR 0015, "An inventory"), where prepare asks ([`Backends`]). A local backend
    /// whose model is ready is answered under the identity of its installation, as
    /// its version, and one whose model is not is not ready, with the reason. A
    /// backend that is not ready for a reason of the config's keeps that reason, and
    /// the inventory is not asked about it; nor is it asked about a backend that
    /// names a recipient. What a need may cost is found under an installation's
    /// identity, which no declaration holds.
    ///
    /// The backends that were made are as they were: a run is handed the one under
    /// the name its call resolved, declaring the identity it was admitted under
    /// (ADR 0015, "The backend a run is handed, and the lease's place").
    pub fn over(mut self, inventory: Arc<dyn Inventory>) -> Offers {
        self.installed = Some(InstalledRoster::over(self.roster.clone(), inventory));
        self
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
    /// The model that answers the capability for a call. A backend that is offered
    /// and is not ready is answered with its reason first. Then the roster of the
    /// ready backends is asked, as prepare asks one: over an inventory when these
    /// offers were put over one ([`Offers::over`]), and alone otherwise. Either way a
    /// backend that names a recipient and declares the identity of an installation
    /// as its version is not resolved, and the reason says so.
    fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        features: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        if let Err(unanswered) = Offers::resolve(self, capability, choice, features) {
            return Err(Unavailable::new(&unanswered.to_string()));
        }
        match &self.installed {
            Some(installed) => installed.resolve(capability, choice, features),
            None => Backends::resolve(&self.roster, capability, choice, features),
        }
    }

    /// What the roster of the ready backends answers ([`Roster`]'s `bound`), or the
    /// roster over the inventory, which finds a local backend under the identity of
    /// its installation ([`InstalledRoster`]'s).
    fn bound(&self, capability: Capability, model: &ModelIdentity) -> Estimate {
        match &self.installed {
            Some(installed) => installed.bound(capability, model),
            None => self.roster.bound(capability, model),
        }
    }
}

/// What a config offers is what a snapshot of the config holds as its backends (ADR
/// 0015, "One snapshot of the config"): prepare asks it, so a backend that is not
/// ready is answered with its reason, and a run is handed the backend its call
/// resolved by its name.
impl Offered for Offers {
    fn default_of(&self, capability: Capability) -> Option<&str> {
        Offers::default_of(self, capability)
    }

    fn made(&self, capability: Capability, name: &str) -> Option<Made> {
        self.made
            .get(&(capability.name(), name.to_owned()))
            .cloned()
    }
}
