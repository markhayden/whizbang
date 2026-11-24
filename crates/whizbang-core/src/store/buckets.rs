//! The generation in buckets (ADR 0047): which of 256 buckets each name is in, and the
//! shapes of the two files of format 5, the generation file and a pack, the file in
//! which a publication writes the buckets it changed.
//!
//! A name's bucket is the first byte of the SHA-256 of its key: a run's names, the run's
//! id; a request id's names, the id; the ledger's names of a paid attempt, the run it
//! belongs to; a mark of an import, the key of what it marks; any other name with a `/`
//! in it, the name itself. A name with no `/` is one of the store's few fixed names and
//! is in no bucket: the generation file holds it. The rule is fixed by the format: a
//! change to it is a new format.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::IMPORTED_PREFIX;
use super::digest::Digest;
use super::generation::{Bucket, Entry, GENERATION_FORMAT, Imported, Run, unique};

/// How many buckets a generation has.
pub const BUCKETS: usize = 256;

/// What the names of a request id start with (`contract::REQUESTS_PREFIX`).
const REQUESTS: &str = "requests/";

/// What the names of the ledger start with (`spend::SPEND_PREFIX`).
const SPEND: &str = "spend/";

/// The forms of the ledger's names that name a run, and at which segment after the form.
const SPEND_RUN_FORMS: [(&str, usize); 8] = [
    ("reservations", 0),
    ("held", 1),
    ("place", 0),
    ("dispatched", 0),
    ("settling", 0),
    ("settled", 0),
    ("closed", 0),
    ("adjusted", 0),
];

/// How an empty bucket is written in the generation file: it has no file.
const EMPTY: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The key of a name outside runs, or `None` for a fixed name.
fn key_of(name: &str) -> Option<&str> {
    if let Some(marked) = name.strip_prefix(IMPORTED_PREFIX) {
        return Some(key_of(marked).unwrap_or(marked));
    }
    if let Some(rest) = name.strip_prefix(REQUESTS) {
        return Some(rest.split('/').next().unwrap_or(rest));
    }
    if let Some(rest) = name.strip_prefix(SPEND) {
        let mut segments = rest.split('/');
        let form = segments.next().unwrap_or_default();
        let run = SPEND_RUN_FORMS
            .iter()
            .find(|(named, _)| *named == form)
            .and_then(|(_, at)| segments.nth(*at))
            .filter(|run| !run.is_empty());
        return Some(run.unwrap_or(name));
    }
    name.contains('/').then_some(name)
}

fn of_key(key: &str) -> u8 {
    Sha256::digest(key.as_bytes())[0]
}

/// The bucket of a name outside runs, or `None` for one of the fixed names the
/// generation file holds.
pub fn of_name(name: &str) -> Option<u8> {
    key_of(name).map(of_key)
}

/// The bucket of a run, and of all its names.
pub fn of_run(id: &str) -> u8 {
    of_key(id)
}

/// The generation file of format 5.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RootFile {
    pub(super) format: u32,
    pub(super) number: u64,
    #[serde(deserialize_with = "unique")]
    pub(super) entries: BTreeMap<String, Entry>,
    pub(super) buckets: String,
}

/// A pack: the file a publication writes, holding each bucket it wrote, by its index.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackFile {
    format: u32,
    #[serde(deserialize_with = "unique")]
    buckets: BTreeMap<String, BucketBody>,
}

/// One bucket in a pack.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BucketBody {
    #[serde(deserialize_with = "unique")]
    entries: BTreeMap<String, Entry>,
    #[serde(deserialize_with = "unique")]
    runs: BTreeMap<String, Run>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique"
    )]
    imports: BTreeMap<String, Imported>,
}

/// The buckets of one pack, by index.
pub(crate) type Pack = BTreeMap<u8, Arc<Bucket>>;

/// The generation file of a generation: its number, its fixed names, and the digest of
/// each bucket in order, an empty one as 64 zeros. So its size does not grow with the
/// store.
pub(super) fn root_text(
    number: u64,
    fixed: &BTreeMap<String, Entry>,
    digests: &[Option<Digest>],
) -> Vec<u8> {
    let file = RootFile {
        format: GENERATION_FORMAT,
        number,
        entries: fixed.clone(),
        buckets: (digests.iter())
            .map(|digest| digest.map_or_else(|| EMPTY.to_owned(), |digest| digest.hex()))
            .collect::<String>(),
    };
    let mut text = serde_json::to_vec_pretty(&file).unwrap_or_default();
    text.push(b'\n');
    text
}

/// A pack's file: these buckets, each under its index.
pub(super) fn pack_text<'a>(buckets: impl Iterator<Item = (u8, &'a Bucket)>) -> Vec<u8> {
    let file = PackFile {
        format: GENERATION_FORMAT,
        buckets: buckets
            .map(|(at, bucket)| {
                let body = BucketBody {
                    entries: bucket.entries.clone(),
                    runs: bucket.runs.clone(),
                    imports: bucket.imports.clone(),
                };
                (at.to_string(), body)
            })
            .collect(),
    };
    let mut text = serde_json::to_vec_pretty(&file).unwrap_or_default();
    text.push(b'\n');
    text
}

/// The digests a generation file of format 5 names, in order, `None` for an empty
/// bucket; or why they are none.
pub(super) fn digests(root: &RootFile) -> Result<Vec<Option<Digest>>, String> {
    let text = root.buckets.as_bytes();
    if text.len() != BUCKETS * EMPTY.len() {
        return Err(format!(
            "its buckets are {} digits, and a generation names {BUCKETS} of 64",
            text.len()
        ));
    }
    (text.chunks(EMPTY.len()))
        .map(|text| match std::str::from_utf8(text).unwrap_or_default() {
            EMPTY => Ok(None),
            text => Digest::from_hex(text)
                .map(Some)
                .ok_or_else(|| format!("{text:?} is not a bucket's digest")),
        })
        .collect()
}

/// Whether every fixed name is one: none that the rule puts in a bucket.
pub(super) fn check_fixed(fixed: &BTreeMap<String, Entry>) -> Result<(), String> {
    match fixed.keys().find(|name| of_name(name).is_some()) {
        Some(name) => Err(format!("the name {name:?} belongs in a bucket")),
        None => Ok(()),
    }
}

/// A pack from its file's bytes, or why it is none: of another format, a bucket of no
/// index, or a bucket holding a name or a run that the rule puts in another.
pub(super) fn pack_from(bytes: &[u8]) -> Result<Pack, String> {
    let file: PackFile = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if file.format != GENERATION_FORMAT {
        return Err(format!("a pack of format {}", file.format));
    }
    let mut pack = Pack::new();
    for (index, body) in file.buckets {
        let at = (index.parse::<u8>().ok())
            .filter(|at| at.to_string() == index)
            .ok_or_else(|| format!("{index:?} is no bucket's index"))?;
        if let Some(name) = body.entries.keys().find(|name| of_name(name) != Some(at)) {
            return Err(format!("the name {name:?} is not of bucket {at}"));
        }
        if let Some(id) = body.runs.keys().find(|id| of_run(id) != at) {
            return Err(format!("the run {id:?} is not of bucket {at}"));
        }
        let mark = |name: &String| format!("{IMPORTED_PREFIX}{name}");
        if let Some(name) = body
            .imports
            .keys()
            .find(|name| of_name(&mark(name)) != Some(at))
        {
            return Err(format!("the import of {name:?} is not of bucket {at}"));
        }
        let bucket = Bucket {
            entries: body.entries,
            runs: body.runs,
            imports: body.imports,
        };
        pack.insert(at, Arc::new(bucket));
    }
    Ok(pack)
}
