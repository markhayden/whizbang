//! `whizbang store export`: an image in a store written out as a PNG file anyone can open
//! (plan 2A, task R2); a mask as a grey PNG a person can edit and bring back with `store
//! import --mask`; and any artifact as its canonical file, which `store import` brings
//! into another store with the same identity (plan m3, task F1, item 7).
//!
//! The image is read by a reader of its own, so the command works while another process
//! writes, and decoded by the core within its limits (ADR 0001), whatever form its file
//! is held in: a PNG, a JPEG. Its canonical pixels (upright, sRGB, 8 bits a channel, red,
//! green, blue and alpha) are written as a PNG of those four channels and nothing else:
//! no text, no time, no colour profile, no EXIF, only the chunks a PNG needs. The PNG is
//! written as `view` writes a crop.
//!
//! # Where it writes
//!
//! - **Not over a file,** unless `--overwrite` is given; then the new file is written
//!   beside it under a name of its own and renamed over it, so the file is either the old
//!   one or the new one whole.
//! - **Not through a link:** a path that is a symbolic link is refused, with or without
//!   `--overwrite`. A new file is created with `O_EXCL`, which also refuses a link that
//!   appears after the look.
//! - **Not inside the store:** the folder the file goes in, with its links followed, is
//!   refused when it is the store's directory or inside it. A store's files are its own.
//!   The file is then written in that folder as it was checked (its canonical path) under
//!   the name given, never by the path as given, so a link among the folders swapped
//!   after the check cannot move the file into the store.
//! - **Not into a folder that is missing:** nothing is made but the file.
//! - **Named by the caller alone:** the file is the path the command was given, and no
//!   part of its name comes from the store: not an artifact's identity, not a run's id,
//!   not a request id (which may be `.` or `..`). A path that names no file (`.`, `..`,
//!   a folder) is refused.
//!
//! # A mask as a grey picture
//!
//! `--mask` writes an 8-bit grey PNG of the mask's size, one sample a pixel: 255 in the
//! core, the blend's sample outside it, 0 elsewhere and under protect. A protect plane,
//! a band's profile and prescriptions are not in it; the canonical file keeps them.
//!
//! # An artifact's canonical file
//!
//! An identity alone, with `--to`, writes the canonical file of the artifact of that
//! identity (ADR 0002), of whatever type the store holds it as, read and checked within
//! the core's limits before a byte is written.

use std::io::Write;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Digest};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;

/// The pixels as a PNG, by the core's one encoder (`whizbang_core::identity::encode`).
pub fn png(pixels: &CanonicalPixels) -> Result<Vec<u8>, String> {
    whizbang_core::identity::encode::png(pixels)
        .map_err(|why| format!("cannot write the image as a PNG: {why}"))
}

/// Where the file goes, refused as the module's description says: the path to write,
/// which is the folder as it was checked, its links followed, joined with the file's
/// name, so that what is written is in the folder that was held out of the store; and
/// whether a file is there to be replaced.
fn destination(out: &Path, root: &Path, overwrite: bool) -> Result<(PathBuf, bool), String> {
    let shown = out.display();
    let Some(name) = out.file_name() else {
        return Err(format!("{shown} names no file"));
    };
    let parent = match out.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let folder = parent
        .canonicalize()
        .map_err(|error| format!("cannot use the folder of {shown}: {error}"))?;
    let store = root
        .canonicalize()
        .map_err(|error| format!("cannot find the store: {error}"))?;
    if folder.starts_with(&store) {
        return Err(format!(
            "{shown} is inside the store: a store's files are its own; write it elsewhere"
        ));
    }
    let target = folder.join(name);
    match std::fs::symlink_metadata(&target) {
        Ok(found) if found.file_type().is_symlink() => Err(format!(
            "{shown} is a link: export writes no file through a link"
        )),
        Ok(found) if !found.is_file() => Err(format!("{shown} is there and is no file")),
        Ok(_) if !overwrite => Err(format!(
            "{shown} is there already: pass --overwrite to replace it"
        )),
        Ok(_) => Ok((target, true)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((target, false)),
        Err(error) => Err(format!("cannot look at {shown}: {error}")),
    }
}

/// Writes `bytes` to a new file at `path`, which must not be there, and makes it
/// durable; a file it began is removed when it fails.
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    if written.is_err() {
        let _ = std::fs::remove_file(path);
    }
    written
}

/// An identity the command was given, or the complaint naming its flag.
fn identity(given: &str, flag: &str) -> Result<Digest, String> {
    given.parse::<Digest>().map_err(|_| {
        format!(
            "{flag} is no artifact identity: an identity is sha256: and 64 lower-case hex \
             digits, as `whizbang store import` prints it"
        )
    })
}

/// Exports the image `image` from the store at `root` to `out`: what to print, or the
/// complaint.
pub fn export(root: &Path, image: &str, out: &Path, overwrite: bool) -> Result<String, String> {
    let id = identity(image, "--image")?;
    let (target, replace) = destination(out, root, overwrite)?;
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let limits = DecodeLimits::default();
    let read = artifact::get(&reader, ArtifactType::Image, ArtifactId(id), &limits);
    let Artifact::Image(held) = read.map_err(|e| e.to_string())? else {
        return Err("the store holds no image of that identity".to_owned());
    };
    let pixels = held.pixels(&limits).map_err(|e| e.to_string())?;
    let bytes = png(&pixels)?;
    written(&target, replace, &bytes)?;
    Ok(format!(
        "exported image {} ({} by {}) to {}\n",
        held.id(),
        pixels.width(),
        pixels.height(),
        whizbang::door::printable(&out.display().to_string())
    ))
}

/// Exports the mask `mask` from the store at `root` to `out` as a grey PNG: what to
/// print, or the complaint.
pub fn export_mask(root: &Path, mask: &str, out: &Path, overwrite: bool) -> Result<String, String> {
    let id = identity(mask, "--mask")?;
    let (target, replace) = destination(out, root, overwrite)?;
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let read = artifact::get(
        &reader,
        ArtifactType::Mask,
        ArtifactId(id),
        &DecodeLimits::default(),
    );
    let Artifact::Mask(held) = read.map_err(|e| e.to_string())? else {
        return Err("the store holds no mask of that identity".to_owned());
    };
    let space = held.binding().source_space();
    let (width, height) = (space.width(), space.height());
    let grey: Vec<u8> = (held.core().iter().zip(held.blend()).zip(held.protect()))
        .map(|((core, blend), protect)| match (*protect, *core) {
            (0, 0) => *blend,
            (0, _) => 255,
            _ => 0,
        })
        .collect();
    let bytes = whizbang_core::identity::encode::png_of(
        &grey,
        width,
        height,
        whizbang_core::identity::encode::Channels::Grey,
    )
    .map_err(|error| format!("cannot write the mask as a PNG: {error}"))?;
    written(&target, replace, &bytes)?;
    Ok(format!(
        "exported mask {} ({width} by {height}) as a grey picture to {}\n",
        held.id(),
        whizbang::door::printable(&out.display().to_string())
    ))
}

/// Exports the canonical file of the artifact of identity `id` from the store at
/// `root` to `to`: what to print, or the complaint.
pub fn export_file(root: &Path, id: &str, to: &Path, overwrite: bool) -> Result<String, String> {
    let id = ArtifactId(identity(id, "the identity")?);
    let (target, replace) = destination(to, root, overwrite)?;
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let kinds = [
        ArtifactType::Mask,
        ArtifactType::Image,
        ArtifactType::Location,
        ArtifactType::BandProfile,
        ArtifactType::Series,
        ArtifactType::Plan,
    ];
    let Some(kind) = kinds
        .into_iter()
        .find(|kind| artifact::holds(&generation, *kind, id))
    else {
        return Err("the store holds no artifact of that identity".to_owned());
    };
    let held =
        artifact::get(&reader, kind, id, &DecodeLimits::default()).map_err(|e| e.to_string())?;
    written(&target, replace, &held.to_bytes())?;
    Ok(format!(
        "exported {kind} {id} as its canonical file to {}\n",
        whizbang::door::printable(&to.display().to_string())
    ))
}

/// Writes `bytes` to `out`, a new file, or over the one there when `replace` says so.
fn written(out: &Path, replace: bool, bytes: &[u8]) -> Result<(), String> {
    let shown = out.display();
    let cannot = |error: std::io::Error| format!("cannot write {shown}: {error}");
    if replace {
        let name = out.file_name().map(|n| n.to_string_lossy().into_owned());
        let beside = out.with_file_name(format!(
            ".{}.{}.export",
            name.unwrap_or_default(),
            std::process::id()
        ));
        write_new(&beside, bytes).map_err(cannot)?;
        if let Err(error) = std::fs::rename(&beside, out) {
            let _ = std::fs::remove_file(&beside);
            return Err(cannot(error));
        }
    } else {
        write_new(out, bytes).map_err(cannot)?;
    }
    Ok(())
}
