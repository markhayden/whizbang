//! Publishing an installation (ADR 0017, "Publishing", "The pointer"): a model's
//! files are staged, read back from the disk, renamed into place under the identity
//! of the installation, and the model's pointer is replaced.
//!
//! One install of a model runs at a time, under the model's lock, for the whole of
//! it. The directory's lock is taken for a moment at the end, around the reading of
//! the draining mark, the making of the hold file and the replacing of the pointer,
//! and around nothing else: no download, no reading back and no deleting happens
//! under it. Only the pointer says that a model is installed, so a crash anywhere
//! before the pointer's rename is durable leaves the model as it was; what such a
//! crash leaves beside it is deleted by the next to take the model's lock.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};
use whizbang_core::contract::{Cancellation, Secrets};
use whizbang_core::egress::Downloader;
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::{Fs, PIECE_MAX_BYTES};

use super::directory::Read;
use super::installation::INSTALLATION_MAX_BYTES;
use super::stage::{FileRefused, StageRefused, lock_model, stage_locked};
use super::status::{self, State, Status};
use super::{
    Entry, InstallationId, InstallationRecord, InstalledFile, ModelName, ModelsDir, Refused,
    Revision,
};

/// The bytes of a pointer: `sha256:`, 64 hex digits and one line feed.
const POINTER_BYTES: usize = 72;

/// Why a model was not installed. Fixed words, a file's name as its catalogue gives
/// it, cut, and no path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallRefused {
    /// A name holds a secret the installer was told of, or the installation's record
    /// would spell one. Nothing was made on the disk.
    HoldsSecret,
    /// Another holds the model's lock: an install or a removal of the model.
    Busy,
    /// The install was cancelled before it began to publish.
    Cancelled,
    /// One file was not staged. `name` is what [`super::FileName::shown`] shows.
    File { name: String, why: FileRefused },
    /// A staged file, read back from the disk, was not the file that was staged.
    NotWhole { name: String },
    /// An installation of the same identity is on the disk, damaged, and held.
    Held,
    /// The model is being removed: nothing is published.
    Draining,
    /// The directory's lock was not let go by whoever has it.
    DirectoryBusy,
    /// The file system refused. The kind of failure, and no path.
    Disk(io::ErrorKind),
}

impl std::fmt::Display for InstallRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallRefused::HoldsSecret => {
                f.write_str("a name or the installation's record holds a secret, and is not shown")
            }
            InstallRefused::Busy => StageRefused::Busy.fmt(f),
            InstallRefused::Cancelled => f.write_str("the install was cancelled"),
            InstallRefused::File { name, why } => write!(f, "the file `{name}` {why}"),
            InstallRefused::NotWhole { name } => {
                write!(
                    f,
                    "the file `{name}` was not on the disk as it was downloaded"
                )
            }
            InstallRefused::Held => {
                f.write_str("a damaged installation of the same identity is held by a run")
            }
            InstallRefused::Draining => {
                f.write_str("the model is being removed, and nothing of it is published")
            }
            InstallRefused::DirectoryBusy => {
                f.write_str("the models directory is locked by another, which did not let it go")
            }
            InstallRefused::Disk(kind) => StageRefused::Disk(*kind).fmt(f),
        }
    }
}

impl std::error::Error for InstallRefused {}

impl From<StageRefused> for InstallRefused {
    fn from(refused: StageRefused) -> InstallRefused {
        match refused {
            StageRefused::HoldsSecret => InstallRefused::HoldsSecret,
            StageRefused::Busy => InstallRefused::Busy,
            StageRefused::Cancelled => InstallRefused::Cancelled,
            StageRefused::File { name, why } => InstallRefused::File { name, why },
            StageRefused::Disk(kind) => InstallRefused::Disk(kind),
        }
    }
}

/// What is wrong with what a model's pointer names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Damage {
    /// The pointer is not an identity's text and one line feed, in a regular file.
    Pointer,
    /// The record of the installation the pointer names is not there, or is not a
    /// record of this model in the one form whizbang writes.
    Record,
    /// The record is one of another installation than its folder is named for.
    Identity,
}

/// What a model's pointer says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Current {
    /// There is no pointer: the model is not installed, whatever else its folder
    /// holds.
    NotInstalled,
    /// The pointer names this installation, whose record was read.
    Installed(InstallationRecord),
    /// The pointer or what it names is damaged. This is never "not installed".
    Damaged(Damage),
}

impl From<io::Error> for InstallRefused {
    fn from(error: io::Error) -> InstallRefused {
        InstallRefused::Disk(error.kind())
    }
}

/// Installs a model: stages the files of its entry, reads each back from the disk,
/// and publishes them as the installation the model's pointer names.
///
/// `adapter_revision` is the revision the adapter the entry names states of itself.
/// No adapter is in the build yet, so the caller states it; it is part of the
/// installation's record and of its identity.
///
/// - The model's lock is held from before anything of the model is touched to the
///   end. Held by another, the answer is [`InstallRefused::Busy`].
/// - What the pointer does not name is deleted first, and again at the end: each
///   such folder when no hold is on it, by one try that does not wait.
/// - A staged file is read back from the disk and held to its size and its hash,
///   both, before anything is published.
/// - **A cancellation is asked for the last time when the staged files have been
///   read back.** From the rename of the staged folder on, the publication runs to
///   its end, and the answer is the installation.
/// - An identity that is on the disk already and whole is not downloaded again: it
///   is pointed to, after the folder that names it and the pointer are synced.
/// - A refused install removes its staging under the model's lock, writes how it
///   ended into the status record, and leaves the pointer naming what it named; a
///   repair refused after it cleared the pointer leaves the model not installed.
///   One step is the exception: when the sync that makes the new pointer durable
///   is refused, the install is refused and the pointer names the new
///   installation, which is whole. Nothing is then deleted until a sync of the
///   model's folder has passed, and asking again makes the pointer durable before
///   it answers.
pub fn install(
    dir: &ModelsDir,
    entry: &Entry,
    adapter_revision: &Revision,
    downloader: &dyn Downloader,
    cancellation: &Cancellation,
    secrets: &Secrets,
) -> Result<InstallationRecord, InstallRefused> {
    let record = match InstallationRecord::of(entry, adapter_revision.clone(), secrets) {
        Ok(record) => record,
        Err(Refused::HoldsSecret) => return Err(InstallRefused::HoldsSecret),
        // An entry's parts make a record, whatever they are: this is not reached.
        Err(_) => return Err(InstallRefused::Disk(io::ErrorKind::InvalidData)),
    };
    let _lock = lock_model(dir, entry, cancellation, secrets)?;
    let model = Model {
        dir,
        name: entry.name(),
        folder: dir.model(entry.name()),
        secrets,
    };
    let files = record.files().len() as u64;
    let hex = record.identity().hex();
    let mut done = 0;
    let ended = (|| {
        // What a publication that died left of its new pointer.
        dir.remove(&model.folder.join("CURRENT.new"))?;
        model.clean(&hex)?;
        // The same identity, on the disk already and whole: in use, superseded and
        // held, or renamed in by an install that was refused before it could point
        // to it. It is what would be staged: nothing is downloaded.
        let there = model.installations().join(&hex);
        let adopted = dir.fs().exists(&there)? && is_whole(dir, &there, &record)?;
        if adopted {
            dir.unstage(entry.name());
            done = files;
        } else {
            model.say(State::Installing, 0, files)?;
            stage_locked(dir, entry, downloader, cancellation, &mut |staged| {
                done = staged as u64;
                model.say(State::Installing, done, files)
            })?;
        }
        model.publish(&record, cancellation, adopted)
    })();
    // Under the model's lock still: the staging a refused install left is removed
    // here, and what the pointer does not name is deleted when no hold is on it,
    // but for a folder of this identity: one that was renamed in and could not be
    // pointed to is left, whole, for the next install of it. Neither changes what
    // the install answers.
    let state = match &ended {
        Ok(()) => State::Installed,
        Err(InstallRefused::Cancelled) => State::Cancelled,
        Err(_) => State::Failed,
    };
    if ended.is_err() {
        dir.unstage(entry.name());
    }
    let _ = model.say(state, done, files);
    let _ = model.clean(&hex);
    ended.map(|()| record)
}

/// What a model's pointer file holds.
#[derive(PartialEq, Eq)]
enum Pointer {
    /// There is no pointer.
    None,
    /// It names this installation.
    Names(InstallationId),
    /// It is not an identity's text and one line feed, in a regular file.
    Damaged,
}

/// Whether deleting an installation's folder was done.
#[derive(PartialEq, Eq)]
enum Deleted {
    Yes,
    /// A hold is on the installation: its folder is left.
    Held,
}

/// One model's folder, for one who holds the model's lock.
struct Model<'a> {
    dir: &'a ModelsDir,
    name: &'a ModelName,
    folder: PathBuf,
    secrets: &'a Secrets,
}

impl Model<'_> {
    fn fs(&self) -> &dyn Fs {
        self.dir.fs()
    }

    fn installations(&self) -> PathBuf {
        self.folder.join("installations")
    }

    fn holds(&self) -> PathBuf {
        self.folder.join("holds")
    }

    fn pointer(&self) -> PathBuf {
        self.folder.join("CURRENT")
    }

    /// Writes the status record. A count of files is one a status takes: the
    /// entry's.
    fn say(&self, state: State, done: u64, files: u64) -> io::Result<()> {
        let status = Status::new(state, done, files)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        status::write(self.dir, self.name, &status)
    }

    /// Publishes the installation of this record: what is staged, or, when
    /// `adopted`, the folder of its identity that is on the disk already and whole.
    fn publish(
        &self,
        record: &InstallationRecord,
        cancellation: &Cancellation,
        adopted: bool,
    ) -> Result<(), InstallRefused> {
        let fs = self.fs();
        let incoming = self.dir.incoming(self.name);
        let id = record.identity();
        let target = self.installations().join(id.hex());
        if !adopted {
            // From the disk, not from the stream: what was staged is what is there
            // now.
            if let Some(name) = first_not_whole(fs, &incoming.join("files"), record.files())? {
                return Err(InstallRefused::NotWhole { name });
            }
            if cancellation.is_cancelled() {
                return Err(InstallRefused::Cancelled);
            }
        }
        self.dir.ensure_dir(&self.installations())?;
        self.dir.ensure_dir(&self.holds())?;
        // Step 1: the installation in its place, under its identity, durably.
        if !adopted {
            if fs.exists(&target)? {
                // The same identity, damaged. In use, this is a repair: the pointer
                // is cleared first, so that no hold is given on what is deleted.
                let named =
                    read_pointer(self.dir, &self.folder, self.secrets)? == Pointer::Names(id);
                if named {
                    self.clear_pointer()?;
                }
                if self.delete(OsStr::new(&id.hex()))? == Deleted::Held {
                    if named {
                        self.point_to(id, false)?;
                    }
                    return Err(InstallRefused::Held);
                }
            }
            fs.write_new(&incoming.join("RECORD"), &record.to_bytes())?;
            fs.sync_dir(&incoming)?;
            fs.rename(&incoming, &target)?;
        }
        // Synced for a folder that was found as for one renamed in now: whoever
        // renamed it may have been refused this sync, and the pointer is about to
        // name it durably.
        fs.sync_dir(&self.installations())?;
        // Step 2: the pointer. Written and synced again when it names this
        // installation already: an earlier publication may have been refused the
        // sync that makes it durable, and no install is reported done on the word
        // of a name that is not.
        self.point_to(id, true)
    }

    /// Whether removal's mark is set, for one who holds the directory's lock. The
    /// mark's content is removal's (task M5b): that it is there is enough to stop
    /// a publication.
    fn draining(&self) -> io::Result<bool> {
        self.fs().exists(&self.folder.join("DRAINING"))
    }

    /// Under the directory's lock: reads the draining mark, when the pointer is to
    /// name a new installation; makes the identity's hold file if it is not there;
    /// and replaces the pointer. Nothing else is done under that lock. A pointer
    /// that a repair cleared is put back whatever the mark says (`new` is false), so
    /// that nobody finds the model with neither a pointer nor a reason.
    fn point_to(&self, id: InstallationId, new: bool) -> Result<(), InstallRefused> {
        let fs = self.fs();
        let hold = self.holds().join(id.hex());
        let beside = self.folder.join("CURRENT.new");
        let _directory = self.dir.lock()?.ok_or(InstallRefused::DirectoryBusy)?;
        if new && self.draining()? {
            return Err(InstallRefused::Draining);
        }
        match fs.write_new(&hold, b"") {
            Err(error) if error.kind() != io::ErrorKind::AlreadyExists => return Err(error.into()),
            _ => {}
        }
        // Synced whether it was made now or found: one that an earlier publication
        // made and could not sync is not durable yet, and the pointer is about to be.
        fs.sync_dir(&self.holds())?;
        fs.write_new(&beside, format!("{id}\n").as_bytes())?;
        fs.rename(&beside, &self.pointer())?;
        fs.sync_dir(&self.folder)?;
        Ok(())
    }

    /// Under the directory's lock: reads the draining mark, and takes the pointer
    /// away, durably. With the mark set the pointer is left: a model that is being
    /// removed is not repaired, and its removal finds it as it was.
    fn clear_pointer(&self) -> Result<(), InstallRefused> {
        let _directory = self.dir.lock()?.ok_or(InstallRefused::DirectoryBusy)?;
        if self.draining()? {
            return Err(InstallRefused::Draining);
        }
        self.dir.remove(&self.pointer())?;
        self.fs().sync_dir(&self.folder)?;
        Ok(())
    }

    /// Deletes the folder of an installation the pointer does not name, when no hold
    /// is on it: its hold file is taken alone by one try, which does not wait, and
    /// let go once the folder is deleted and the deletion synced. A folder with no
    /// hold file was never published: it is deleted, and no hold file is made.
    fn delete(&self, name: &OsStr) -> io::Result<Deleted> {
        let alone = match self.fs().try_alone(&self.holds().join(name)) {
            Ok(Some(alone)) => Some(alone),
            Ok(None) => return Ok(Deleted::Held),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        self.dir.remove(&self.installations().join(name))?;
        self.fs().sync_dir(&self.installations())?;
        drop(alone);
        Ok(Deleted::Yes)
    }

    /// Deletes what is under `installations/` and is not named by the pointer: a
    /// superseded installation when no hold is on it, a folder a crash left before
    /// its pointer was set, and anything that is no installation's folder at all.
    /// Each is deleted by its name as the listing gave it, which is one name.
    /// With a pointer that cannot be read nothing is deleted: which installation is
    /// in use is then not known.
    ///
    /// **Nothing is deleted on the word of a pointer that is not durable.** The
    /// model's folder is synced first, and when that is refused nothing is deleted:
    /// a pointer whose rename a crash can still undo may come back naming what was
    /// superseded.
    ///
    /// A folder named `keep` is left: the identity being installed, which the
    /// install adopts when it is whole and replaces when it is not.
    ///
    /// The pointer is read without the directory's lock: it is changed only by one
    /// who holds the model's lock, which the caller does.
    fn clean(&self, keep: &str) -> io::Result<()> {
        self.fs().sync_dir(&self.folder)?;
        let named = match read_pointer(self.dir, &self.folder, self.secrets)? {
            Pointer::Names(id) => Some(id.hex()),
            Pointer::None => None,
            Pointer::Damaged => return Ok(()),
        };
        let found = match self.fs().list(&self.installations()) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            found => found?,
        };
        for path in found {
            let Some(name) = path.file_name() else {
                continue;
            };
            if name == keep || named.as_deref().is_some_and(|named| name == named) {
                continue;
            }
            // Whatever it is: an installation's folder, or a name this layout does
            // not give, which has no hold file. A link is removed, not followed.
            self.delete(name)?;
        }
        Ok(())
    }
}

/// Reads a model's pointer: exactly an identity's text and one line feed, in a
/// regular file under its own name. Anything else that is there is damage.
fn read_pointer(dir: &ModelsDir, folder: &Path, secrets: &Secrets) -> io::Result<Pointer> {
    let bytes = match dir.read(&folder.join("CURRENT"), POINTER_BYTES as u64)? {
        Read::Missing => return Ok(Pointer::None),
        Read::NotAFile => return Ok(Pointer::Damaged),
        Read::Bytes(bytes) => bytes,
    };
    let text = match bytes.split_last() {
        Some((b'\n', text)) if bytes.len() == POINTER_BYTES => text,
        _ => return Ok(Pointer::Damaged),
    };
    let id = std::str::from_utf8(text)
        .ok()
        .and_then(|text| InstallationId::from_text(text, secrets).ok());
    Ok(id.map_or(Pointer::Damaged, Pointer::Names))
}

/// The first of these files, by its name as it is shown, that is not on the disk
/// under `files` as a regular file of its size and its hash; none when all are.
fn first_not_whole(
    fs: &dyn Fs,
    files: &Path,
    listed: &[InstalledFile],
) -> io::Result<Option<String>> {
    for file in listed {
        if !file_is_whole(fs, &files.join(file.name().as_str()), file)? {
            return Ok(Some(file.name().shown()));
        }
    }
    Ok(None)
}

/// Whether the file at `path` is a regular file under its own name with the size
/// and the hash stated: read piece by piece, counted, and stopped one byte past its
/// size. A file is held to both, never to its length alone: a crash can leave a
/// file of the right length whose bytes were never written.
fn file_is_whole(fs: &dyn Fs, path: &Path, file: &InstalledFile) -> io::Result<bool> {
    let mut open = match fs.open_file(path) {
        Ok(open) => open,
        Err(error) => {
            return match error.kind() {
                io::ErrorKind::NotFound | io::ErrorKind::InvalidData => Ok(false),
                _ => Err(error),
            };
        }
    };
    let size = file.size();
    let mut buffer = vec![
        0;
        usize::try_from(size + 1)
            .map_or(PIECE_MAX_BYTES, |most| most.min(PIECE_MAX_BYTES))
    ];
    let mut hash = Sha256::new();
    let mut read = 0u64;
    loop {
        // No more is asked for than one byte past the size.
        let left =
            usize::try_from(size + 1 - read).map_or(buffer.len(), |left| left.min(buffer.len()));
        let filled = open.read(&mut buffer[..left])?;
        read += filled as u64;
        if read > size {
            return Ok(false);
        }
        hash.update(&buffer[..filled]);
        if filled < left {
            break;
        }
    }
    Ok(read == size && Digest::from_bytes(hash.finalize().into()) == file.sha256())
}

/// The names in a folder, in order; none when what is there is not a folder.
fn names_in(fs: &dyn Fs, folder: &Path) -> io::Result<Option<Vec<String>>> {
    match fs.list(folder) {
        Ok(found) => Ok(Some(
            found
                .iter()
                .map(|path| {
                    let name = path.file_name().unwrap_or_default();
                    name.to_string_lossy().into_owned()
                })
                .collect(),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotADirectory => Ok(None),
        Err(error) => Err(error),
    }
}

/// Whether the folder at `folder` is the installation of this record, whole: its
/// record in the one form, the files the record lists, each of its size and its
/// hash, and nothing else.
fn is_whole(dir: &ModelsDir, folder: &Path, record: &InstallationRecord) -> io::Result<bool> {
    let fs = dir.fs();
    let written = record.to_bytes();
    match dir.read(&folder.join("RECORD"), INSTALLATION_MAX_BYTES)? {
        Read::Bytes(bytes) if bytes == written => {}
        _ => return Ok(false),
    }
    if names_in(fs, folder)? != Some(vec!["RECORD".to_owned(), "files".to_owned()]) {
        return Ok(false);
    }
    // The record's files are in the order of their names, as a listing is.
    let listed: Vec<String> = record
        .files()
        .iter()
        .map(|file| file.name().as_str().to_owned())
        .collect();
    if names_in(fs, &folder.join("files"))? != Some(listed) {
        return Ok(false);
    }
    Ok(first_not_whole(fs, &folder.join("files"), record.files())?.is_none())
}

/// What the model's pointer names, read by one who holds no lock.
///
/// The pointer is read, and then the record it names. When the record is not
/// there, the pointer is read again before anything is said: an install or a
/// removal may have replaced it between the two reads. Damage is reported only
/// when the pointer still names the installation whose record is missing.
///
/// The record is held to its form and to its folder: a record whose identity is not
/// the name of its folder is damage. The files are not read here.
pub fn current(
    dir: &ModelsDir,
    model: &ModelName,
    secrets: &Secrets,
) -> Result<Current, io::ErrorKind> {
    let folder = dir.model(model);
    let pointer = || read_pointer(dir, &folder, secrets).map_err(|error| error.kind());
    let mut named = pointer()?;
    // A pointer that moves under every read is not followed for ever.
    for _ in 0..8 {
        let id = match named {
            Pointer::None => return Ok(Current::NotInstalled),
            Pointer::Damaged => return Ok(Current::Damaged(Damage::Pointer)),
            Pointer::Names(id) => id,
        };
        let at = folder.join("installations").join(id.hex()).join("RECORD");
        let bytes = match dir.read(&at, INSTALLATION_MAX_BYTES) {
            Ok(Read::Bytes(bytes)) => bytes,
            Ok(Read::NotAFile) => return Ok(Current::Damaged(Damage::Record)),
            Ok(Read::Missing) => {
                named = pointer()?;
                if named == Pointer::Names(id) {
                    return Ok(Current::Damaged(Damage::Record));
                }
                continue;
            }
            Err(error) => return Err(error.kind()),
        };
        return Ok(match InstallationRecord::parse(&bytes, secrets) {
            // The model's name is part of the identity: a record of another model
            // is a record of another identity.
            Ok(record) if record.identity() != id => Current::Damaged(Damage::Identity),
            Ok(record) => Current::Installed(record),
            Err(_) => Current::Damaged(Damage::Record),
        });
    }
    Err(io::ErrorKind::ResourceBusy)
}
