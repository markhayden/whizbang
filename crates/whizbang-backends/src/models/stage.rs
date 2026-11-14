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
use std::path::Path;

use sha2::{Digest as _, Sha256};
use whizbang_core::capability::FaultKind;
use whizbang_core::contract::{Cancellation, Secrets};
use whizbang_core::egress::{DownloadRequest, Downloaded, Downloader};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::{LockFile, NewFile, PIECE_MAX_BYTES};

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
    /// One file was not staged. `name` is what [`super::FileName::shown`] shows of it.
    File { name: String, why: FileRefused },
    /// The file system refused. The kind of failure, and no path.
    Disk(io::ErrorKind),
}

impl std::fmt::Display for StageRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StageRefused::HoldsSecret => f.write_str("a name holds a secret, and is not shown"),
            StageRefused::Busy => f.write_str("the model is being installed or removed by another"),
            StageRefused::Cancelled => f.write_str("the download was cancelled"),
            StageRefused::File { name, why } => write!(f, "the file `{name}` {why}"),
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
/// - What an earlier staging left is removed, whatever it is, and not looked at.
/// - Each file is asked for at its address, and at an address it moved to only when
///   the entry names that address's origin. One byte more than its size is asked
///   for and no more than its size is kept: a byte beyond it refuses the file.
/// - On any refusal the staging folder is removed.
pub fn stage(
    dir: &ModelsDir,
    entry: &Entry,
    downloader: &dyn Downloader,
    cancellation: &Cancellation,
    secrets: &Secrets,
) -> Result<Staged, StageRefused> {
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
    let fs = dir.fs();
    let model = dir.model(entry.name());
    dir.ensure_dir(&dir.models())?;
    dir.ensure_dir(&model)?;
    let Some(lock) = fs.hold_lock_file(&model.join("LOCK"), true)? else {
        return Err(StageRefused::Busy);
    };
    let staging = model.join("staging");
    let incoming = staging.join("incoming");
    let files = incoming.join("files");
    let staged = (|| {
        dir.remove(&incoming)?;
        dir.ensure_dir(&staging)?;
        fs.create_dir(&incoming)?;
        fs.sync_dir(&staging)?;
        fs.create_dir(&files)?;
        fs.sync_dir(&incoming)?;
        let mut staged = Vec::new();
        for file in entry.files() {
            stage_file(dir, entry, file, &files, downloader, cancellation)?;
            // Made from what the catalogue states, which the file was just held to.
            let installed = InstalledFile::new(file.name().clone(), file.size(), file.sha256());
            staged.push(installed.map_err(|_| StageRefused::Disk(io::ErrorKind::InvalidData))?);
        }
        fs.sync_dir(&files)?;
        Ok(staged)
    })();
    match staged {
        Ok(files) => Ok(Staged {
            model: entry.name().clone(),
            files,
            _lock: lock,
        }),
        Err(refused) => {
            // Under the lock still. What cannot be removed now is removed by the next
            // to stage, which trusts nothing it finds here.
            let _ = dir.remove(&incoming);
            let _ = fs.sync_dir(&staging);
            Err(refused)
        }
    }
}

/// What the sink of one file's download has seen.
struct Received {
    file: Box<dyn NewFile>,
    kept: u64,
    hash: Sha256,
    /// A byte came beyond the size.
    beyond: bool,
    /// The file system refused an append.
    disk: Option<io::ErrorKind>,
}

impl Received {
    /// Takes one piece, and says whether to go on. The file was begun with its size
    /// as the most it may hold, so a piece that would take it past its size is
    /// refused by the file system before any of it is written: the file is too long,
    /// and nothing more is wanted of it. The handle is broken by that, as by any
    /// failure, and what it wrote is removed with the staging folder.
    fn take(&mut self, piece: &[u8]) -> bool {
        if self.beyond || self.disk.is_some() {
            return false;
        }
        for part in piece.chunks(PIECE_MAX_BYTES) {
            match self.file.append(part) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::FileTooLarge => {
                    self.beyond = true;
                    return false;
                }
                Err(error) => {
                    self.disk = Some(error.kind());
                    return false;
                }
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
    cancellation: &Cancellation,
) -> Result<(), StageRefused> {
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
        let ended = downloader.download(&request, cancellation, &mut |piece| received.take(piece));
        match ended {
            Err(fault) if fault.kind() == FaultKind::Cancelled => {
                return Err(StageRefused::Cancelled);
            }
            Err(fault) => return Err(refused(FileRefused::Download(fault.kind()))),
            Ok(Downloaded::Bytes(_)) => break,
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
