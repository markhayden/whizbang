//! The file system the store talks to, as a trait, so that a test can stand between the store
//! and the disk, record the order of writes, syncs and renames, and drop what was never
//! synced, as a power loss would. [`RealFs`] is the real one.
//!
//! Durability has two halves. A file's content is durable when [`Fs::write_new`] returns:
//! it syncs through the handle it wrote with, so there is no unsynced write for the store to
//! forget. A name in a directory (a new file, a new directory, the new name of a rename, a
//! removal) is durable after [`Fs::sync_dir`] on that directory. Neither implies the other.
//!
//! The store reads through the same trait, so a store that goes on working after its file
//! system has crashed is caught too.
//!
//! Locks go through it as well, because a lock belongs to the process that took it and a
//! test's file system stands for a process: when it crashes, its locks are gone. A lock is
//! advisory and is held on an open file: it ends when the value that holds it is dropped,
//! or when the process ends, however it ends. Two holders in one process exclude each other
//! exactly as two processes do.

use std::io;
use std::path::{Path, PathBuf};

/// A lock that is held until this value is dropped.
pub trait Held: Send + Sync {}

/// What the store needs from a file system.
pub trait Fs: Send + Sync {
    /// Creates one directory. Its parent exists; the directory does not.
    fn create_dir(&self, path: &Path) -> io::Result<()>;
    /// Creates a file with this content and makes the content durable. Fails when the name
    /// is taken: the store never writes over a file, it replaces one by a rename. After a
    /// failure the file may be there, with any part of the content; the name is not durable
    /// either way until its directory is synced.
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    /// Makes a directory's entries durable.
    fn sync_dir(&self, path: &Path) -> io::Result<()>;
    /// Renames, replacing a file already at `to`. Atomic within one file system; across two
    /// it fails with [`io::ErrorKind::CrossesDevices`].
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Removes a file, or a directory with everything in it.
    fn remove_all(&self, path: &Path) -> io::Result<()>;
    /// The whole content of a file.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    /// The entries of a directory, as full paths, in order.
    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    /// Whether something has this name.
    fn exists(&self, path: &Path) -> io::Result<bool>;
    /// Takes the lock file at `path` for this process alone, without waiting, creating the
    /// file when there is none, and then writes `holder` as its whole content, for whoever
    /// is refused to read. `None` when another holder has it: nothing was created or
    /// written.
    fn hold_lock_file(&self, path: &Path, holder: &[u8]) -> io::Result<Option<Box<dyn Held>>>;
    /// Shares the file at `path` with any number of others, so that nobody can take it
    /// alone ([`Fs::try_alone`]) while this is held. Waits for one who has it alone. Writes
    /// and creates nothing; fails with [`io::ErrorKind::NotFound`] when there is no file.
    fn share(&self, path: &Path) -> io::Result<Box<dyn Held>>;
    /// Takes the file at `path` alone, without waiting: `None` when anyone shares it.
    /// Writes and creates nothing.
    fn try_alone(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>>;
}

/// An open file with a lock on it. Closing the file releases the lock.
struct LockedFile(#[allow(dead_code)] std::fs::File);

impl Held for LockedFile {}

fn taken(
    file: std::fs::File,
    lock: Result<(), std::fs::TryLockError>,
) -> io::Result<Option<std::fs::File>> {
    match lock {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

/// The real file system. What it creates is readable by its owner only.
///
/// On Linux a sync is `fsync`. On macOS the standard library's `sync_all` asks for
/// `F_FULLFSYNC`, which has the drive empty its cache too.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl Fs for RealFs {
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path)
    }

    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(bytes)?;
        // Through the handle that wrote: a sync on a handle opened afterwards can miss a
        // write error the system already reported, or dropped, before that handle existed.
        file.sync_all()
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::File::open(path)?.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn remove_all(&self, path: &Path) -> io::Result<()> {
        if std::fs::symlink_metadata(path)?.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path)? {
            entries.push(entry?.path());
        }
        entries.sort();
        Ok(entries)
    }

    fn exists(&self, path: &Path) -> io::Result<bool> {
        path.try_exists()
    }

    fn hold_lock_file(&self, path: &Path, holder: &[u8]) -> io::Result<Option<Box<dyn Held>>> {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        let lock = file.try_lock();
        let Some(mut file) = taken(file, lock)? else {
            return Ok(None);
        };
        // Only now, with the lock held: the content is the holder's to write.
        file.set_len(0)?;
        file.write_all(holder)?;
        Ok(Some(Box::new(LockedFile(file))))
    }

    fn share(&self, path: &Path) -> io::Result<Box<dyn Held>> {
        let file = std::fs::File::open(path)?;
        file.lock_shared()?;
        Ok(Box::new(LockedFile(file)))
    }

    fn try_alone(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        let file = std::fs::File::open(path)?;
        let lock = file.try_lock();
        Ok(taken(file, lock)?.map(|file| Box::new(LockedFile(file)) as Box<dyn Held>))
    }
}
