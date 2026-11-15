//! Downloading into staging (ADR 0017, "Downloading into staging"): each file of a
//! catalogue's entry is downloaded, under the model's lock, into the model's one
//! staging folder, stopped at its stated size and checked by its length and its hash.
//! Nothing is published here: what is staged is handed to the publication (task M2b)
//! with the model's lock still held.
//!
//! A file is written as a stream and never held whole in memory. A download that
//! fails, for any file and for any reason, removes the staging folder, so no staged
//! file that passed is left behind.

use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};
use whizbang_core::capability::FaultKind;
use whizbang_core::contract::{Cancellation, Secrets};
use whizbang_core::egress::{DownloadRequest, Downloaded, Downloader};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::{LockFile, NewFile, PIECE_MAX_BYTES};

use super::directory::{LINKED, is_linked};
use super::name::holds_secret;
use super::{CatalogueFile, Entry, InstalledFile, ModelName, ModelsDir};

/// The most times one file may be answered with another address before it is refused.
pub const MOVES_LIMIT: usize = 5;

/// Why one file was not staged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRefused {
    /// Fewer bytes came than the catalogue states.
    TooShort,
    /// A byte came beyond the size the catalogue states.
    TooLong,
    /// The bytes that came do not have the hash the catalogue states.
    WrongHash,
    /// The file moved to an address at an origin the entry does not name. Nothing was
    /// asked of that address.
    Elsewhere,
    /// The file moved more than [`MOVES_LIMIT`] times.
    TooManyMoves,
    /// The download failed, with a fault of this kind. The fault's words are not
    /// kept: they are a server's.
    Download(FaultKind),
}

impl std::fmt::Display for FileRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileRefused::TooShort => f.write_str("is shorter than the catalogue states"),
            FileRefused::TooLong => f.write_str("is longer than the catalogue states"),
            FileRefused::WrongHash => f.write_str("does not have the hash the catalogue states"),
            FileRefused::Elsewhere => f.write_str("moved to an origin its model does not name"),
            FileRefused::TooManyMoves => write!(f, "moved more than {MOVES_LIMIT} times"),
            FileRefused::Download(kind) => {
                write!(f, "could not be downloaded: {}", kind.name())
            }
        }
    }
}

/// Why a model's files were not staged. It is fixed words, a file's name as its
/// catalogue gives it, cut, and no path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageRefused {
    /// The model's name or a file's holds a secret the installer was told of. Nothing
    /// was made on the disk, and nothing of the name is shown.
    HoldsSecret,
    /// Another holds the model's lock: an install or a removal of the model.
    Busy,
    /// The download was cancelled.
    Cancelled,
    /// The model is marked as draining: a removal of it has begun, and the download
    /// stopped for it.
    Draining,
    /// One file was not staged. `name` is what [`super::FileName::shown`] shows of it.
    File { name: String, why: FileRefused },
    /// A folder of the model's layout is a link: `models/`, the model's folder, its
    /// `installations/`, `staging/` or `holds/`. Nothing is made, listed or deleted
    /// through it.
    Linked,
    /// The file system refused. The kind of failure, and no path.
    Disk(io::ErrorKind),
}

impl std::fmt::Display for StageRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StageRefused::HoldsSecret => f.write_str("a name holds a secret, and is not shown"),
            StageRefused::Busy => f.write_str("the model is being installed or removed by another"),
            StageRefused::Cancelled => f.write_str("the download was cancelled"),
            StageRefused::Draining => {
                f.write_str("the model is being removed, and nothing of it is staged")
            }
            StageRefused::File { name, why } => write!(f, "the file `{name}` {why}"),
            StageRefused::Linked => f.write_str(LINKED),
            StageRefused::Disk(kind) => {
                let why = match kind {
                    io::ErrorKind::PermissionDenied => "permission is denied",
                    io::ErrorKind::StorageFull => "the disk is full",
                    _ => "the system refused",
                };
                write!(f, "the models directory could not be written: {why}")
            }
        }
    }
}

impl std::error::Error for StageRefused {}

impl From<io::Error> for StageRefused {
    fn from(error: io::Error) -> StageRefused {
        if is_linked(&error) {
            return StageRefused::Linked;
        }
        StageRefused::Disk(error.kind())
    }
}

/// A model's files, staged and verified, with the model's lock held: nobody else
/// stages, publishes or removes that model until this is dropped. What `{:?}` prints
/// is the model and how many files.
pub struct Staged {
    model: ModelName,
    files: Vec<InstalledFile>,
    _lock: Box<dyn LockFile>,
}

impl std::fmt::Debug for Staged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Staged({}, {} files)",
            self.model.shown(),
            self.files.len()
        )
    }
}

impl Staged {
    pub fn model(&self) -> &ModelName {
        &self.model
    }

    /// The files that were staged, each with the size and the hash it was verified
    /// to have, in the order of their names.
    pub fn files(&self) -> &[InstalledFile] {
        &self.files
    }
}

/// Downloads every file of the entry into the model's staging folder.
///
/// - The model's name and every file's are looked at for a secret first, with the
///   installer's own secrets: the entry may have been made by a constructor that was
///   told of others. With one found, nothing is made on the disk.
/// - The model's lock is taken without waiting; held by another, the answer is
///   [`StageRefused::Busy`] and nothing is touched.
/// - A folder of the model's layout that is a link is refused, before the lock and
///   again before anything is removed ([`StageRefused::Linked`]): nothing is made,
///   listed or deleted through it.
/// - What an earlier staging left is removed, whatever it is, and not looked at.
/// - Where the cancellation is asked once the lock is held, before each file, at
///   each piece and after each answer of the downloader, the model's draining mark
///   is read too: with one set, the staging stops as [`StageRefused::Draining`].
/// - Each file is asked for at its address, and at an address it moved to only when
///   the entry names that address's origin. One byte more than its size is asked
///   for and no more than its size is kept: a byte beyond it refuses the file.
/// - A staging refused after it took the lock removes the staging folder and syncs
///   the removal. When the removal itself is refused, what is left is removed by the
///   next to stage, which trusts none of it. A refusal before the lock touches
///   nothing.
///
/// What is staged is published by [`super::install`], which stages for itself: a
/// [`Staged`] is what the files were held to, and the lock.
pub fn stage(
    dir: &ModelsDir,
    entry: &Entry,
    downloader: &dyn Downloader,
    cancellation: &Cancellation,
    secrets: &Secrets,
) -> Result<Staged, StageRefused> {
    let lock = lock_model(dir, entry, cancellation, secrets)?;
    let files = stage_locked(dir, entry, downloader, cancellation, &mut |_| Ok(()))?;
    Ok(Staged {
        model: entry.name().clone(),
        files,
        _lock: lock,
    })
}

/// The model's lock, taken alone by a try, for an install of this entry: after the
/// names were looked at for a secret and the cancellation was asked, and before
/// anything of the model but its folder and its lock file is made.
pub(super) fn lock_model(
    dir: &ModelsDir,
    entry: &Entry,
    cancellation: &Cancellation,
    secrets: &Secrets,
) -> Result<Box<dyn LockFile>, StageRefused> {
    let named = entry.files().iter().map(|file| file.name().as_str());
    if std::iter::once(entry.name().as_str())
        .chain(named)
        .any(|name| holds_secret(name, secrets))
    {
        return Err(StageRefused::HoldsSecret);
    }
    if cancellation.is_cancelled() {
        return Err(StageRefused::Cancelled);
    }
    // Before anything is made: a folder of the layout that is a link is not gone
    // through, to make a folder or a lock file any more than to delete.
    dir.unlinked(entry.name())?;
    let model = dir.model(entry.name());
    dir.ensure_dir(&dir.models())?;
    dir.ensure_dir(&model)?;
    dir.fs()
        .hold_lock_file(&model.join("LOCK"), true)?
        .ok_or(StageRefused::Busy)
}

/// What stops a staging between files and inside one: the cancellation of the
/// request that started it, and the model's draining mark, which a removal sets
/// from any process. The mark is read as there or not there, without the
/// directory's lock: stopping is always safe, and what a publication decides from
/// is read under that lock, later.
struct Stop<'a> {
    dir: &'a ModelsDir,
    mark: PathBuf,
    cancellation: &'a Cancellation,
}

impl Stop<'_> {
    /// Why the staging stops now, if it does. A mark that cannot be looked for is
    /// no mark: the step after it meets the same disk.
    fn asked(&self) -> Option<StageRefused> {
        if self.cancellation.is_cancelled() {
            return Some(StageRefused::Cancelled);
        }
        self.dir
            .fs()
            .exists(&self.mark)
            .unwrap_or(false)
            .then_some(StageRefused::Draining)
    }
}

/// Stages every file of the entry, for one who holds the model's lock. `staged` is
/// told after each file how many are staged, and its refusal refuses the staging.
pub(super) fn stage_locked(
    dir: &ModelsDir,
    entry: &Entry,
    downloader: &dyn Downloader,
    cancellation: &Cancellation,
    staged: &mut dyn FnMut(usize) -> io::Result<()>,
) -> Result<Vec<InstalledFile>, StageRefused> {
    let fs = dir.fs();
    let staging = dir.staging(entry.name());
    let incoming = dir.incoming(entry.name());
    let files = incoming.join("files");
    let stop = Stop {
        dir,
        mark: dir.model(entry.name()).join("DRAINING"),
        cancellation,
    };
    let done = (|| {
        // Looked at again before what an earlier staging left is removed.
        dir.unlinked(entry.name())?;
        dir.remove(&incoming)?;
        dir.ensure_dir(&staging)?;
        fs.create_dir(&incoming)?;
        fs.sync_dir(&staging)?;
        fs.create_dir(&files)?;
        fs.sync_dir(&incoming)?;
        let mut done = Vec::new();
        for file in entry.files() {
            // Asked before each file: nothing is begun after a cancel, or under a
            // mark.
            if let Some(stopped) = stop.asked() {
                return Err(stopped);
            }
            stage_file(dir, entry, file, &files, downloader, &stop)?;
            // Made from what the catalogue states, which the file was just held to.
            let installed = InstalledFile::new(file.name().clone(), file.size(), file.sha256());
            done.push(installed.map_err(|_| StageRefused::Disk(io::ErrorKind::InvalidData))?);
            staged(done.len())?;
        }
        fs.sync_dir(&files)?;
        Ok(done)
    })();
    if done.is_err() {
        // Under the lock still. What cannot be removed now is removed by the next
        // to stage, which trusts nothing it finds here.
        dir.unstage(entry.name());
    }
    done
}

/// What the sink of one file's download has seen.
struct Received {
    file: Box<dyn NewFile>,
    /// The size the catalogue states: the most that is kept.
    size: u64,
    kept: u64,
    hash: Sha256,
    /// A byte came beyond the size.
    beyond: bool,
    /// The file system refused an append.
    disk: Option<io::ErrorKind>,
}

impl Received {
    /// Takes one piece, and says whether to go on. A piece that would take the file
    /// past its size is a byte beyond it: the file is too long, none of the piece is
    /// written, and nothing more is wanted. The file was begun with its size as the
    /// most it may hold, so the file system would refuse such a piece too; but what
    /// the file system refuses is the disk's refusal, whatever it calls it. A full
    /// disk and a limit on a file's size both say that a file grew too large, of a
    /// file that is as long as its catalogue states. A handle is broken by a refusal,
    /// and what it wrote is removed with the staging folder.
    fn take(&mut self, piece: &[u8]) -> bool {
        if self.beyond || self.disk.is_some() {
            return false;
        }
        if self.kept.saturating_add(piece.len() as u64) > self.size {
            self.beyond = true;
            return false;
        }
        for part in piece.chunks(PIECE_MAX_BYTES) {
            if let Err(error) = self.file.append(part) {
                self.disk = Some(error.kind());
                return false;
            }
        }
        self.hash.update(piece);
        self.kept += piece.len() as u64;
        true
    }
}

/// Downloads one file into `files`, and holds it to its size and its hash.
fn stage_file(
    dir: &ModelsDir,
    entry: &Entry,
    file: &CatalogueFile,
    files: &Path,
    downloader: &dyn Downloader,
    stop: &Stop,
) -> Result<(), StageRefused> {
    let cancellation = stop.cancellation;
    let refused = |why: FileRefused| StageRefused::File {
        name: file.name().shown(),
        why,
    };
    let size = file.size();
    // The file may hold its size and no more: the file system holds it to that.
    let mut received = Received {
        file: dir
            .fs()
            .begin_new(&files.join(file.name().as_str()), size)?,
        size,
        kept: 0,
        hash: Sha256::new(),
        beyond: false,
        disk: None,
    };
    let mut from = file.from().clone();
    let mut moves = 0;
    loop {
        // One byte more than the size: a file that is too long is then seen to be,
        // where a downloader that keeps to what it is asked would cut it at its size.
        let request = DownloadRequest::new(from, size + 1)
            .map_err(|_| refused(FileRefused::Download(FaultKind::Malformed)))?;
        // Asked at each piece: a downloader that does not ask is not handed a sink
        // that keeps on taking. And asked when the downloader has answered, whatever
        // it answered: after a cancel no move is followed, and nothing more is
        // asked for. The mark is read at the same places: a removal stops a
        // download as a cancel does, from any process.
        let ended = downloader.download(&request, cancellation, &mut |piece| {
            stop.asked().is_none() && received.take(piece)
        });
        if let Some(stopped) = stop.asked() {
            return Err(stopped);
        }
        match ended {
            Err(fault) if fault.kind() == FaultKind::Cancelled => {
                return Err(StageRefused::Cancelled);
            }
            Err(fault) => return Err(refused(FileRefused::Download(fault.kind()))),
            Ok(Downloaded::Bytes(_)) => break,
            Ok(Downloaded::Moved(_)) if received.disk.is_some() => break,
            Ok(Downloaded::Moved(_)) if received.kept > 0 || received.beyond => {
                // Bytes, and then another address: no downloader answers so.
                return Err(refused(FileRefused::Download(FaultKind::Malformed)));
            }
            Ok(Downloaded::Moved(to)) => {
                moves += 1;
                if moves > MOVES_LIMIT {
                    return Err(refused(FileRefused::TooManyMoves));
                }
                if !entry.may_download_from(to.origin()) {
                    return Err(refused(FileRefused::Elsewhere));
                }
                from = to;
            }
        }
    }
    // What the downloader says it handed over is not asked: what the sink kept is.
    if let Some(kind) = received.disk {
        return Err(StageRefused::Disk(kind));
    }
    if received.beyond {
        return Err(refused(FileRefused::TooLong));
    }
    if received.kept < size {
        return Err(refused(FileRefused::TooShort));
    }
    if Digest::from_bytes(received.hash.finalize().into()) != file.sha256() {
        return Err(refused(FileRefused::WrongHash));
    }
    received.file.sync()?;
    if received.file.finish()? != size {
        return Err(StageRefused::Disk(io::ErrorKind::InvalidData));
    }
    Ok(())
}
