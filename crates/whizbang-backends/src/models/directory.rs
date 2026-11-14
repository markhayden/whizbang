//! A models directory (ADR 0017, "The directory"): one folder, given by whoever opens
//! it, reached only through the store's file-system interface, so that a test can
//! stand between the installer and the disk.
//!
//! Every name under it is made here, from a [`ModelName`], a [`super::FileName`] or
//! the names this layout gives, and from nothing else. No error carries a path out.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::store::fs::{Fs, LockFile};

use super::ModelName;

/// How many times the directory's lock is tried before it is said to be busy.
const LOCK_TRIES: usize = 20_000;

/// What reading a small file of the directory found.
pub(super) enum Read {
    /// The file's bytes: all of them, or the most asked for and one more.
    Bytes(Vec<u8>),
    /// Nothing has the name, or a folder on the way to it is not there.
    Missing,
    /// What has the name is not a regular file under its own name: a link, a folder.
    NotAFile,
}

/// One models directory. What `{:?}` prints holds no path.
#[derive(Clone)]
pub struct ModelsDir {
    fs: Arc<dyn Fs>,
    root: PathBuf,
}

impl std::fmt::Debug for ModelsDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ModelsDir")
    }
}

impl ModelsDir {
    /// The models directory at this folder, which is there, over this file system.
    /// Nothing is read or made until something is asked of it.
    pub fn at(fs: Arc<dyn Fs>, root: &Path) -> ModelsDir {
        ModelsDir {
            fs,
            root: root.to_path_buf(),
        }
    }

    pub(super) fn fs(&self) -> &dyn Fs {
        &*self.fs
    }

    /// `models/`: the folder that holds a folder for each model and nothing else.
    pub(super) fn models(&self) -> PathBuf {
        self.root.join("models")
    }

    /// `models/<model>/`: one model's folder.
    pub(super) fn model(&self, model: &ModelName) -> PathBuf {
        self.models().join(model.as_str())
    }

    /// `models/<model>/staging/`: where an install of the model keeps its work.
    pub(super) fn staging(&self, model: &ModelName) -> PathBuf {
        self.model(model).join("staging")
    }

    /// `models/<model>/staging/incoming/`: the one staging folder of the model.
    pub(super) fn incoming(&self, model: &ModelName) -> PathBuf {
        self.staging(model).join("incoming")
    }

    /// Removes the model's staging folder and syncs the removal, for one who holds
    /// the model's lock. What cannot be removed now is removed by the next to stage.
    pub(super) fn unstage(&self, model: &ModelName) {
        let _ = self.remove(&self.incoming(model));
        let _ = self.fs.sync_dir(&self.staging(model));
    }

    /// The directory's lock (`LOCK`), taken alone for a moment: around every change
    /// of a pointer and the making of a hold file, and around the reading of the
    /// mark that a publication decides from. `None` when another did not let it go.
    ///
    /// The store's interface has no lock taken alone that waits, so this tries, a
    /// bounded count of times, giving way to other threads between tries, and
    /// sleeps nowhere. Whoever has the lock waits for nothing while it has it, so it
    /// is let go after a few calls of the file system.
    pub(super) fn lock(&self) -> io::Result<Option<Box<dyn LockFile>>> {
        let path = self.root.join("LOCK");
        for _ in 0..LOCK_TRIES {
            if let Some(lock) = self.fs.hold_lock_file(&path, true)? {
                return Ok(Some(lock));
            }
            std::thread::yield_now();
        }
        Ok(None)
    }

    /// Reads a small file whizbang wrote: at most `limit` bytes and one more, so
    /// that one that is too long is seen to be. Only a regular file under its own
    /// name is read: a link is not followed.
    pub(super) fn read(&self, path: &Path, limit: u64) -> io::Result<Read> {
        let mut file = match self.fs.open_file(path) {
            Ok(file) => file,
            Err(error) => {
                return match error.kind() {
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => Ok(Read::Missing),
                    io::ErrorKind::InvalidData => Ok(Read::NotAFile),
                    _ => Err(error),
                };
            }
        };
        let mut bytes = vec![
            0;
            usize::try_from(limit)
                .unwrap_or(usize::MAX)
                .saturating_add(1)
        ];
        let filled = file.read(&mut bytes)?;
        bytes.truncate(filled);
        Ok(Read::Bytes(bytes))
    }

    /// Makes a folder that may be there already, and makes its name durable when it
    /// was made. It is made and not looked for first: of two who make it at once, one
    /// makes it and the other finds it made.
    pub(super) fn ensure_dir(&self, path: &Path) -> io::Result<()> {
        match self.fs.create_dir(path) {
            Ok(()) => path
                .parent()
                .map_or(Ok(()), |parent| self.fs.sync_dir(parent)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Removes a file or a folder with all in it, that may not be there. A link is
    /// removed and not followed.
    pub(super) fn remove(&self, path: &Path) -> io::Result<()> {
        match self.fs.remove_all(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}
