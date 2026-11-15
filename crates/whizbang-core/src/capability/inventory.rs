//! What is installed, as readiness asks it (ADR 0015, "An inventory").
//!
//! A local backend, one that names no recipient, answers with a model that is on this
//! machine or is not. An [`Inventory`] says which: the model is ready, and then under
//! which installation; or it is not, for one of four reasons. A roster over an
//! inventory ([`InstalledRoster`]) resolves a call as a [`Roster`] does and then asks
//! the inventory about the one backend that was picked, when it is local. A model that
//! is not ready makes its backend not ready, with the reason, and no other backend is
//! looked for. A model that is ready is answered under the identity of its
//! installation, as its version: that is what a preparation saw, so an installation
//! replaced under a prepared call makes the preparation stale (ADR 0004, ADR 0017).
//!
//! Nothing here reads a disk. The inventory over the models directory is the models
//! strand's; the default here knows no model.

use std::sync::Arc;

use crate::contract::{Backends, Capability, Estimate, ModelIdentity, Unavailable};
use crate::identity::Digest;

use super::{
    Bounds, Declaration, Fault, FillRequest, Filler, Judge, JudgeRequest, Painted, Roster,
    SegmentRequest, SegmentResponse, Segmenter, Turn,
};

/// The identity of one installation of a model: a digest, as ADR 0017 makes it. Its
/// text is `sha256:` and 64 lower-case hex digits and nothing else, so it is a version
/// a declaration takes, and it has no place for a path, a name or a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Installation(Digest);

impl Installation {
    /// The installation of this identity.
    pub fn of(identity: Digest) -> Installation {
        Installation(identity)
    }

    /// The identity.
    pub fn identity(&self) -> Digest {
        self.0
    }

    /// The installation whose identity is written so: `sha256:` and 64 lower-case
    /// hex digits, and nothing else. None for any other text. It is how a version
    /// is known to be an installation's: no other version is written so by anything
    /// here, and one that is, is taken for a local model's ([`under`]).
    pub fn from_version(version: &str) -> Option<Installation> {
        version.parse().ok().map(Installation)
    }
}

/// A local backend, as a run that was admitted under one installation of its model
/// is handed it: the backend, declaring the installation's identity as its version.
/// Every request goes to the backend as it is.
struct Under<B: ?Sized> {
    backend: Arc<B>,
    declaration: Declaration,
}

impl Segmenter for Under<dyn Segmenter> {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn segment(&self, request: &SegmentRequest, bounds: &Bounds) -> Result<SegmentResponse, Fault> {
        self.backend.segment(request, bounds)
    }
}

impl Filler for Under<dyn Filler> {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        self.backend.fill(request, bounds)
    }
}

impl Judge for Under<dyn Judge> {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn send(&self, request: &JudgeRequest, bounds: &Bounds) -> Result<Turn, Fault> {
        self.backend.send(request, bounds)
    }
}

/// How a kind of backend is handed under an installation.
pub(crate) trait Installable {
    fn declared(&self) -> &Declaration;
    /// This backend, declaring this installation's identity as its version.
    fn under(self: &Arc<Self>, installation: Installation) -> Arc<Self>;
}

macro_rules! installable {
    ($kind:ident) => {
        impl Installable for dyn $kind {
            fn declared(&self) -> &Declaration {
                self.declaration()
            }

            fn under(self: &Arc<Self>, installation: Installation) -> Arc<Self> {
                Arc::new(Under {
                    declaration: self.declaration().installed_as(installation),
                    backend: self.clone(),
                })
            }
        }
    };
}

installable!(Segmenter);
installable!(Filler);
installable!(Judge);

/// Of these backends, the one a call that resolved this model is handed (ADR 0009;
/// ADR 0015, "An inventory"), or none.
///
/// - **A version that is not an installation's identity** is matched as it always
///   was: the first backend that declares this model, by its backend, its model and
///   its version.
/// - **A version that is an installation's identity is a local model's.** A roster
///   over an inventory resolves a local backend under the identity of its model's
///   installation, which no backend declares. So the run is handed a local
///   backend that declares this backend and this model: one that declares this
///   very version, when there is one, as it is and wherever it stands; and
///   otherwise the first, whatever version it declares, wrapped so that it
///   declares the identity: the one the admission recorded, which is the one in
///   the prepared call. **A backend that names a
///   recipient is never handed for such a version,** though it declare the same
///   three texts: a run that was resolved as local sends nothing away.
pub(crate) fn under<B: Installable + ?Sized>(
    backends: &[Arc<B>],
    model: &ModelIdentity,
) -> Option<Arc<B>> {
    let Some(installation) = Installation::from_version(model.version()) else {
        let mut backends = backends.iter();
        return backends
            .find(|backend| backend.declared().model() == model)
            .cloned();
    };
    let local = || {
        backends.iter().filter(|backend| {
            let declared = backend.declared();
            declared.is_local()
                && declared.model().backend() == model.backend()
                && declared.model().model() == model.model()
        })
    };
    // One that declares the identity itself is the exact match, by all three
    // texts, as it was before an inventory: it is handed as it is, wherever it
    // stands among the others. Only with none is the first of that backend and
    // that model handed, under the identity.
    if let Some(exact) = local().find(|backend| backend.declared().model() == model) {
        return Some(exact.clone());
    }
    local().next().map(|first| first.under(installation))
}

impl std::fmt::Display for Installation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Why a local model is not ready. Each is fixed words: none holds a name, a path or
/// anything read from a disk. There is no fifth: an inventory that cannot tell, or
/// cannot read what it found, says [`Unready::Damaged`], never ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unready {
    /// The catalogue holds the model, and no installation of it is in use.
    NotInstalled,
    /// An installation is in use, and it was made from another revision of the
    /// catalogue's entry or of the adapter than the current one.
    OutOfDate,
    /// What is on the disk is not a whole, verified installation, or could not be
    /// read.
    Damaged,
    /// The catalogue does not hold the model.
    NotInCatalogue,
}

impl std::fmt::Display for Unready {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Unready::NotInstalled => "it is not installed",
            Unready::OutOfDate => "its installation is out of date",
            Unready::Damaged => "its installation is damaged",
            Unready::NotInCatalogue => "it is not in the catalogue",
        })
    }
}

/// What an inventory answers of one model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installed {
    /// It is ready, under this installation.
    Ready(Installation),
    /// It is not, and why.
    NotReady(Unready),
}

/// What is installed on this machine. It is asked when a call is prepared, about the
/// model of the one local backend the call resolved, by the name the backend declares
/// for its model. It answers from what is there now, sends nothing and changes
/// nothing.
pub trait Inventory: Send + Sync {
    /// Whether this model is ready, and under which installation; or why it is not.
    fn look(&self, model: &str) -> Installed;
}

/// The inventory that knows no model: every model is not in its catalogue, so no
/// local backend is ready under it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoInventory;

impl Inventory for NoInventory {
    fn look(&self, _: &str) -> Installed {
        Installed::NotReady(Unready::NotInCatalogue)
    }
}

/// A roster over an inventory: what prepare asks ([`Backends`]) where local models
/// are installed and removed.
#[derive(Clone)]
pub struct InstalledRoster {
    roster: Roster,
    inventory: Arc<dyn Inventory>,
}

impl InstalledRoster {
    /// This roster, with the models of its local backends looked up in this
    /// inventory.
    pub fn over(roster: Roster, inventory: Arc<dyn Inventory>) -> InstalledRoster {
        InstalledRoster { roster, inventory }
    }

    /// The roster, as it was given.
    pub fn roster(&self) -> &Roster {
        &self.roster
    }
}

impl Backends for InstalledRoster {
    fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        features: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        let resolved = match self.roster.resolve(capability, choice, features) {
            Ok(resolved) => resolved,
            Err(unresolved) => return Err(Unavailable::new(&unresolved.to_string())),
        };
        let declared = resolved.declaration().model();
        if !resolved.declaration().is_local() {
            // The text of an installation's identity is a version only a local
            // model has: a run resolved under one is handed a local backend or
            // none ([`under`]), so a backend that names a recipient and declares
            // one would be admitted and handed nothing. It is not ready, and says
            // so.
            if Installation::from_version(declared.version()).is_some() {
                return Err(Unavailable::new(&format!(
                    "the {} backend {} names a recipient, and declares the identity of an \
                     installation as its version",
                    capability.name(),
                    resolved.name()
                )));
            }
            return Ok(declared.clone());
        }
        match self.inventory.look(declared.model()) {
            Installed::Ready(installation) => Ok(ModelIdentity::new(
                declared.backend(),
                declared.model(),
                &installation.to_string(),
            )),
            // The name is one the roster offers, and so a name; the reason is fixed
            // words. Nothing of the model is said.
            Installed::NotReady(why) => Err(Unavailable::new(&format!(
                "the {} backend {} is local, and its model is not ready: {why}",
                capability.name(),
                resolved.name()
            ))),
        }
    }

    /// The most one request may cost when this model answers the capability, as a
    /// [`Roster`] answers it, with one difference. The model is named as it was
    /// prepared, so a local model comes under its installation's identity, which no
    /// declaration holds: a local backend is matched by its backend and its model
    /// alone, whatever the version. The inventory is not asked. So the answer is
    /// never under what the roster would say of the backend that was resolved; where
    /// two local backends of one capability declare one backend and one model under
    /// two versions, it is the larger of theirs.
    fn bound(&self, capability: Capability, model: &ModelIdentity) -> Estimate {
        self.roster
            .offered()
            .filter(|(offered, declaration)| {
                let declared = declaration.model();
                *offered == capability
                    && match declaration.is_local() {
                        true => {
                            declared.backend() == model.backend()
                                && declared.model() == model.model()
                        }
                        false => declared == model,
                    }
            })
            .map(|(_, declaration)| declaration.most().micros())
            .max()
            .map_or_else(Estimate::default, Estimate::of_micros)
    }
}
