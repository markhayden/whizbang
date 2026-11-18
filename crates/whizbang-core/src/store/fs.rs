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

/// The most bytes one piece of a streamed file holds: what [`NewFile::append`] takes at
/// once, and the longest buffer [`OpenFile::read`] fills.
pub const PIECE_MAX_BYTES: usize = 1024 * 1024;

/// A lock that is held until this value is dropped.
pub trait Held: Send + Sync {}

/// A new file that is being written piece by piece ([`Fs::begin_new`]).
///
/// A call that fails breaks the handle: every call after it fails too, so a handle that
/// once answered with an error never says that its file is whole.
pub trait NewFile: Send {
    /// Adds a piece to the end of the file. A piece holds [`PIECE_MAX_BYTES`] at most,
    /// and the file never grows past the most its opening stated: a piece that would
    /// take it further is refused with [`io::ErrorKind::FileTooLarge`] before any of it
    /// is written. After any other failure the file may hold any part of the piece.
    /// Nothing appended is durable until [`NewFile::sync`] returns.
    fn append(&mut self, piece: &[u8]) -> io::Result<()>;
    /// Makes durable every byte appended so far, through the handle that wrote them.
    fn sync(&mut self) -> io::Result<()>;
    /// Closes the file and says how many bytes it holds. It answers only for a file
    /// whose every byte was synced through this handle: with bytes appended since the
    /// last sync it fails, and writes nothing either way. Only a file that this
    /// answered for is whole; the name is not durable until its directory is synced.
    /// An empty file is whole without a sync; its name is durable only by
    /// [`Fs::sync_dir`].
    fn finish(self: Box<Self>) -> io::Result<u64>;
}

/// A file open to be read piece by piece ([`Fs::open_file`]).
///
/// A call that fails breaks the handle, as a [`NewFile`]'s does: a read that failed
/// part-way has moved on in the file, and the next would skip what was lost.
pub trait OpenFile: Send {
    /// Reads the next bytes of the file into `buffer` and says how many. The buffer
    /// is filled unless the file ends first, so fewer than it holds means the end,
    /// and none means nothing was left. A buffer that holds nothing, or more than
    /// [`PIECE_MAX_BYTES`], is refused.
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize>;
}

/// The count of a file being streamed, and the rules of [`NewFile`] that need no
/// disk: one copy of them, for the real file system and for a test's.
#[derive(Debug)]
pub struct Streamed {
    most: u64,
    written: u64,
    durable: u64,
    broken: bool,
}

impl Streamed {
    /// An empty file that may grow to `most` bytes.
    pub fn new(most: u64) -> Streamed {
        Streamed {
            most,
            written: 0,
            durable: 0,
            broken: false,
        }
    }

    /// Whether a piece of this many bytes may be appended now: the handle is usable,
    /// the piece holds [`PIECE_MAX_BYTES`] at most, and the file stays within its most.
    /// A refusal breaks the handle.
    pub fn admit(&mut self, piece: usize) -> io::Result<()> {
        self.usable()?;
        if piece > PIECE_MAX_BYTES {
            return self.refuse(
                io::ErrorKind::InvalidInput,
                "a piece of a streamed file holds more than a piece may",
            );
        }
        let within = u64::try_from(piece)
            .ok()
            .and_then(|piece| self.written.checked_add(piece))
            .is_some_and(|after| after <= self.most);
        if !within {
            return self.refuse(
                io::ErrorKind::FileTooLarge,
                "a streamed file would grow past the most it may hold",
            );
        }
        Ok(())
    }

    /// A piece of this many bytes, which was admitted, was appended.
    pub fn appended(&mut self, piece: usize) {
        self.written = self
            .written
            .saturating_add(u64::try_from(piece).unwrap_or(u64::MAX));
    }

    /// Whether the handle may still be used: not after a call of it failed.
    pub fn usable(&self) -> io::Result<()> {
        if self.broken {
            return Err(io::Error::other(
                "a call on this streamed file failed before, and it answers nothing more",
            ));
        }
        Ok(())
    }

    /// Everything appended so far was made durable.
    pub fn synced(&mut self) {
        self.durable = self.written;
    }

    /// A call failed: the handle is broken.
    pub fn fail(&mut self) {
        self.broken = true;
    }

    /// The bytes appended so far.
    pub fn written(&self) -> u64 {
        self.written
    }

    /// How many of them a sync made durable.
    pub fn durable(&self) -> u64 {
        self.durable
    }

    /// The length of the file, when it is whole: usable, with every byte synced. A
    /// refusal breaks the handle.
    pub fn whole(&mut self) -> io::Result<u64> {
        self.usable()?;
        if self.durable != self.written {
            return self.refuse(
                io::ErrorKind::InvalidInput,
                "a streamed file was finished with bytes that no sync made durable",
            );
        }
        Ok(self.written)
    }

    fn refuse<T>(&mut self, kind: io::ErrorKind, why: &'static str) -> io::Result<T> {
        self.broken = true;
        Err(io::Error::new(kind, why))
    }
}

/// What a file system that streams nothing answers.
fn streams_nothing<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this file system streams no file, tries no share and looks at no link",
    ))
}

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
    /// The first `limit` bytes of a file, or all of it when it has no more. It is the
    /// one way a file of the store is read: the caller knows how long the file may be,
    /// asks for one byte more, and so learns that a file is too long without holding
    /// all of it. Nothing reads a file whole, whatever it has grown to.
    fn read_at_most(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>>;
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
    /// Creates an empty file to be written piece by piece, which may grow to `most`
    /// bytes and no further. The name is taken as [`Fs::write_new`] takes one: it fails
    /// when the name is taken, by a file, a directory or a link, and follows no link
    /// there. The file is under that name from this moment, with whatever was appended
    /// so far, so the name is one that nothing reads as a whole file: it is written
    /// where work in progress is kept and brought to its place by [`Fs::rename`], with
    /// [`Fs::sync_dir`], once [`NewFile::finish`] has answered.
    ///
    /// The three calls below were added after the store's own; a file system that
    /// does not give them answers [`io::ErrorKind::Unsupported`] to each.
    fn begin_new(&self, path: &Path, most: u64) -> io::Result<Box<dyn NewFile>> {
        let _ = (path, most);
        streams_nothing()
    }
    /// Opens a file to be read piece by piece, however long it is. What the name
    /// leads to is a regular file and the name itself is no link, or the call fails
    /// with [`io::ErrorKind::InvalidData`]: a link, a directory and anything else that
    /// is not a file are refused, not followed. A missing file fails with
    /// [`io::ErrorKind::NotFound`]. How long the file may be is the caller's to hold:
    /// it counts what it reads and stops one byte past what it expects.
    fn open_file(&self, path: &Path) -> io::Result<Box<dyn OpenFile>> {
        let _ = path;
        streams_nothing()
    }
    /// As [`Fs::share`], without waiting: `None` when one has the file alone
    /// ([`Fs::try_alone`], or a lock file held on it). It returns at once either way.
    fn try_share(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        let _ = path;
        streams_nothing()
    }
    /// Whether the name itself is a symbolic link, looked at without following it:
    /// what it leads to, and whether anything is there, is not asked. False when
    /// nothing has the name, or a name on the way to it is no directory; a link on
    /// the way to the name is followed, as by every call here. It reads one name and
    /// changes nothing. Added with the models directory (ADR 0017, "A folder of the
    /// layout that is a link"), and answered as the three calls above are by a file
    /// system that does not give it.
    fn is_link(&self, path: &Path) -> io::Result<bool> {
        let _ = path;
        streams_nothing()
    }
    /// Whose this folder is and who else may write it, by its owner and its mode. A
    /// link at the name is followed: the folder asked about is the one the name
    /// leads to. What has the name and is no folder is an error, and so is a name
    /// nothing has. It reads one name and changes nothing in the folder or beside
    /// it. Added with the models directory (ADR 0017, "A models directory is its
    /// owner's alone"), and answered as the calls above are by a file system that
    /// does not give it. Lists of who else may reach a folder that a system keeps
    /// beside its mode are not read.
    fn who_may_write(&self, path: &Path) -> io::Result<Writers> {
        let _ = path;
        streams_nothing()
    }
    /// What the system says of a regular file under its own name, without reading it:
    /// its size, the times it was last written and last changed, and its inode
    /// ([`Stamp`]). A link, a
    /// directory and anything else that is no regular file are refused with
    /// [`io::ErrorKind::InvalidData`], as [`Fs::open_file`] refuses them, and nothing
    /// is followed. Added with the cached check of a model's files (plan 2A, task R1),
    /// and answered as the calls above are by a file system that does not give it.
    fn stamp(&self, path: &Path) -> io::Result<Stamp> {
        let _ = path;
        streams_nothing()
    }
    /// Gives the regular file at `from` a second name, `to`: one file under two names,
    /// which lives until both are removed. Fails when `to` is taken, by anything, and
    /// across two file systems ([`io::ErrorKind::CrossesDevices`]). What `from` is is
    /// not asked here: whoever links reads the file under its new name and holds it to
    /// what it must be. The name is not durable until its directory is synced. Added
    /// with the files shared between installations of models (plan 1D, task Q1; ADR
    /// 0017, "A file shared between installations"), and answered as the calls above
    /// are by a file system that does not give it.
    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        let _ = (from, to);
        streams_nothing()
    }
    /// Makes the file at `path` readable by its owner alone and writable by nobody
    /// (mode `0400`), following no link: a link at the name is refused with
    /// [`io::ErrorKind::InvalidData`]. Added for the copy of a user's adapter in the
    /// models directory (plan 1D, task U1), and answered as the calls above are by a
    /// file system that does not give it.
    fn set_read_only(&self, path: &Path) -> io::Result<()> {
        let _ = path;
        streams_nothing()
    }
}

/// What [`Fs::stamp`] answers of a file: its size in bytes, the time it was last
/// written and the time its inode last changed, each in whole seconds since 1970 and
/// the nanoseconds past them, and its inode. A file whose stamp is the same at two
/// moments was, as far as the system says, not written in between. A writer who sets
/// the time of last write back still moves the time of change, which no call sets; a
/// writer that keeps every one of them, below the system's calls, is not seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub size: u64,
    pub modified_s: i64,
    pub modified_ns: u32,
    pub changed_s: i64,
    pub changed_ns: u32,
    pub inode: u64,
}

/// [`Fs::stamp`] for the real file system.
#[cfg(unix)]
fn stamp(path: &Path) -> io::Result<Stamp> {
    use std::os::unix::fs::MetadataExt;
    let found = std::fs::symlink_metadata(path)?;
    if !found.is_file() {
        return Err(not_a_file());
    }
    Ok(Stamp {
        size: found.size(),
        modified_s: found.mtime(),
        modified_ns: u32::try_from(found.mtime_nsec()).map_err(|_| not_a_file())?,
        changed_s: found.ctime(),
        changed_ns: u32::try_from(found.ctime_nsec()).map_err(|_| not_a_file())?,
        inode: found.ino(),
    })
}

/// Where a file has no inode to read, nothing is said of it.
#[cfg(not(unix))]
fn stamp(_path: &Path) -> io::Result<Stamp> {
    streams_nothing()
}

/// Who may write a folder, by its owner and its mode: what [`Fs::who_may_write`]
/// answers. It holds no name and no number of a user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Writers {
    /// The folder's owner is the user this process runs as.
    pub owner_is_this_user: bool,
    /// The folder's mode lets its group write it.
    pub group: bool,
    /// The folder's mode lets anyone write it.
    pub anyone: bool,
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

/// [`Fs::try_share`] for the real file system.
fn try_share(path: &Path) -> io::Result<Option<LockedFile>> {
    let file = std::fs::File::open(path)?;
    let lock = file.try_lock_shared();
    taken(file, lock)
}

/// A new file, open to be written: the name was free, and no link was followed to it.
fn new_file(path: &Path) -> io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// What an error of the file system carries when the user this process runs as
/// could not be learned ([`Fs::who_may_write`]). It holds no path and no number. Its
/// error is never of the kind "not found", which one who opens a folder reads as a
/// folder that is still to be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserUnknown;

impl UserUnknown {
    /// The error that carries this.
    pub fn error() -> io::Error {
        io::Error::other(UserUnknown)
    }

    /// Whether this error carries it.
    pub fn is(error: &io::Error) -> bool {
        error
            .get_ref()
            .is_some_and(|inner| inner.is::<UserUnknown>())
    }
}

impl std::fmt::Display for UserUnknown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the user this process runs as could not be learned")
    }
}

impl std::error::Error for UserUnknown {}

/// The user this process runs as, as the number the system knows it by. The standard
/// library does not say who the process runs as, and the workspace has no crate and
/// no `unsafe` that would. The system gives what a process makes to the user it runs
/// as, so this is the owner of something the process has just made: one end of a
/// pair of connected sockets, which has no name anywhere, touches no folder and is
/// closed at once. That it is the owner of a file the process makes is held on
/// macOS and on Linux by
/// `fs::the_real_file_system_says_that_a_folder_of_another_user_is_not_this_users`.
/// It is asked once in a process. A failure is [`UserUnknown`], whatever the system
/// said. Public for the binary's reader of the Hub's sign-in, which holds the token
/// file it opened to this user (plan 1C, task N4), so that there is one way to learn
/// it.
#[cfg(unix)]
pub fn this_user() -> io::Result<u32> {
    use std::os::fd::OwnedFd;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::net::UnixStream;
    use std::sync::OnceLock;

    static USER: OnceLock<u32> = OnceLock::new();
    if let Some(user) = USER.get() {
        return Ok(*user);
    }
    let unknown = |_: io::Error| UserUnknown::error();
    let (end, _other) = UnixStream::pair().map_err(unknown)?;
    // Read through a file's metadata: the system is asked about the open socket
    // itself, by no name.
    let end = std::fs::File::from(OwnedFd::from(end));
    let user = end.metadata().map_err(unknown)?.uid();
    Ok(*USER.get_or_init(|| user))
}

/// [`Fs::who_may_write`] for the real file system.
#[cfg(unix)]
fn who_may_write(path: &Path) -> io::Result<Writers> {
    use std::os::unix::fs::MetadataExt;
    // Before the folder is looked at: one who cannot learn its user is told so, and
    // never that a folder is not there.
    let user = this_user()?;
    let found = std::fs::metadata(path)?;
    if !found.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "what has the name is no folder",
        ));
    }
    Ok(Writers {
        owner_is_this_user: found.uid() == user,
        group: found.mode() & 0o020 != 0,
        anyone: found.mode() & 0o002 != 0,
    })
}

/// Where a folder has no owner and no mode to read, nothing is said of it.
#[cfg(not(unix))]
fn who_may_write(_path: &Path) -> io::Result<Writers> {
    streams_nothing()
}

/// [`NewFile`] for the real file system.
struct NewRealFile {
    file: std::fs::File,
    count: Streamed,
}

impl NewFile for NewRealFile {
    fn append(&mut self, piece: &[u8]) -> io::Result<()> {
        use std::io::Write;
        self.count.admit(piece.len())?;
        if let Err(error) = self.file.write_all(piece) {
            self.count.fail();
            return Err(error);
        }
        self.count.appended(piece.len());
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        self.count.usable()?;
        // Through the handle that wrote, as `write_new` does and for its reason.
        if let Err(error) = self.file.sync_all() {
            self.count.fail();
            return Err(error);
        }
        self.count.synced();
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> io::Result<u64> {
        // Nothing is left to write: every byte was synced, or this fails. The file is
        // closed when the handle is dropped, here or by one who never finished.
        self.count.whole()
    }
}

/// What [`Fs::open_file`] answers for a name that is not a regular file's own.
fn not_a_file() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "what was to be read piece by piece is not a regular file under its own name",
    )
}

/// Whether two files are one. Without a way to tell, they are taken to be.
#[cfg(unix)]
fn same_file(one: &std::fs::Metadata, other: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    (one.dev(), one.ino()) == (other.dev(), other.ino())
}

#[cfg(not(unix))]
fn same_file(_one: &std::fs::Metadata, _other: &std::fs::Metadata) -> bool {
    true
}

/// [`Fs::open_file`] for the real file system. `looked` runs after the name was looked
/// at and before it is opened: the moment a test needs.
///
/// The name is looked at without following a link, and refused unless a regular file
/// has it; then it is opened, and the open file must be a regular file and the very
/// one the look found. So what is read is a file that had this name itself, as no link,
/// at one moment of this call: a link put there after the look is followed by the open
/// to another file, which is not the one that was found, and is refused. The look
/// comes first for another reason too: opening a pipe would wait for its writer.
fn open_file(path: &Path, looked: &dyn Fn()) -> io::Result<OpenRealFile> {
    let named = std::fs::symlink_metadata(path)?;
    if !named.is_file() {
        return Err(not_a_file());
    }
    looked();
    let file = std::fs::File::open(path)?;
    let open = file.metadata()?;
    if !open.is_file() || !same_file(&named, &open) {
        return Err(not_a_file());
    }
    Ok(OpenRealFile {
        file,
        broken: false,
    })
}

/// [`OpenFile`] for the real file system.
struct OpenRealFile {
    file: std::fs::File,
    broken: bool,
}

impl OpenRealFile {
    fn fill(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        use std::io::Read;
        if self.broken {
            return Err(io::Error::other(
                "a read of this file failed before, and it answers nothing more",
            ));
        }
        if buffer.is_empty() || buffer.len() > PIECE_MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a buffer for a piece of a file holds nothing, or more than a piece may",
            ));
        }
        let mut filled = 0;
        while filled < buffer.len() {
            match self.file.read(&mut buffer[filled..]) {
                Ok(0) => break,
                Ok(read) => filled += read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
        Ok(filled)
    }
}

impl OpenFile for OpenRealFile {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let filled = self.fill(buffer);
        self.broken |= filled.is_err();
        filled
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
        let mut file = new_file(path)?;
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

    fn read_at_most(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(limit)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
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

    fn begin_new(&self, path: &Path, most: u64) -> io::Result<Box<dyn NewFile>> {
        Ok(Box::new(NewRealFile {
            file: new_file(path)?,
            count: Streamed::new(most),
        }))
    }

    fn open_file(&self, path: &Path) -> io::Result<Box<dyn OpenFile>> {
        Ok(Box::new(open_file(path, &|| {})?))
    }

    fn try_share(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        Ok(try_share(path)?.map(|file| Box::new(file) as Box<dyn Held>))
    }

    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::hard_link(from, to)
    }

    fn set_read_only(&self, path: &Path) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::symlink_metadata(path)?.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a link is not followed",
            ));
        }
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o400))
    }

    fn is_link(&self, path: &Path) -> io::Result<bool> {
        match std::fs::symlink_metadata(path) {
            Ok(found) => Ok(found.is_symlink()),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    fn who_may_write(&self, path: &Path) -> io::Result<Writers> {
        who_may_write(path)
    }

    fn stamp(&self, path: &Path) -> io::Result<Stamp> {
        stamp(path)
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
    fn a_share_given_by_a_try_and_let_go_is_gone_though_a_copy_of_the_open_file_is_still_held() {
        let dir = Scratch::new("core-fs-copy-try-share");
        let path = dir.path().join("1.json");
        std::fs::write(&path, b"{}").unwrap();
        let shared = try_share(&path).unwrap().unwrap();
        let copy = shared.0.try_clone().unwrap();
        assert!(try_alone(&path).unwrap().is_none());
        drop(shared);
        let alone = try_alone(&path).unwrap();
        assert!(alone.is_some(), "the copy kept the share its holder let go");
        drop(copy);
    }

    /// The name is looked at, and then opened: what is put there between the two is
    /// not read. Each file that takes the name was there before, beside the first, so
    /// no two of them can be one file to the system.
    #[cfg(unix)]
    #[test]
    fn a_name_that_leads_elsewhere_between_the_look_and_the_opening_is_refused() {
        use std::io::Read;
        let dir = Scratch::new("core-fs-open-swapped");
        let path = dir.path().join("f");
        let other = dir.path().join("other");
        std::fs::write(&other, b"another file").unwrap();

        // A link to another file.
        std::fs::write(&path, b"the file").unwrap();
        let swap = || {
            std::fs::remove_file(&path).unwrap();
            std::os::unix::fs::symlink(&other, &path).unwrap();
        };
        let error = open_file(&path, &swap).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        std::fs::remove_file(&path).unwrap();

        // Another regular file, by a rename over the name.
        std::fs::write(&path, b"the file").unwrap();
        let spare = dir.path().join("spare");
        std::fs::write(&spare, b"a spare file").unwrap();
        let swap = || std::fs::rename(&spare, &path).unwrap();
        let error = open_file(&path, &swap).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        // A folder.
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"the file").unwrap();
        let swap = || {
            std::fs::remove_file(&path).unwrap();
            std::fs::create_dir(&path).unwrap();
        };
        let error = open_file(&path, &swap).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        std::fs::remove_dir(&path).unwrap();

        // Gone.
        std::fs::write(&path, b"the file").unwrap();
        let swap = || std::fs::remove_file(&path).unwrap();
        let error = open_file(&path, &swap).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);

        // Left alone, it is opened; and a link to the very file that was found, put
        // in its place, reads that file: the one that had the name itself.
        std::fs::write(&path, b"the file").unwrap();
        let mut content = Vec::new();
        let mut open = open_file(&path, &|| {}).unwrap();
        open.file.read_to_end(&mut content).unwrap();
        assert_eq!(content, b"the file");
        let kept = dir.path().join("kept");
        let swap = || {
            std::fs::rename(&path, &kept).unwrap();
            std::os::unix::fs::symlink(&kept, &path).unwrap();
        };
        let mut content = Vec::new();
        let mut open = open_file(&path, &swap).unwrap();
        open.file.read_to_end(&mut content).unwrap();
        assert_eq!(content, b"the file");
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
