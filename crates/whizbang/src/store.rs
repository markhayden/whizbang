//! `whizbang store list|delete|prune|import|export`: wiring from arguments to the core's
//! store (`export` is the module `export`).
//!
//! One rule for all five: the store must be there. None of them makes a store at a path
//! with nothing at it or completes one in an empty directory, except `import --create`,
//! which says so when it has (the module `import` says what it makes). The three that
//! write take the writer lock and are refused while another process holds it. Each of
//! the three opens the store through the one entry point (`wiring::opening`), so the
//! store is recovered before the command changes anything; where the recovery fails
//! the command is refused with the reason, and where it could not be published at the
//! limit on a generation file, `delete` and `prune` still work and `import` is refused.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Args, Subcommand};
use whizbang::wiring::{self, Make};
use whizbang_core::contract::{Recovery, Remover};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Pruned, Reader};

#[derive(Args)]
pub struct Where {
    /// The store's directory.
    #[arg(long, value_name = "DIR")]
    store: PathBuf,
}

impl Where {
    /// The store's directory.
    pub fn root(&self) -> &Path {
        &self.store
    }

    /// The store at this directory.
    #[cfg(test)]
    pub fn at(store: &Path) -> Where {
        Where {
            store: store.to_path_buf(),
        }
    }
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
        /// The runs to delete, each by its id, or as `runs list` prints it when that
        /// is no run's id: the same run `runs show` shows for the same text.
        #[arg(required = true, value_name = "RUN")]
        runs: Vec<String>,
    },
    /// Remove what nothing references: older generations and unreferenced content.
    Prune {
        #[command(flatten)]
        at: Where,
    },
    /// For developers: bring a canonical artifact file into a store, under its type and
    /// identity, or, with `--picture`, a PNG or JPEG picture as an image. The file is a
    /// regular file, or a link to one, of at most 65 MiB: the largest picture an image
    /// artifact holds, and the record around it.
    Import {
        /// The canonical file of a mask, a location, an image, a band profile, a series
        /// or a plan: a regular file, or a link to one, of at most 65 MiB. A picture is
        /// not an artifact file: give it with `--picture`.
        #[arg(required_unless_present = "picture")]
        file: Option<PathBuf>,
        /// A PNG or JPEG picture to commit as an image whose role is a picture. It is
        /// decoded within the core's limits (64 MiB, 50,000,000 pixels) and written
        /// again as a PNG of its pixels alone, upright and in sRGB: nothing else the file
        /// holds (its text, its EXIF, its colour profile, its name) is kept.
        #[arg(long, value_name = "FILE", conflicts_with = "file")]
        picture: Option<PathBuf>,
        #[command(flatten)]
        at: Where,
        /// Make the store if there is none: at a path with nothing at it, in an empty
        /// directory, or in a directory that holds only what an unfinished creation of
        /// a store left, which is completed. Only the last part of the path is made: a
        /// path whose parent directory is missing is refused. Without this a store
        /// must be there already.
        #[arg(long)]
        create: bool,
    },
    /// Write an image in the store as a PNG file of its pixels alone, upright and in
    /// sRGB, whatever form the store holds it in: no text, time, colour profile or EXIF.
    /// Reads only: it works while another process writes. It writes no file over
    /// another unless `--overwrite` is given, none through a link, and none inside the
    /// store.
    Export {
        /// The image, by its identity in the store, as `store import` or `rascal render`
        /// prints it.
        #[arg(long)]
        image: String,
        /// The PNG file to write, in a folder that is there.
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        /// Replace a file that is there already.
        #[arg(long)]
        overwrite: bool,
        #[command(flatten)]
        at: Where,
    },
}

/// Runs a store command: what to print on success, or the complaint.
pub fn run(command: &Command) -> Result<String, String> {
    run_with(command, wiring::recovery())
}

/// The same, with a store that is opened to write recovered by this.
pub fn run_with(command: &Command, recovery: Recovery) -> Result<String, String> {
    match command {
        Command::List { at } => list(&at.store),
        Command::Export {
            image,
            out,
            overwrite,
            at,
        } => crate::export::export(&at.store, image, out, *overwrite),
        Command::Delete { at, runs } => delete(&at.store, runs, recovery),
        Command::Import {
            file,
            picture,
            at,
            create,
        } => match (file, picture) {
            (_, Some(picture)) => {
                whizbang::import::import_picture(picture, &at.store, *create, recovery)
            }
            (Some(file), None) => whizbang::import::import(file, &at.store, *create, recovery),
            (None, None) => Err("import takes an artifact file or --picture".to_owned()),
        },
        Command::Prune { at } => {
            let pruned = remover(&at.store, recovery)?;
            let pruned = pruned.prune().map_err(|e| e.to_string())?;
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

fn delete(root: &Path, runs: &[String], recovery: Recovery) -> Result<String, String> {
    let store = remover(root, recovery)?;
    // Each run is found as `runs show` finds it, by the same function: by its id, or
    // as `runs list` prints it when that is no run's id. One that could mean two runs
    // is refused, and then none is deleted. A run named twice is one run.
    let generation = store.snapshot().map_err(|e| e.to_string())?;
    let mut meant = BTreeSet::new();
    for given in runs {
        meant.insert(crate::runs::meant(&generation, given)?.0);
    }
    let runs: Vec<&str> = meant.into_iter().collect();
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

/// Opens the store to write, for deleting and pruning: after its recovery, or, where
/// the recovery could not be published at the limit on a generation file, without it,
/// since these two make the file smaller. A command that removes things makes no
/// store where there is none, and completes none.
fn remover(root: &Path, recovery: Recovery) -> Result<Remover, String> {
    let opening = wiring::opening(root, Make::Nothing, recovery)
        .map_err(|error| wiring::not_opened(&error))?;
    opening.remover().map_err(|why| why.to_string())
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
