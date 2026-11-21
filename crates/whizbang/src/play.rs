//! `whizbang play` (`SPEC.md` sections 1, 3 and 7; plan m5, task H1; ADR 0041): the
//! playground, a page in Chrome served by this process over loopback.
//!
//! It builds its executor as `whizbang serve` does, over the config, its env file and
//! the models directory, with one difference: it asks for no backend at start
//! (`Uses::Nothing`), so a config that names a key and no env file opens, and a backend
//! whose key is missing is not ready, which the page says. It holds the store to write
//! until it stops, writes `play.json` in it, opens the page in Chrome, and serves until
//! the page's Quit, a signal from the terminal (`SIGINT`, `SIGTERM`, `SIGHUP`), or, under
//! `--stdin-quit`, the end of its input. **This is the one command that handles a
//! signal:** through `signal-hook`'s safe iterator, on a counted thread of the host's.
//! No other command changes how it ends.

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use clap::Args as ClapArgs;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use whizbang::playing::{self, Hosting, Playground};
use whizbang::wiring::{self, Make, Platform, Uses};
use whizbang_serve::play::{Host, PORT_DEFAULT, Settings, Stopper, Token, threads};

use crate::rascal::Under;
use crate::store::Where;

#[derive(ClapArgs)]
pub struct Args {
    #[command(flatten)]
    at: Where,
    #[command(flatten)]
    under: Under,
    /// The port on 127.0.0.1 the playground listens on. It stays the same from one
    /// launch to the next, so the page keeps its drafts; 0 asks the system for a free
    /// one. Nothing listens on any other address.
    #[arg(long, value_name = "PORT", default_value_t = PORT_DEFAULT)]
    port: u16,
    /// Print the address to open, with its token, instead of opening Chrome.
    #[arg(long)]
    no_open: bool,
    /// Stop when the standard input ends, as the page's Quit does: for a script or a
    /// test that starts the playground.
    #[arg(long)]
    stdin_quit: bool,
    /// The stated time of a run, in seconds: a run still going at it is cancelled.
    #[arg(long, value_name = "SECONDS", default_value_t = 3600,
          value_parser = clap::value_parser!(u64).range(1..=604_800))]
    run_limit_s: u64,
}

/// Serves the playground until it is told to stop: what to print then, or the
/// complaint.
pub fn run(args: &Args, platform: Platform) -> Result<String, String> {
    let root = args.at.root();
    // A host that already serves this store is opened, not refused.
    if let Some(hosting) = playing::read_hosting(root)
        && playing::answers(&hosting)
    {
        let token = Token::parse(&hosting.token);
        hand_over(hosting.port, token.as_ref(), args.no_open);
        return Ok("the playground was already running for this store\n".to_owned());
    }
    // The store is made where there is none, its last folder alone, as `store import`
    // makes one: a person may start the playground on a new folder.
    let over = args.under.over(Some(platform), Uses::Nothing)?;
    let cached = over.decisions_cached()?;
    let opening = wiring::opening(root, Make::TheRoot, wiring::recovery())
        .map_err(|error| wiring::not_opened(&error))?;
    let store = opening.writer().map_err(|why| why.to_string())?;
    let registry = wiring::registry_over(&over, cached)?;
    let descriptors = registry.descriptors().cloned().collect();
    let executor = wiring::runner(registry, store, &over)?;
    let play = Arc::new(Playground::new(descriptors, Arc::new(over)));
    let token = Token::new().map_err(|error| error.to_string())?;
    let mut settings = Settings::new(root.to_path_buf());
    settings.port = args.port;
    settings.run_limit = Duration::from_secs(args.run_limit_s);
    let host = match Host::start(Arc::new(executor), play, settings, token.clone()) {
        Ok(host) => host,
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            return Err(format!(
                "port {} on 127.0.0.1 is in use: give another with --port",
                args.port
            ));
        }
        Err(error) => return Err(format!("the playground could not start: {error}")),
    };
    let port = host.address().port();
    let hosting = Hosting {
        port,
        process: std::process::id(),
        token: token.hex(),
    };
    let stopper = host.stopper();
    if let Err(error) = playing::write_hosting(root, &hosting) {
        stopper.quit();
        stopper.quit();
        host.wait();
        return Err(format!(
            "the playground could not write {}: {error}",
            playing::PLAY_FILE
        ));
    }
    let listening = signals(&stopper).and_then(|()| match args.stdin_quit {
        true => input_ends(&stopper),
        false => Ok(()),
    });
    if let Err(error) = listening {
        stopper.quit();
        stopper.quit();
        host.wait();
        playing::remove_hosting(root);
        return Err(format!("the playground could not start: {error}"));
    }
    hand_over(port, Some(&token), args.no_open);
    let stopped = host.wait();
    playing::remove_hosting(root);
    Ok(match stopped.left {
        0 => "the playground stopped; every run it started has ended\n".to_owned(),
        left => format!(
            "the playground stopped; {left} run(s) it started had not ended, and will be \
             ended as interrupted when the store is next opened to write\n"
        ),
    })
}

/// Opens the page in Chrome with its token, and prints the address without it; or,
/// with `--no-open` or where Chrome cannot be started, prints the address with it.
fn hand_over(port: u16, token: Option<&Token>, no_open: bool) {
    let with_token = playing::address(port, token);
    let mut out = std::io::stdout();
    if no_open || !open_in_chrome(&with_token) {
        let _ = writeln!(out, "{with_token}");
    } else {
        let _ = writeln!(out, "{}", playing::address(port, None));
    }
    let _ = out.flush();
}

/// Asks the system to open the address in Google Chrome: whether it was opened, as far
/// as this can tell. On the Mac `open` answers at once whether Chrome is there; on
/// Linux Chrome may stay attached to what started it, so it is waited for on a thread
/// of its own, and nothing is left behind it.
fn open_in_chrome(address: &str) -> bool {
    use std::process::{Command, Stdio};
    let quiet = |command: &mut Command| {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    };
    if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.args(["-a", "Google Chrome", address]);
        quiet(&mut command);
        return command.status().is_ok_and(|status| status.success());
    }
    let mut command = Command::new("google-chrome");
    command.arg(address);
    quiet(&mut command);
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    threads::spawn(
        "opener",
        Box::new(move || {
            let _ = child.wait();
        }),
    )
    .is_ok()
}

/// The terminal's signals, each a quit: the first starts it, a second ends its grace.
fn signals(stopper: &Stopper) -> std::io::Result<()> {
    let mut signals = Signals::new([SIGINT, SIGTERM, SIGHUP])?;
    let stopper = stopper.clone();
    threads::spawn(
        "signals",
        Box::new(move || {
            for _ in signals.forever() {
                stopper.quit();
            }
        }),
    )?;
    Ok(())
}

/// The end of the standard input, a quit.
fn input_ends(stopper: &Stopper) -> std::io::Result<()> {
    let stopper = stopper.clone();
    threads::spawn(
        "input",
        Box::new(move || {
            let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
            stopper.quit();
        }),
    )?;
    Ok(())
}
