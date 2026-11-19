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
//! picture; its space is flat and bounded unless the caller or its file says it is a
//! complete equirectangular panorama.
//! Its origin is one step, `import-picture`, whose one input is the digest of the file
//! given, so what was imported can be told apart from what was not; the origin is not
//! part of the image's identity, so the same pixels from two files are one image. The
//! PNG is written as `view` writes a crop. Its file digest is that encoder's; its pixel
//! digest is the picture's, whatever the file was.
//!
//! # What is read, and how much
//!
//! A picture (`--picture`, and a job's pictures) is a caller's, and is read by
//! [`read_picture`]: from the file its path names, through no link, by an open that does
//! not wait, and only when what was opened is the file that was looked at. Its projection
//! is the caller's word (`--projection`), else the word of its file's XMP
//! (`GPano:ProjectionType`), else flat; nothing of the metadata is kept. Each import of a
//! picture leaves a name of its own, `imports/picture/` and its file's SHA-256, which
//! holds the image's identity: where it came from, found without reading the image.
//!
//! An artifact file is a regular file, or a link that leads to one: a link is followed, and what
//! it leads to is what is judged. A directory, a FIFO, a socket and a device are refused
//! by a look at the path, before anything is opened, because opening a FIFO waits for a
//! writer that may never come and a device may never end. The file that was opened is
//! then looked at again through its handle, so what is judged is what is read: a path
//! that was a regular file at the first look and is a directory, a device or a larger
//! file by the time it is opened is refused at the second. The open does not wait
//! (`O_NONBLOCK`), so a path made a FIFO between the look and the open is opened at once
//! and refused at the second look, never waited on.
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

use crate::wiring::{self, Make};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use whizbang_core::artifact::{self, Artifact, ArtifactId, Image, ImageRole, Origin, Step};
use whizbang_core::contract::Recovery;
use whizbang_core::identity::{
    CanonicalPixels, DecodeLimits, Digest, Projection, Topology, identify,
};
use whizbang_core::store::Error;

/// The step a picture's origin names.
const PICTURE_STEP: &str = "import-picture";

/// The names that say where an imported picture came from: this, then the hex of the
/// SHA-256 of the file it was brought in from. Each holds the image's identity.
pub const IMPORTED: &str = "imports/picture/";

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
    // An open that does not wait, so a FIFO put at the path after the look is opened at
    // once and refused by the second look (plan m3, task O5).
    let opened = crate::sign_in::open_without_waiting(file).map_err(cannot)?;
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

/// The bytes of a picture a caller hands in, by a path (plan m3, task O5): the file the
/// path names and nothing else. The path is looked at without following a link, and a
/// link there is refused, as are a directory, a FIFO, a socket and a device, and a file
/// over the limit, all before anything is opened. The open does not wait (`O_NONBLOCK`),
/// so a FIFO put at the path after the look is opened at once; and what was opened must
/// be the file looked at, the same device and inode, and a regular file within the limit,
/// or nothing is read. Reading stops one byte past the limit. The folders the path goes
/// through are the caller's to name, links among them too.
pub fn read_picture(file: &Path) -> Result<Vec<u8>, String> {
    read_picture_with(file, &|| {})
}

/// [`read_picture`]. `looked` runs between the look and the open.
fn read_picture_with(file: &Path, looked: &dyn Fn()) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::MetadataExt;
    let cannot = |error: std::io::Error| format!("cannot read {}: {error}", file.display());
    let found = std::fs::symlink_metadata(file).map_err(cannot)?;
    if found.file_type().is_symlink() {
        return Err(format!(
            "{} is a link: a picture is read from the file its path names, through no link",
            file.display()
        ));
    }
    judge(&found, file)?;
    looked();
    let opened = crate::sign_in::open_without_waiting(file).map_err(cannot)?;
    let now = opened.metadata().map_err(cannot)?;
    if (now.dev(), now.ino()) != (found.dev(), found.ino()) {
        return Err(format!(
            "{} changed while it was opened, and is not read",
            file.display()
        ));
    }
    let bytes = judge(&now, file)?;
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
    commit(&artifact, root, create, recovery, None)
}

/// Reads a PNG or JPEG picture and commits it as an image whose role is a picture,
/// written again as a PNG of its pixels alone; the module's description says how. A
/// picture that is refused creates nothing. A file whose metadata says it is an
/// equirectangular panorama is one, and any other is flat, with a word printed when it is
/// two to one.
pub fn import_picture(
    file: &Path,
    root: &Path,
    create: bool,
    recovery: Recovery,
) -> Result<String, String> {
    import_picture_as(file, root, create, recovery, None)
}

/// [`import_picture`], with the caller's word of the picture's projection.
pub fn import_picture_as(
    file: &Path,
    root: &Path,
    create: bool,
    recovery: Recovery,
    projection: Option<Projection>,
) -> Result<String, String> {
    let limits = DecodeLimits::default();
    let bytes = read_picture(file)?;
    let identified =
        identify(&bytes, &limits).map_err(|error| format!("{}: {error}", file.display()))?;
    let (width, height) = (identified.pixels.width(), identified.pixels.height());
    let mut said = String::new();
    let projection = match projection {
        Some(projection) => projection,
        None if says_equirectangular(&bytes) => Projection::Equirectangular,
        None => {
            if width == 2 * height {
                said = "the picture is two to one and says no projection: it is taken as flat; \
                        pass --projection equirectangular if it is a panorama\n"
                    .to_owned();
            }
            Projection::Flat
        }
    };
    drop(bytes);
    let image = picture(
        &identified.pixels,
        identified.identity.file.0,
        &limits,
        projection,
    )
    .map_err(|error| format!("{}: {error}", file.display()))?;
    let printed = commit(
        &Artifact::Image(image),
        root,
        create,
        recovery,
        Some(identified.identity.file.0),
    )?;
    Ok(format!("{said}{printed}"))
}

/// True when a file's bytes carry XMP's word that the picture is an equirectangular
/// panorama (`GPano:ProjectionType`, as an attribute or an element): squint's plain scan
/// (`crates/squint-core/src/image.rs`, `has_gpano_equirectangular`, at the commit
/// `docs/sources.toml` pins). It decides the projection only; nothing of the metadata is
/// kept.
pub fn says_equirectangular(bytes: &[u8]) -> bool {
    const KEY: &[u8] = b"GPano:ProjectionType";
    const VALUE: &[u8] = b"equirectangular";
    let find =
        |haystack: &[u8], needle: &[u8]| haystack.windows(needle.len()).position(|w| w == needle);
    let mut from = 0;
    while let Some(at) = find(&bytes[from..], KEY) {
        let start = from + at + KEY.len();
        let end = (start + 64).min(bytes.len());
        if find(&bytes[start..end], VALUE).is_some() {
            return true;
        }
        from = start;
    }
    false
}

/// Brings in the picture of a job's image (plan 2C, task A1, item 4), as `--picture`
/// does, and gives its identity: from `file`, which, when the job pins its SHA-256 (as
/// `sha256`, with or without `sha256:` before it), is refused before the store is
/// touched if its digest is another. The store must be there.
pub fn job_picture(
    file: &Path,
    sha256: Option<&str>,
    root: &Path,
    recovery: Recovery,
) -> Result<ArtifactId, String> {
    let bytes = read_picture(file)?;
    picture_of_bytes(&bytes, &file.display().to_string(), sha256, root, recovery)
}

/// The same, of a picture's bytes already read, at most [`MAX_FILE_BYTES`], which `shown`
/// names in a complaint: what `whizbang eval sets fetch` brings in, by the SHA-256 the
/// set's job pins (plan 2C, task E1, item 3). Bytes whose digest is another are refused
/// before the store is touched.
pub fn picture_of_bytes(
    bytes: &[u8],
    shown: &str,
    sha256: Option<&str>,
    root: &Path,
    recovery: Recovery,
) -> Result<ArtifactId, String> {
    let limits = DecodeLimits::default();
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!("{shown}: the picture is over {}", limit()));
    }
    if let Some(pinned) = sha256 {
        let pinned = pinned.strip_prefix("sha256:").unwrap_or(pinned);
        if !Digest::of_bytes(bytes).hex().eq_ignore_ascii_case(pinned) {
            return Err(format!(
                "{shown}: the file's SHA-256 is not the one the job pins"
            ));
        }
    }
    let identified = identify(bytes, &limits).map_err(|error| format!("{shown}: {error}"))?;
    let file = identified.identity.file.0;
    let image = picture(&identified.pixels, file, &limits, Projection::Flat)
        .map_err(|error| format!("{shown}: {error}"))?;
    let id = image.id();
    commit(&Artifact::Image(image), root, false, recovery, Some(file))?;
    Ok(id)
}

/// The images a store holds that were brought in as pictures, each by the SHA-256 of the
/// file it was brought in from: how a set's pictures, pinned by their files' digests, are
/// found once fetched. Each import leaves a name of its own for that (`imports/picture/`
/// and the file's digest, which holds the image's identity; plan m3, task O5), so no image
/// is read here. A picture a store took before those names were left is not found, and is
/// fetched again by `whizbang eval sets fetch`, whose import then leaves the name.
pub fn pictures_by_file(
    root: &Path,
) -> Result<std::collections::BTreeMap<Digest, ArtifactId>, String> {
    use whizbang_core::store::{Reader, fs::RealFs};
    let reader = Reader::open(root, std::sync::Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let mut found = std::collections::BTreeMap::new();
    let names = (generation.entries().range(IMPORTED.to_owned()..))
        .take_while(|(name, _)| name.starts_with(IMPORTED));
    for (name, entry) in names {
        let Ok(file) = format!("sha256:{}", &name[IMPORTED.len()..]).parse::<Digest>() else {
            continue;
        };
        let bytes = reader.read(entry).map_err(|e| e.to_string())?;
        let id = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| text.parse::<Digest>().ok())
            .map(ArtifactId);
        if let Some(id) = id
            && artifact::holds(&generation, artifact::ArtifactType::Image, id)
        {
            found.insert(file, id);
        }
    }
    Ok(found)
}

/// The image of these pixels: a PNG of them alone, with an origin that names the file
/// they came from by its digest.
fn picture(
    pixels: &CanonicalPixels,
    file: whizbang_core::identity::Digest,
    limits: &DecodeLimits,
    projection: Projection,
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
    // A panorama the caller or its file names is a complete one, which wraps.
    let topology = match projection {
        Projection::Equirectangular => Topology::Wrap,
        _ => Topology::Bounded,
    };
    Image::new(
        png,
        projection,
        topology,
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
    from_file: Option<Digest>,
) -> Result<String, String> {
    let (kind, id) = (artifact.artifact_type(), artifact.id());

    let mut printed = String::new();
    let opening = if create {
        let opening = wiring::opening(root, Make::TheRoot, recovery)
            .map_err(|error| wiring::not_opened(&error))?;
        // The open says whether it made the store: it knows, having held the lock.
        if opening.created() {
            // On the standard output, which nothing else escapes.
            let shown = crate::door::printable(&root.display().to_string());
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
    // A picture's name of where it came from: the file's digest, holding the image's
    // identity. One file decodes to one picture, so a name is never given another.
    let named = from_file.map(|file| format!("{IMPORTED}{}", file.hex()));
    let unnamed = named.as_ref().filter(|name| generation.get(name).is_none());
    let held = artifact::holds(&generation, kind, id);
    if held && unnamed.is_none() {
        printed.push_str(&format!("already there: {kind} {id}\n"));
        return Ok(printed);
    }
    let mut publication = store.begin().map_err(|error| error.to_string())?;
    if !held {
        artifact::put(&mut publication, artifact).map_err(|error| error.to_string())?;
    }
    if let Some(name) = unnamed {
        publication
            .put(name, id.to_string().as_bytes())
            .map_err(|error| error.to_string())?;
    }
    publication.publish().map_err(|error| error.to_string())?;
    if held {
        printed.push_str(&format!("already there: {kind} {id}\n"));
        return Ok(printed);
    }
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

        // Or it becomes a FIFO no one writes to: the open does not wait, and the look at
        // what was opened refuses it.
        small();
        let to_fifo = || {
            std::fs::remove_file(&path).unwrap();
            let made = std::process::Command::new("mkfifo").arg(&path).status();
            assert!(made.is_ok_and(|status| status.success()), "mkfifo");
        };
        let complaint = read_swapped(&path, &to_fifo).unwrap_err();
        assert!(complaint.contains("is a FIFO"), "{complaint}");
    }

    #[test]
    fn a_picture_swapped_after_the_look_for_a_link_or_a_fifo_is_not_read_and_not_waited_for() {
        use whizbang_testkit::scratch::Scratch;
        let dir = Scratch::new("import-picture-swap");
        let path = dir.path().join("given");
        let other = dir.path().join("other");
        std::fs::write(&other, b"elsewhere").unwrap();
        let small = || std::fs::write(&path, b"a picture").unwrap();
        small();
        assert_eq!(read_picture_with(&path, &|| {}).unwrap(), b"a picture");
        // Made a link to another file after the look: what opens is not what was looked
        // at, and nothing is read.
        let to_link = || {
            std::fs::remove_file(&path).unwrap();
            std::os::unix::fs::symlink(&other, &path).unwrap();
        };
        let complaint = read_picture_with(&path, &to_link).unwrap_err();
        assert!(
            complaint.contains("changed while it was opened"),
            "{complaint}"
        );
        std::fs::remove_file(&path).unwrap();
        // Made a FIFO no one writes to: the open does not wait for a writer, and the
        // FIFO is refused.
        small();
        let to_fifo = || {
            std::fs::remove_file(&path).unwrap();
            let made = std::process::Command::new("mkfifo").arg(&path).status();
            assert!(made.is_ok_and(|status| status.success()), "mkfifo");
        };
        let complaint = read_picture_with(&path, &to_fifo).unwrap_err();
        assert!(
            complaint.contains("changed while it was opened"),
            "{complaint}"
        );
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
