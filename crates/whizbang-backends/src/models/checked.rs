//! The record of an installation's last full check (plan 2A, task R1; ADR 0017, "The
//! cached check"): what [`super::verify_checked`] trusts in place of reading every file
//! again, while the files are unchanged.
//!
//! It is one file in the model's folder, `checks/<64 hex digits>`, named by the
//! identity of the installation it is of, and never inside the installation's own
//! folder, which holds its record and its files and nothing else. Its bytes are
//! exactly these lines, each ended by one line feed:
//!
//! ```text
//! check/2
//! sha256:<the installation's identity>
//! <size> <written s> <written ns> <changed s> <changed ns> <inode> sha256:<hash> <name>
//! ```
//!
//! with one line of the last form for each file, in the record's order: the times are
//! of the file's last write and of its inode's last change ([`Stamp`]).
//!
//! A record is held by its bytes alone: the bytes a record of this installation and
//! these stamps would have, and nothing else, so no record that is damaged, cut, of
//! another identity, another hash or another stamp is trusted, and none is parsed.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use whizbang_core::store::fs::{Fs, Stamp};

use super::directory::Read;
use super::{InstallationRecord, InstalledFile, ModelsDir};

/// The first line of a check record.
pub const CHECK_FORMAT: &str = "check/2";

/// Told apart, the names a process writes a record under before it is renamed.
static WRITING: AtomicU64 = AtomicU64::new(0);

/// The stamp of each file the record lists, in its order; none when one cannot be
/// taken (a file system that gives none, a file that is not there or is no regular
/// file), and then no record is trusted or written.
pub(super) fn stamps(fs: &dyn Fs, files: &Path, listed: &[InstalledFile]) -> Option<Vec<Stamp>> {
    listed
        .iter()
        .map(|file| fs.stamp(&files.join(file.name().as_str())).ok())
        .collect()
}

/// The bytes of the record of this installation's files with these stamps.
fn bytes_of(record: &InstallationRecord, stamps: &[Stamp]) -> Vec<u8> {
    let mut text = format!("{CHECK_FORMAT}\n{}\n", record.identity());
    for (file, stamp) in record.files().iter().zip(stamps) {
        text.push_str(&format!(
            "{} {} {} {} {} {} {} {}\n",
            stamp.size,
            stamp.modified_s,
            stamp.modified_ns,
            stamp.changed_s,
            stamp.changed_ns,
            stamp.inode,
            file.sha256(),
            file.name().as_str()
        ));
    }
    text.into_bytes()
}

/// Whether `checks/` may be read and written: it is no link. A file system that
/// cannot say is not trusted.
fn usable(fs: &dyn Fs, checks: &Path) -> bool {
    matches!(fs.is_link(checks), Ok(false))
}

fn at(checks: &Path, record: &InstallationRecord) -> PathBuf {
    checks.join(record.identity().hex())
}

/// Whether the check record of this installation is there, as a regular file under
/// its own name, and says that every file the record lists had, at a check that
/// passed, the size and the hash the record states and the stamp it has now.
pub(super) fn holds(
    dir: &ModelsDir,
    checks: &Path,
    record: &InstallationRecord,
    stamps: &[Stamp],
) -> bool {
    if stamps.len() != record.files().len() || !usable(dir.fs(), checks) {
        return false;
    }
    // A stamp whose size is not the record's is a file that is not whole; no record
    // of a check that passed could say otherwise.
    let sized = record
        .files()
        .iter()
        .zip(stamps)
        .all(|(file, stamp)| file.size() == stamp.size);
    if !sized {
        return false;
    }
    let expected = bytes_of(record, stamps);
    match dir.read(&at(checks, record), expected.len() as u64) {
        Ok(Read::Bytes(bytes)) => bytes == expected,
        _ => false,
    }
}

/// Removes the check record of this installation, whose files a full check asked for
/// by hand found not whole, so that the next load reads them again whatever their
/// stamps say. Nothing is removed through a `checks/` that is a link; a failure leaves
/// the record, which then holds only while no file's stamp changes.
pub(super) fn forget(dir: &ModelsDir, checks: &Path, record: &InstallationRecord) {
    if usable(dir.fs(), checks) {
        let _ = dir.remove(&at(checks, record));
    }
}

/// Writes the check record of this installation, whose files a full check found
/// whole, with the stamps they had before it read them: under a name of this
/// writing's own, readable by its owner only, then renamed into place and the
/// rename made durable. Nothing is written through a `checks/` that is a link, and a
/// failure leaves no record of this writing's: the check passed all the same.
pub(super) fn write(dir: &ModelsDir, checks: &Path, record: &InstallationRecord, stamps: &[Stamp]) {
    if stamps.len() != record.files().len() {
        return;
    }
    let fs = dir.fs();
    if dir.ensure_dir(checks).is_err() || !usable(fs, checks) {
        return;
    }
    let writing = WRITING.fetch_add(1, Ordering::Relaxed);
    let temporary = checks.join(format!(
        "{}.{}.{writing}.new",
        record.identity().hex(),
        std::process::id()
    ));
    let written = (|| -> io::Result<()> {
        fs.write_new(&temporary, &bytes_of(record, stamps))?;
        fs.rename(&temporary, &at(checks, record))?;
        fs.sync_dir(checks)
    })();
    if written.is_err() {
        let _ = dir.remove(&temporary);
    }
}
