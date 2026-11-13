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
//! - **Names:** a new file, a new directory, the new name of a rename and a removal, once
//!   [`Fs::sync_dir`] ran on the directory that holds the name (for a rename, the directory
//!   of the new name). Syncing any other directory does nothing for it.
//!
//! What a crash does with the rest is the caller's choice ([`Lost`]), because a real disk may
//! keep more than was asked of it, and may keep one directory's names without another's, or
//! one name of a directory without the next. A correct protocol passes under every choice.
//!
//! The shim never calls the real sync: it models durability, and a real sync is slow.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use whizbang_core::store::fs::{Fs, Held, LockFile, RealFs};

/// One recorded call that changes the disk or makes a change durable. Reads are not events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A directory was created.
    CreateDir(PathBuf),
    /// A file was created with its content: the first half of [`Fs::write_new`].
    Write(PathBuf),
    /// A file's content was made durable: the second half of [`Fs::write_new`].
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
}

/// What a crash loses. Under every choice, a file whose content was not synced is cut to
/// half its length if its name survives.
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
}

/// The file system that can lose power.
pub struct ShimFs {
    real: RealFs,
    state: Mutex<State>,
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
            state: Mutex::new(State::default()),
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
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
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
        state.pending.push(Pending::Removed {
            path: path.to_path_buf(),
            saved,
        });
        state.events.push(event);
        Ok(())
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let _state = self.alive()?;
        self.real.read(path)
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
}
