//! `whizbang store import`: a developer's command that brings a canonical artifact file
//! (ADR 0002) into a store, so that a fixture can be an action's input; and, with
//! `--picture`, a PNG or JPEG picture, committed as an image.
//!
//! # A picture
//!
//! A picture file is read as an artifact file is, within the same limit, then decoded by
//! the core's `identify`, within the core's decode limits (ADR 0001): a file over them is
//! refused before the store is touched. Its canonical pixels (upright, sRGB, 8 bits a
//! channel, red, green, blue and alpha) are written again as a PNG that holds nothing
//! else, and that PNG is the file the image holds: no text, EXIF, colour profile or other
//! chunk or segment of the file given is kept, and neither is its name. Its role is a
//! picture; its space is flat and bounded, as no file can be trusted to say otherwise.
//! Its origin is one step, `import-picture`, whose one input is the digest of the file
//! given, so what was imported can be told apart from what was not; the origin is not
//! part of the image's identity, so the same pixels from two files are one image. The
//! PNG is written as `view` writes a crop. Its file digest is that encoder's; its pixel
//! digest is the picture's, whatever the file was.
//!
//! # What is read, and how much
//!
//! The file is a regular file, or a link that leads to one: a link is followed, and what
//! it leads to is what is judged. A directory, a FIFO, a socket and a device are refused
//! by a look at the path, before anything is opened, because opening a FIFO waits for a
//! writer that may never come and a device may never end. The file that was opened is
//! then looked at again through its handle, so what is judged is what is read: a path
//! that was a regular file at the first look and is a directory, a device or a larger
//! file by the time it is opened is refused at the second. One gap is left: a path that
//! is a regular file at the look and is made a FIFO before the open waits inside the
//! open, for a writer, and the second look is never reached. Whoever can replace the
//! file at the path can do that. Closing it takes an open that does not wait
//! (`O_NONBLOCK`), then the look; `docs/carry-over.md` holds it.
//!
//! The file is at most [`MAX_FILE_BYTES`], 65 MiB, which is checked by its size before
//! a byte is read, and again while reading: the reader stops one byte past the limit, so
//! a file that grows while it is read costs no more than the limit and is refused.
//!
//! The limit is the largest file an artifact is expected to have. That is an image: it
//! holds its picture whole, and a picture is held to 64 MiB (ADR 0001's limit on a
//! file). Around the picture an image has a kind, a digest, a space, a role and an
//! origin, which are hundreds of bytes as actions write them; a mebibyte is allowed for
//! them. Every other type is far smaller: a location, a plan, a band profile and a
//! series are a few numbers for each row at most, and a mask's planes are written as
//! runs, 66 bytes for an empty mask of any size and some megabytes for a feathered band
//! across the largest image.
//!
//! The format itself allows more than the limit in two ways, and both are refused
//! here, though the file is canonical:
//!
//! - **An origin has no bound.** Nothing limits how many steps an origin has, or how
//!   long a step's argument is, and every type has an origin. An image with a picture
//!   of 64 MiB and an argument of a mebibyte is over the limit by the few hundred bytes
//!   of the rest of its record; a location with one argument of 70 MiB is over it too.
//! - **A plane costs five bytes for each run.** A mask whose every sample differs from
//!   the one before, at the limit of 50,000,000 pixels, has a file of hundreds of
//!   megabytes. No mask drawn from a shape is like that.
//!
//! `docs/carry-over.md` holds both, with the bound on masks.
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

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use whizbang::wiring::{self, Make};
use whizbang_core::artifact::{self, Artifact, Image, ImageRole, Origin, Step};
use whizbang_core::contract::Recovery;
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Projection, Topology, identify};
use whizbang_core::store::Error;

/// The step a picture's origin names.
const PICTURE_STEP: &str = "import-picture";

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
    read_with(file, &|| {})
}

/// [`read`]. `looked` runs once the path has been looked at and before it is opened: the
/// moment a test needs.
fn read_with(file: &Path, looked: &dyn Fn()) -> Result<Vec<u8>, String> {
    let cannot = |error: std::io::Error| format!("cannot read {}: {error}", file.display());
    // By the path first, which follows a link and opens nothing.
    judge(&std::fs::metadata(file).map_err(cannot)?, file)?;
    looked();
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
/// that a store was created, when this process created it. The store is opened through
/// the one entry point, and recovered by `recovery` before anything is brought in.
pub fn import(
    file: &Path,
    root: &Path,
    create: bool,
    recovery: Recovery,
) -> Result<String, String> {
    let bytes = read(file)?;
    let artifact = Artifact::from_bytes(&bytes, &DecodeLimits::default())
        .map_err(|error| format!("{}: {error}", file.display()))?;
    drop(bytes);
    commit(&artifact, root, create, recovery)
}

/// Reads a PNG or JPEG picture and commits it as an image whose role is a picture,
/// written again as a PNG of its pixels alone; the module's description says how. A
/// picture that is refused creates nothing.
pub fn import_picture(
    file: &Path,
    root: &Path,
    create: bool,
    recovery: Recovery,
) -> Result<String, String> {
    let limits = DecodeLimits::default();
    let bytes = read(file)?;
    let identified =
        identify(&bytes, &limits).map_err(|error| format!("{}: {error}", file.display()))?;
    drop(bytes);
    let image = picture(&identified.pixels, identified.identity.file.0, &limits)
        .map_err(|error| format!("{}: {error}", file.display()))?;
    commit(&Artifact::Image(image), root, create, recovery)
}

/// The image of these pixels: a PNG of them alone, with an origin that names the file
/// they came from by its digest.
fn picture(
    pixels: &CanonicalPixels,
    file: whizbang_core::identity::Digest,
    limits: &DecodeLimits,
) -> Result<Image, String> {
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Default, FilterType::Adaptive)
        .write_image(
            pixels.rgba(),
            pixels.width(),
            pixels.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| format!("cannot write the picture as a PNG: {error}"))?;
    let origin =
        Origin::new(Step::new(PICTURE_STEP, "1").input("file", file)).map_err(|e| e.to_string())?;
    Image::new(
        png,
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        origin,
        limits,
    )
    .map_err(|error| error.to_string())
}

/// Commits an artifact that has been read and checked: the store part of both imports.
fn commit(
    artifact: &Artifact,
    root: &Path,
    create: bool,
    recovery: Recovery,
) -> Result<String, String> {
    let (kind, id) = (artifact.artifact_type(), artifact.id());

    let mut printed = String::new();
    let opening = if create {
        let opening = wiring::opening(root, Make::TheRoot, recovery)
            .map_err(|error| wiring::not_opened(&error))?;
        // The open says whether it made the store: it knows, having held the lock.
        if opening.created() {
            // On the standard output, which nothing else escapes.
            let shown = whizbang::door::printable(&root.display().to_string());
            printed.push_str(&format!("created a store at {shown}\n"));
        }
        opening
    } else {
        wiring::opening(root, Make::Nothing, recovery).map_err(|error| match error {
            // Where a store could be made, say how.
            Error::Missing { .. } | Error::NotAStore { found: None, .. } => {
                format!("{error}; pass --create to have one made there")
            }
            other => wiring::not_opened(&other),
        })?
    };
    // The writer, once the store is recovered. A store that was made here and could
    // not be recovered is still said to have been made.
    let store = opening.writer().map_err(|why| format!("{printed}{why}"))?;
    let generation = store.snapshot().map_err(|error| error.to_string())?;
    if artifact::holds(&generation, kind, id) {
        printed.push_str(&format!("already there: {kind} {id}\n"));
        return Ok(printed);
    }
    let mut publication = store.begin().map_err(|error| error.to_string())?;
    artifact::put(&mut publication, artifact).map_err(|error| error.to_string())?;
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

    /// Reads the file at `path`, which `swap` changes once the path has been looked at.
    fn read_swapped(path: &Path, swap: &dyn Fn()) -> Result<Vec<u8>, String> {
        read_with(path, swap)
    }

    #[test]
    fn what_is_judged_is_the_file_that_was_opened_and_not_only_the_path() {
        use whizbang_testkit::scratch::Scratch;
        let dir = Scratch::new("import-swap");
        let path = dir.path().join("given");
        let small = || std::fs::write(&path, b"a small file").unwrap();

        // Nothing swapped: the file is read.
        small();
        assert_eq!(read_swapped(&path, &|| {}).unwrap(), b"a small file");

        // After the look at the path and before the open, the file becomes a
        // directory. A directory opens without waiting, so the look at what was opened
        // is what refuses it.
        let to_directory = || {
            std::fs::remove_file(&path).unwrap();
            std::fs::create_dir(&path).unwrap();
        };
        let complaint = read_swapped(&path, &to_directory).unwrap_err();
        assert!(complaint.contains("is a directory"), "{complaint}");
        assert!(complaint.contains("a regular file"), "{complaint}");
        std::fs::remove_dir(&path).unwrap();

        // Or it becomes a file over the limit: refused by the size of what was opened,
        // unread. The file is sparse.
        small();
        let to_large = || {
            let file = std::fs::File::create(&path).unwrap();
            file.set_len(MAX_FILE_BYTES + 1).unwrap();
        };
        let complaint = read_swapped(&path, &to_large).unwrap_err();
        let size = format!("is {} bytes, over the limit", MAX_FILE_BYTES + 1);
        assert!(complaint.contains(&size), "{complaint}");
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
