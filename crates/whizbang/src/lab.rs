//! `whizbang lab`: the lab's corpus (plan m3, task L1): wiring from arguments to
//! `whizbang::laboratory`, which says what each does.

use std::path::PathBuf;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum Command {
    /// The corpus: its pictures, outside the repository, checked against its manifest.
    Corpus {
        #[command(subcommand)]
        command: CorpusCommand,
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
pub fn run(command: &Command) -> Result<(String, Option<String>), String> {
    match command {
        Command::Corpus {
            command: CorpusCommand::Verify { manifest, root },
        } => {
            let (text, whole) = whizbang::laboratory::verify(manifest, root)?;
            let failed = (!whole).then(|| "some pictures are missing or changed".to_owned());
            Ok((text, failed))
        }
    }
}
