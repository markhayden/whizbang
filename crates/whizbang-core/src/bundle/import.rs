//! A bundle in a store (ADR 0048, "Importing one" and "A bundle is deleted"; plan m7,
//! task B1, item 4): its entry, `bundles/<hex>`, and every member named under it.
//!
//! | Member | Its name in the store |
//! |---|---|
//! | `bundle.json` | `bundles/<hex>/manifest` |
//! | `files/sha256-<file hex>` | `bundles/<hex>/files/<file hex>`, an output |
//! | `runs/<run id>/...` | `bundles/<hex>/runs/<run id>/...`, a diagnostic |
//!
//! A file is named under the bundle's entry alone, never with an ordinary import's name
//! or mark, so two files of one identity with other origins are both kept byte for byte,
//! and an independent import keeps its own name. A bundle's runs are names outside runs,
//! never runs of the store: no history, stats or ledger counts them.

use std::io::Read;

use sha2::{Digest as _, Sha256};

use crate::artifact::Contents;
use crate::identity::Digest;
use crate::store::{self, BUNDLES_PREFIX, Generation, Kind, Published, Store};

use super::read::Hashing;
use super::{FILES_PREFIX, MANIFEST, MAX_BYTES, Manifest, Refused, check, tar};

/// The name of a bundle's entry: [`BUNDLES_PREFIX`] and the hex of its digest.
pub fn entry(digest: &Digest) -> String {
    format!("{BUNDLES_PREFIX}{}", digest.hex())
}

/// The name a member of a bundle is held under in a store, and the kind of its
/// reference.
fn held_as(bundle: &Digest, member: &str) -> (String, Kind) {
    let entry = entry(bundle);
    if member == MANIFEST {
        return (format!("{entry}/manifest"), Kind::Output);
    }
    if let Some(hex) = member.strip_prefix(FILES_PREFIX) {
        return (format!("{entry}/files/{hex}"), Kind::Output);
    }
    (format!("{entry}/{member}"), Kind::Diagnostic)
}

/// The digests of the bundles a generation holds, in order.
pub fn held(generation: &Generation) -> Vec<Digest> {
    let names = generation.entries().range(BUNDLES_PREFIX.to_owned()..);
    names
        .map(|(name, _)| name)
        .take_while(|name| name.starts_with(BUNDLES_PREFIX))
        .filter_map(|name| {
            let rest = name.strip_prefix(BUNDLES_PREFIX)?;
            let hex = rest.strip_suffix("/manifest")?;
            Digest::from_hex(hex)
        })
        .collect()
}

/// The manifest of a bundle the store holds, or `None` when it holds none of that
/// digest.
pub fn manifest_of(
    store: &(impl Contents + ?Sized),
    generation: &Generation,
    digest: &Digest,
) -> Option<Result<Manifest, String>> {
    let entry = generation.get(&format!("{}/manifest", entry(digest)))?;
    Some(
        store
            .read(entry)
            .map_err(|why| why.to_string())
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|why| why.to_string())),
    )
}

/// What an import did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The bundle's digest.
    pub digest: Digest,
    /// Its manifest.
    pub manifest: Manifest,
    /// The generation it was published in; `None` when the store held it already, and
    /// nothing was published.
    pub generation: Option<u64>,
}

/// Why an import did nothing.
#[derive(Debug)]
pub enum ImportError {
    /// The bundle broke a rule.
    Refused(Refused),
    /// The store failed.
    Store(store::Error),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Refused(refused) => refused.fmt(f),
            ImportError::Store(error) => {
                write!(
                    f,
                    "the store failed: {error}; nothing was imported and nothing ran"
                )
            }
        }
    }
}

impl std::error::Error for ImportError {}

/// Imports the bundle `open` reads, into `store`, in one publication: it is read whole
/// and checked first ([`check`]); then read again, and each member, once its digest is
/// the one the first reading found, is staged under the bundle's entry. A bundle the
/// store holds already changes nothing. `open` gives the file from its start each time
/// it is called.
pub fn import<R: Read>(
    store: &Store,
    open: impl Fn() -> std::io::Result<R>,
) -> Result<Imported, ImportError> {
    let io = |why: std::io::Error| {
        ImportError::Refused(Refused::new(format!("the file could not be read: {why}")))
    };
    let checked = check(open().map_err(io)?).map_err(ImportError::Refused)?;
    let digest = checked.digest;
    let generation = store.snapshot().map_err(ImportError::Store)?;
    let already = Imported {
        digest,
        manifest: checked.manifest.clone(),
        generation: None,
    };
    let (manifest_name, _) = held_as(&digest, MANIFEST);
    if generation.get(&manifest_name).is_some() {
        return Ok(already);
    }
    let mut hashing = Hashing {
        from: open().map_err(io)?,
        hash: Sha256::new(),
    };
    let mut publication = store.begin().map_err(ImportError::Store)?;
    let changed = || {
        ImportError::Refused(Refused::new(
            "the file changed while it was read: it is not the bundle that was checked",
        ))
    };
    {
        let mut tar = tar::Reader::new(&mut hashing, MAX_BYTES);
        let mut expected = checked.members.iter();
        let unread = |why: tar::Unread| ImportError::Refused(Refused::new(why.to_string()));
        while let Some(head) = tar.next_head().map_err(unread)? {
            let Some((name, member_digest, _)) = expected.next() else {
                return Err(changed());
            };
            if head.name != *name {
                return Err(changed());
            }
            let bytes = tar.bytes(&head).map_err(unread)?;
            if Digest::of_bytes(&bytes) != *member_digest {
                return Err(changed());
            }
            let (held, kind) = held_as(&digest, name);
            publication
                .create(&held, kind, &bytes)
                .map_err(ImportError::Store)?;
        }
        if expected.next().is_some() {
            return Err(changed());
        }
    }
    if Digest::from_bytes(hashing.hash.finalize().into()) != digest {
        return Err(changed());
    }
    match publication.publish() {
        Ok(published) => Ok(Imported {
            generation: Some(published.generation),
            ..already
        }),
        // Another import of the same bundle was published first: it holds it.
        Err(store::Error::NameExists { name }) if name == manifest_name => Ok(already),
        Err(error) => Err(ImportError::Store(error)),
    }
}

/// Deletes a bundle from a store: its entry's names, in one publication
/// ([`crate::store::Publication::let_go_of_entry`]). Its artifacts stay while another
/// name names them, and a prune removes the rest. A store that holds no bundle of that
/// digest is refused, and nothing is published.
pub fn delete(store: &Store, digest: &Digest) -> store::Result<Published> {
    let mut publication = store.begin()?;
    publication.let_go_of_entry(&entry(digest))?;
    publication.publish()
}
