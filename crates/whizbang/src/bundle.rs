//! `whizbang bundle import|show|list|delete`: wiring from arguments to the binary's
//! bundling (`whizbang::bundling`; ADR 0048). `whizbang runs bundle` writes one (the
//! module `runs`).

use std::path::PathBuf;

use clap::Subcommand;
use whizbang::bundling;
use whizbang::wiring;

use crate::store::Where;

#[derive(Subcommand)]
pub enum Command {
    /// Bring a bundle into a store, in one publication: it is read whole and checked
    /// first, and refused, with nothing imported, at its first fault. Its runs are kept
    /// as evidence under the bundle's own names, never as runs of the store: `runs list`,
    /// `runs stats` and the spend books do not count them. A bundle the store holds
    /// already changes nothing.
    Import {
        /// The bundle: a regular file, or a link to one, as `runs bundle` writes it.
        file: PathBuf,
        #[command(flatten)]
        at: Where,
        /// Make the store if there is none: at a path with nothing at it, in an empty
        /// directory, or in a directory that holds only what an unfinished creation of
        /// a store left. Only the last part of the path is made.
        #[arg(long)]
        create: bool,
    },
    /// Read a bundle and check it whole, and print its steps, the links of a chain, how
    /// many files it carries and what it withholds, with why. Nothing is read into a
    /// store.
    Show {
        /// The bundle: a regular file, or a link to one.
        file: PathBuf,
    },
    /// List the bundles a store holds, each by its digest, with its steps' actions.
    /// Reads only: it works while another process writes.
    List {
        #[command(flatten)]
        at: Where,
    },
    /// Delete a bundle from a store: its names go in one publication, and what nothing
    /// else names is pruned. An artifact a run, an ordinary import or another bundle
    /// still names stays.
    Delete {
        /// The bundle's digest, as `bundle list` prints it.
        digest: String,
        #[command(flatten)]
        at: Where,
    },
}

/// Runs a bundle command: what to print on success, or the complaint.
pub fn run(command: &Command) -> Result<String, String> {
    match command {
        Command::Import { file, at, create } => {
            bundling::import(file, at.root(), *create, wiring::recovery())
        }
        Command::Show { file } => bundling::show(file),
        Command::List { at } => bundling::list(at.root()),
        Command::Delete { digest, at } => bundling::delete(at.root(), digest, wiring::recovery()),
    }
}
