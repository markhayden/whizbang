//! Removal of a model (ADR 0017, "Removal"): the model drains, then it is removed.
//!
//! A removal takes the remover's lock of the model, alone, and keeps it for as long
//! as its mark is set: a mark is live only while its remover is ([`draining`]), so
//! the mark of a remover that died stops nobody. It marks the model as draining,
//! under the directory's lock, and raises the model's count of removals under the
//! same lock ([`removals`]): from then no
//! hold on it is given and no install of it publishes. It takes the model's lock, by
//! trying until its deadline, and says `removing` in the status record. It waits for
//! the holds by trying each hold file alone, again and again until the same
//! deadline, which is one of the injected clock; its own tries, for the remover's
//! lock, the model's lock and a hold file, do not sleep, and what comes between
//! two of them is the caller's (`pause`). The directory's lock, which it asks for
//! around its mark, its count and the pointer, is another matter: each asking may
//! wait up to five seconds of the directory's clock ([`ModelsDir::open`]), which on
//! the machine's clock sleeps between its tries. With every hold file its own, it
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

/// Whether a remover of the model is alive: a try for a share of the remover's
/// lock (`models/<model>/REMOVER`), which a remover holds alone for as long as its
/// mark is set. The share is let go at once. Two that ask at the same moment are
/// each given a share, so neither reads the other as a remover; a lock file that
/// was never made is no remover's.
fn remover_lives(dir: &ModelsDir, folder: &Path) -> io::Result<bool> {
    match dir.fs().try_share(&folder.join("REMOVER")) {
        Ok(Some(_share)) => Ok(false),
        Ok(None) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Whether the model is draining: whether a mark is set **and its remover is
/// alive**. This is the one question everybody asks of a mark: a hold, a
/// publication, an install where it asks its cancellation, and whoever lists models
/// (the plan of checkpoint 1C, decision D6).
///
/// - Nothing at the mark's name: no. The remover's lock is not tried for.
/// - Something there, and no remover has the remover's lock: no, whatever is
///   there. It is what a remover that died left, and it stops nobody.
/// - Something there under a remover that is alive: yes when it is a mark as
///   whizbang writes it. Anything else is refused, never followed and never
///   guessed at; whoever decides by the mark stops for a refusal as for a mark
///   ([`stops`]), so a live remover's mark that cannot be read still stops them.
pub fn draining(dir: &ModelsDir, model: &ModelName, secrets: &Secrets) -> Result<bool, Refused> {
    let what = What::Mark;
    let unreadable = |error: io::Error| Refused::Unreadable {
        what,
        kind: error.kind(),
    };
    let folder = dir.model(model);
    let mark = folder.join("DRAINING");
    if !dir.fs().exists(&mark).map_err(unreadable)? {
        return Ok(false);
    }
    if !remover_lives(dir, &folder).map_err(unreadable)? {
        return Ok(false);
    }
    match dir.read(&mark, DRAINING_MAX_BYTES) {
        Ok(Read::Bytes(bytes)) => Mark::parse(&bytes, secrets).map(|_| true),
        // Taken off since the look: its remover is ending.
        Ok(Read::Missing) => Ok(false),
        Ok(Read::NotAFile) => Err(unreadable(io::ErrorKind::InvalidData.into())),
        Err(error) => Err(unreadable(error)),
    }
}

/// [`draining`], for one who decides by the answer: a hold, a publication, an
/// install's look where it asks its cancellation. What is at the mark's name under
/// a live remover and cannot be read as a mark stops them as a mark does; a file
/// system that refused is an error, and has not said "draining".
pub(super) fn stops(dir: &ModelsDir, model: &ModelName, secrets: &Secrets) -> io::Result<bool> {
    match draining(dir, model, secrets) {
        Ok(answer) => Ok(answer),
        Err(Refused::Unreadable { kind, .. }) if kind != io::ErrorKind::InvalidData => {
            Err(kind.into())
        }
        Err(_) => Ok(true),
    }
}

/// The format of a count of removals.
pub const REMOVALS_FORMAT: &str = "removals/1";

/// The most bytes a count of removals may be. The longest whizbang writes is 52.
pub const REMOVALS_MAX_BYTES: u64 = 64;

/// The count of removals as it is written, `models/<model>/REMOVALS`: compact JSON,
/// the format and then the count, nothing after the brace.
fn count_bytes(count: u64) -> Vec<u8> {
    format!(r#"{{"format":"{REMOVALS_FORMAT}","count":{count}}}"#).into_bytes()
}

/// Reads these bytes as a count of removals, as every record here is read.
fn parse_count(bytes: &[u8], secrets: &Secrets) -> Result<u64, Refused> {
    let what = What::Removals;
    let json = json::parse(what, bytes, REMOVALS_MAX_BYTES, secrets)?;
    let mut top = Object::of(json, "")?;
    if top.text("format")? != REMOVALS_FORMAT {
        return Err(Refused::at("format", Rule::Format));
    }
    let count = top.whole("count")?;
    top.done()?;
    if count_bytes(count) != bytes {
        return Err(Refused::OtherForm { what });
    }
    Ok(count)
}

/// How many removals of the model have set their mark: the model's count of
/// removals, which each removal raises under the directory's lock when it sets its
/// mark, and which nothing lowers (the plan of checkpoint 1C, decision D5). Nothing
/// when no removal of the model ever did. It may be read by one who holds no lock:
/// the count is replaced by a rename, so a reader finds the old one or the new.
///
/// What is at the count's name and is no count is refused, never followed and
/// never guessed at.
pub fn removals(dir: &ModelsDir, model: &ModelName, secrets: &Secrets) -> Result<u64, Refused> {
    let what = What::Removals;
    match dir.read(&dir.model(model).join("REMOVALS"), REMOVALS_MAX_BYTES) {
        Ok(Read::Bytes(bytes)) => parse_count(&bytes, secrets),
        Ok(Read::Missing) => Ok(0),
        Ok(Read::NotAFile) => Err(Refused::Unreadable {
            what,
            kind: io::ErrorKind::InvalidData,
        }),
        Err(error) => Err(Refused::Unreadable {
            what,
            kind: error.kind(),
        }),
    }
}

/// [`removals`], for a hold, which reads it under the directory's lock: a count
/// that cannot be read as one is `None`, and a file system that refused is an error.
pub(super) fn removals_read(
    dir: &ModelsDir,
    model: &ModelName,
    secrets: &Secrets,
) -> io::Result<Option<u64>> {
    match removals(dir, model, secrets) {
        Ok(count) => Ok(Some(count)),
        Err(Refused::Unreadable { kind, .. }) if kind != io::ErrorKind::InvalidData => {
            Err(kind.into())
        }
        Err(_) => Ok(None),
    }
}

/// Raises the model's count of removals by one, for a remover that holds the
/// directory's lock and has just set its mark. The new count is written whole
/// beside the old one and renamed over it. It is given no sync of its own: a
/// crash that loses it has ended every process that held the old one. A count
/// that cannot be read as one is counted from nothing again.
fn raise(dir: &ModelsDir, model: &ModelName, secrets: &Secrets) -> io::Result<()> {
    let folder = dir.model(model);
    let (count, new) = (folder.join("REMOVALS"), folder.join("REMOVALS.new"));
    let was = match removals_read(dir, model, secrets)? {
        Some(was) => was,
        None => {
            // What is there may be no file at all, which a rename does not replace.
            dir.remove(&count)?;
            0
        }
    };
    dir.remove(&new)?;
    dir.fs()
        .write_new(&new, &count_bytes(was.saturating_add(1)))?;
    dir.fs().rename(&new, &count)
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
/// - **The remover's lock comes before the mark,** taken alone by trying until the
///   deadline, with `pause` between two tries, and kept until the mark is taken
///   off: while it is held the mark is live ([`draining`]), and when its process
///   dies the mark stops nobody. Not given by the deadline, because another
///   removal of the model is alive, the answer is [`RemoveRefused::Busy`], and no
///   mark was set and no count raised.
/// - **A folder of the model's layout that is a link is refused**
///   ([`RemoveRefused::Linked`]): looked for before the mark, again under the
///   model's lock before anything is listed, again when the holds have been
///   waited for and before the pointer is cleared, and once more just before
///   the deleting. Nothing is deleted through it.
/// - **The mark comes next,** under the directory's lock, and under the same lock
///   the model's count of removals is raised ([`removals`]). What was at the
///   mark's name is a dead remover's, and is replaced: with the remover's lock
///   the mark is the removal's own, and it is taken off at the end. A model with
///   no folder was never installed: it is not marked, and nothing is made.
/// - **The model's lock is tried for until the deadline;** `pause` is called once
///   between two tries, and these tries do not sleep. Not given by then, the
///   answer is [`RemoveRefused::Busy`]. Each asking for the directory's lock may
///   wait up to five seconds of the directory's clock on top of the patience, and
///   then answers [`RemoveRefused::DirectoryBusy`].
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
    remove_and_tell(dir, model, clock, patience, secrets, pause, &mut || {})
}

/// [`remove`], telling `marked` each time the removal has set its mark, with the
/// count raised, and has let the directory's lock go: the first time, and each time
/// it sets the mark again. What the removal does is the same; `marked` runs on its
/// thread, holding the remover's lock and no other. It is how a removal tells the
/// worker's supervisor of its own process (task P5b of checkpoint 1C, decision D5),
/// which ends the requests queued for the model and unloads it when it is idle.
pub fn remove_and_tell(
    dir: &ModelsDir,
    model: &ModelName,
    clock: &dyn Clock,
    patience: Duration,
    secrets: &Secrets,
    pause: &mut dyn FnMut(),
    marked: &mut dyn FnMut(),
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
    // The remover's lock, alone, from before the mark is set until it is taken
    // off: a mark is live only while this is held. Another removal of the model
    // has it, or one who asks whether a mark is live has a share of it for a
    // moment: tried again until the deadline. It is never removed, as no lock
    // file is.
    let remover = folder.join("REMOVER");
    let _remover = loop {
        if let Some(lock) = dir.fs().hold_lock_file(&remover, true)? {
            break lock;
        }
        if past(clock, deadline) {
            return Err(RemoveRefused::Busy);
        }
        pause();
    };
    // With the remover's lock whatever is at the mark's name is this removal's
    // own: a remover that died left it, and it is replaced.
    let ended = set_mark(dir, model, secrets).and_then(|()| {
        marked();
        drain(dir, model, clock, deadline, secrets, pause, marked)
    });
    // However it ended. A mark that cannot be taken off now is a dead remover's
    // once this one's lock is let go, and stops nobody; it does not change what
    // this one answers.
    let _ = clear_mark(dir, &folder);
    ended
}

/// Whether the deadline has come. No deadline is one that never does.
fn past(clock: &dyn Clock, deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| clock.now() >= deadline)
}

/// Sets the model's mark, under the directory's lock, and under the same lock
/// raises the model's count of removals: no mark is written without the count
/// moving. Whatever was at the mark's name is replaced.
fn set_mark(dir: &ModelsDir, model: &ModelName, secrets: &Secrets) -> Result<(), RemoveRefused> {
    let mark = dir.model(model).join("DRAINING");
    let _directory = dir.lock()?.ok_or(RemoveRefused::DirectoryBusy)?;
    dir.remove(&mark)?;
    dir.fs().write_new(&mark, &Mark.to_bytes())?;
    raise(dir, model, secrets)?;
    Ok(())
}

/// Sets the model's mark when nothing is at its name, with its count raised, and
/// says whether it did. Looked for again under the directory's lock. For a
/// removal whose mark was taken off under it: by a person, or by a version of
/// whizbang that could not tell a live remover's mark from a dead one's.
fn mark_if_none(
    dir: &ModelsDir,
    model: &ModelName,
    secrets: &Secrets,
) -> Result<bool, RemoveRefused> {
    let mark = dir.model(model).join("DRAINING");
    if dir.fs().exists(&mark)? {
        return Ok(false);
    }
    let _directory = dir.lock()?.ok_or(RemoveRefused::DirectoryBusy)?;
    if dir.fs().exists(&mark)? {
        return Ok(false);
    }
    dir.fs().write_new(&mark, &Mark.to_bytes())?;
    raise(dir, model, secrets)?;
    Ok(true)
}

/// Whether what is at the mark's name is the mark as whizbang writes it.
fn mark_is_written(dir: &ModelsDir, model: &ModelName) -> bool {
    let mark = dir.model(model).join("DRAINING");
    matches!(dir.read(&mark, DRAINING_MAX_BYTES), Ok(Read::Bytes(bytes)) if bytes == Mark.to_bytes())
}

/// Takes the model's mark off, under the directory's lock, and makes that durable.
fn clear_mark(dir: &ModelsDir, folder: &Path) -> Result<(), RemoveRefused> {
    let _directory = dir.lock()?.ok_or(RemoveRefused::DirectoryBusy)?;
    dir.remove(&folder.join("DRAINING"))?;
    dir.fs().sync_dir(folder)?;
    Ok(())
}

/// What a removal does once its mark is set: the model's lock, the status, the
/// holds, and the pointer. `marked` is told each time the mark is set again.
#[allow(clippy::too_many_arguments)]
fn drain(
    dir: &ModelsDir,
    name: &ModelName,
    clock: &dyn Clock,
    deadline: Option<Instant>,
    secrets: &Secrets,
    pause: &mut dyn FnMut(),
    marked: &mut dyn FnMut(),
) -> Result<Removed, RemoveRefused> {
    let fs = dir.fs();
    let lock = dir.model(name).join("LOCK");
    let _lock = loop {
        if let Some(lock) = fs.hold_lock_file(&lock, true)? {
            break lock;
        }
        if past(clock, deadline) {
            return Err(RemoveRefused::Busy);
        }
        // A mark that is gone is set again, so that an install that is running
        // reads it.
        if mark_if_none(dir, name, secrets)? {
            marked();
        }
        pause();
    };
    // Looked at again under the lock, which may have been waited for: nothing is
    // listed or deleted through a folder of the layout that is a link.
    dir.unlinked(name)?;
    // What is there and is no mark as it is written is replaced.
    if !mark_is_written(dir, name) {
        set_mark(dir, name, secrets)?;
        marked();
    }
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
        for installation in &names {
            let file = model.holds().join(installation);
            loop {
                match fs.try_alone(&file) {
                    Ok(Some(held)) => {
                        alone.push(held);
                        break;
                    }
                    Ok(None) if past(clock, deadline) => return Err(RemoveRefused::Held),
                    Ok(None) => {
                        // A mark that is gone is set again.
                        if mark_if_none(dir, name, secrets)? {
                            marked();
                        }
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
