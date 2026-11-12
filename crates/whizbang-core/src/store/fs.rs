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
//!
//! # What these locks are, and are not
//!
//! [`RealFs`] takes them with the standard library's file locks, which are `flock` on
//! Linux and on macOS. That sets their limits:
//!
//! - **Local file systems only.** On a network file system (NFS, SMB) `flock` may fail, may
//!   lock only on the machine that asked, or may be turned into another kind of lock with
//!   other rules. On Linux over NFS it becomes a byte-range lock, which a file opened only
//!   to read cannot take alone, so [`Fs::try_alone`], and with it the store's prune, fails
//!   there. A store on such a file system is not supported.
//! - **The lock is on the file, not on its name.** A lock file that is removed or replaced
//!   while it is held stays locked for its holder, and the next to come locks the new
//!   file and is admitted beside it. Nothing may remove or replace a store's `LOCK`.
//!   [`Fs::hold_lock_file`] closes the gap it can: having locked the file it opened, it
//!   looks at the name again, and if that is now another file, or none, it starts over.
//!   One that replaces the file after that look is not seen.
//! - **Advisory.** A program that does not ask for the lock is not stopped by it.

use std::io;
use std::path::{Path, PathBuf};

/// A lock that is held until this value is dropped.
pub trait Held: Send + Sync {}

/// A lock file, held until this value is dropped.
pub trait LockFile: Send + Sync {
    /// Makes `holder` the whole content of the file, for whoever is refused to read.
    /// Nothing depends on it, and it is not made durable.
    fn record(&self, holder: &[u8]) -> io::Result<()>;
}

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
    /// Takes the lock file at `path` for this process alone, without waiting. With
    /// `create`, the file is created when there is none; without, a missing file fails
    /// with [`io::ErrorKind::NotFound`]. `None` when another holder has it. The content of
    /// the file is not touched, whoever gets it.
    fn hold_lock_file(&self, path: &Path, create: bool) -> io::Result<Option<Box<dyn LockFile>>>;
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

impl LockFile for LockedFile {
    fn record(&self, holder: &[u8]) -> io::Result<()> {
        use std::io::{Seek, Write};
        let mut file = &self.0;
        file.set_len(0)?;
        file.seek(io::SeekFrom::Start(0))?;
        file.write_all(holder)
    }
}

/// Whether the open file is the one `path` names now.
#[cfg(unix)]
fn is_at(file: &std::fs::File, path: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let named = match std::fs::metadata(path) {
        Ok(named) => named,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let open = file.metadata()?;
    Ok((named.dev(), named.ino()) == (open.dev(), open.ino()))
}

/// Without a way to tell one file from another, the open file is taken to be the one.
#[cfg(not(unix))]
fn is_at(_file: &std::fs::File, _path: &Path) -> io::Result<bool> {
    Ok(true)
}

/// [`Fs::hold_lock_file`] for the real file system. `locked` runs each time a file has
/// been locked and before its name is looked at again: the moment a test needs.
fn hold_lock_file(
    path: &Path,
    create: bool,
    locked: &dyn Fn(),
) -> io::Result<Option<Box<dyn LockFile>>> {
    loop {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(create).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        let lock = file.try_lock();
        let Some(file) = taken(file, lock)? else {
            return Ok(None);
        };
        locked();
        // The lock is on the file that was opened. If the name now leads to another file,
        // or to none, this lock keeps nobody out who comes by the name: start over.
        if is_at(&file, path)? {
            return Ok(Some(Box::new(LockedFile(file))));
        }
    }
}

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

    fn hold_lock_file(&self, path: &Path, create: bool) -> io::Result<Option<Box<dyn LockFile>>> {
        hold_lock_file(path, create, &|| {})
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    // The test kit's scratch directory, removed when it is dropped, whether the test
    // passed or not. The kit is built on this crate as it ships, not as it is tested, so
    // only what holds none of this crate's types can come from it here.
    use whizbang_testkit::scratch::Scratch;

    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_lock_file_replaced_between_its_lock_and_the_look_at_its_name_is_not_the_one_held() {
        let dir = Scratch::new("core-fs-replaced");
        let path = dir.path().join("LOCK");
        std::fs::write(&path, b"the first file").unwrap();
        // Once, just after the first file is locked: it is replaced by another.
        let replaced = std::cell::Cell::new(false);
        let replace = || {
            if !replaced.replace(true) {
                std::fs::remove_file(&path).unwrap();
                std::fs::write(&path, b"the second file").unwrap();
            }
        };
        let held = hold_lock_file(&path, true, &replace).unwrap().unwrap();
        assert!(replaced.get());
        // What is held is the file the name leads to now: the next to come is refused,
        // and what the holder records is read by the name.
        assert!(hold_lock_file(&path, true, &|| {}).unwrap().is_none());
        held.record(b"holder\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"holder\n");
        drop(held);
        assert!(hold_lock_file(&path, true, &|| {}).unwrap().is_some());
    }

    #[test]
    fn a_lock_file_is_made_only_when_asked_and_recording_replaces_its_whole_content() {
        let dir = Scratch::new("core-fs-made");
        let path = dir.path().join("LOCK");
        let error = RealFs.hold_lock_file(&path, false).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!path.exists());

        let held = RealFs.hold_lock_file(&path, true).unwrap().unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"",
            "taking it writes nothing"
        );
        held.record(b"a long name for a holder\n").unwrap();
        held.record(b"short\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"short\n");
        assert!(RealFs.hold_lock_file(&path, false).unwrap().is_none());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"short\n",
            "nor does a refusal"
        );
        held.record(b"").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"");
    }
}
