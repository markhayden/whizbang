//! `whizbang store import`: a developer's command that brings a canonical artifact file
//! (ADR 0002) into a store, so that a fixture can be an action's input.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Subcommand;
use whizbang_core::artifact::{self, Artifact};
use whizbang_core::identity::DecodeLimits;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;

#[derive(Subcommand)]
pub enum Command {
    /// For developers: bring a canonical artifact file into a store, under its type and
    /// identity. A picture is not an artifact file.
    Import {
        /// The canonical file of a mask, a location, an image, a band profile, a series
        /// or a plan.
        file: PathBuf,
        /// The store's directory. A store is made there if there is none.
        #[arg(long)]
        store: PathBuf,
    },
}

/// Runs a store command. `Ok` is what to print; `Err` is the complaint.
pub fn run(command: &Command) -> Result<String, String> {
    match command {
        Command::Import { file, store } => import(file, store),
    }
}

/// Reads and checks the file before the store is touched: a file that is refused creates
/// nothing.
fn import(file: &Path, store: &Path) -> Result<String, String> {
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
