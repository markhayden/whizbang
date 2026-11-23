//! `whizbang serve` (`SPEC.md` section 11; plan 2C, task M1; ADR 0031): the MCP server
//! over stdio, for a local agent. Its executor is built as `squint check` builds one,
//! over the config, its env file and the models directory, with every backend the
//! config offers in use: a config that names a credential is not served without its
//! env file. Nothing but the protocol is written to the standard output; a complaint
//! before the server starts goes to the error output, as every command's does.
//!
//! `whizbang serve --http <addr>` (plan m6, task H1; ADR 0045) is the engine API: the same
//! tools over HTTP, for a hosted app, with plain routes for pictures and runs. It holds the
//! store as its one writer until a signal stops it (`SIGINT`, `SIGTERM`, `SIGHUP`, taken as
//! `whizbang play` takes them), writes one line to the error output when it starts (where
//! it listens, its host names, whether off loopback was stated; never the token), and
//! one line to the standard output when it stops.

use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use clap::Args as ClapArgs;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use whizbang::engining::Served;
use whizbang::serving::Actions;
use whizbang::wiring::{self, Make, Platform, Uses};
use whizbang_core::contract::Recovery;
use whizbang_serve::engine::{self, Host, Secret, Settings, Stopper};

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
    /// Serve the engine API over HTTP at this address, for a hosted app, instead of MCP
    /// over stdio. A loopback address needs nothing more; any other needs --off-loopback
    /// and a --host-name, behind a reverse proxy that ends TLS.
    #[arg(long, value_name = "ADDR")]
    http: Option<SocketAddr>,
    /// With --http off loopback: the statement that the server stands behind a reverse
    /// proxy that ends TLS, which it cannot see.
    #[arg(long, requires = "http")]
    off_loopback: bool,
    /// With --http: a name the server answers to in a request's Host, beside loopback's.
    /// May be given more than once.
    #[arg(long = "host-name", value_name = "NAME", requires = "http")]
    host_name: Vec<String>,
    /// With --http: an Origin a request may name; none by default. May be given more
    /// than once.
    #[arg(long = "allow-origin", value_name = "ORIGIN", requires = "http")]
    allow_origin: Vec<String>,
    /// With --http: the variable that holds the engine's token, at least 32 characters.
    #[arg(long, value_name = "NAME", requires = "http")]
    token_env: Option<String>,
    /// With --http: the file that holds the engine's token, its owner's alone.
    #[arg(long, value_name = "FILE", requires = "http")]
    token_file: Option<PathBuf>,
    /// With --http: the most runs started here that go at once (4 by default).
    #[arg(long, value_name = "N", requires = "http",
          value_parser = clap::value_parser!(u64).range(1..=16))]
    calls: Option<u64>,
    /// With --http: the most pixels of a picture or a mask that arrives (25000000 by
    /// default).
    #[arg(long, value_name = "PIXELS", requires = "http",
          value_parser = clap::value_parser!(u64).range(1..=25_000_000))]
    pixel_limit: Option<u64>,
    /// With --http: how long an upload that no run took is kept, in hours (24 by
    /// default). The hourly collection that removes one comes with plan m6, task D1.
    #[arg(long, value_name = "HOURS", requires = "http",
          value_parser = clap::value_parser!(u64).range(1..=8_760))]
    uploads_kept_h: Option<u64>,
}

/// How the binary reads one variable of its environment: `main`'s, handed on.
pub type Variable = fn(&str) -> Option<OsString>;

/// Serves one client on stdio until its input ends, or the engine API until a signal
/// stops it: what to print then, or the complaint.
pub fn run(args: &Args, platform: Platform, variable: Variable) -> Result<String, String> {
    match args.http {
        None => run_at(args, wiring::recovery(), Some(platform)),
        Some(address) => http(args, address, platform, variable),
    }
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

/// The engine's token, from exactly one of the two places, held to its rules. Nothing of
/// it is ever said.
fn token(args: &Args, variable: Variable) -> Result<Secret, String> {
    match (&args.token_env, &args.token_file) {
        (Some(name), None) => {
            let value = variable(name)
                .ok_or_else(|| "the variable --token-env names is not set".to_owned())?;
            let value = value
                .into_string()
                .map_err(|_| "the token holds only printable ASCII, with no spaces".to_owned())?;
            Secret::new(&value)
        }
        (None, Some(file)) => Secret::new(&token_file(file)?),
        _ => Err(
            "--http needs the engine's token from exactly one of --token-env and --token-file"
                .to_owned(),
        ),
    }
}

/// The token file's text: a regular file of this user's, that neither its group nor
/// anyone may read or write, of at most 4 KiB.
fn token_file(file: &Path) -> Result<String, String> {
    use std::io::Read;
    let opened = std::fs::File::open(file)
        .map_err(|error| format!("the token file cannot be opened: {}", error.kind()))?;
    let found = opened
        .metadata()
        .map_err(|error| format!("the token file cannot be read: {}", error.kind()))?;
    if !found.is_file() {
        return Err("the token file is no regular file".to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let user = whizbang_core::store::fs::this_user()
            .map_err(|_| "this process cannot learn whose it is".to_owned())?;
        if found.uid() != user {
            return Err("the token file is another user's".to_owned());
        }
        if found.mode() & 0o077 != 0 {
            return Err(
                "the token file may be read or written by its group or anyone: make it its \
                 owner's alone (chmod 600)"
                    .to_owned(),
            );
        }
    }
    let mut text = String::new();
    opened
        .take(4096)
        .read_to_string(&mut text)
        .map_err(|_| "the token holds only printable ASCII, with no spaces".to_owned())?;
    Ok(text)
}

fn http(
    args: &Args,
    address: SocketAddr,
    platform: Platform,
    variable: Variable,
) -> Result<String, String> {
    let token = token(args, variable)?;
    let off_loopback = !address.ip().is_loopback();
    if off_loopback && (!args.off_loopback || args.host_name.is_empty()) {
        return Err(format!(
            "{address} is not a loopback address: give --off-loopback and at least one \
             --host-name, and stand the server behind a reverse proxy that ends TLS"
        ));
    }
    let root = args.at.root();
    let over = args.under.over(Some(platform), Uses::Every)?;
    let cached = over.decisions_cached()?;
    // The store is made where there is none, its last folder alone, as the playground
    // makes one: an app's server may start on a new folder.
    let opening = wiring::opening(root, Make::TheRoot, wiring::recovery())
        .map_err(|error| wiring::not_opened(&error))?;
    let store = opening.writer().map_err(|why| why.to_string())?;
    let registry = wiring::registry_over(&over, cached)?;
    let descriptors = registry.descriptors().cloned().collect();
    let worker = over.worker().cloned();
    let executor = wiring::runner(registry, store.clone(), &over)?;
    let door = Served::new(Actions::new(descriptors, root.to_path_buf(), worker), store);
    let mut settings = Settings::new(address);
    settings.host_names = args.host_name.clone();
    settings.allow_origins = args.allow_origin.clone();
    settings.wait = Duration::from_secs(args.wait_s);
    if let Some(calls) = args.calls {
        settings.calls = usize::try_from(calls).unwrap_or(1);
    }
    if let Some(pixels) = args.pixel_limit {
        settings.pixel_limit = pixels;
    }
    let host = Host::start(Arc::new(executor), Arc::new(door), settings, token)
        .map_err(|error| format!("the engine API could not listen at {address}: {error}"))?;
    let names = match args.host_name.is_empty() {
        true => "none beside loopback's".to_owned(),
        false => args.host_name.join(", "),
    };
    let stated = match (off_loopback, args.off_loopback) {
        (true, _) => "off loopback, as stated: behind a reverse proxy that ends TLS",
        (false, true) => "on loopback, --off-loopback given",
        (false, false) => "on loopback",
    };
    eprintln!(
        "whizbang serve --http: listening on {}; host names: {}; {stated}",
        host.address(),
        whizbang::door::printable(&names)
    );
    if let Err(error) = signals(&host.stopper()) {
        let stopper = host.stopper();
        stopper.quit();
        stopper.quit();
        host.wait();
        return Err(format!("the engine API could not take signals: {error}"));
    }
    let stopped = host.wait();
    Ok(match stopped.left {
        0 => "the engine API stopped; every run it started has ended\n".to_owned(),
        left => format!(
            "the engine API stopped; {left} run(s) it started had not ended, and will be \
             ended as interrupted when the store is next opened to write\n"
        ),
    })
}

/// The terminal's signals, each a stop: the first starts it, a second ends its grace.
pub(crate) fn signals(stopper: &Stopper) -> std::io::Result<()> {
    let mut signals = Signals::new([SIGINT, SIGTERM, SIGHUP])?;
    let stopper = stopper.clone();
    engine::threads::spawn(
        "signals",
        Box::new(move || {
            for _ in signals.forever() {
                stopper.quit();
            }
        }),
    )?;
    Ok(())
}
