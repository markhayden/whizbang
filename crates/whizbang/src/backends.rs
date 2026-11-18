//! `whizbang backends status [--probe]`: wiring from arguments to the binary's own
//! function (`whizbang::backend_status`), which says what it reads and what it starts
//! (plan 1C, task P7b; `docs/guides/backends.md`). It replaces the stub of task C1a.

use std::sync::Arc;

use clap::Subcommand;
use whizbang::backend_status::{self, Asked, NoProber};
use whizbang::wiring::{Platform, Uses};

use crate::rascal::Under;

const EXIT_CODES: &str = "Exit codes:
  0  the backends were listed, and probed when it was asked, whatever each came to
  1  the config could not be used, or the models directory could not be opened
  2  the command line was not understood
  3  what was to be printed could not be written";

#[derive(Subcommand)]
pub enum Command {
    /// List each backend the config offers: its capability, name, kind and model, and
    /// whether it is ready, apart by tabs. Nothing is started and no host is contacted.
    #[command(after_help = EXIT_CODES)]
    Status {
        /// Then probe each: start the pack's worker for a local backend and print its
        /// hello, and ask a remote one the config approves for probing whether it is
        /// reachable. This build has no network client, so no host is contacted.
        #[arg(long)]
        probe: bool,
        #[command(flatten)]
        under: Under,
    },
}

/// Runs a backends command: what to print on success, or the complaint. `platform`
/// says where the models directory in this machine's data folder is; it is `main`'s
/// reading of the environment, and is asked only if a models directory is to be
/// opened and nothing else named one. A remote backend is probed by nothing that
/// reaches a host: this build has none ([`NoProber`]).
pub fn run(command: &Command, platform: Platform) -> Result<String, String> {
    match command {
        Command::Status { probe, under } => {
            let over = under.over(Some(platform), Uses::Every)?;
            let asked = Asked { probe: *probe };
            backend_status::status(&over, asked, Arc::new(NoProber))
        }
    }
}
