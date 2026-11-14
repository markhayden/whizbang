//! The models directory (`SPEC.md` section 3, "Storage"; ADR 0017): where installed
//! models live, outside any store, and what is written in it.
//!
//! So far this is the texts that become names on a disk or words in a record, and what
//! a refusal says. **Nothing here installs, downloads or touches a disk.** The
//! catalogue, the record of an installation and its identity come next, and after
//! them the installer, its status, the holds, removal and the inventory, by the tasks
//! of checkpoint 1B's models strand (`docs/plans/`).
//!
//! Every type has a private field and is made through a constructor that checks it. A
//! constructor that takes a text from outside the build is told the executor's secrets,
//! and looks at the whole text for one before it judges it by any rule: a text that
//! holds a secret is refused whole, and nothing of it is shown. A refusal
//! ([`Refused`]) is fixed words, a place in a record and numbers: it quotes nothing of
//! what was refused, and holds no path.

mod name;
mod refused;

pub use name::{
    FILE_MAX_BYTES, FILE_NAME_LIMIT, FileName, LICENCE_LIMIT, Licence, MODEL_NAME_LIMIT, ModelName,
    REVISION_LIMIT, Revision, SHOWN_LIMIT,
};
pub use refused::{Refused, Rule, What};
