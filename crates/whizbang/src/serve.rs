//! `whizbang serve` (`SPEC.md` section 11; plan 2C, task M1; ADR 0031): the MCP server
//! over stdio, for a local agent. Its executor is built as `squint check` builds one,
//! over the config, its env file and the models directory, with every backend the
//! config offers in use: a config that names a credential is not served without its
//! env file. Nothing but the protocol is written to the standard output; a complaint
//! before the server starts goes to the error output, as every command's does.

use std::sync::Arc;
use std::time::Duration;

use clap::Args as ClapArgs;
use whizbang::serving::Actions;
use whizbang::wiring::{self, Platform, Uses};
use whizbang_core::contract::Recovery;

use crate::rascal::{Under, executor_caching};
use crate::store::Where;

#[derive(ClapArgs)]
pub struct Args {
    #[command(flatten)]
    at: Where,
    #[command(flatten)]
    under: Under,
    /// The stated time of a run, in seconds: a run still going at it is cancelled, and
    /// its call is answered that the server gave up on it.
    #[arg(long, value_name = "SECONDS", default_value_t = 900,
          value_parser = clap::value_parser!(u64).range(1..=86_400))]
    wait_s: u64,
}

/// Serves one client on stdio until its input ends: nothing to print on success, or
/// the complaint.
pub fn run(args: &Args, platform: Platform) -> Result<String, String> {
    run_at(args, wiring::recovery(), Some(platform))
}

fn run_at(args: &Args, recovery: Recovery, platform: Option<Platform>) -> Result<String, String> {
    let (executor, over) =
        executor_caching(&args.at, &args.under, recovery, platform, Uses::Every)?;
    let registry = wiring::registry()?;
    let descriptors = registry.descriptors().cloned().collect();
    let door = Actions::new(
        descriptors,
        args.at.root().to_path_buf(),
        over.worker().cloned(),
    );
    let limits = whizbang_serve::Limits {
        wait: Duration::from_secs(args.wait_s),
        ..whizbang_serve::Limits::default()
    };
    let served = whizbang_serve::serve(
        Arc::new(executor),
        Arc::new(door),
        limits,
        std::io::stdin(),
        std::io::stdout(),
    );
    match served {
        Ok(()) => Ok(String::new()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(String::new()),
        Err(error) => Err(format!("the server stopped: {error}")),
    }
}
