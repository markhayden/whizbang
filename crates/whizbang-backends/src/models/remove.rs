//! Removal of a model (ADR 0017, "Removal"): the model drains, then it is removed.
//!
//! A removal marks the model as draining, under the directory's lock: from then no
//! hold on it is given and no install of it publishes. It takes the model's lock, by
//! trying until its deadline, and says `removing` in the status record. It waits for
//! the holds by trying each hold file alone, again and again until the same
//! deadline, which is one of the injected clock; nothing here sleeps, and what comes
//! between two tries is the caller's (`pause`). With every hold file its own, it
//! clears the pointer under the directory's lock, and that is the decision: the
//! model is not installed. Past the deadline it answers that the model is held, has
//! changed no pointer, and takes the mark off.
//!
//! Then, with the hold files still its own, it deletes every installation of the
//! model and syncs the deleting. A removal that died or was refused after it
//! cleared the pointer is finished by the next to take the model's lock: an
//! install, or a removal, which finds no pointer, says that the model is not
//! installed, and deletes what no pointer names.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use whizbang_core::contract::{Clock, Secrets};
use whizbang_core::store::fs::Held;

use super::directory::{LINKED, Read, is_linked};
use super::install::{Model, Pointer};
use super::json::{self, Object};
use super::name::holds_secret;
use super::status::{self, State, Status};
use super::{InstallationId, ModelName, ModelsDir, Refused, Rule, What};

/// The format of a draining mark.
pub const DRAINING_FORMAT: &str = "draining/1";

/// The most bytes a draining mark may be. The one whizbang writes is 23.
pub const DRAINING_MAX_BYTES: u64 = 64;

/// The draining mark of a model, `models/<model>/DRAINING`: a record that says a
/// removal of the model has begun, and nothing else. It names no process and no
/// time: whether its remover is alive is not read from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark;

impl Mark {
    /// Reads these bytes as a mark, as every record here is read: the length first,
    /// then the bytes whole for a secret, then strictly, and last held to the one
    /// form whizbang writes.
    pub fn parse(bytes: &[u8], secrets: &Secrets) -> Result<Mark, Refused> {
        let what = What::Mark;
        let json = json::parse(what, bytes, DRAINING_MAX_BYTES, secrets)?;
        let mut top = Object::of(json, "")?;
        if top.text("format")? != DRAINING_FORMAT {
            return Err(Refused::at("format", Rule::Format));
        }
        top.done()?;
        if Mark.to_bytes() != bytes {
            return Err(Refused::OtherForm { what });
        }
        Ok(Mark)
    }

    /// The mark in its one form: compact JSON, nothing after the brace.
    pub fn to_bytes(&self) -> Vec<u8> {
        format!(r#"{{"format":"{DRAINING_FORMAT}"}}"#).into_bytes()
    }
}

/// Whether the model is marked as draining, read strictly by one who holds no lock:
/// no when there is no mark, yes when there is one as whizbang writes it. Anything
/// else at the mark's name is refused, never followed and never guessed at.
///
/// **Who decides by the mark does not ask this.** A hold and a publication take
/// anything at the mark's name for a mark, so that a mark that cannot be read still
/// stops them.
pub fn draining(dir: &ModelsDir, model: &ModelName, secrets: &Secrets) -> Result<bool, Refused> {
    let what = What::Mark;
    let unreadable = |kind| Refused::Unreadable { what, kind };
    match dir.read(&dir.model(model).join("DRAINING"), DRAINING_MAX_BYTES) {
        Ok(Read::Bytes(bytes)) => Mark::parse(&bytes, secrets).map(|_| true),
        Ok(Read::Missing) => Ok(false),
        Ok(Read::NotAFile) => Err(unreadable(io::ErrorKind::InvalidData)),
        Err(error) => Err(unreadable(error.kind())),
    }
}

/// Why a model was not removed. Fixed words, and no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveRefused {
    /// The model's name holds a secret the remover was told of. Nothing was touched.
    HoldsSecret,
    /// The model is not installed: it has no pointer.
    NotInstalled,
    /// Busy: another had the model's lock until the deadline, an install or a removal
    /// of the model. Nothing but the mark was changed, and the mark is taken off.
    Busy,
    /// Busy: a hold on an installation of the model was kept past the deadline. No
    /// pointer was changed, and the mark is taken off.
    Held,
    /// The directory's lock was not let go by whoever has it.
    DirectoryBusy,
    /// A folder of the model's layout is a link. Nothing is listed or deleted
    /// through it, and nothing was changed.
    Linked,
    /// The file system refused. The kind of failure, and no path.
    Disk(io::ErrorKind),
}

impl std::fmt::Display for RemoveRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RemoveRefused::HoldsSecret => "the model's name holds a secret, and is not shown",
            RemoveRefused::NotInstalled => "the model is not installed",
            RemoveRefused::Busy => "the model is being installed or removed by another",
            RemoveRefused::Held => "the model is held by a run, which did not let it go in time",
            RemoveRefused::DirectoryBusy => {
                "the models directory is locked by another, which did not let it go"
            }
            RemoveRefused::Linked => LINKED,
            RemoveRefused::Disk(_) => "the models directory could not be read or written",
        })
    }
}

impl std::error::Error for RemoveRefused {}

impl From<io::Error> for RemoveRefused {
    fn from(error: io::Error) -> RemoveRefused {
        if is_linked(&error) {
            return RemoveRefused::Linked;
        }
        RemoveRefused::Disk(error.kind())
    }
}

/// What a removal removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Removed {
    was: Option<InstallationId>,
}

impl Removed {
    /// The installation the pointer named; none when the pointer could not be read
    /// as one.
    pub fn installation(&self) -> Option<InstallationId> {
        self.was
    }
}

/// Removes a model: marks it as draining, waits for the holds on it until
/// `patience` of this clock has passed, and clears its pointer.
///
/// - **A folder of the model's layout that is a link is refused**
///   ([`RemoveRefused::Linked`]): looked for before the mark, again under the
///   model's lock before anything is listed, again when the holds have been
///   waited for and before the pointer is cleared, and once more just before
///   the deleting. Nothing is deleted through it.
/// - **The mark comes first,** under the directory's lock, when none is there. A
///   mark is the removal's own when it made it or has the model's lock, and only
///   its own mark is taken off at the end. A model with no folder
///   was never installed: it is not marked, and nothing is made.
/// - **The model's lock is tried for until the deadline;** `pause` is called once
///   between two tries, and nothing here sleeps. Not given by then, the answer is
///   [`RemoveRefused::Busy`].
/// - **The status says `removing` before anything else is done under the lock,** and
///   `removed` once the pointer is cleared. A removal that changed no pointer puts
///   the status back as it found it.
/// - **A model with no pointer is not installed,** and the removal says so.
/// - **The holds are waited for by trying:** the hold file of each installation of
///   the model that is on the disk, the one in use and any that was superseded and
///   is still held, is tried alone, without waiting, with `pause` between two
///   tries. Past the deadline the answer is [`RemoveRefused::Held`]: no pointer was
///   changed.
/// - **The decision is the pointer,** cleared under the directory's lock while
///   every hold file is the removal's alone, and synced.
/// - **Then the files are deleted:** every folder under `installations/`, with the
///   hold files still the removal's alone, and the deleting synced. A removal that
///   finds no pointer deletes what a removal before it left.
/// - **The mark is taken off at the end,** however the removal ended, under the
///   directory's lock, and the model's folder synced.
pub fn remove(
    dir: &ModelsDir,
    model: &ModelName,
    clock: &dyn Clock,
    patience: Duration,
    secrets: &Secrets,
    pause: &mut dyn FnMut(),
) -> Result<Removed, RemoveRefused> {
    if holds_secret(model.as_str(), secrets) {
        return Err(RemoveRefused::HoldsSecret);
    }
    // Before the mark, which is written in the model's folder: a folder of the
    // layout that is a link is not gone through, to write any more than to delete.
    dir.unlinked(model)?;
    let folder = dir.model(model);
    if !dir.fs().exists(&folder)? {
        return Err(RemoveRefused::NotInstalled);
    }
    // A wait too long for the clock to count is a wait with no end.
    let deadline = clock.now().checked_add(patience);
    // Whether the mark is this removal's own: it made it where none was, or it has
    // the model's lock. A mark it found is another's, of the removal that has the
    // lock or of a remover that died, and is left when this one ends busy.
    let mut own = mark_if_none(dir, &folder)?;
    let ended = drain(dir, model, clock, deadline, secrets, pause, &mut own);
    // However it ended. A mark that cannot be taken off now is taken off by the
    // next to take the model's lock; it does not change what this one answers.
    if own {
        let _ = clear_mark(dir, &folder);
    }
    ended
}

/// Whether the deadline has come. No deadline is one that never does.
fn past(clock: &dyn Clock, deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| clock.now() >= deadline)
}

/// Sets the model's mark, under the directory's lock. Whatever was at its name is
/// replaced: nobody decides by the mark but under that lock.
fn set_mark(dir: &ModelsDir, folder: &Path) -> Result<(), RemoveRefused> {
    let mark = folder.join("DRAINING");
    let _directory = dir.lock()?.ok_or(RemoveRefused::DirectoryBusy)?;
    dir.remove(&mark)?;
    dir.fs().write_new(&mark, &Mark.to_bytes())?;
    Ok(())
}

/// Sets the model's mark when nothing is at its name, and says whether it did.
/// Looked for again under the directory's lock: of two removals that begin
/// together, one makes the mark and the other finds it.
fn mark_if_none(dir: &ModelsDir, folder: &Path) -> Result<bool, RemoveRefused> {
    let mark = folder.join("DRAINING");
    if dir.fs().exists(&mark)? {
        return Ok(false);
    }
    let _directory = dir.lock()?.ok_or(RemoveRefused::DirectoryBusy)?;
    if dir.fs().exists(&mark)? {
        return Ok(false);
    }
    dir.fs().write_new(&mark, &Mark.to_bytes())?;
    Ok(true)
}

/// Takes the model's mark off, under the directory's lock, and makes that durable.
fn clear_mark(dir: &ModelsDir, folder: &Path) -> Result<(), RemoveRefused> {
    let _directory = dir.lock()?.ok_or(RemoveRefused::DirectoryBusy)?;
    dir.remove(&folder.join("DRAINING"))?;
    dir.fs().sync_dir(folder)?;
    Ok(())
}

/// What a removal does once its mark is set: the model's lock, the status, the
/// holds, and the pointer.
fn drain(
    dir: &ModelsDir,
    name: &ModelName,
    clock: &dyn Clock,
    deadline: Option<Instant>,
    secrets: &Secrets,
    pause: &mut dyn FnMut(),
    own: &mut bool,
) -> Result<Removed, RemoveRefused> {
    let fs = dir.fs();
    let lock = dir.model(name).join("LOCK");
    let folder = dir.model(name);
    let _lock = loop {
        if let Some(lock) = fs.hold_lock_file(&lock, true)? {
            break lock;
        }
        if past(clock, deadline) {
            return Err(RemoveRefused::Busy);
        }
        // Whoever has the lock may have taken the mark off as one a dead remover
        // left: it is set again, so that an install that is running reads it.
        *own |= mark_if_none(dir, &folder)?;
        pause();
    };
    // Looked at again under the lock, which may have been waited for: nothing is
    // listed or deleted through a folder of the layout that is a link.
    dir.unlinked(name)?;
    // And with the lock the mark is this removal's own, whoever made it: what is
    // there and is no mark as it is written is replaced.
    if !matches!(draining(dir, name, secrets), Ok(true)) {
        set_mark(dir, &folder)?;
    }
    *own = true;
    let model = Model::of(dir, name, secrets);
    // What the status said before: a removal that changes no pointer puts it back.
    let before = status::status(dir, name, secrets).ok().flatten();
    let say = |state: State| match Status::of_removal(state) {
        Ok(status) => model.say(&status),
        Err(_) => Err(io::Error::from(io::ErrorKind::InvalidInput)),
    };
    // Before anything else: whoever looks while the lock is held is shown a removal.
    say(State::Removing)?;
    // Whether this removal took the pointer away, durably or not.
    let mut gone = false;
    let ended = (|| {
        // Nothing is decided on the word of a pointer that is not durable: one
        // whose removal a crash can still undo would be read as a model that is
        // not installed.
        fs.sync_dir(&dir.model(name))?;
        let was = match model.named()? {
            Pointer::None => {
                // What a removal that was interrupted after it cleared the pointer
                // left, and what a dead install left: deleted as every install
                // deletes what no pointer names, each folder when no hold is on
                // it, after the sync above.
                model.clean("")?;
                model.unstage()?;
                return Err(RemoveRefused::NotInstalled);
            }
            Pointer::Names(id) => Some(id),
            Pointer::Damaged => None,
        };
        // Every installation of the model that is on the disk, and the one the
        // pointer names, whether or not its folder is there: a hold is given on
        // what the pointer names.
        let mut names: BTreeSet<OsString> = match fs.list(&model.installations()) {
            Ok(found) => found
                .iter()
                .filter_map(|path| path.file_name().map(OsString::from))
                .collect(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => BTreeSet::new(),
            Err(error) => return Err(error.into()),
        };
        names.extend(was.map(|id| OsString::from(id.hex())));
        let mut alone: Vec<Box<dyn Held>> = Vec::new();
        for name in &names {
            let file = model.holds().join(name);
            loop {
                match fs.try_alone(&file) {
                    Ok(Some(held)) => {
                        alone.push(held);
                        break;
                    }
                    Ok(None) if past(clock, deadline) => return Err(RemoveRefused::Held),
                    Ok(None) => {
                        // A second removal that made the mark before this one began
                        // takes it off when it ends busy: it is set again.
                        mark_if_none(dir, &folder)?;
                        pause();
                    }
                    // No hold file: nothing was ever held there.
                    Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                    Err(error) => return Err(error.into()),
                }
            }
        }
        // Looked at again: the look under the lock is as old as the wait for the
        // holds was long. Nothing is decided for a model whose layout has a link.
        dir.unlinked(name)?;
        // The decision, with every hold file this removal's alone: under the
        // directory's lock the pointer is taken away, and that is made durable.
        let directory = dir.lock()?.ok_or(RemoveRefused::DirectoryBusy)?;
        dir.remove(&model.pointer())?;
        gone = true;
        fs.sync_dir(&dir.model(name))?;
        drop(directory);
        // The deleting, now that the cleared pointer is durable and while every
        // hold file is still this removal's: everything under `installations/`,
        // a link itself and not what it leads to. And what an install left staged.
        // And once more, just before the deleting.
        dir.unlinked(name)?;
        let installations = model.installations();
        for name in &names {
            dir.remove(&installations.join(name))?;
        }
        if fs.exists(&installations)? {
            fs.sync_dir(&installations)?;
        }
        model.unstage()?;
        drop(alone);
        Ok(Removed { was })
    })();
    // Under the model's lock still: the status says `removed` of a model whose
    // pointer this removal took away, whether or not the step after it was refused,
    // and otherwise what it said before. It does not change what the removal answers.
    let _ = match before {
        _ if gone => say(State::Removed),
        Some(before) => model.say(&before),
        None => status::clear(dir, name),
    };
    ended
}
