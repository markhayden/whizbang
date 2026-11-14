//! The `whizbang` binary: argument parsing and wiring only.

mod import;
mod rascal;
mod runs;
mod spend;
mod store;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use whizbang::{door, wiring};

#[derive(Parser)]
#[command(
    name = "whizbang",
    version,
    about = "One toolkit for working on images with models"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// What this build can do. Identical on every machine for the same build.
    Describe {
        /// Print the description as JSON, on one line: the form a program reads.
        #[arg(long)]
        json: bool,
        /// With --json: indent it, one field per line. The reference holds this form.
        #[arg(long, requires = "json")]
        pretty: bool,
    },
    /// The store: what it holds, removing runs and what only they reference, and
    /// bringing an artifact file into it.
    Store {
        #[command(subcommand)]
        command: store::Command,
    },
    /// Runs: finding one, and reading its record and what it kept.
    Runs {
        #[command(subcommand)]
        command: runs::Command,
    },
    /// Rascal: its operations on masks, each run as an action against a store.
    Rascal {
        #[command(subcommand)]
        command: rascal::Command,
    },
    /// Spend: a store's allowance. Not built yet: it says so and exits 1.
    Spend,
}

fn describe(json: bool, pretty: bool) -> Result<String, String> {
    let registry = wiring::registry()?;
    let description = wiring::description(&registry);
    if json {
        let document = if pretty {
            serde_json::to_string_pretty(&description)
        } else {
            serde_json::to_string(&description)
        };
        return match document {
            Ok(document) => Ok(format!("{document}\n")),
            Err(error) => Err(error.to_string()),
        };
    }
    Ok(wiring::describe_text(&description))
}

/// Writes a command's output. A reader that has stopped reading, as `head` does once it
/// has its lines, is not a failure of the command: nothing is said and the exit is 0.
///
/// A write that fails is exit 3 with the reason, and a test shows it
/// (`describe::a_write_that_fails_is_exit_3_with_the_reason`). No test can tell the
/// `flush` below from its absence: every output this writes ends in a new line, and the
/// standard output is flushed at each new line, so a write that fails has failed inside
/// `write_all`, and by the time `flush` is called nothing is left for it to write. It
/// stays for an output that some day does not end in one.
fn emit(bytes: &[u8]) -> ExitCode {
    use std::io::Write;
    let mut out = std::io::stdout();
    match out.write_all(bytes).and_then(|()| out.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("whizbang: could not write: {error}");
            ExitCode::from(3)
        }
    }
}

/// Prints a command's complaint, and gives the exit code 1. Every complaint goes
/// through here, and so through one escape: a store's error names files by their
/// paths, and a path is whatever the caller typed or a folder held.
fn complain(complaint: &str) -> ExitCode {
    eprintln!("whizbang: {}", door::printable(complaint));
    ExitCode::from(1)
}

/// What the parser of the command line answers by itself: help and the version, which
/// are printed as it prints them, and a usage error, which quotes what the caller
/// typed. A usage error is printed line by line through the door's escape, so that no
/// control character in it reaches a terminal; the line breaks are the parser's own,
/// and one a caller typed inside an argument stays a line break, which a terminal does
/// not obey as it obeys a carriage return or an escape sequence. It is printed without
/// colour. The exit codes are the parser's: 0 for help and the version, 2 for an error.
fn answered_by_the_parser(error: &clap::Error) -> ExitCode {
    if !error.use_stderr() {
        // Help and the version hold nothing a caller typed.
        let _ = error.print();
        return ExitCode::SUCCESS;
    }
    let said = error.render().to_string();
    for line in said.lines() {
        eprintln!("{}", door::printable(line));
    }
    ExitCode::from(2)
}

fn main() -> ExitCode {
    door::quiet_panics();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return answered_by_the_parser(&error),
    };
    match cli.command {
        Command::Describe { json, pretty } => match describe(json, pretty) {
            Ok(text) => emit(text.as_bytes()),
            Err(error) => {
                eprintln!("whizbang: could not write the description: {error}");
                ExitCode::from(3)
            }
        },
        Command::Store { command } => match store::run(&command) {
            Ok(text) => emit(text.as_bytes()),
            Err(complaint) => complain(&complaint),
        },
        Command::Runs { command } => match runs::run(&command) {
            Ok(bytes) => emit(&bytes),
            Err(complaint) => complain(&complaint),
        },
        Command::Rascal { command } => match rascal::run(&command) {
            Ok(text) => emit(text.as_bytes()),
            Err(complaint) => complain(&complaint),
        },
        Command::Spend => match spend::run() {
            Ok(text) => emit(text.as_bytes()),
            Err(complaint) => complain(&complaint),
        },
    }
}
