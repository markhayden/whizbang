//! Readiness from what is installed (ADR 0017, "The inventory"; ADR 0015, "An
//! inventory"): the core's [`Inventory`] over a models directory and a catalogue.
//!
//! It is asked by the name a backend declares for its model, which comes from a
//! config: the name is looked up in the catalogue, compared exactly, and is never
//! made into a path. Only a model the catalogue holds has a folder that is read,
//! and that folder is named from the catalogue's own entry.
//!
//! It holds no lock and reads no file of an installation: it answers from the
//! pointer and the record the pointer names ([`super::current`]), after a look
//! at the model's layout for a folder that is a link, which is damage. That the files
//! are whole was held when they were published. It answers from what is there now,
//! each time it is asked.

use std::collections::BTreeMap;
use std::sync::Arc;

use whizbang_core::capability::{
    Around, Ended, Fault, FaultKind, Installation, Installed, Inventory, Nothing, Open, Stop,
    Unready, Wrapped,
};
use whizbang_core::contract::{RefusalKind, Secrets};

use super::install::{Current, current};
use super::name::holds_secret;
use super::{
    Catalogue, Hold, HoldRefused, Holds, InstallationId, InstallationRecord, ModelName, ModelsDir,
    Revision,
};

/// What is installed in one models directory, as readiness asks it: the models of
/// one catalogue, under the adapters this build has. What `{:?}` prints holds no
/// path and no name.
pub struct Installations {
    dir: ModelsDir,
    catalogue: Catalogue,
    adapters: BTreeMap<ModelName, Revision>,
    secrets: Secrets,
    /// This process's holds on the installations of the directory.
    holds: Holds,
}

impl std::fmt::Debug for Installations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Installations({} models)",
            self.catalogue.entries().len()
        )
    }
}

impl Installations {
    /// The inventory of this directory, for the models of this catalogue. `adapters`
    /// is each adapter this build has with the revision it states of itself: no
    /// adapter is in the build yet, so whoever makes the inventory states them.
    pub fn of(
        dir: ModelsDir,
        catalogue: Catalogue,
        adapters: impl IntoIterator<Item = (ModelName, Revision)>,
        secrets: Secrets,
    ) -> Installations {
        Installations {
            holds: Holds::over(dir.clone()),
            dir,
            catalogue,
            adapters: adapters.into_iter().collect(),
            secrets,
        }
    }

    /// The record of the installation of this model that is ready, when one is: what
    /// [`Inventory::look`] answers, with the record and not only its identity.
    pub fn ready(&self, model: &str) -> Result<InstallationRecord, Unready> {
        // The name comes from a config. It is compared with the names the catalogue
        // holds, exactly, and goes no further: the folder that is read is named by
        // the catalogue's entry.
        let Some(entry) = self.catalogue.entry(model) else {
            return Err(Unready::NotInCatalogue);
        };
        // The catalogue may have been made by a reader that was told of other
        // secrets than this inventory.
        if holds_secret(entry.name().as_str(), &self.secrets) {
            return Err(Unready::NotInCatalogue);
        }
        // A model one of whose layout folders is a link is refused by every
        // install, hold and removal (ADR 0017, "A folder of the layout that is a
        // link"), so it is not called ready on the word of a pointer read through
        // the link. A disk that cannot be looked at is damaged, as one that
        // cannot be read is.
        if self.dir.unlinked(entry.name()).is_err() {
            return Err(Unready::Damaged);
        }
        let record = match current(&self.dir, entry.name(), &self.secrets) {
            Ok(Current::Installed(record)) => record,
            Ok(Current::NotInstalled) => return Err(Unready::NotInstalled),
            // What cannot be read, and a pointer that would not hold still, is
            // never ready and never "not installed".
            Ok(Current::Damaged(_)) | Err(_) => return Err(Unready::Damaged),
        };
        // Current is what an install would make now, from this entry under the
        // revision this build's adapter states: the same record, or the
        // installation is out of date. Two revisions are the same or they are not.
        let Some(revision) = self.adapters.get(entry.adapter()) else {
            return Err(Unready::OutOfDate);
        };
        match InstallationRecord::of(entry, revision.clone(), &self.secrets) {
            Ok(now) if now == record => Ok(record),
            Ok(_) => Err(Unready::OutOfDate),
            Err(_) => Err(Unready::Damaged),
        }
    }
}

impl Installations {
    /// A hold on the installation a run was admitted with, by its identity: what a
    /// lease takes before a request to a local model leaves (ADR 0017, "The
    /// lease"). It is given or refused at once.
    ///
    /// `model` is the name the backend declares, as [`Inventory::look`] is asked by
    /// it: looked up in the catalogue, and never made into a path. `admitted` is
    /// the installation the admission recorded, not the one that is current now:
    /// when the model's pointer no longer names it, the hold is refused
    /// ([`HoldRefused::NotNamed`]) and no other installation answers in its place;
    /// when the model is draining, it is refused as [`HoldRefused::Draining`]. A
    /// model the catalogue does not hold has no installation to name.
    pub fn hold(&self, model: &str, admitted: Installation) -> Result<Hold, HoldRefused> {
        let Some(entry) = self.catalogue.entry(model) else {
            return Err(HoldRefused::NotNamed);
        };
        // An installation's identity is a digest, written as one: the text is read
        // back as the identity it is.
        let Ok(identity) = InstallationId::from_text(&admitted.to_string(), &self.secrets) else {
            return Err(HoldRefused::HoldsSecret);
        };
        self.holds.hold(entry.name(), identity, &self.secrets)
    }
}

impl Inventory for Installations {
    fn look(&self, model: &str) -> Installed {
        match self.ready(model) {
            Ok(record) => Installed::Ready(Installation::of(record.identity().digest())),
            Err(why) => Installed::NotReady(why),
        }
    }
}

/// The lease on an installation (ADR 0017, "The lease"; ADR 0015, "The backend a run
/// is handed, and the lease's place"): what stands in the lease's place of an
/// executor over this inventory ([`whizbang_core::contract::Place::Lease`]).
///
/// Before a request to a local model leaves, it takes a hold
/// ([`Installations::hold`]) on the installation the run was admitted with: the
/// identity the backend the run was handed declares as its version, which is the one
/// the admission recorded. With the hold the request goes on, and the hold is let go
/// when the request is over, however it ends. Without it the request is refused
/// before the meter's place and the backend see it: its action is answered a fault
/// that says nothing left, and the refusal is noted in the run's record. No other
/// installation answers in the place of the one that was admitted.
#[derive(Debug, Clone)]
pub struct Lease {
    installations: Arc<Installations>,
}

impl Lease {
    /// The lease over the holds of this inventory's process.
    pub fn over(installations: Arc<Installations>) -> Lease {
        Lease { installations }
    }
}

/// Why a request was refused a lease. Fixed words: no name, no path, no identity.
fn why_not(refused: Option<HoldRefused>) -> String {
    match refused {
        None => "the run was not admitted under an installation of the model".to_owned(),
        Some(HoldRefused::NotNamed) => {
            "the model's installation was replaced or removed after the run was admitted".to_owned()
        }
        Some(other) => other.to_string(),
    }
}

impl Around for Lease {
    fn before(&self, asked: &Wrapped<'_>) -> Result<Box<dyn Open>, Stop> {
        let declaration = asked.declaration;
        // A backend that names a recipient answers with no model of this machine:
        // nothing is held for it.
        if !declaration.is_local() {
            return Ok(Box::new(Nothing));
        }
        // A local backend is handed to a run under the identity of the installation
        // the run was admitted with (ADR 0015). One that comes under any other
        // version was admitted over no inventory: there is nothing to hold it to,
        // and it is refused.
        let model = declaration.model();
        let held = match Installation::from_version(model.version()) {
            Some(admitted) => self
                .installations
                .hold(model.model(), admitted)
                .map_err(Some),
            None => Err(None),
        };
        let why = match held {
            Ok(hold) => return Ok(Box::new(Leased(hold))),
            Err(refused) => why_not(refused),
        };
        let fault = Fault::new(FaultKind::Rejected, &why);
        match Stop::new(RefusalKind::RequestNotReady, &why, fault) {
            Some(stop) => Err(stop),
            // The kind is one of a request: this is not reached.
            None => Ok(Box::new(Nothing)),
        }
    }
}

/// What the lease holds for one request: the hold, which is let go when the lease
/// is told how the request ended, and when it is dropped without being told (the
/// backend panicked, or a place after the lease's did before the request left).
struct Leased(Hold);

impl Open for Leased {
    fn after(self: Box<Self>, _: &Wrapped<'_>, _: &Ended<'_>) -> Result<(), String> {
        self.0.let_go();
        Ok(())
    }
}
