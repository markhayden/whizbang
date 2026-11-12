//! `whizbang store import`: a developer's command that brings a canonical artifact file
//! (ADR 0002) into a store, so that a fixture can be an action's input.
//!
//! # What is read, and how much
//!
//! The file is a regular file, or a link that leads to one: a link is followed, and what
//! it leads to is what is judged. A directory, a FIFO, a socket and a device are refused
//! by a look at the path, before anything is opened, because opening a FIFO waits for a
//! writer that may never come and a device may never end. The file that was opened is
//! then looked at again through its handle, so what is judged is what is read. One gap
//! is left: a path that is a regular file at the look and is made a FIFO before the open
//! waits at the open. Nothing but the one who gave the path can do that.
//!
//! The file is at most [`MAX_FILE_BYTES`], 65 MiB, which is checked by its size before
//! a byte is read, and again while reading: the reader stops one byte past the limit, so
//! a file that grows while it is read costs no more than the limit and is refused.
//!
//! The limit is the largest file an artifact is expected to have. That is an image: it
//! holds its picture whole, and a picture is held to 64 MiB (ADR 0001's limit on a
//! file). Around the picture an image has a kind, a digest, a space, a role and an
//! origin, which are hundreds of bytes; a mebibyte is allowed for them. Every other
//! type is far smaller: a location, a plan, a band profile and a series are a few
//! numbers for each row at most, and a mask's planes are written as runs, 66 bytes for
//! an empty mask of any size and some megabytes for a feathered band across the largest
//! image.
//!
//! The format itself allows more than the limit in one case, and it is refused here: a
//! plane costs five bytes for each run, so a mask whose every sample differs from the
//! one before, at the limit of 50,000,000 pixels, has a file of hundreds of megabytes.
//! No mask drawn from a shape is like that. `docs/carry-over.md` holds it, with the
//! bound on masks.
//!
//! # What `--create` makes
//!
//! A store, where there is none and one may be made: at a path with nothing at it, in
//! an empty directory, and in a directory that holds only what an unfinished creation
//! of a store left (its lock file and empty folders), which is completed: such a
//! directory was meant to be a store and nothing was ever published in it. Only the last
//! part of the path is made. A path whose parent directory is missing is refused and
//! nothing is made, as `mkdir` without `-p` refuses: a mistyped part of a path should
//! not become a store somewhere unintended. Whether a store was made is what the open
//! itself says ([`Store::created`]), decided under the writer lock, so the line
//! "created a store at" is printed by the process that made it and by no other.

use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use whizbang_core::artifact::{self, Artifact};
use whizbang_core::identity::DecodeLimits;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Error, Store};

/// The most bytes the picture an image artifact holds may have: ADR 0001's limit on a
/// file, which is the default of [`DecodeLimits`].
const MAX_PICTURE_BYTES: u64 = 64 * 1024 * 1024;

/// What is allowed for the rest of an image's file: its kind, its digest, its space, its
/// role and its origin.
const RECORD_ALLOWANCE: u64 = 1024 * 1024;

/// The most bytes a file to import may have: 65 MiB, the largest picture an image
/// artifact holds and a mebibyte for the record around it. The command's help says the
/// same number.
pub const MAX_FILE_BYTES: u64 = MAX_PICTURE_BYTES + RECORD_ALLOWANCE;

/// The limit, in words for a refusal.
fn limit() -> String {
    format!(
        "the limit of {} MiB ({MAX_FILE_BYTES} bytes) on a file to import",
        MAX_FILE_BYTES / (1024 * 1024)
    )
}

/// What a path leads to when it is not a regular file, in words; `None` for a regular
/// file.
fn other_than_a_file(found: &std::fs::Metadata) -> Option<&'static str> {
    let kind = found.file_type();
    if kind.is_file() {
        return None;
    }
    if kind.is_dir() {
        return Some("a directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if kind.is_fifo() {
            return Some("a FIFO");
        }
        if kind.is_char_device() || kind.is_block_device() {
            return Some("a device");
        }
        if kind.is_socket() {
            return Some("a socket");
        }
    }
    Some("no regular file")
}

/// Refuses what is not a regular file, and a file over the limit; gives the file's size.
fn judge(found: &std::fs::Metadata, file: &Path) -> Result<u64, String> {
    if let Some(what) = other_than_a_file(found) {
        return Err(format!(
            "{} is {what}: import reads a regular file, or a link to one",
            file.display()
        ));
    }
    let bytes = found.len();
    if bytes > MAX_FILE_BYTES {
        return Err(format!(
            "{} is {bytes} bytes, over {}",
            file.display(),
            limit()
        ));
    }
    Ok(bytes)
}

/// Reads to the end, or to one byte past `most`: `None` when there was more than `most`.
/// `expected` is the size to set aside at the start, and is not trusted beyond `most`.
fn read_within(from: impl Read, most: u64, expected: u64) -> std::io::Result<Option<Vec<u8>>> {
    let room = usize::try_from(expected.min(most)).unwrap_or(0);
    let mut bytes = Vec::with_capacity(room);
    from.take(most.saturating_add(1)).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= most).then_some(bytes))
}

/// The bytes of the file to import, within the limit. The module's description says what
/// is refused and when.
fn read(file: &Path) -> Result<Vec<u8>, String> {
    let cannot = |error: std::io::Error| format!("cannot read {}: {error}", file.display());
    // By the path first, which follows a link and opens nothing.
    judge(&std::fs::metadata(file).map_err(cannot)?, file)?;
    let opened = std::fs::File::open(file).map_err(cannot)?;
    // Then the file that was opened, which is the one that is read.
    let bytes = judge(&opened.metadata().map_err(cannot)?, file)?;
    read_within(opened, MAX_FILE_BYTES, bytes)
        .map_err(cannot)?
        .ok_or_else(|| {
            format!(
                "{} grew while it was read, past {}",
                file.display(),
                limit()
            )
        })
}

/// Reads and checks the file before the store is touched: a file that is refused creates
/// nothing. The store must be there, like the store of every store command, unless
/// `create` says to make one where there is none; what is printed then begins by saying
/// that a store was created, when this process created it.
pub fn import(file: &Path, root: &Path, create: bool) -> Result<String, String> {
    let bytes = read(file)?;
    let artifact = Artifact::from_bytes(&bytes, &DecodeLimits::default())
        .map_err(|error| format!("{}: {error}", file.display()))?;
    drop(bytes);
    let (kind, id) = (artifact.artifact_type(), artifact.id());

    let mut printed = String::new();
    let store = if create {
        let store = Store::open_in_existing_parent(root, Arc::new(RealFs))
            .map_err(|error| error.to_string())?;
        // The open says whether it made the store: it knows, having held the lock.
        if store.created() {
            printed.push_str(&format!("created a store at {}\n", root.display()));
        }
        store
    } else {
        Store::open_existing(root, Arc::new(RealFs)).map_err(|error| match error {
            // Where a store could be made, say how.
            Error::Missing { .. } | Error::NotAStore { found: None, .. } => {
                format!("{error}; pass --create to have one made there")
            }
            other => other.to_string(),
        })?
    };
    let generation = store.snapshot().map_err(|error| error.to_string())?;
    if artifact::holds(&generation, kind, id) {
        printed.push_str(&format!("already there: {kind} {id}\n"));
        return Ok(printed);
    }
    let mut publication = store.begin().map_err(|error| error.to_string())?;
    artifact::put(&mut publication, &artifact).map_err(|error| error.to_string())?;
    publication.publish().map_err(|error| error.to_string())?;
    printed.push_str(&format!("imported {kind} {id}\n"));
    Ok(printed)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_is_the_largest_picture_and_a_mebibyte() {
        assert_eq!(MAX_PICTURE_BYTES, DecodeLimits::default().max_bytes);
        assert_eq!(MAX_FILE_BYTES, 65 * 1024 * 1024);
        assert!(limit().starts_with("the limit of 65 MiB (68157440 bytes)"));
    }

    #[test]
    fn reading_stops_one_byte_past_the_limit_however_much_there_is() {
        // What is read has no end, as a file that grows while it is read has none.
        assert_eq!(read_within(std::io::repeat(7), 1000, 10).unwrap(), None);
        // What was set aside is the size the file claimed, and never over the limit.
        let endless = read_within(std::io::repeat(7), 1000, u64::MAX);
        assert_eq!(endless.unwrap(), None);
        // Up to the limit, everything; the size claimed is only where to start.
        let whole = vec![7u8; 1000];
        for claimed in [0, 1000, 5000] {
            let read = read_within(whole.as_slice(), 1000, claimed).unwrap();
            assert_eq!(read.as_deref(), Some(whole.as_slice()), "{claimed}");
        }
        assert_eq!(read_within(&whole[..], 999, 1000).unwrap(), None);
        assert_eq!(read_within(&b""[..], 0, 0).unwrap(), Some(Vec::new()));
    }
}
