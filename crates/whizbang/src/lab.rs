//! `whizbang lab`: the lab's corpus, a round, and its decode (plan m3, tasks L1 and L2):
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
        Command::Decode {
            protocol,
            assignment,
            picks,
        } => Ok((laboratory::decode_files(protocol, assignment, picks)?, None)),
    }
}
