//! `whizbang store export`: an image in a store written out as a PNG file anyone can open
//! (plan 2A, task R2).
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
//! - **Not into a folder that is missing:** nothing is made but the file.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Digest};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;

/// The pixels as a PNG: 8 bits a channel, red, green, blue and alpha, with no other chunk.
pub fn png(pixels: &CanonicalPixels) -> Result<Vec<u8>, String> {
    let mut file = Vec::new();
    PngEncoder::new_with_quality(&mut file, CompressionType::Default, FilterType::Adaptive)
        .write_image(
            pixels.rgba(),
            pixels.width(),
            pixels.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| format!("cannot write the image as a PNG: {error}"))?;
    Ok(file)
}

/// Where the file goes, refused as the module's description says: the path to write, and
/// whether a file is there to be replaced.
fn destination(out: &Path, root: &Path, overwrite: bool) -> Result<bool, String> {
    let shown = out.display();
    if out.file_name().is_none() {
        return Err(format!("{shown} names no file"));
    }
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
    match std::fs::symlink_metadata(out) {
        Ok(found) if found.file_type().is_symlink() => Err(format!(
            "{shown} is a link: export writes no file through a link"
        )),
        Ok(found) if !found.is_file() => Err(format!("{shown} is there and is no file")),
        Ok(_) if !overwrite => Err(format!(
            "{shown} is there already: pass --overwrite to replace it"
        )),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
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

/// Exports the image `image` from the store at `root` to `out`: what to print, or the
/// complaint.
pub fn export(root: &Path, image: &str, out: &Path, overwrite: bool) -> Result<String, String> {
    let Ok(id) = image.parse::<Digest>() else {
        return Err(
            "--image is no artifact identity: an identity is sha256: and 64 lower-case hex \
             digits, as `whizbang store import` prints it"
                .to_owned(),
        );
    };
    let replace = destination(out, root, overwrite)?;
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let limits = DecodeLimits::default();
    let read = artifact::get(&reader, ArtifactType::Image, ArtifactId(id), &limits);
    let Artifact::Image(held) = read.map_err(|e| e.to_string())? else {
        return Err("the store holds no image of that identity".to_owned());
    };
    let pixels = held.pixels(&limits).map_err(|e| e.to_string())?;
    let bytes = png(&pixels)?;
    let shown = out.display();
    let cannot = |error: std::io::Error| format!("cannot write {shown}: {error}");
    if replace {
        let name = out.file_name().map(|n| n.to_string_lossy().into_owned());
        let beside = out.with_file_name(format!(
            ".{}.{}.export",
            name.unwrap_or_default(),
            std::process::id()
        ));
        write_new(&beside, &bytes).map_err(cannot)?;
        if let Err(error) = std::fs::rename(&beside, out) {
            let _ = std::fs::remove_file(&beside);
            return Err(cannot(error));
        }
    } else {
        write_new(out, &bytes).map_err(cannot)?;
    }
    Ok(format!(
        "exported image {} ({} by {}) to {}\n",
        held.id(),
        pixels.width(),
        pixels.height(),
        whizbang::door::printable(&shown.to_string())
    ))
}
