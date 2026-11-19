//! `whizbang try <tool>` (plan m2c, task X1, item 5): wiring from arguments to
//! `whizbang::trying`, which says what a read of a pair is.

use clap::Args as ClapArgs;
use whizbang::wiring::Uses;
use whizbang_core::artifact::ArtifactId;
use whizbang_core::identity::Digest;

use crate::rascal::{Under, executor};
use crate::store::Where;

#[derive(ClapArgs)]
pub struct Args {
    /// The tool: change_map or edge_similarity.
    tool: String,
    /// The picture the candidate was edited from, by its identity in the store: sha256:
    /// and 64 hex digits, as `whizbang store import` prints it.
    #[arg(long, value_name = "ID")]
    source: String,
    /// The edited picture, by its identity in the store.
    #[arg(long, value_name = "ID")]
    candidate: String,
    #[command(flatten)]
    at: Where,
    #[command(flatten)]
    under: Under,
}

fn identity(flag: &str, text: &str) -> Result<ArtifactId, String> {
    text.parse::<Digest>().map(ArtifactId).map_err(|_| {
        format!(
            "--{flag} is no artifact identity: an identity is sha256: and 64 lower-case hex \
             digits, as `whizbang store import` prints it"
        )
    })
}

/// Runs `try`: what to print, or the complaint.
pub fn run(args: &Args) -> Result<String, String> {
    let declared = whizbang::trying::declared(&args.tool)?;
    let source = identity("source", &args.source)?;
    let candidate = identity("candidate", &args.candidate)?;
    let recovery = whizbang::wiring::recovery();
    let executor = executor(&args.at, &args.under, recovery, None, Uses::Nothing)?;
    let root = args.at.root().to_path_buf();
    let tried = whizbang::trying::read_pair(&executor, &root, &declared, source, candidate)?;
    whizbang::trying::tried_text(&declared, &tried)
}
