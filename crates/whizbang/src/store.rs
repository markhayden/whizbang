//! `whizbang store list|delete|prune|import`: wiring from arguments to the core's store.
//!
//! One rule for all four: the store must be there. None of them makes a store at a path
//! with nothing at it or completes one in an empty directory, except `import --create`,
//! which says so when it has. The three that write take the writer lock and are refused
//! while another process holds it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Args, Subcommand};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Pruned, Reader, Store};

#[derive(Args)]
pub struct Where {
    /// The store's directory.
    #[arg(long, value_name = "DIR")]
    store: PathBuf,
}

#[derive(Subcommand)]
pub enum Command {
    /// List the runs the store holds. Reads only: it works while another process writes.
    List {
        #[command(flatten)]
        at: Where,
    },
    /// Delete runs, and remove what only they referenced. What another run shares is
    /// kept; spend and request-id records are never removed. If the store does not hold
    /// one of the runs, none is deleted.
    Delete {
        #[command(flatten)]
        at: Where,
        /// The runs to delete, by id.
        #[arg(required = true, value_name = "RUN")]
        runs: Vec<String>,
    },
    /// Remove what nothing references: older generations and unreferenced content.
    Prune {
        #[command(flatten)]
        at: Where,
    },
    /// For developers: bring a canonical artifact file into a store, under its type and
    /// identity. A picture is not an artifact file.
    Import {
        /// The canonical file of a mask, a location, an image, a band profile, a series
        /// or a plan.
        file: PathBuf,
        #[command(flatten)]
        at: Where,
        /// Make the store if there is none: at a path with nothing at it, or in an
        /// empty directory. Without this a store must be there already.
        #[arg(long)]
        create: bool,
    },
}

/// Runs a store command: what to print on success, or the complaint.
pub fn run(command: &Command) -> Result<String, String> {
    match command {
        Command::List { at } => list(&at.store),
        Command::Delete { at, runs } => delete(&at.store, runs),
        Command::Import { file, at, create } => crate::import::import(file, &at.store, *create),
        Command::Prune { at } => {
            let pruned = writer(&at.store)?.prune().map_err(|e| e.to_string())?;
            Ok(report(&pruned))
        }
    }
}

fn list(root: &Path) -> Result<String, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let mut text = format!(
        "generation: {}\nnames outside runs: {}\nruns: {}\n",
        generation.number(),
        generation.entries().len(),
        generation.runs().len()
    );
    for (id, run) in generation.runs() {
        let count = |kind: Kind| {
            let of_kind = run.entries().values().filter(|entry| entry.kind == kind);
            of_kind.count()
        };
        // The sizes are what the store's files say, and a sum of them stops at the
        // largest number there is.
        let bytes = run
            .entries()
            .values()
            .fold(0u64, |sum, entry| sum.saturating_add(entry.bytes));
        // One run, one line: an id is printed with anything that is not printable, and
        // the quote and backslash that the escapes use, escaped.
        text.push_str(&format!(
            "{}\toutputs {}\tdiagnostics {}\tbytes {bytes}\n",
            id.escape_debug(),
            count(Kind::Output),
            count(Kind::Diagnostic)
        ));
    }
    Ok(text)
}

fn delete(root: &Path, runs: &[String]) -> Result<String, String> {
    let store = writer(root)?;
    // A run named twice is one run.
    let runs: BTreeSet<&str> = runs.iter().map(String::as_str).collect();
    let runs: Vec<&str> = runs.into_iter().collect();
    let published = store.delete(&runs).map_err(|e| e.to_string())?;
    let mut text = format!(
        "runs deleted: {}\ngeneration: {}\n",
        runs.len(),
        published.generation
    );
    // The deletion is durable from here. What it left without a reference goes now.
    let pruned = store.prune().map_err(|e| {
        format!("the runs are deleted, and the removal of what they left failed: {e}")
    })?;
    text.push_str(&report(&pruned));
    Ok(text)
}

/// Opens the store to write. A command that removes things makes no store where there
/// is none, and completes none.
fn writer(root: &Path) -> Result<Store, String> {
    Store::open_existing(root, Arc::new(RealFs)).map_err(|e| e.to_string())
}

fn report(pruned: &Pruned) -> String {
    let mut text = format!(
        "generations removed: {}\ncontent files removed: {}\n",
        pruned.generations_removed.len(),
        pruned.objects_removed
    );
    if !pruned.generations_in_use.is_empty() {
        text.push_str(&format!(
            "generations left for a reader that still holds them: {} (prune again once it has finished)\n",
            pruned.generations_in_use.len()
        ));
    }
    text
}
