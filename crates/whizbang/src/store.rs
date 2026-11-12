//! `whizbang store list|delete|prune`: wiring from arguments to the core's store.

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
    /// kept; spend and request-id records are never removed.
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
}

/// Runs a store command: what to print on success, or the complaint.
pub fn run(command: &Command) -> Result<String, String> {
    match command {
        Command::List { at } => list(&at.store),
        Command::Delete { at, runs } => delete(&at.store, runs),
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
        let bytes: u64 = run.entries().values().map(|entry| entry.bytes).sum();
        text.push_str(&format!(
            "{id}\toutputs {}\tdiagnostics {}\tbytes {bytes}\n",
            count(Kind::Output),
            count(Kind::Diagnostic)
        ));
    }
    Ok(text)
}

fn delete(root: &Path, runs: &[String]) -> Result<String, String> {
    let store = writer(root)?;
    let runs: Vec<&str> = runs.iter().map(String::as_str).collect();
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

/// Opens an existing store to write. A writer makes a store where there is none, and a
/// command that removes things has no business doing that, so a reader looks first.
fn writer(root: &Path) -> Result<Store, String> {
    Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    Store::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())
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
