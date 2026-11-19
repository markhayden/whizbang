//! What a user adds to a models directory beside what this build ships (plan 1D, task
//! U1; ADR 0017, "A user's weights and a user's adapter"): a catalogue of the user's own
//! models, and the registry of the adapters the user added.
//!
//! ```text
//! <models>/
//!   catalogue.json                 the user's catalogue, catalogue/2, written by the user
//!   adapters/
//!     REGISTRY                     the registry, adapters/1: each user's adapter and its hash
//!     REGISTRY.new                 a registry being written, until it is renamed over REGISTRY
//!     <64 hex digits>.py           a copy of a user's adapter, by its hash, read-only
//!     <64 hex digits>.new          a copy being written, until it is renamed into place
//! ```
//!
//! **A user's adapter is code the user chose.** It comes into the directory only by
//! [`add_adapter`], from a file the user names, never from a download; it is named
//! `user-<name>`, a prefix no adapter of this build has, so no user's adapter takes the
//! name of one this build ships; its copy is named by its SHA-256 and made read-only for
//! its owner; and its revision is that hash, as 64 hex digits. An installation of a
//! model under it records that revision, so its identity binds the hash, and the worker
//! runs the copy only when its bytes hash to it (`pack/src/whizbang_pack/installed.py`).
//! Adding a changed file under the same name is a new hash: the registry names the new
//! copy, and the earlier copy is kept, so an installation made under it still runs it.
//! A copy is removed only by [`remove_unnamed_adapters`], once neither the registry nor
//! any installation's record names it (plan 2C, task F1).

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use serde::Serialize;
use whizbang_core::contract::Secrets;
use whizbang_core::identity::Digest;

use super::catalogue::{CATALOGUE_MAX_BYTES, hash_of};
use super::directory::Read;
use super::installation::INSTALLATION_MAX_BYTES;
use super::json::{self, Object};
use super::{Catalogue, InstallationRecord, ModelName, ModelsDir, Refused, Revision, Rule, What};

/// The format of the registry this reader reads.
pub const REGISTRY_FORMAT: &str = "adapters/1";

/// The most bytes of the registry.
pub const REGISTRY_MAX_BYTES: u64 = 64 << 10;

/// The most adapters a user adds to one directory.
pub const USER_ADAPTERS_LIMIT: usize = 64;

/// The most bytes of a user's adapter: one Python file.
pub const ADAPTER_MAX_BYTES: u64 = 1 << 20;

/// The prefix of the name of every adapter a user adds. No adapter of this build has it.
pub const USER_PREFIX: &str = "user-";

/// Whether this adapter's name is one a user added.
pub fn is_users(adapter: &ModelName) -> bool {
    adapter.as_str().starts_with(USER_PREFIX)
}

impl ModelsDir {
    /// `adapters/`: the registry of users' adapters and their copies.
    pub(super) fn adapters(&self) -> PathBuf {
        self.root().join("adapters")
    }
}

/// The user's catalogue in this directory (`catalogue.json`), read by the reader of
/// the catalogue this build ships, and by its rules: none when nothing has the name.
/// What is there and is not a regular file under its own name, or cannot be read, is
/// refused.
pub fn users_catalogue(dir: &ModelsDir, secrets: &Secrets) -> Result<Option<Catalogue>, Refused> {
    let unreadable = |kind| Refused::Unreadable {
        what: What::Catalogue,
        kind,
    };
    match dir.read(&dir.root().join("catalogue.json"), CATALOGUE_MAX_BYTES) {
        Ok(Read::Missing) => Ok(None),
        Ok(Read::NotAFile) => Err(unreadable(io::ErrorKind::InvalidData)),
        Ok(Read::Bytes(bytes)) => Catalogue::parse(&bytes, secrets).map(Some),
        Err(error) => Err(unreadable(error.kind())),
    }
}

/// Why a user's adapter was not added. Fixed words, and no path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterRefused {
    /// `user-` and the name given is not a model's name.
    Name,
    /// The file is empty.
    Empty,
    /// The file is over [`ADAPTER_MAX_BYTES`].
    TooLarge,
    /// The registry holds [`USER_ADAPTERS_LIMIT`] adapters, and this is a new name.
    Full,
    /// Another held the directory's lock past its wait.
    Busy,
    /// `adapters/`, the registry or a copy is a link, or not a file where one belongs.
    Linked,
    /// The registry cannot be read as one.
    Registry(Refused),
    /// The disk refused: the kind of failure.
    Disk(io::ErrorKind),
}

impl std::fmt::Display for AdapterRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdapterRefused::Name => write!(
                f,
                "`{USER_PREFIX}` and the name is not an adapter's name: lower-case letters, \
                 digits, underscores, dots and hyphens"
            ),
            AdapterRefused::Empty => f.write_str("the adapter's file is empty"),
            AdapterRefused::TooLarge => {
                write!(f, "the adapter's file is over {ADAPTER_MAX_BYTES} bytes")
            }
            AdapterRefused::Full => write!(
                f,
                "the models directory holds {USER_ADAPTERS_LIMIT} users' adapters already"
            ),
            AdapterRefused::Busy => f.write_str("another holds the models directory; ask again"),
            AdapterRefused::Linked => f.write_str(
                "the folder of users' adapters, its registry or a copy is a link or not a \
                 file, and nothing is done through it",
            ),
            AdapterRefused::Registry(refused) => {
                write!(
                    f,
                    "the registry of users' adapters cannot be read: {refused}"
                )
            }
            AdapterRefused::Disk(_) => {
                f.write_str("the adapter could not be copied into the models directory")
            }
        }
    }
}

impl std::error::Error for AdapterRefused {}

impl From<io::Error> for AdapterRefused {
    fn from(error: io::Error) -> AdapterRefused {
        AdapterRefused::Disk(error.kind())
    }
}

/// What [`add_adapter`] did: the adapter's name, its revision (the copy's hash as 64 hex
/// digits), and whether the copy was written now or found whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub name: ModelName,
    pub revision: Revision,
    pub copied: bool,
}

#[derive(Serialize)]
struct Written<'a> {
    format: &'a str,
    adapters: Vec<WrittenAdapter>,
}

#[derive(Serialize)]
struct WrittenAdapter {
    name: String,
    sha256: String,
}

/// The registry in its one form: compact JSON, the adapters in the order of their names.
fn to_bytes(table: &BTreeMap<ModelName, Revision>) -> Vec<u8> {
    let written = Written {
        format: REGISTRY_FORMAT,
        adapters: table
            .iter()
            .map(|(name, revision)| WrittenAdapter {
                name: name.as_str().to_owned(),
                sha256: format!("sha256:{}", revision.as_str()),
            })
            .collect(),
    };
    // Writing texts and a list of them as JSON cannot fail.
    serde_json::to_vec(&written).unwrap_or_default()
}

/// Reads these bytes as the registry, strictly and in its one form: each adapter's name
/// starts with [`USER_PREFIX`] and is named once, and its hash is a SHA-256.
fn parse(bytes: &[u8], secrets: &Secrets) -> Result<BTreeMap<ModelName, Revision>, Refused> {
    let what = What::Registry;
    let json = json::parse(what, bytes, REGISTRY_MAX_BYTES, secrets)?;
    let mut top = Object::of(json, "")?;
    if top.text("format")? != REGISTRY_FORMAT {
        return Err(Refused::at("format", Rule::Format));
    }
    let mut table = BTreeMap::new();
    let listed = top.list("adapters", USER_ADAPTERS_LIMIT)?;
    for (n, item) in listed.into_iter().enumerate() {
        let at = format!("adapters[{n}]");
        let mut object = Object::of(item, &at)?;
        let name = ModelName::new(&object.text("name")?, secrets)
            .map_err(|refused| refused.under(&format!("{at}.name")))?;
        if !is_users(&name) {
            return Err(Refused::at(&format!("{at}.name"), Rule::ModelName));
        }
        let digest =
            hash_of(&object.text("sha256")?).map_err(|r| r.under(&format!("{at}.sha256")))?;
        object.done()?;
        let revision = Revision::new(&digest.hex(), secrets)?;
        if table.insert(name, revision).is_some() {
            return Err(Refused::at("adapters", Rule::NamedTwice));
        }
    }
    top.done()?;
    if to_bytes(&table) != bytes {
        return Err(Refused::OtherForm { what });
    }
    Ok(table)
}

/// The registry as it is now: empty when there is none.
fn read_registry(
    dir: &ModelsDir,
    secrets: &Secrets,
) -> Result<BTreeMap<ModelName, Revision>, AdapterRefused> {
    if dir.fs().is_link(&dir.adapters())? {
        return Err(AdapterRefused::Linked);
    }
    match dir.read(&dir.adapters().join("REGISTRY"), REGISTRY_MAX_BYTES)? {
        Read::Missing => Ok(BTreeMap::new()),
        Read::NotAFile => Err(AdapterRefused::Linked),
        Read::Bytes(bytes) => parse(&bytes, secrets).map_err(AdapterRefused::Registry),
    }
}

/// The adapters a user added to this directory, each with its revision: what the
/// inventory and an install are told of beside this build's own. Empty where none was
/// added.
pub fn registered(
    dir: &ModelsDir,
    secrets: &Secrets,
) -> Result<Vec<(ModelName, Revision)>, AdapterRefused> {
    Ok(read_registry(dir, secrets)?.into_iter().collect())
}

/// Adds a user's adapter (`whizbang models adapter add <file> --name <name>`): `code` is
/// the file's bytes, which the caller read from the file the user named. Under the
/// directory's lock: the copy `adapters/<hash>.py` is written, made read-only and renamed
/// into place, unless a copy of those bytes is there already; and the registry is
/// written with `user-<name>` naming that hash, by a new file renamed over the old. A
/// copy an earlier registry named is kept.
pub fn add_adapter(
    dir: &ModelsDir,
    name: &str,
    code: &[u8],
    secrets: &Secrets,
) -> Result<Added, AdapterRefused> {
    let named = ModelName::new(&format!("{USER_PREFIX}{name}"), secrets)
        .map_err(|_| AdapterRefused::Name)?;
    if code.is_empty() {
        return Err(AdapterRefused::Empty);
    }
    if code.len() as u64 > ADAPTER_MAX_BYTES {
        return Err(AdapterRefused::TooLarge);
    }
    let digest = Digest::of_bytes(code);
    let revision = Revision::new(&digest.hex(), secrets).map_err(|_| AdapterRefused::Name)?;
    let fs = dir.fs();
    let folder = dir.adapters();
    if fs.is_link(&folder)? {
        return Err(AdapterRefused::Linked);
    }
    dir.ensure_dir(&folder)?;
    let _lock = dir.lock()?.ok_or(AdapterRefused::Busy)?;
    let mut table = read_registry(dir, secrets)?;
    if !table.contains_key(&named) && table.len() >= USER_ADAPTERS_LIMIT {
        return Err(AdapterRefused::Full);
    }
    let copy = folder.join(format!("{}.py", digest.hex()));
    let whole = match dir.read(&copy, ADAPTER_MAX_BYTES)? {
        Read::Bytes(bytes) => Digest::of_bytes(&bytes) == digest,
        Read::Missing => false,
        Read::NotAFile => return Err(AdapterRefused::Linked),
    };
    if !whole {
        let new = folder.join(format!("{}.new", digest.hex()));
        dir.remove(&new)?;
        fs.write_new(&new, code)?;
        fs.set_read_only(&new)?;
        fs.rename(&new, &copy)?;
        fs.sync_dir(&folder)?;
    }
    table.insert(named.clone(), revision.clone());
    let beside = folder.join("REGISTRY.new");
    dir.remove(&beside)?;
    fs.write_new(&beside, &to_bytes(&table))?;
    fs.rename(&beside, &folder.join("REGISTRY"))?;
    fs.sync_dir(&folder)?;
    Ok(Added {
        name: named,
        revision,
        copied: !whole,
    })
}

/// Removes each copy of a user's adapter that neither the registry nor the record of
/// any installation in the directory names (plan 2C, task F1; the carry-over row on
/// copies of users' adapters never removed), under the directory's lock: what `models
/// remove` and `models get` run after they end. It answers how many it removed. It
/// removes nothing when the lock is held by another, when `adapters/` or a model's
/// layout is a link, or when any installation's record cannot be read: a copy is taken
/// away only when every name that could still run it was read. A copy being written
/// (`.new`) and the registry are never touched.
pub fn remove_unnamed_adapters(dir: &ModelsDir, secrets: &Secrets) -> io::Result<usize> {
    let fs = dir.fs();
    let folder = dir.adapters();
    if fs.is_link(&folder)? {
        return Ok(0);
    }
    let copies = match fs.list(&folder) {
        Ok(copies) => copies,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let Some(_lock) = dir.lock()? else {
        return Ok(0);
    };
    let Ok(table) = read_registry(dir, secrets) else {
        return Ok(0);
    };
    let mut named: Vec<String> = table.values().map(|r| r.as_str().to_owned()).collect();
    let models = match fs.list(&dir.models()) {
        Ok(models) => models,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    for path in models {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let Ok(model) = ModelName::new(name, secrets) else {
            return Ok(0);
        };
        if dir.has_link(&model)? {
            return Ok(0);
        }
        let folders = match fs.list(&dir.model(&model).join("installations")) {
            Ok(folders) => folders,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        for installation in folders {
            let record = installation.join("RECORD");
            let Read::Bytes(bytes) = dir.read(&record, INSTALLATION_MAX_BYTES)? else {
                return Ok(0);
            };
            let Ok(record) = InstallationRecord::parse(&bytes, secrets) else {
                return Ok(0);
            };
            if is_users(record.adapter()) {
                named.push(record.adapter_revision().as_str().to_owned());
            }
        }
    }
    let mut removed = 0;
    for path in copies {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let Some(hex) = name.strip_suffix(".py") else {
            continue;
        };
        let is_hash =
            hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if is_hash && !named.iter().any(|kept| kept == hex) {
            dir.remove(&path)?;
            removed += 1;
        }
    }
    if removed > 0 {
        fs.sync_dir(&folder)?;
    }
    Ok(removed)
}
