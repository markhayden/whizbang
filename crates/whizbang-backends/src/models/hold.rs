//! Holds on an installation (ADR 0017, "Holds"): what keeps an installation's files on
//! the disk while a run uses them.
//!
//! A hold is a shared lock on the installation's hold file, `holds/<64 hex digits>`,
//! and nothing else: no process id is written or read, and nothing is signalled. The
//! operating system lets a lock go when its process ends, however it ends, so a hold
//! of a process that died is no hold.
//!
//! A hold is given or refused at once, under the directory's lock, after the draining
//! mark and the pointer were read there: it is given only on the installation the
//! pointer names, and only while no mark is set whose remover is alive
//! ([`super::draining`]). It has the model's count of removals as it read it there
//! ([`Hold::removals`]). It makes nothing on the disk but the
//! directory's `LOCK`, when that is not there yet; the hold file was made by the
//! install that first published the identity.
//!
//! Inside one process the holds on one installation are counted ([`Holds`]): the
//! process keeps one share of the hold file while the count is not nothing, and lets
//! the share go with the last hold. The count is an economy of open files, never a
//! reason to give a hold: every hold reads the mark and the pointer.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use whizbang_core::contract::Secrets;
use whizbang_core::store::fs::Held;

use super::directory::{LINKED, is_linked};
use super::install::{Pointer, read_pointer};
use super::name::holds_secret;
use super::remove::{removals_read, stops};
use super::{InstallationId, ModelName, ModelsDir};

/// Why a hold was not given. Fixed words, and no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldRefused {
    /// The model's name holds a secret the holder was told of. Nothing was opened by
    /// it, and nothing of it is shown.
    HoldsSecret,
    /// The model's pointer names another installation, or none: this one is not the
    /// installation in use.
    NotNamed,
    /// The model is being removed.
    Draining,
    /// The pointer cannot be read as a pointer, the model's count of removals
    /// cannot be read as a count, or the installation the pointer names has no
    /// hold file. A hold makes none.
    Damaged,
    /// The hold file is held alone by one who is deleting the installation's folder.
    /// Nothing was waited for: the hold may be asked for again.
    Deleting,
    /// The directory's lock was not let go by whoever has it.
    DirectoryBusy,
    /// The identity that was asked for cannot be read back as an installation's:
    /// its text holds a secret the holder was told of. No installation is named by
    /// it, and nothing was opened.
    NotAnIdentity,
    /// A folder of the model's layout is a link. No hold is taken through it, and
    /// nothing was made.
    Linked,
    /// The file system refused, or cannot try for a share. The kind of failure, and no
    /// path.
    Disk(io::ErrorKind),
}

impl std::fmt::Display for HoldRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            HoldRefused::HoldsSecret => "the model's name holds a secret, and is not shown",
            HoldRefused::NotNamed => "the installation is not the one the model's pointer names",
            HoldRefused::Draining => "the model is being removed, and no hold on it is given",
            HoldRefused::Damaged => {
                "the model's pointer or its count of removals cannot be read, or its installation has no hold file"
            }
            HoldRefused::Deleting => "the installation is being deleted",
            HoldRefused::DirectoryBusy => {
                "the models directory is locked by another, which did not let it go"
            }
            HoldRefused::NotAnIdentity => {
                "the installation's identity cannot be read as one, and is not shown"
            }
            HoldRefused::Linked => LINKED,
            HoldRefused::Disk(_) => "the models directory could not be read or locked",
        })
    }
}

impl std::error::Error for HoldRefused {}

impl From<io::Error> for HoldRefused {
    fn from(error: io::Error) -> HoldRefused {
        if is_linked(&error) {
            return HoldRefused::Linked;
        }
        HoldRefused::Disk(error.kind())
    }
}

/// One process's share of one hold file, and how many holds stand on it.
struct Share {
    _held: Box<dyn Held>,
    count: usize,
}

/// The shares of a process, by their hold file.
type Shares = Arc<Mutex<BTreeMap<PathBuf, Share>>>;

fn guard(shares: &Shares) -> MutexGuard<'_, BTreeMap<PathBuf, Share>> {
    shares.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The holds of one process on the installations of one models directory. A clone is
/// the same set. A process makes one of these for a directory and takes every hold
/// through it; two of them over one directory are as two processes are, each with a
/// share of its own. What `{:?}` prints holds no path.
#[derive(Clone)]
pub struct Holds {
    dir: ModelsDir,
    shares: Shares,
}

impl std::fmt::Debug for Holds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Holds")
    }
}

impl Holds {
    /// The holds of this process on the installations of this directory: none yet.
    pub fn over(dir: ModelsDir) -> Holds {
        Holds {
            dir,
            shares: Shares::default(),
        }
    }

    /// A hold on this installation of this model, given or refused at once.
    ///
    /// Under the directory's lock, for a moment: the draining mark is asked after
    /// ([`super::draining`]), then the pointer is read, then the count of removals,
    /// and the hold is given only when no live mark is set and the pointer names
    /// this installation. Then the process's share of the hold file is counted up, or,
    /// when the process has none, tried for, without waiting. Nothing is made but
    /// the directory's lock file when there is none, and nothing is waited for but
    /// that lock.
    ///
    /// The hold is let go by [`Hold::let_go`], or when it is dropped.
    pub fn hold(
        &self,
        model: &ModelName,
        installation: InstallationId,
        secrets: &Secrets,
    ) -> Result<Hold, HoldRefused> {
        if holds_secret(model.as_str(), secrets) {
            return Err(HoldRefused::HoldsSecret);
        }
        let dir = &self.dir;
        // Before the directory's lock is taken, or made: no lock is taken, and no
        // pointer read, through a folder of the layout that is a link.
        dir.unlinked(model)?;
        let folder = dir.model(model);
        let file = folder.join("holds").join(installation.hex());
        let _directory = dir.lock()?.ok_or(HoldRefused::DirectoryBusy)?;
        // The one question of a mark: set, and its remover alive. The mark of a
        // remover that died stops no hold.
        if stops(dir, model, secrets)? {
            return Err(HoldRefused::Draining);
        }
        match read_pointer(dir, &folder, secrets)? {
            Pointer::Names(named) if named == installation => {}
            Pointer::Names(_) | Pointer::None => return Err(HoldRefused::NotNamed),
            Pointer::Damaged => return Err(HoldRefused::Damaged),
        }
        // The count of removals, read under the same lock a removal raises it
        // under: a removal that sets its mark after this hold was given has moved
        // it from what the hold has.
        let Some(removals) = removals_read(dir, model, secrets)? else {
            return Err(HoldRefused::Damaged);
        };
        // One guard for the count and the share: the last hold let go by one thread
        // and a new one taken by another cannot cross, so no share is let go under a
        // hold that was just counted.
        let mut shares = guard(&self.shares);
        match shares.get_mut(&file) {
            Some(share) => share.count += 1,
            None => {
                let held = match dir.fs().try_share(&file) {
                    Ok(Some(held)) => held,
                    Ok(None) => return Err(HoldRefused::Deleting),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        return Err(HoldRefused::Damaged);
                    }
                    // A file system that cannot try has not said "held alone".
                    Err(error) => return Err(error.into()),
                };
                let share = Share {
                    _held: held,
                    count: 1,
                };
                shares.insert(file.clone(), share);
            }
        }
        drop(shares);
        Ok(Hold {
            shares: self.shares.clone(),
            file,
            installation,
            removals,
            gone: AtomicBool::new(false),
        })
    }

    /// How many holds this process has on this installation now.
    pub fn count(&self, model: &ModelName, installation: InstallationId) -> usize {
        let file = self.dir.model(model).join("holds").join(installation.hex());
        guard(&self.shares)
            .get(&file)
            .map_or(0, |share| share.count)
    }
}

/// One hold on one installation. While it is there the installation's folder is not
/// deleted, by an install that superseded it or by a removal. It is let go once,
/// however often it is told to and when it is dropped. What `{:?}` prints is the
/// installation's identity, and no path.
pub struct Hold {
    shares: Shares,
    file: PathBuf,
    installation: InstallationId,
    removals: u64,
    gone: AtomicBool,
}

impl std::fmt::Debug for Hold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hold({})", self.installation)
    }
}

impl Hold {
    /// The installation that is held.
    pub fn installation(&self) -> InstallationId {
        self.installation
    }

    /// The model's count of removals as this hold read it, under the directory's
    /// lock, when it was given ([`super::removals`]). Each removal that sets its
    /// mark raises the count under that lock, and a hold is given only while no
    /// live mark is set: so a count that has moved from this one says that a
    /// removal of the model began after the hold was given, whether or not its
    /// mark is still there (the plan of checkpoint 1C, decision D5).
    pub fn removals(&self) -> u64 {
        self.removals
    }

    /// Lets the hold go. The first call counts the process's holds on the installation
    /// down by one, and lets the process's share of the hold file go with the last of
    /// them; every later call, and the drop, does nothing.
    pub fn let_go(&self) {
        if self.gone.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut shares = guard(&self.shares);
        let last = shares.get_mut(&self.file).is_none_or(|share| {
            share.count = share.count.saturating_sub(1);
            share.count == 0
        });
        if last {
            // Dropped under the guard: the share is gone before the next hold is
            // counted.
            shares.remove(&self.file);
        }
    }
}

impl Drop for Hold {
    /// Lets the hold go if it was not let go. **Nothing here can panic:** a lease
    /// drops its hold while a panic unwinds through the places around a request
    /// (ADR 0015, "A wrapper around each request"), and a drop that panicked there
    /// would abort the process. `let_go` takes its guard whether or not the lock is
    /// poisoned, counts down without going under nothing, and drops a share.
    fn drop(&mut self) {
        self.let_go();
    }
}
