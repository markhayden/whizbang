//! `whizbang lab`: the lab's corpus, a round, its pairs and its decode (plan m3, tasks L1 to L3):
//! wiring from arguments to `whizbang::laboratory`, which says what each does.

use std::path::PathBuf;

use clap::Subcommand;
use whizbang::harness::Running;
use whizbang::laboratory::{self, Asked};
use whizbang::wiring::{Platform, Uses};

use crate::rascal::Under;
use crate::store::Where;

#[derive(Subcommand)]
pub enum Command {
    /// The corpus: its pictures, outside the repository, checked against its manifest.
    Corpus {
        #[command(subcommand)]
        command: CorpusCommand,
    },
    /// Run a round: each arm of the matrix on each image, a run of its recipe in the store
    /// under the config's filler and finder, one at a time; a reference arm's cell hands
    /// back the picture it binds and runs nothing. The pictures the matrix binds are read
    /// from the corpus's folder, checked against the manifest's digests and brought into
    /// the store first. Run again, it resumes: a cell that completed or failed is not run
    /// again, and one that was interrupted is. Refused before its first cell when its
    /// forecast passes what the store's allowance has left, and when the store holds runs
    /// of the lab under another protocol. Prints counts only.
    Run {
        /// The matrix file; the corpus manifest and the protocol it names are beside it.
        #[arg(long, value_name = "FILE")]
        matrix: PathBuf,
        /// The folder the corpus manifest's files are relative to.
        #[arg(long, value_name = "DIR")]
        corpus: PathBuf,
        #[command(flatten)]
        at: Where,
        #[command(flatten)]
        under: Under,
    },
    /// Write a round's blinded pairs as a static page, for one person to pick on, and the
    /// assignment beside it, frozen by its digest: for each image the protocol judges, the
    /// two arms' results side by side in an order and on sides drawn from the protocol's
    /// seed, at their focus (the region both changed, at full size) or whole. The page
    /// loads nothing from the network, runs no script from elsewhere, and writes the picks
    /// only as a file you save (`picks.json`); open a saved one on it to resume. Takes the
    /// round's own matrix, corpus folder and config, and reads the store; refused while a
    /// cell has no completed attempt. Prints counts and the assignment's digest.
    Pairs {
        /// The matrix file; the corpus manifest and the protocol it names are beside it.
        #[arg(long, value_name = "FILE")]
        matrix: PathBuf,
        /// The folder the corpus manifest's files are relative to.
        #[arg(long, value_name = "DIR")]
        corpus: PathBuf,
        /// The folder the page, its pictures and the assignment are written in: by default
        /// `pairs/<a>-vs-<b>` in the corpus's folder. Never the store's; never inside a git
        /// work tree unless --force.
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
        /// A lighter round: this many pairs, drawn by the protocol's groups from its seed,
        /// each group its share. The rules do not change: a group with fewer decided pairs
        /// than the protocol's `min_decided` is inconclusive, never a win.
        #[arg(long, value_name = "N")]
        limit: Option<u32>,
        /// Write the page inside a git work tree.
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        at: Where,
        #[command(flatten)]
        under: Under,
    },
    /// Decode a round's picks against the frozen assignment of its pairs, under its
    /// protocol: for each group its pairs, decided pairs, wins, losses, same and neither,
    /// and its outcome, `preferred`, `not_worse`, `worse` or `inconclusive`. Prints counts
    /// only.
    Decode {
        /// The protocol.
        #[arg(long, value_name = "FILE")]
        protocol: PathBuf,
        /// The frozen assignment of the pairs.
        #[arg(long, value_name = "FILE")]
        assignment: PathBuf,
        /// The picks made on it.
        #[arg(long, value_name = "FILE")]
        picks: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum CorpusCommand {
    /// Check a folder against a corpus manifest: a line for each picture, ok, missing, or
    /// changed with the digest wanted and the one found, then a count. Fails after the
    /// whole list when any picture is missing or changed. Each file is read through no
    /// link, from the folder down.
    Verify {
        /// The corpus manifest.
        #[arg(long, value_name = "FILE")]
        manifest: PathBuf,
        /// The folder the manifest's files are relative to.
        #[arg(long, value_name = "DIR")]
        root: PathBuf,
    },
}

/// What the command prints, and why it fails when it does after printing it.
pub fn run(command: &Command, platform: Platform) -> Result<(String, Option<String>), String> {
    match command {
        Command::Corpus {
            command: CorpusCommand::Verify { manifest, root },
        } => {
            let (text, whole) = laboratory::verify(manifest, root)?;
            let failed = (!whole).then(|| "some pictures are missing or changed".to_owned());
            Ok((text, failed))
        }
        Command::Run {
            matrix,
            corpus,
            at,
            under,
        } => {
            let recovery = whizbang::wiring::recovery();
            let over = under.over(Some(platform), Uses::Every)?;
            // The pictures are brought in before the executor holds the store's writer.
            let asked = Asked {
                matrix,
                corpus,
                root: at.root(),
                over: &over,
                recovery: recovery.clone(),
            };
            let prepared = laboratory::prepare(&asked)?;
            let left_micros = crate::eval::left(at.root())?;
            let executor = crate::rascal::built(at, &over, recovery, false)?;
            let running = Running {
                executor: &executor,
                root: at.root(),
                left_micros,
                stop: &|| false,
            };
            let report = laboratory::round(&prepared, &running)?;
            let failed = (report.failed > 0).then(|| "some cells failed".to_owned());
            Ok((report.text(), failed))
        }
        Command::Pairs {
            matrix,
            corpus,
            out,
            limit,
            force,
            at,
            under,
        } => {
            let over = under.over(Some(platform), Uses::Every)?;
            // A folder named that cannot be written is refused before the round is read.
            if let Some(out) = out {
                whizbang::pairs_page::checked_out(out, at.root(), *force)?;
            }
            let asked = Asked {
                matrix,
                corpus,
                root: at.root(),
                over: &over,
                recovery: whizbang::wiring::recovery(),
            };
            let prepared = laboratory::prepare(&asked)?;
            let [a, b] = &prepared.validated.protocol.arms;
            let default = corpus.join("pairs").join(format!("{a}-vs-{b}"));
            let written = whizbang::pairs_page::pairs(
                &prepared,
                &whizbang::pairs_page::Asked {
                    root: at.root(),
                    out: out.as_deref().unwrap_or(&default),
                    limit: *limit,
                    force: *force,
                },
            )?;
            Ok((written.text(), None))
        }
        Command::Decode {
            protocol,
            assignment,
            picks,
        } => Ok((laboratory::decode_files(protocol, assignment, picks)?, None)),
    }
}
