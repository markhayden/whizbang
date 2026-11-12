//! `whizbang store import`: a developer's command that brings a canonical artifact file
//! (ADR 0002) into a store, so that a fixture can be an action's input.

use std::path::Path;
use std::sync::Arc;

use whizbang_core::artifact::{self, Artifact};
use whizbang_core::identity::DecodeLimits;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Error, Reader, Store};

/// Reads and checks the file before the store is touched: a file that is refused creates
/// nothing. The store must be there, like the store of every store command, unless
/// `create` says to make one where there is none (nothing at the path, or an empty
/// directory); what is printed then begins by saying that a store was created.
pub fn import(file: &Path, root: &Path, create: bool) -> Result<String, String> {
    let bytes =
        std::fs::read(file).map_err(|error| format!("cannot read {}: {error}", file.display()))?;
    let artifact = Artifact::from_bytes(&bytes, &DecodeLimits::default())
        .map_err(|error| format!("{}: {error}", file.display()))?;
    let (kind, id) = (artifact.artifact_type(), artifact.id());

    let mut printed = String::new();
    let store = if create {
        // Whether a store is there is asked before the writer that would make one opens.
        let was_there = Reader::open(root, Arc::new(RealFs)).is_ok();
        let store = Store::open(root, Arc::new(RealFs)).map_err(|error| error.to_string())?;
        if !was_there {
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
