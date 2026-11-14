//! The models directory (`SPEC.md` section 3, "Storage"; ADR 0017): where installed
//! models live, outside any store, and what is written in it.
//!
//! This is its types, and no more: the names that become names on a disk, the catalogue
//! of what can be installed, the record of one installation, and that installation's
//! identity. **Nothing here installs, downloads or touches a disk.** The installer, its
//! status, the holds, removal and the inventory are built on these by the later tasks
//! of checkpoint 1B's models strand (`docs/plans/`).
//!
//! Every type has private fields and is made through a constructor that checks it. A
//! constructor that takes a text from outside the build is told the executor's secrets,
//! and looks at the whole text for one before it judges it by any rule: a text that
//! holds a secret is refused whole, and nothing of it is shown. A refusal
//! ([`Refused`]) is fixed words, a place in a record and numbers: it quotes nothing of
//! what was refused, and holds no path.

mod catalogue;
mod installation;
mod json;
mod name;
mod refused;

pub use catalogue::{
    CATALOGUE_FORMAT, CATALOGUE_MAX_BYTES, Catalogue, CatalogueFile, Entry, EntryParts,
    FEATURES_LIMIT, FILES_LIMIT, MODELS_LIMIT, ORIGINS_LIMIT,
};
pub use installation::{
    INSTALLATION_FORMAT, INSTALLATION_MAX_BYTES, InstallationId, InstallationRecord, InstalledFile,
};
pub use name::{
    FILE_MAX_BYTES, FILE_NAME_LIMIT, FileName, LICENCE_LIMIT, Licence, MODEL_NAME_LIMIT, ModelName,
    REVISION_LIMIT, Revision, SHOWN_LIMIT,
};
pub use refused::{Refused, Rule, What};
