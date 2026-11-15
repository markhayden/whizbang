//! Readiness from what is installed (ADR 0017, "The inventory"; ADR 0015, "An
//! inventory"): the core's [`Inventory`] over a models directory and a catalogue.
//!
//! It is asked by the name a backend declares for its model, which comes from a
//! config: the name is looked up in the catalogue, compared exactly, and is never
//! made into a path. Only a model the catalogue holds has a folder that is read,
//! and that folder is named from the catalogue's own entry.
//!
//! It holds no lock and reads no file of an installation: it answers from the
//! pointer and the record the pointer names ([`super::current`]). That the files
//! are whole was held when they were published. It answers from what is there now,
//! each time it is asked.

use std::collections::BTreeMap;

use whizbang_core::capability::{Installation, Installed, Inventory, Unready};
use whizbang_core::contract::Secrets;

use super::install::{Current, current};
use super::name::holds_secret;
use super::{Catalogue, InstallationRecord, ModelName, ModelsDir, Revision};

/// What is installed in one models directory, as readiness asks it: the models of
/// one catalogue, under the adapters this build has. What `{:?}` prints holds no
/// path and no name.
pub struct Installations {
    dir: ModelsDir,
    catalogue: Catalogue,
    adapters: BTreeMap<ModelName, Revision>,
    secrets: Secrets,
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

impl Inventory for Installations {
    fn look(&self, model: &str) -> Installed {
        match self.ready(model) {
            Ok(record) => Installed::Ready(Installation::of(record.identity().digest())),
            Err(why) => Installed::NotReady(why),
        }
    }
}
