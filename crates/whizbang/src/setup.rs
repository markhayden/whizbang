//! `whizbang setup`: wiring from arguments to the binary's own function
//! (`whizbang::pack_setup`), which says what it installs, where, and what it starts (plan
//! 1C, task Y1; ADR 0021; `docs/guides/setup.md`).

use std::ffi::OsString;
use std::path::PathBuf;

use whizbang::pack_setup::{self, Asked, Machine};
use whizbang::wiring::Platform;

use crate::rascal::Under;

const EXIT_CODES: &str = "Exit codes:
  0  done: the pack is installed and the worker it made said hello
  1  it was not done: the complaint says why
  2  the command line was not understood
  3  what was to be printed could not be written";

/// Install the Python pack: a locked install by uv of the lock file this build carries,
/// into the folder `pack` of the models directory, and a check that the worker it made
/// says hello. With no extra it installs no package and contacts nothing; with one it
/// downloads that extra's packages from the package index the lock names.
#[derive(clap::Args)]
#[command(after_help = EXIT_CODES)]
pub struct Args {
    /// The extra to install with the pack's core, by its name in the pack's file:
    /// `torch`, the runtime local models run on. Downloads its packages.
    #[arg(long, value_name = "NAME")]
    extra: Option<String>,
    /// The uv to run, an absolute path. With none it is the first on PATH. It must be
    /// the version scripts/tool-versions.txt pins.
    #[arg(long, value_name = "FILE")]
    uv: Option<PathBuf>,
    /// The Python to make the environment with, an absolute path. With none, uv finds
    /// the version the pack pins among the Pythons it manages, and downloads none.
    #[arg(long, value_name = "FILE")]
    python: Option<PathBuf>,
    #[command(flatten)]
    under: Under,
}

/// Runs `whizbang setup`. `platform` says where the models directory in this
/// machine's data folder is, asked only when neither the flag nor the config names
/// one; `path` is this process's `PATH`, where `uv` is looked for when `--uv` names
/// none, and `parent` its environment, which `uv`'s is chosen from. `main` reads both.
pub fn run(
    args: &Args,
    platform: Platform,
    path: Option<OsString>,
    parent: Vec<(OsString, OsString)>,
) -> Result<String, String> {
    let asked = args.under.asked();
    pack_setup::run(
        Asked {
            config: asked.config,
            env_file: asked.env_file,
            models_dir: args.under.models_dir(),
            extra: args.extra.as_deref(),
            uv: args.uv.as_deref(),
            python: args.python.as_deref(),
        },
        Machine {
            platform,
            path,
            parent,
        },
    )
}
