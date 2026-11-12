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
//! - **Content:** a file's bytes, once [`Fs::sync_file`] ran on it after its last write. A
//!   rename does not change that: a file renamed before it was synced is still unsynced
//!   under its new name.
//! - **Names:** a new file, a new directory, the new name of a rename and a removal, once
//!   [`Fs::sync_dir`] ran on the directory that holds the name (for a rename, the directory
//!   of the new name). Syncing any other directory, or the file itself, does nothing for it.
//!
//! What a crash does with the rest is the caller's choice ([`Lost`]), because a real disk may
//! keep more than was asked of it. A correct protocol passes under both.
//!
//! The shim never calls the real sync: it models durability, and a real sync is slow.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use whizbang_core::store::fs::{Fs, RealFs};

/// One recorded call that changes the disk or makes a change durable. Reads are not events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A directory was created.
    CreateDir(PathBuf),
    /// A file was created with its content.
    Write(PathBuf),
    /// A file's content was made durable.
    SyncFile(PathBuf),
    /// A directory's names were made durable.
    SyncDir(PathBuf),
    /// A rename, from the first path to the second.
    Rename(PathBuf, PathBuf),
    /// A file or a directory tree was removed.
    Remove(PathBuf),
}

/// What a crash loses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lost {
    /// Everything that was not synced. Names that were not durable are undone, newest first:
    /// a new file or directory disappears, a rename goes back (and the file it replaced
    /// returns), a removed file or tree returns. A file whose name is durable and whose
    /// content is not is cut to half its length.
    Unsynced,
    /// Unsynced content only. Every name stays as it was at the crash, synced or not, and
    /// every file whose content was not synced is cut to half its length. This is the disk
    /// that kept a rename and lost the data of the file that was renamed.
    ContentOnly,
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
    /// The path whose directory must be synced for the change to be durable.
    fn name(&self) -> &Path {
        match self {
            Pending::Created(path) | Pending::Removed { path, .. } => path,
            Pending::Renamed { to, .. } => to,
        }
    }
}

#[derive(Default)]
struct State {
    events: Vec<Event>,
    /// Files whose content was written and not synced since.
    unsynced: BTreeSet<PathBuf>,
    /// Changes of names not yet durable, oldest first.
    pending: Vec<Pending>,
    crash_before: Option<(usize, Lost)>,
    fail_before: Option<usize>,
    crashed: bool,
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
        self.lock().fail_before = Some(event);
        self
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

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The state, for a read: refused after a crash.
    fn alive(&self) -> io::Result<MutexGuard<'_, State>> {
        let state = self.lock();
        if state.crashed {
            return Err(io::Error::other("the file system has crashed"));
        }
        Ok(state)
    }

    /// The state, for a call that would be the next event: refused after a crash, crashing
    /// when this is the chosen event, and refused once when it is the event chosen to fail.
    fn next_event(&self) -> io::Result<MutexGuard<'_, State>> {
        let mut state = self.alive()?;
        let index = state.events.len();
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
        Ok(state)
    }
}

/// Puts the disk in the state a power loss could have left.
fn crash(state: &mut State, lost: Lost) {
    if state.crashed {
        return;
    }
    state.crashed = true;
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
    if lost == Lost::ContentOnly {
        return;
    }
    for change in pending.into_iter().rev() {
        match change {
            Pending::Created(path) => {
                let _ = RealFs.remove_all(&path);
            }
            Pending::Renamed { from, to, replaced } => {
                let _ = std::fs::rename(&to, &from);
                if let Some(saved) = replaced {
                    restore(&to, &saved);
                }
            }
            Pending::Removed { path, saved } => restore(&path, &saved),
        }
    }
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
            let _ = RealFs.write_new(path, bytes);
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
        let mut state = self.next_event()?;
        self.real.create_dir(path)?;
        state.pending.push(Pending::Created(path.to_path_buf()));
        state.events.push(Event::CreateDir(path.to_path_buf()));
        Ok(())
    }

    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut state = self.next_event()?;
        self.real.write_new(path, bytes)?;
        state.unsynced.insert(path.to_path_buf());
        state.pending.push(Pending::Created(path.to_path_buf()));
        state.events.push(Event::Write(path.to_path_buf()));
        Ok(())
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        let mut state = self.next_event()?;
        if !std::fs::metadata(path)?.is_file() {
            return Err(io::Error::other(format!(
                "{} is not a file",
                path.display()
            )));
        }
        state.unsynced.remove(path);
        state.events.push(Event::SyncFile(path.to_path_buf()));
        Ok(())
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        let mut state = self.next_event()?;
        if !std::fs::metadata(path)?.is_dir() {
            return Err(io::Error::other(format!(
                "{} is not a directory",
                path.display()
            )));
        }
        state
            .pending
            .retain(|change| change.name().parent() != Some(path));
        state.events.push(Event::SyncDir(path.to_path_buf()));
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut state = self.next_event()?;
        let replaced = match std::fs::symlink_metadata(to) {
            Ok(_) => Some(save(to)?),
            Err(_) => None,
        };
        self.real.rename(from, to)?;
        // Unsynced content keeps that state under the new name: the file itself, or, when a
        // directory was renamed, every file under it.
        let moved: Vec<PathBuf> = state
            .unsynced
            .iter()
            .filter(|path| path.starts_with(from))
            .cloned()
            .collect();
        state.unsynced.remove(to);
        for path in moved {
            state.unsynced.remove(&path);
            if let Ok(rest) = path.strip_prefix(from) {
                let renamed = if rest.as_os_str().is_empty() {
                    to.to_path_buf()
                } else {
                    to.join(rest)
                };
                state.unsynced.insert(renamed);
            }
        }
        state.pending.push(Pending::Renamed {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
            replaced,
        });
        state
            .events
            .push(Event::Rename(from.to_path_buf(), to.to_path_buf()));
        Ok(())
    }

    fn remove_all(&self, path: &Path) -> io::Result<()> {
        let mut state = self.next_event()?;
        let saved = save(path)?;
        self.real.remove_all(path)?;
        state.unsynced.retain(|file| !file.starts_with(path));
        state.pending.push(Pending::Removed {
            path: path.to_path_buf(),
            saved,
        });
        state.events.push(Event::Remove(path.to_path_buf()));
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
}
