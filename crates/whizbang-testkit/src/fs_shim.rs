//! A file system that can lose power, for the tests of the store's publication.
//!
//! [`ShimFs`] performs every call on the real disk, records the calls that change it or make
//! a change durable, and can crash: at a chosen event, or when told to. A crash puts the disk
//! in the state a power loss could have left, and from then on every call fails, reads
//! included, because the process that made them is gone. To look at what survived, open the
//! same directory through a new `ShimFs`: whatever is on disk after a crash is durable.
//!
//! What the shim holds to be durable:
//!
//! - **Content:** a file's bytes, once the sync inside [`Fs::write_new`] ran. That call is
//!   two events, the write and the sync, so a crash or a refusal can fall between them.
//!   Of a file written piece by piece ([`Fs::begin_new`]): the bytes appended before its
//!   last sync that ran.
//! - **Names:** a new file, a new directory, the new name of a rename and a removal, once
//!   [`Fs::sync_dir`] ran on the directory that holds the name (for a rename, the directory
//!   of the new name). Syncing any other directory does nothing for it.
//!
//! What a crash does with the rest is the caller's choice ([`Lost`]), because a real disk may
//! keep more than was asked of it, and may keep one directory's names without another's, or
//! one name of a directory without the next. A correct protocol passes under every choice.
//!
//! The shim never calls the real sync: it models durability, and a real sync is slow.
//!
//! # A file written piece by piece
//!
//! Opening one, each piece appended, each sync and the finish are events, each of which
//! can be refused or be where the crash falls. So are the opening of a file to be read
//! piece by piece and each piece read, and a share that a try was given: the reads and
//! the locks of the store's own calls are not events, but one piece of many must be
//! refusable, and so must a try.
//!
//! What a crash leaves of such a file is one outcome, the same every time. If its name
//! survives (the name is a new file's, lost or kept as [`Lost`] says), the file holds
//! every byte that a sync made durable, then the first half, rounded down, of the bytes
//! appended since the last sync, and nothing after. So a file whose every byte was
//! synced is whole, finished or not; one never synced is the first half of itself, as a
//! file whose [`Fs::write_new`] was interrupted is; and one byte appended and not synced
//! is lost. The cut falls where half of those bytes ends, inside a piece or between two:
//! that stands for a crash inside an append, which as an event happens whole or not at
//! all. The file is cut under the name it has at the crash, renamed or in a folder that
//! was. What the shim does not model: a disk that kept all of what was never synced, or
//! none of it; a refused append that left part of its piece; and bytes on the disk that
//! were never appended. A handle used after the crash fails and writes nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use whizbang_core::store::fs::{Fs, Held, LockFile, NewFile, OpenFile, RealFs, Streamed};

/// One recorded call that changes the disk or makes a change durable. The reads of the
/// store's own calls are not events; the opening and the pieces of a file read piece by
/// piece are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A directory was created.
    CreateDir(PathBuf),
    /// A file was created with its content: the first half of [`Fs::write_new`].
    Write(PathBuf),
    /// A file's content was made durable: the second half of [`Fs::write_new`], or the
    /// sync of a file being written piece by piece.
    SyncFile(PathBuf),
    /// A directory's names were made durable.
    SyncDir(PathBuf),
    /// A rename, from the first path to the second.
    Rename(PathBuf, PathBuf),
    /// A file or a directory tree was removed.
    Remove(PathBuf),
    /// A lock file was taken ([`Fs::hold_lock_file`]). A refused attempt is not an event,
    /// and neither is recording the holder, sharing a file or taking one alone: nothing
    /// durable changes by them.
    Lock(PathBuf),
    /// A new file was created empty, to be written piece by piece ([`Fs::begin_new`]).
    Begin(PathBuf),
    /// A piece was appended to a file being written piece by piece.
    Append(PathBuf),
    /// A file being written piece by piece was closed as whole.
    Finish(PathBuf),
    /// A file was opened to be read piece by piece ([`Fs::open_file`]).
    Open(PathBuf),
    /// A piece of such a file was read.
    Read(PathBuf),
    /// A file was shared by a try that does not wait ([`Fs::try_share`]). A try that
    /// found the file held alone is not an event.
    TryShare(PathBuf),
}

/// What a crash loses. Under every choice, a file whose content was not synced is cut to
/// half its length if its name survives; and of a file written piece by piece, the bytes
/// appended since its last sync are cut to their first half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lost {
    /// Every name that was not durable is undone, newest first: a new file or directory
    /// disappears, a rename goes back (and the file it replaced returns), a removed file
    /// or tree returns.
    Unsynced,
    /// Every name stays as it was at the crash, synced or not. This is the disk that kept
    /// a rename and lost the data of the file that was renamed.
    ContentOnly,
    /// The disk kept the unsynced names of some directories and lost those of others. The
    /// directories that hold names not yet durable are taken in order of their paths, and
    /// directory number `i` loses its names when bit `i` of the mask is set.
    /// [`ShimFs::crash_choices`] says how many there were, so a test can try every mask.
    Subset(u64),
    /// The disk kept some unsynced names and lost others, whatever directory they are in:
    /// two names of one directory can fare differently. The names with changes not yet
    /// durable are taken in order of their paths (for a rename, the new name), and name
    /// number `i` loses every change made to it when bit `i` of the mask is set.
    /// [`ShimFs::crash_names`] says how many there were. A removal that removes many names
    /// from one directory is tested under this.
    Names(u64),
}

/// A copy of what a path held, kept so that a crash can bring it back.
enum Saved {
    File(Vec<u8>),
    Dir(Vec<(PathBuf, Saved)>),
}

/// A change of names that no sync has made durable yet.
enum Pending {
    Created(PathBuf),
    Renamed {
        from: PathBuf,
        to: PathBuf,
        replaced: Option<Saved>,
    },
    Removed {
        path: PathBuf,
        saved: Saved,
    },
}

impl Pending {
    /// The directory that must be synced for the change to be durable.
    fn dir(&self) -> Option<&Path> {
        match self {
            Pending::Created(path) | Pending::Removed { path, .. } => path.parent(),
            Pending::Renamed { to, .. } => to.parent(),
        }
    }

    /// The name the change is made to: for a rename, the new name.
    fn name(&self) -> &Path {
        match self {
            Pending::Created(path) | Pending::Removed { path, .. } => path,
            Pending::Renamed { to, .. } => to,
        }
    }
}

/// A file written piece by piece.
struct Stream {
    path: PathBuf,
    durable: u64,
}

/// `path` under its new name, when it is `from` or inside it.
fn renamed(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    path.strip_prefix(from).ok().map(|rest| {
        if rest.as_os_str().is_empty() {
            to.to_path_buf()
        } else {
            to.join(rest)
        }
    })
}

/// A lock on the real disk, shared between the value handed to the store and the shim, so
/// that either can end it: the store by dropping its value, the shim by crashing.
type Lock = Arc<Mutex<Option<Box<dyn Held>>>>;

struct ShimHeld(Lock);

impl Held for ShimHeld {}

impl Drop for ShimHeld {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
}

/// The same for a lock file.
type HeldLockFile = Arc<Mutex<Option<Box<dyn LockFile>>>>;

struct ShimLockFile(HeldLockFile);

impl LockFile for ShimLockFile {
    fn record(&self, holder: &[u8]) -> io::Result<()> {
        match self.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            Some(lock) => lock.record(holder),
            None => Err(io::Error::other("the file system has crashed")),
        }
    }
}

impl Drop for ShimLockFile {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
}

type Hook = Box<dyn FnOnce() + Send>;
type Matches = Box<dyn Fn(&Event) -> bool + Send>;

#[derive(Default)]
struct State {
    events: Vec<Event>,
    /// Files whose content was written and not synced.
    unsynced: BTreeSet<PathBuf>,
    /// The files written piece by piece, each by the number its handle has: the name
    /// the file has now, and how many of its bytes a sync made durable. One stays
    /// here until the file is removed or replaced, or its handle finishes it.
    streams: BTreeMap<u64, Stream>,
    next_stream: u64,
    /// Changes of names not yet durable, oldest first.
    pending: Vec<Pending>,
    crash_before: Option<(usize, Lost)>,
    fail_before: Option<usize>,
    /// Refuse a call whose event matches, after letting this many such calls through.
    fail_matching: Option<(usize, Matches)>,
    cross_device: Option<PathBuf>,
    hook: Option<(usize, Hook)>,
    /// Run once after the first rename onto a path of this file name.
    after_rename: Option<(String, Hook)>,
    /// The locks this file system holds, each until its holder drops it or the crash.
    locks: Vec<Lock>,
    lock_files: Vec<HeldLockFile>,
    crashed: bool,
    crash_choices: usize,
    crash_names: usize,
    /// The names that had changes not yet durable at the crash, in order of their paths.
    crash_named: Vec<PathBuf>,
}

/// The file system that can lose power.
pub struct ShimFs {
    real: RealFs,
    /// Shared with the handles of the files it streams, which are events of this file
    /// system and end with its crash.
    state: Arc<Mutex<State>>,
}

impl Default for ShimFs {
    fn default() -> Self {
        Self::new()
    }
}

impl ShimFs {
    /// A shim that crashes only when told to ([`ShimFs::crash`]).
    pub fn new() -> ShimFs {
        ShimFs {
            real: RealFs,
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    /// This file system again, for a handle it gave out: the same events, the same
    /// crash.
    fn same(&self) -> ShimFs {
        ShimFs {
            real: self.real,
            state: self.state.clone(),
        }
    }

    /// Crashes in place of event number `event`, counted from 0: the events before it
    /// happened, it and everything after did not.
    pub fn crash_before(self, event: usize, lost: Lost) -> ShimFs {
        self.lock().crash_before = Some((event, lost));
        self
    }

    /// Refuses the call that would have been event number `event`, once, with an error and
    /// without a crash: nothing is dropped, and the refused call is not recorded. This shows
    /// what is visible at a point. It does not test durability; only a crash does.
    pub fn fail_before(self, event: usize) -> ShimFs {
        self.refuse_before(event);
        self
    }

    /// As [`ShimFs::fail_before`], on a shim already in use; it replaces a refusal that is
    /// still waiting.
    pub fn refuse_before(&self, event: usize) {
        self.lock().fail_before = Some(event);
    }

    /// Refuses, once, a coming call by what it is: the first whose event `matches`, after
    /// `skip` such calls were let through. As with [`ShimFs::fail_before`], nothing is
    /// dropped and the refused call is not recorded. It replaces one still waiting.
    pub fn refuse_matching(&self, skip: usize, matches: impl Fn(&Event) -> bool + Send + 'static) {
        self.lock().fail_matching = Some((skip, Box::new(matches)));
    }

    /// Treats `dir` as another file system: a rename from inside it to outside it, or the
    /// other way, is refused with [`io::ErrorKind::CrossesDevices`] and is not an event.
    pub fn cross_device(self, dir: &Path) -> ShimFs {
        self.lock().cross_device = Some(dir.to_path_buf());
        self
    }

    /// Runs `hook` once, just before the call that would be event number `event`. The hook
    /// may use this shim: what it does comes first, as another thread's calls would, and
    /// the paused call then goes on, or fails if the hook crashed the shim.
    pub fn pause_before(&self, event: usize, hook: impl FnOnce() + Send + 'static) {
        self.lock().hook = Some((event, Box::new(hook)));
    }

    /// Runs `hook` once, straight after the first rename onto a file of this name has
    /// happened and before the call returns to whoever made it: what the hook sees is
    /// what any other thread could see at that moment. The hook may use this shim.
    pub fn pause_after_rename_onto(&self, name: &str, hook: impl FnOnce() + Send + 'static) {
        self.lock().after_rename = Some((name.to_owned(), Box::new(hook)));
    }

    /// The events so far, in order.
    pub fn events(&self) -> Vec<Event> {
        self.lock().events.clone()
    }

    /// True once the shim has crashed.
    pub fn crashed(&self) -> bool {
        self.lock().crashed
    }

    /// Loses power now.
    pub fn crash(&self, lost: Lost) {
        crash(&mut self.lock(), lost);
    }

    /// After a crash: how many directories held names that were not durable. Each could
    /// have lost them or kept them, so [`Lost::Subset`] has two to this power outcomes.
    pub fn crash_choices(&self) -> usize {
        self.lock().crash_choices
    }

    /// After a crash: how many names had changes that were not durable. Each could have
    /// lost them or kept them, so [`Lost::Names`] has two to this power outcomes.
    pub fn crash_names(&self) -> usize {
        self.lock().crash_names
    }

    /// After a crash: the names that had changes not yet durable, in the order
    /// [`Lost::Names`] numbers them, so that a test can choose masks by what a name is.
    pub fn crash_named(&self) -> Vec<PathBuf> {
        self.lock().crash_named.clone()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keeps a lock taken on the real disk, to end it at a crash.
    fn keep(state: &mut State, held: Box<dyn Held>) -> Box<dyn Held> {
        state.locks.retain(|lock| Arc::strong_count(lock) > 1);
        let lock: Lock = Arc::new(Mutex::new(Some(held)));
        state.locks.push(lock.clone());
        Box::new(ShimHeld(lock))
    }

    /// The state, for a read: refused after a crash.
    fn alive(&self) -> io::Result<MutexGuard<'_, State>> {
        let state = self.lock();
        if state.crashed {
            return Err(io::Error::other("the file system has crashed"));
        }
        Ok(state)
    }

    /// The state, for a call that would be the next event: after any hook for this event
    /// has run, refused after a crash, crashing when this is the chosen event, and refused
    /// once when it is the event chosen to fail.
    fn next_event(&self, event: &Event) -> io::Result<MutexGuard<'_, State>> {
        loop {
            let mut state = self.alive()?;
            let index = state.events.len();
            if state.hook.as_ref().is_some_and(|(at, _)| *at == index) {
                let hook = state.hook.take();
                drop(state);
                if let Some((_, hook)) = hook {
                    hook();
                }
                continue;
            }
            if let Some((at, lost)) = state.crash_before
                && at == index
            {
                crash(&mut state, lost);
                return Err(io::Error::other(format!(
                    "the file system crashed before event {index}"
                )));
            }
            if state.fail_before == Some(index) {
                state.fail_before = None;
                return Err(io::Error::other(format!(
                    "injected failure before event {index}"
                )));
            }
            if let Some((skip, matches)) = &mut state.fail_matching
                && matches(event)
            {
                if *skip == 0 {
                    state.fail_matching = None;
                    return Err(io::Error::other(format!("injected failure of {event:?}")));
                }
                *skip -= 1;
            }
            return Ok(state);
        }
    }
}

/// Puts the disk in the state a power loss could have left.
fn crash(state: &mut State, lost: Lost) {
    if state.crashed {
        return;
    }
    state.crashed = true;
    // The process is gone, and its locks with it.
    for lock in std::mem::take(&mut state.locks) {
        lock.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
    for lock in std::mem::take(&mut state.lock_files) {
        lock.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
    // Content first, while every file still has the name the shim knows it by.
    for path in std::mem::take(&mut state.unsynced) {
        if let Ok(meta) = std::fs::metadata(&path)
            && meta.is_file()
            && let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path)
        {
            let _ = file.set_len(meta.len() / 2);
        }
    }
    // A file written piece by piece keeps what a sync made durable, and the first half
    // of what was appended since.
    for stream in std::mem::take(&mut state.streams).into_values() {
        if let Ok(meta) = std::fs::symlink_metadata(&stream.path)
            && meta.is_file()
            && meta.len() > stream.durable
            && let Ok(file) = std::fs::OpenOptions::new().write(true).open(&stream.path)
        {
            let _ = file.set_len(stream.durable + (meta.len() - stream.durable) / 2);
        }
    }
    let pending = std::mem::take(&mut state.pending);
    let dirs: Vec<&Path> = pending
        .iter()
        .filter_map(Pending::dir)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let names: Vec<&Path> = pending
        .iter()
        .map(Pending::name)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    state.crash_choices = dirs.len();
    state.crash_names = names.len();
    state.crash_named = names.iter().map(|name| name.to_path_buf()).collect();
    let chosen = |mask: u64, i: Option<usize>| i.is_some_and(|i| i < 64 && mask >> i & 1 == 1);
    let loses: Vec<bool> = pending
        .iter()
        .map(|change| match lost {
            Lost::Unsynced => true,
            Lost::ContentOnly => false,
            Lost::Subset(mask) => chosen(
                mask,
                change
                    .dir()
                    .and_then(|dir| dirs.iter().position(|d| *d == dir)),
            ),
            Lost::Names(mask) => chosen(mask, names.iter().position(|n| *n == change.name())),
        })
        .collect();
    for (change, lost) in pending.into_iter().zip(loses).rev() {
        if !lost {
            continue;
        }
        match change {
            Pending::Created(path) => {
                let _ = RealFs.remove_all(&path);
            }
            Pending::Renamed { from, to, replaced } => {
                // Back under its old name; or, when the directory it came from is durably
                // gone, under no name at all.
                if std::fs::rename(&to, &from).is_err() {
                    let _ = RealFs.remove_all(&to);
                }
                if let Some(saved) = replaced {
                    restore(&to, &saved);
                }
            }
            Pending::Removed { path, saved } => restore(&path, &saved),
        }
    }
}

/// Creates a file with this content, owner-only, without a sync.
fn create(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    new_file(path)?.write_all(bytes)
}

fn save(path: &Path) -> io::Result<Saved> {
    if std::fs::symlink_metadata(path)?.is_dir() {
        let mut entries = Vec::new();
        for entry in RealFs.list(path)? {
            let saved = save(&entry)?;
            entries.push((entry, saved));
        }
        Ok(Saved::Dir(entries))
    } else {
        Ok(Saved::File(std::fs::read(path)?))
    }
}

fn restore(path: &Path, saved: &Saved) {
    match saved {
        Saved::File(bytes) => {
            let _ = create(path, bytes);
        }
        Saved::Dir(entries) => {
            let _ = RealFs.create_dir(path);
            for (entry, saved) in entries {
                restore(entry, saved);
            }
        }
    }
}

impl Fs for ShimFs {
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        let event = Event::CreateDir(path.to_path_buf());
        let mut state = self.next_event(&event)?;
        self.real.create_dir(path)?;
        state.pending.push(Pending::Created(path.to_path_buf()));
        state.events.push(event);
        Ok(())
    }

    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let event = Event::Write(path.to_path_buf());
        let mut state = self.next_event(&event)?;
        create(path, bytes)?;
        state.unsynced.insert(path.to_path_buf());
        state.pending.push(Pending::Created(path.to_path_buf()));
        state.events.push(event);
        drop(state);
        // The sync is an event of its own: a crash or a refusal here leaves the file
        // written and not synced.
        let event = Event::SyncFile(path.to_path_buf());
        let mut state = self.next_event(&event)?;
        state.unsynced.remove(path);
        state.events.push(event);
        Ok(())
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        let event = Event::SyncDir(path.to_path_buf());
        let mut state = self.next_event(&event)?;
        if !std::fs::metadata(path)?.is_dir() {
            return Err(io::Error::other(format!(
                "{} is not a directory",
                path.display()
            )));
        }
        state.pending.retain(|change| change.dir() != Some(path));
        state.events.push(event);
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let event = Event::Rename(from.to_path_buf(), to.to_path_buf());
        let mut state = self.next_event(&event)?;
        if let Some(other) = &state.cross_device
            && from.starts_with(other) != to.starts_with(other)
        {
            return Err(io::Error::new(
                io::ErrorKind::CrossesDevices,
                "the shim holds these to be two file systems",
            ));
        }
        let replaced = match std::fs::symlink_metadata(to) {
            Ok(_) => Some(save(to)?),
            Err(_) => None,
        };
        self.real.rename(from, to)?;
        // A file is renamed only once its write has returned, so its content is synced;
        // one that another thread is still writing keeps its state under its new name.
        if state.unsynced.remove(from) {
            state.unsynced.insert(to.to_path_buf());
        }
        // A streamed file that was replaced is no more; one that was renamed, or whose
        // folder was, is the same file under its new name.
        state
            .streams
            .retain(|_, stream| !stream.path.starts_with(to));
        for stream in state.streams.values_mut() {
            if let Some(path) = renamed(&stream.path, from, to) {
                stream.path = path;
            }
        }
        state.pending.push(Pending::Renamed {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
            replaced,
        });
        state.events.push(event);
        let onto = |name: &String| to.file_name().is_some_and(|last| last == name.as_str());
        if state
            .after_rename
            .as_ref()
            .is_some_and(|(name, _)| onto(name))
        {
            let hook = state.after_rename.take();
            drop(state);
            if let Some((_, hook)) = hook {
                hook();
            }
        }
        Ok(())
    }

    fn remove_all(&self, path: &Path) -> io::Result<()> {
        let event = Event::Remove(path.to_path_buf());
        let mut state = self.next_event(&event)?;
        let saved = save(path)?;
        self.real.remove_all(path)?;
        state.unsynced.retain(|file| !file.starts_with(path));
        state
            .streams
            .retain(|_, stream| !stream.path.starts_with(path));
        state.pending.push(Pending::Removed {
            path: path.to_path_buf(),
            saved,
        });
        state.events.push(event);
        Ok(())
    }

    fn read_at_most(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        let _state = self.alive()?;
        self.real.read_at_most(path, limit)
    }

    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let _state = self.alive()?;
        self.real.list(path)
    }

    fn exists(&self, path: &Path) -> io::Result<bool> {
        let _state = self.alive()?;
        self.real.exists(path)
    }
    fn hold_lock_file(&self, path: &Path, create: bool) -> io::Result<Option<Box<dyn LockFile>>> {
        let event = Event::Lock(path.to_path_buf());
        let mut state = self.next_event(&event)?;
        let was_there = self.real.exists(path)?;
        let Some(held) = self.real.hold_lock_file(path, create)? else {
            return Ok(None);
        };
        if !was_there {
            state.pending.push(Pending::Created(path.to_path_buf()));
        }
        state.events.push(event);
        state.lock_files.retain(|lock| Arc::strong_count(lock) > 1);
        let lock: HeldLockFile = Arc::new(Mutex::new(Some(held)));
        state.lock_files.push(lock.clone());
        Ok(Some(Box::new(ShimLockFile(lock))))
    }

    fn share(&self, path: &Path) -> io::Result<Box<dyn Held>> {
        // Not under the shim's own lock: this can wait for one who has the file alone.
        drop(self.alive()?);
        let held = self.real.share(path)?;
        let mut state = self.alive()?;
        Ok(Self::keep(&mut state, held))
    }

    fn try_alone(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        let mut state = self.alive()?;
        Ok(self
            .real
            .try_alone(path)?
            .map(|held| Self::keep(&mut state, held)))
    }

    fn begin_new(&self, path: &Path, most: u64) -> io::Result<Box<dyn NewFile>> {
        let event = Event::Begin(path.to_path_buf());
        let mut state = self.next_event(&event)?;
        let file = new_file(path)?;
        let id = state.next_stream;
        state.next_stream += 1;
        state.streams.insert(
            id,
            Stream {
                path: path.to_path_buf(),
                durable: 0,
            },
        );
        state.pending.push(Pending::Created(path.to_path_buf()));
        state.events.push(event);
        Ok(Box::new(ShimNewFile {
            fs: self.same(),
            id,
            path: path.to_path_buf(),
            file,
            count: Streamed::new(most),
        }))
    }

    fn open_file(&self, path: &Path) -> io::Result<Box<dyn OpenFile>> {
        let event = Event::Open(path.to_path_buf());
        let mut state = self.next_event(&event)?;
        let real = self.real.open_file(path)?;
        state.events.push(event);
        Ok(Box::new(ShimOpenFile {
            fs: self.same(),
            path: path.to_path_buf(),
            real,
            broken: false,
        }))
    }

    fn try_share(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        let event = Event::TryShare(path.to_path_buf());
        // Under the shim's own lock, as taking a file alone is: neither waits.
        let mut state = self.next_event(&event)?;
        let Some(held) = self.real.try_share(path)? else {
            return Ok(None);
        };
        state.events.push(event);
        Ok(Some(Self::keep(&mut state, held)))
    }
}

/// Creates an empty file, owner-only, to be written to.
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

/// A file being written piece by piece through the shim. Its bytes go to the real
/// disk as they are appended; its sync is an event and no more, as every sync here.
struct ShimNewFile {
    fs: ShimFs,
    id: u64,
    /// The name it was opened under: what its events say once the shim has forgotten
    /// the file, because it was removed or replaced.
    path: PathBuf,
    file: std::fs::File,
    count: Streamed,
}

impl ShimNewFile {
    /// The name the file has now.
    fn path(&self) -> PathBuf {
        match self.fs.lock().streams.get(&self.id) {
            Some(stream) => stream.path.clone(),
            None => self.path.clone(),
        }
    }

    fn append_to(&mut self, piece: &[u8]) -> io::Result<()> {
        use std::io::Write;
        self.count.admit(piece.len())?;
        let event = Event::Append(self.path());
        let mut state = self.fs.next_event(&event)?;
        self.file.write_all(piece)?;
        self.count.appended(piece.len());
        state.events.push(event);
        Ok(())
    }

    fn sync_it(&mut self) -> io::Result<()> {
        self.count.usable()?;
        let event = Event::SyncFile(self.path());
        let mut state = self.fs.next_event(&event)?;
        self.count.synced();
        if let Some(stream) = state.streams.get_mut(&self.id) {
            stream.durable = self.count.durable();
        }
        state.events.push(event);
        Ok(())
    }
}

impl NewFile for ShimNewFile {
    fn append(&mut self, piece: &[u8]) -> io::Result<()> {
        let appended = self.append_to(piece);
        if appended.is_err() {
            self.count.fail();
        }
        appended
    }

    fn sync(&mut self) -> io::Result<()> {
        let synced = self.sync_it();
        if synced.is_err() {
            self.count.fail();
        }
        synced
    }

    fn finish(mut self: Box<Self>) -> io::Result<u64> {
        let length = self.count.whole()?;
        let event = Event::Finish(self.path());
        let mut state = self.fs.next_event(&event)?;
        // Every byte is durable: a crash has nothing of this file to cut.
        state.streams.remove(&self.id);
        state.events.push(event);
        Ok(length)
    }
}

/// A file being read piece by piece through the shim.
struct ShimOpenFile {
    fs: ShimFs,
    path: PathBuf,
    real: Box<dyn OpenFile>,
    broken: bool,
}

impl ShimOpenFile {
    fn fill(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.broken {
            return Err(io::Error::other(
                "a read of this file failed before, and it answers nothing more",
            ));
        }
        let event = Event::Read(self.path.clone());
        let mut state = self.fs.next_event(&event)?;
        let filled = self.real.read(buffer)?;
        state.events.push(event);
        Ok(filled)
    }
}

impl OpenFile for ShimOpenFile {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let filled = self.fill(buffer);
        self.broken |= filled.is_err();
        filled
    }
}
