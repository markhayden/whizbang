//! The corpus manifest, `corpus.toml` (rascal's `lab/corpus.rs`): pictures by id, each with
//! its file relative to the corpus's root, its SHA-256, its group and a note of where it
//! came from. The pictures themselves are never committed; the manifest is, and it names no
//! path but one relative to a root the user gives (`whizbang lab corpus verify --root`).
//!
//! A picture may say its projection (`projection = "flat"` or `"equirectangular"`), the word
//! `whizbang store import --projection` takes: a plain 2:1 file whose metadata says nothing
//! is otherwise brought in flat, which a panorama's recipe refuses. It is said only of a
//! file brought in as an image.
//!
//! A picture with no group is a file the matrix binds (a mask, a clean picture for a
//! reference arm), not one judged; the protocol's groups name only pictures with a group.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{digest_of, is_digest, is_name, is_plain_relative};

/// The words a picture's projection may be.
pub const PROJECTIONS: [&str; 2] = ["flat", "equirectangular"];

/// The most pictures a corpus lists.
pub const PICTURES_LIMIT: usize = 4096;

/// `corpus.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    pub version: u32,
    #[serde(default)]
    pub image: Vec<Picture>,
}

/// One picture of the corpus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Picture {
    pub id: String,
    /// The file, relative to the corpus's root.
    pub file: String,
    /// `sha256:<hex>` of the file's bytes.
    pub sha256: String,
    /// The group the protocol judges it under; none for a file the matrix only binds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Where the picture came from and under what terms it may be used.
    pub licence: String,
    /// What the picture is: `flat` or `equirectangular`, the word `whizbang store import
    /// --projection` takes. None for a picture brought in as before, flat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<String>,
}

/// What verifying one picture found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verification {
    Ok,
    Missing,
    Changed { found: String },
}

impl Corpus {
    /// A manifest's text, read strictly: version 1, unique plain ids, plain relative files,
    /// digests of the form `sha256:<64 hex>`, and a projection, when said, of the two words.
    pub fn parse(text: &str) -> Result<Corpus, String> {
        let corpus: Corpus =
            toml::from_str(text).map_err(|e| super::unreadable("the corpus", text, &e))?;
        if corpus.version != 1 {
            return Err(format!("the corpus: version {} is not 1", corpus.version));
        }
        if corpus.image.len() > PICTURES_LIMIT {
            return Err(format!("the corpus: at most {PICTURES_LIMIT} pictures"));
        }
        let mut ids = BTreeSet::new();
        for (at, picture) in corpus.image.iter().enumerate() {
            if !is_name(&picture.id) || !ids.insert(picture.id.as_str()) {
                return Err(format!(
                    "the corpus: the id of picture {} of the list is no plain name, or is \
                     repeated",
                    at + 1
                ));
            }
            let id = &picture.id;
            if !is_plain_relative(&picture.file) {
                return Err(format!(
                    "the corpus: picture {id}: its file is not a plain relative path"
                ));
            }
            if !is_digest(&picture.sha256) {
                return Err(format!(
                    "the corpus: picture {id}: sha256 is not sha256:<64 hex digits>"
                ));
            }
            if picture.group.as_deref().is_some_and(|g| !is_name(g)) {
                return Err(format!(
                    "the corpus: picture {id}: its group is no plain name"
                ));
            }
            if (picture.projection.as_deref()).is_some_and(|p| !PROJECTIONS.contains(&p)) {
                return Err(format!(
                    "the corpus: picture {id}: its projection is flat or equirectangular"
                ));
            }
        }
        Ok(corpus)
    }

    /// The picture with this id.
    pub fn picture(&self, id: &str) -> Option<&Picture> {
        self.image.iter().find(|p| p.id == id)
    }

    /// Every picture's file under `root`, read through no link and held to `limit` bytes,
    /// checked against its digest, in the manifest's order. A file that is not there is
    /// missing; one that cannot be read is refused, naming it.
    pub fn verify(&self, root: &Path, limit: u64) -> Result<Vec<(&Picture, Verification)>, String> {
        let mut out = Vec::with_capacity(self.image.len());
        for picture in &self.image {
            let path = root.join(&picture.file);
            let found = match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Verification::Missing,
                _ => {
                    let bytes = super::read_relative(root, &picture.file, limit)
                        .map_err(|why| format!("picture {}: {why}", picture.id))?;
                    let found = digest_of(&bytes);
                    if found == picture.sha256 {
                        Verification::Ok
                    } else {
                        Verification::Changed { found }
                    }
                }
            };
            out.push((picture, found));
        }
        Ok(out)
    }
}

/// What `whizbang lab corpus verify` prints of a verification: a line for each picture
/// (`ok`, `missing`, or `changed` with the digest wanted and the one found) and a count;
/// and whether every picture was there as the manifest pins it.
pub fn report(found: &[(&Picture, Verification)]) -> (String, bool) {
    let mut text = String::new();
    let mut bad = 0;
    for (picture, verification) in found {
        let line = match verification {
            Verification::Ok => format!("ok {}\n", picture.id),
            Verification::Missing => format!("missing {}\n", picture.id),
            Verification::Changed { found } => {
                format!(
                    "changed {} wanted {} found {found}\n",
                    picture.id, picture.sha256
                )
            }
        };
        bad += usize::from(*verification != Verification::Ok);
        text.push_str(&line);
    }
    text.push_str(&format!(
        "pictures: {}, ok: {}, missing or changed: {bad}\n",
        found.len(),
        found.len() - bad
    ));
    (text, bad == 0)
}
