//! The models directory (`SPEC.md` section 3, "Storage"; ADR 0017): where installed
//! models live, outside any store, and what is written in it.
//!
//! Its types are the names that become names on a disk, the catalogue of what can be
//! installed, the record of one installation, and that installation's identity. On
//! them: a download into staging ([`stage`]), an install that publishes what it staged
//! and replaces the model's pointer ([`install`]), the reader of that pointer
//! ([`current`]), the status record an installer writes ([`status`]), a request
//! that attaches to an install that is running ([`install_or_attach`]), and the holds
//! of a process on the installations in use ([`Holds`]), and a removal that drains
//! them, clears the pointer and deletes the model's files ([`remove`]); and what is
//! installed as readiness asks it ([`Installations`]). The disk is reached only through
//! the store's file-system interface, and every download is behind the interface of
//! ADR 0012: no network client is in the build.
//!
//! Every type has private fields and is made through a constructor that checks it. A
//! constructor that takes a text from outside the build is told the executor's secrets,
//! and looks at the whole text for one before it judges it by any rule: a text that
//! holds a secret is refused whole, and nothing of it is shown. A refusal
//! ([`Refused`]) is fixed words, a place in a record and numbers: it quotes nothing of
//! what was refused, and holds no path.

mod catalogue;
mod directory;
mod hold;
mod install;
mod installation;
mod inventory;
mod json;
mod name;
mod refused;
mod remove;
mod stage;
mod status;

pub use catalogue::{
    CATALOGUE_FORMAT, CATALOGUE_MAX_BYTES, Catalogue, CatalogueFile, Entry, EntryParts,
    FEATURES_LIMIT, FILES_LIMIT, MODELS_LIMIT, ORIGINS_LIMIT, PLATFORMS_LIMIT,
};
pub use directory::{ModelsDir, OpenRefused};
pub use hold::{Hold, HoldRefused, Holds};
pub use install::{Current, Damage, InstallRefused, current, install, install_or_attach};
pub use installation::{
    INSTALLATION_FORMAT, INSTALLATION_MAX_BYTES, InstallationId, InstallationRecord, InstalledFile,
};
pub use inventory::{Installations, Lease};
pub use name::{
    FILE_MAX_BYTES, FILE_NAME_LIMIT, FileName, LICENCE_LIMIT, Licence, MODEL_NAME_LIMIT, ModelName,
    Platform, REVISION_LIMIT, Revision, SHOWN_LIMIT, TITLE_LIMIT, Title,
};
pub use refused::{Refused, Rule, What};
pub use remove::{
    DRAINING_FORMAT, DRAINING_MAX_BYTES, Mark, RemoveRefused, Removed, draining, remove,
};
pub use stage::{FileRefused, MOVES_LIMIT, StageRefused, Staged, stage};
pub use status::{STATUS_FORMAT, STATUS_MAX_BYTES, State, Status, status};
