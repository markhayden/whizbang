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
//! or when the process ends, however it ends, unless a child of that process still has a
//! copy of the open file then ("A child being started holds a copy", below). Two holders
//! in one process exclude each other exactly as two processes do.
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
//! - **A child being started holds a copy.** A child process has a copy of every file its
//!   parent has open, from the moment it is forked until it starts its program, where the
//!   copies end (every file here is opened to be closed there). `flock` is on the open
//!   file, which the copy shares, so a lock whose file was only closed would stay held by
//!   the copy for that moment, and one who asked for it then would be refused though
//!   nobody holds it: it happened to a thread that opened a store again while another
//!   thread started a child. So a lock here is let go, not only closed, when its value is
//!   dropped: that ends it for every copy at once. Only the process that took a lock
//!   lets it go: a forked copy of the holder that runs on has the holding value too, and
//!   when it drops that value the file is closed and no more, so the lock stays its
//!   parent's (the value keeps the id of the process that took the lock, and compares).
//!   Two cases are left, and neither admits a second holder; each only keeps one out,
//!   or keeps one waiting. A process that ends without dropping its locks (killed by a
//!   signal, which is how Ctrl-C ends it, aborted, or ended by `exit`) while one of its
//!   threads is starting a child leaves them to that child for the moment until its
//!   program starts. And a child that never starts a program, or is made to keep the
//!   file across the start, keeps the locks of a parent that ended so for as long as it
//!   lives; nothing in whizbang makes such a child. For that moment, or that life, a
//!   writer is refused; a prune leaves a generation it finds shared, for the next prune;
//!   and a reader is not refused but waits: [`Fs::share`] waits for one who has a file
//!   alone, so where the process that ended so was pruning, a reader's snapshot of that
//!   generation does not return until the child's copy is gone.
//!
//!   What else was weighed for the child's copy. Opening the file so that a child never
//!   has it: every file here is already closed where a child's program starts, and
//!   nothing closes it earlier, between the fork and that start. A lock that belongs to
//!   the process and not to the open file (the classic record lock of `fcntl`), which a
//!   forked child does not get: the process loses it whenever it closes any descriptor
//!   for the file, and the store reads `LOCK` to name a holder and reads a generation
//!   file while a reader shares it; two holders in one process would not exclude each
//!   other; and the standard library has no call for it, so it needs a dependency or
//!   `unsafe`, which this workspace forbids. The newer `fcntl` lock on the open file is
//!   copied to a child exactly as `flock` is. Asking again for a short time when a
//!   refusal names no holder: it waits on a clock, it guesses how long a child takes to
//!   start, and a writer in its first moment looks the same; it stays the answer to the
//!   two cases left, if they are ever met (`docs/carry-over.md`). Forbidding a writer to
//!   start children: the worker of a model pack is started by a process that holds a
//!   store.

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

/// An open file with a lock on it, which ends when this value is dropped, and the id of
/// the process that took the lock.
struct LockedFile(std::fs::File, u32);

impl LockedFile {
    /// A file this process has just locked.
    fn of(file: std::fs::File) -> LockedFile {
        LockedFile(file, std::process::id())
    }

    /// Whether this is the process that took the lock. It is not in a forked copy of
    /// that process: the copy has this value, over a copy of the open file, and the lock
    /// is its parent's. What the copy does with the value must change nothing for the
    /// parent.
    fn is_ours(&self) -> bool {
        self.1 == std::process::id()
    }
}

impl Drop for LockedFile {
    /// Lets the lock go, and then the file is closed. Closing alone ends the lock only
    /// when no copy of the open file is left, and a child process that another thread is
    /// starting holds one until its program starts (the module's description). A failure
    /// leaves the lock to the close that follows, as before.
    ///
    /// Only the process that took the lock lets it go. Letting go ends the lock for
    /// every copy of the open file, the holder's own among them: a forked copy of the
    /// holder that ran on and dropped this value would otherwise end a lock its parent
    /// still relies on, and a second holder would be admitted beside the parent. In such
    /// a copy the file is only closed, which ends nothing while the parent has it open.
    fn drop(&mut self) {
        if self.is_ours() {
            let _ = self.0.unlock();
        }
    }
}

impl Held for LockedFile {}

impl LockFile for LockedFile {
    fn record(&self, holder: &[u8]) -> io::Result<()> {
        use std::io::{Seek, Write};
        // The name in the file is the holder's: a forked copy leaves it alone.
        if !self.is_ours() {
            return Ok(());
        }
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
fn hold_lock_file(path: &Path, create: bool, locked: &dyn Fn()) -> io::Result<Option<LockedFile>> {
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
        if is_at(&file.0, path)? {
            return Ok(Some(file));
        }
    }
}

/// [`Fs::share`] for the real file system.
fn share(path: &Path) -> io::Result<LockedFile> {
    let file = std::fs::File::open(path)?;
    file.lock_shared()?;
    Ok(LockedFile::of(file))
}

/// [`Fs::try_alone`] for the real file system.
fn try_alone(path: &Path) -> io::Result<Option<LockedFile>> {
    let file = std::fs::File::open(path)?;
    let lock = file.try_lock();
    taken(file, lock)
}

/// The file with the lock on it, when the lock was given.
fn taken(
    file: std::fs::File,
    lock: Result<(), std::fs::TryLockError>,
) -> io::Result<Option<LockedFile>> {
    match lock {
        Ok(()) => Ok(Some(LockedFile::of(file))),
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
        Ok(hold_lock_file(path, create, &|| {})?.map(|file| Box::new(file) as Box<dyn LockFile>))
    }

    fn share(&self, path: &Path) -> io::Result<Box<dyn Held>> {
        Ok(Box::new(share(path)?))
    }

    fn try_alone(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        Ok(try_alone(path)?.map(|file| Box::new(file) as Box<dyn Held>))
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

    // A second descriptor for the same open file is what a child process holds from the
    // moment it is forked until it starts its program. `try_clone` makes one without a
    // child, so these do not wait on a race. The same with real children is in the
    // store's tests (`store/locking.rs`).

    #[test]
    fn a_lock_file_let_go_is_free_though_a_copy_of_the_open_file_is_still_held() {
        let dir = Scratch::new("core-fs-copy-lock");
        let path = dir.path().join("LOCK");
        let held = hold_lock_file(&path, true, &|| {}).unwrap().unwrap();
        let copy = held.0.try_clone().unwrap();
        // While the holder lives, a copy changes nothing.
        assert!(hold_lock_file(&path, true, &|| {}).unwrap().is_none());
        drop(held);
        let next = hold_lock_file(&path, true, &|| {}).unwrap();
        assert!(next.is_some(), "the copy kept the lock its holder let go");
        // And the copy holds nothing now: the next holder is the only one.
        assert!(hold_lock_file(&path, true, &|| {}).unwrap().is_none());
        drop(copy);
        assert!(hold_lock_file(&path, true, &|| {}).unwrap().is_none());
    }

    /// A copy of the holder itself, in a process that was forked and runs on, is another
    /// matter: when that copy is dropped, the lock is its parent's still. No process is
    /// forked here: the value is given the id of another process, which is all that a
    /// forked copy of it differs by.
    #[test]
    fn a_lock_dropped_in_a_process_that_did_not_take_it_is_still_held() {
        let dir = Scratch::new("core-fs-forked-copy");
        let path = dir.path().join("LOCK");
        let held = hold_lock_file(&path, true, &|| {}).unwrap().unwrap();
        // The parent's own descriptor, and the forked copy of the value over a copy of it.
        let parent = held.0.try_clone().unwrap();
        held.record(b"the parent\n").unwrap();
        let mut forked = held;
        forked.1 = std::process::id().wrapping_add(1);
        // What a store does when it is dropped: it takes its name out, then lets go.
        forked.record(b"").unwrap();
        drop(forked);
        assert_eq!(std::fs::read(&path).unwrap(), b"the parent\n");
        assert!(
            hold_lock_file(&path, true, &|| {}).unwrap().is_none(),
            "a copy in another process let go of its parent's lock"
        );
        // The same value in the process that took the lock lets it go.
        drop(LockedFile::of(parent.try_clone().unwrap()));
        assert!(hold_lock_file(&path, true, &|| {}).unwrap().is_some());
        drop(parent);
    }

    #[test]
    fn a_share_let_go_is_gone_though_a_copy_of_the_open_file_is_still_held() {
        let dir = Scratch::new("core-fs-copy-share");
        let path = dir.path().join("1.json");
        std::fs::write(&path, b"{}").unwrap();
        let shared = share(&path).unwrap();
        let copy = shared.0.try_clone().unwrap();
        assert!(try_alone(&path).unwrap().is_none());
        drop(shared);
        let alone = try_alone(&path).unwrap();
        assert!(alone.is_some(), "the copy kept the share its holder let go");
        drop(copy);
    }

    #[test]
    fn a_file_taken_alone_and_let_go_is_free_though_a_copy_of_the_open_file_is_still_held() {
        let dir = Scratch::new("core-fs-copy-alone");
        let path = dir.path().join("1.json");
        std::fs::write(&path, b"{}").unwrap();
        let alone = try_alone(&path).unwrap().unwrap();
        let copy = alone.0.try_clone().unwrap();
        drop(alone);
        // A share waits for one who has the file alone: with the lock kept by the copy
        // this would never return.
        assert!(try_alone(&path).unwrap().is_some());
        drop(share(&path).unwrap());
        drop(copy);
    }
}
