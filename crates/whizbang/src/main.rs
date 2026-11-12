//! The `whizbang` binary: argument parsing and wiring only.

mod import;
mod runs;
mod store;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use whizbang::wiring;

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
}

fn describe(json: bool, pretty: bool) -> Result<String, String> {
    let registry = wiring::registry().map_err(|error| error.to_string())?;
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

fn main() -> ExitCode {
    let cli = Cli::parse();
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
            Err(complaint) => {
                eprintln!("whizbang: {complaint}");
                ExitCode::from(1)
            }
        },
        Command::Runs { command } => match runs::run(&command) {
            Ok(bytes) => emit(&bytes),
            Err(complaint) => {
                eprintln!("whizbang: {complaint}");
                ExitCode::from(1)
            }
        },
    }
}
