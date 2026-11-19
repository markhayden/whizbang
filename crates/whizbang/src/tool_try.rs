//! `whizbang try <tool>` (plan m2c, task X1, item 5): wiring from arguments to
//! `whizbang::trying`, which says what a read of a pair is.

use clap::Args as ClapArgs;
use whizbang::wiring::Uses;
use whizbang_core::artifact::ArtifactId;
use whizbang_core::identity::Digest;

use crate::rascal::{Under, executor_caching};
use crate::store::Where;

#[derive(ClapArgs)]
pub struct Args {
    /// The tool: change_map, edge_similarity, or a tool file the config lists under
    /// `[tools] paths` whose images are source and candidate.
    tool: String,
    /// An argument of a tool file, as NAME=VALUE: a text, or a number for one it declares
    /// a number. Give it again for more.
    #[arg(long = "arg", value_name = "NAME=VALUE")]
    arguments: Vec<String>,
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
    let source = identity("source", &args.source)?;
    let candidate = identity("candidate", &args.candidate)?;
    let recovery = whizbang::wiring::recovery();
    let (executor, over) = executor_caching(&args.at, &args.under, recovery, None, Uses::Nothing)?;
    let files = over.tool_files()?;
    let declared = whizbang::trying::declared_in(&args.tool, files.as_deref())?;
    let arguments = whizbang::trying::arguments_of(&declared, files.as_deref(), &args.arguments)?;
    let root = args.at.root().to_path_buf();
    let tried = whizbang::trying::read_pair_with(
        &executor, &root, &declared, source, candidate, &arguments,
    )?;
    whizbang::trying::tried_text(&declared, &tried)
}
