//! The `whizbang` binary: argument parsing and wiring only.

mod import;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use whizbang_core::description::{ActionSummary, Description};

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
    /// The store: bringing an artifact file into it.
    Store {
        #[command(subcommand)]
        command: import::Command,
    },
}

/// Every action this build registers. Tools add theirs here as they are ported.
fn registered_actions() -> Vec<ActionSummary> {
    Vec::new()
}

fn describe(json: bool, pretty: bool) -> Result<String, serde_json::Error> {
    let description = Description::new(env!("CARGO_PKG_VERSION"), registered_actions());
    if json {
        let document = if pretty {
            serde_json::to_string_pretty(&description)?
        } else {
            serde_json::to_string(&description)?
        };
        return Ok(format!("{document}\n"));
    }
    let mut text = format!("{} {}\n", description.name, description.version);
    if description.actions.is_empty() {
        text.push_str("no actions yet\n");
    }
    for action in &description.actions {
        text.push_str(&format!("  {}  {}\n", action.name, action.purpose));
    }
    Ok(text)
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
        Command::Store { command } => match import::run(&command) {
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
