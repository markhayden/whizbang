//! `whizbang store import`: a developer's command that brings a canonical artifact file
//! (ADR 0002) into a store, so that a fixture can be an action's input.

use std::path::Path;
use std::sync::Arc;

use whizbang_core::artifact::{self, Artifact};
use whizbang_core::identity::DecodeLimits;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;

/// Reads and checks the file before the store is touched: a file that is refused creates
/// nothing.
pub fn import(file: &Path, store: &Path) -> Result<String, String> {
    let bytes =
        std::fs::read(file).map_err(|error| format!("cannot read {}: {error}", file.display()))?;
    let artifact = Artifact::from_bytes(&bytes, &DecodeLimits::default())
        .map_err(|error| format!("{}: {error}", file.display()))?;
    let (kind, id) = (artifact.artifact_type(), artifact.id());

    let store = Store::open(store, Arc::new(RealFs)).map_err(|error| error.to_string())?;
    let generation = store.snapshot().map_err(|error| error.to_string())?;
    if artifact::holds(&generation, kind, id) {
        return Ok(format!("already there: {kind} {id}\n"));
    }
    let mut publication = store.begin().map_err(|error| error.to_string())?;
    artifact::put(&mut publication, &artifact).map_err(|error| error.to_string())?;
    publication.publish().map_err(|error| error.to_string())?;
    Ok(format!("imported {kind} {id}\n"))
}
