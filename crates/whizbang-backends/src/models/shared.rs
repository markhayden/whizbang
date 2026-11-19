//! Files shared between installations (ADR 0017, "A file shared between installations";
//! plan 1D, task Q1): an install whose file, by its size and its hash, is held already by
//! another installation in the models directory gives that file a second name in its
//! staging folder and downloads nothing for it; two installs that stage one file at once
//! download it once; and a removal of one installation leaves the file to the other.
//!
//! - **The link** is a second name for one file ([`Fs::hard_link`]): the file lives until
//!   its last name is removed, so deleting one installation, by a removal or by the
//!   cleaning of an install, keeps the file of another. It is read back under its new
//!   name and held to its size and its hash before it is kept, as a downloaded file is;
//!   one that is not whole is removed and the next held copy is tried, and then the
//!   download.
//! - **Where a held copy is looked for:** every installation of every model of the
//!   directory, by the record in its folder. A model whose layout has a link, a folder
//!   that is a link, a held file that is a link, a record that cannot be read and a name
//!   that is not an identity are passed over: nothing is followed or trusted, and what is linked is read whole.
//! - **The lock of a shared file** (`shared/<64 hex digits>`, by the file's hash) is
//!   taken alone by an install before it stages, for every file of its entry, in the
//!   order of the hashes, and held until the install ends: after its publication or its
//!   refusal. A second install of an entry that holds one of those files waits for it,
//!   by tries paced by the directory's clock, and looks for a held copy only once it has
//!   it, so it finds the first install's file published, or downloads when that install
//!   was refused. Taking them in one order means that no two installs each wait for a
//!   lock the other holds. The wait has no deadline, as a download has none; it stops
//!   at the install's cancellation and at a removal's mark.

use std::io;
use std::path::{Path, PathBuf};

use whizbang_core::contract::Secrets;
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::LockFile;

use super::directory::Read;
use super::install::file_is_whole;
use super::installation::INSTALLATION_MAX_BYTES;
use super::stage::StageRefused;
use super::{Entry, InstallationRecord, InstalledFile, ModelName, ModelsDir};

/// The locks of an entry's files, taken alone, in the order of their hashes, each once:
/// waited for until each is given, or `stopped` says why the wait ends.
pub(super) fn lock_files(
    dir: &ModelsDir,
    entry: &Entry,
    stopped: &dyn Fn() -> Option<StageRefused>,
) -> Result<Vec<Box<dyn LockFile>>, StageRefused> {
    let mut hashes: Vec<String> = entry
        .files()
        .iter()
        .map(|file| file.sha256().hex())
        .collect();
    hashes.sort();
    hashes.dedup();
    let folder = dir.shared();
    if dir.fs().is_link(&folder)? {
        return Err(StageRefused::Linked);
    }
    dir.ensure_dir(&folder)?;
    let mut held = Vec::new();
    for hash in hashes {
        let path = folder.join(hash);
        loop {
            if let Some(lock) = dir.fs().hold_lock_file(&path, true)? {
                held.push(lock);
                break;
            }
            if let Some(why) = stopped() {
                return Err(why);
            }
            dir.pace();
        }
    }
    Ok(held)
}

/// The files of this size and hash that the installations of the directory hold, by
/// their records: a path for each, in the order of the models' names and then of the
/// identities.
fn held(dir: &ModelsDir, size: u64, sha256: Digest, secrets: &Secrets) -> io::Result<Vec<PathBuf>> {
    let fs = dir.fs();
    let mut found = Vec::new();
    let models = match fs.list(&dir.models()) {
        Ok(models) => models,
        Err(error) if absent(&error) => return Ok(found),
        Err(error) => return Err(error),
    };
    for path in models {
        let Some(model) = name_of(&path).and_then(|name| ModelName::new(name, secrets).ok()) else {
            continue;
        };
        if dir.has_link(&model)? {
            continue;
        }
        let folders = match fs.list(&dir.model(&model).join("installations")) {
            Ok(folders) => folders,
            Err(error) if absent(&error) => continue,
            Err(error) => return Err(error),
        };
        for folder in folders {
            let is_identity = name_of(&folder).is_some_and(|name| {
                name.len() == 64
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            });
            if !is_identity || fs.is_link(&folder)? || fs.is_link(&folder.join("files"))? {
                continue;
            }
            let Read::Bytes(bytes) = dir.read(&folder.join("RECORD"), INSTALLATION_MAX_BYTES)?
            else {
                continue;
            };
            let Ok(record) = InstallationRecord::parse(&bytes, secrets) else {
                continue;
            };
            for file in record.files() {
                let path = folder.join("files").join(file.name().as_str());
                // A held file that is a link is passed over, as a folder that is one
                // is: it would be linked, and read, through it.
                if file.size() == size && file.sha256() == sha256 && !fs.is_link(&path)? {
                    found.push(path);
                }
            }
        }
    }
    Ok(found)
}

fn name_of(path: &Path) -> Option<&str> {
    path.file_name().and_then(|name| name.to_str())
}

fn absent(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}

/// Gives `target` the name of a file of this size and hash that another installation
/// holds, read back whole under its new name: true when one was linked, false when none
/// is held, none could be linked, or none was whole. `stop` is asked before each piece
/// read; a link that is stopped is removed, and the answer is false.
pub(super) fn link_held(
    dir: &ModelsDir,
    file: &InstalledFile,
    target: &Path,
    secrets: &Secrets,
    stop: &dyn Fn() -> bool,
) -> Result<bool, StageRefused> {
    let fs = dir.fs();
    for from in held(dir, file.size(), file.sha256(), secrets)? {
        if stop() {
            return Ok(false);
        }
        match fs.hard_link(&from, target) {
            Ok(()) => {}
            // Gone meanwhile, on another disk, or a file system that links nothing:
            // the next copy, or the download.
            Err(error) if is_a_link_refusal(&error) => continue,
            Err(error) => return Err(error.into()),
        }
        let whole = file_is_whole(fs, target, file, stop);
        if matches!(whole, Ok(true)) && !stop() {
            return Ok(true);
        }
        dir.remove(target)?;
        whole?;
    }
    Ok(false)
}

/// Whether a link was refused for a reason that a download goes round: the file is gone,
/// the two names are on two file systems, the system links nothing there, or too many
/// names have the file.
fn is_a_link_refusal(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound
            | io::ErrorKind::CrossesDevices
            | io::ErrorKind::Unsupported
            | io::ErrorKind::TooManyLinks
            | io::ErrorKind::PermissionDenied
    )
}

/// Whether every file of the entry is held by an installation of the directory, by its
/// record: what an install would link, and so download nothing for. Read without a lock;
/// what it says may be out of date by the time an install looks again, under its locks.
pub fn all_held(dir: &ModelsDir, entry: &Entry, secrets: &Secrets) -> io::Result<bool> {
    for file in entry.files() {
        if held(dir, file.size(), file.sha256(), secrets)?.is_empty() {
            return Ok(false);
        }
    }
    Ok(true)
}
