//! The lab's files (plan m3, task L1; `SPEC.md` section 7, "The lab"; ADR 0038): the
//! corpus manifest, the protocol frozen before a round, and the matrix of arms, each read
//! strictly and checked before anything runs; and a cell's identity, everything that
//! changes its work.
//!
//! Ported from rascal (`crates/rascal-core/src/lab/corpus.rs`, `protocol.rs`, `matrix.rs`
//! and `cell.rs`, and `docs/lab.md`, at the commit `docs/sources.toml` pins), reshaped for
//! whizbang: an arm runs a recipe of the build through the executor, its filler and finders
//! being backends of the config; every file a matrix binds is a picture of the corpus, by
//! id, so no file of the lab names a path of the owner's, only paths relative to the
//! corpus's root, which the user gives; and two bindings rascal's matrix lacked, a text for
//! each image (`texts`) and a reference arm (`reference`).

pub mod cell;
pub mod corpus;
pub mod decode;
pub mod matrix;
pub mod protocol;

use std::path::Path;

use whizbang_core::identity::Digest;

/// The `sha256:<hex>` of bytes, as rascal's lab files write a digest.
pub fn digest_of(bytes: &[u8]) -> String {
    format!("sha256:{}", Digest::of_bytes(bytes).hex())
}

/// Whether text is a digest as the lab writes one: `sha256:` and 64 lower-case hex digits.
pub fn is_digest(text: &str) -> bool {
    text.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && (hex.bytes()).all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// A name in a lab's files (a lab, an image, a group, an arm, a binding): 1 to 64 ASCII
/// letters, digits, `-`, `_` and `.`, starting with a letter or a digit.
pub fn is_name(text: &str) -> bool {
    text.len() <= 64
        && text.starts_with(|c: char| c.is_ascii_alphanumeric())
        && (text.chars()).all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// A path of a lab's file: plain names joined by `/`, relative, with no `.` or `..`.
pub fn is_plain_relative(text: &str) -> bool {
    !text.is_empty()
        && text.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && (part.chars()).all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

/// Why a lab's file could not be read as its form, in fixed words and the place (plan m3,
/// task L2): the reader's own words are not passed on, since they can quote a line of the
/// file, and a corpus, a protocol and the picks are the owner's.
pub fn unreadable(what: &str, text: &str, error: &toml::de::Error) -> String {
    let place = error.span().map(|span| place_of(text, span.start));
    format!(
        "{what} is not of the form this build reads{}",
        place.unwrap_or_default()
    )
}

/// `, at line <l>, column <c>` of a byte's place in a text, both from 1.
pub fn place_of(text: &str, at: usize) -> String {
    let before = &text.as_bytes()[..at.min(text.len())];
    let line = before.iter().filter(|b| **b == b'\n').count() + 1;
    let start = before
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |i| i + 1);
    let column = String::from_utf8_lossy(&before[start..]).chars().count() + 1;
    format!(", at line {line}, column {column}")
}

/// The bytes of a file under `root`, read as [`crate::sets::read_under`] reads one, with
/// the root left out of any refusal: the file is named by its path under the root, never
/// by where the root is on the machine.
pub fn read_relative(root: &Path, relative: &str, limit: u64) -> Result<Vec<u8>, String> {
    crate::sets::read_under(root, relative, limit).map_err(|why| {
        let base = root.display().to_string();
        let base = base.trim_end_matches('/');
        why.replace(&format!("{base}/"), "").replace(base, ".")
    })
}
