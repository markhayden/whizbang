//! The `whizbang` binary: argument parsing and wiring only.

mod import;
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

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Describe { json, pretty } => match describe(json, pretty) {
            Ok(text) => {
                print!("{text}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("whizbang: could not write the description: {error}");
                ExitCode::from(3)
            }
        },
        Command::Store { command } => match store::run(&command) {
            Ok(text) => {
                print!("{text}");
                ExitCode::SUCCESS
            }
            Err(complaint) => {
                eprintln!("whizbang: {complaint}");
                ExitCode::from(1)
            }
        },
    }
}
