//! The `whizbang` binary: argument parsing and wiring only.

mod backends;
mod eval;
mod export;
mod lab;
mod models;
mod play;
mod rascal;
#[cfg(test)]
mod recovering;
mod runs;
mod serve;
mod setup;
mod spend;
#[cfg(test)]
mod spending;
mod squint;
mod stats;
mod store;
mod tool_try;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use whizbang::{door, wiring};

/// What `whizbang --version` prints after the name: the version, and the commit the
/// binary was built from, which `build.rs` asks git for (`unknown` where it could not).
/// A release is held to both (plan m4, tasks R1 and R2). The commit is in no run record
/// and not in `whizbang describe`, which stays the same for every build of a version.
const VERSION_AND_COMMIT: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (commit ",
    env!("WHIZBANG_COMMIT"),
    ")"
);

/// The commit alone, as `eval judges` records it beside what it scored (plan e1, task S1).
const COMMIT: &str = env!("WHIZBANG_COMMIT");

#[derive(Parser)]
#[command(
    name = "whizbang",
    version = VERSION_AND_COMMIT,
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
    /// Squint: having a judge check images under a use case, as an action against a
    /// store.
    Squint {
        #[command(subcommand)]
        command: squint::Command,
    },
    /// Evaluation: the evaluation sets, their pictures, and matrices of runs of
    /// `squint check` over them.
    Eval {
        #[command(subcommand)]
        command: eval::Command,
    },
    /// The lab: its corpus of pictures checked against the manifest that pins them, a
    /// round of a recipe and filler matrix run and resumed, and its picks decoded.
    Lab {
        #[command(subcommand)]
        command: lab::Command,
    },
    /// Spend: a store's allowance, what is counted against it, and the owner's
    /// reset, which sets the allowance and clears the books.
    Spend {
        #[command(subcommand)]
        command: spend::Command,
    },
    /// Models: what the catalogue holds and what is installed, and getting and
    /// removing one.
    Models {
        #[command(subcommand)]
        command: models::Command,
    },
    /// Backends: whether each one the config offers is ready, and probing them.
    Backends {
        #[command(subcommand)]
        command: backends::Command,
    },
    /// Setup: installing the Python pack from the lock file this build carries, and
    /// checking that its worker says hello.
    Setup(setup::Args),
    /// Try a tool on one pair, with no judge: the candidate registered onto the source by
    /// align, then the tool on the source and the aligned frame. Prints the two runs and
    /// what the tool measured.
    #[command(name = "try")]
    Try(tool_try::Args),
    /// Serve: the MCP server over stdio, for a local agent. Each action is a tool; a
    /// call runs through admission, spend and the egress gate as the command line's
    /// do. It ends when its input does.
    Serve(serve::Args),
    /// Play: the playground, a page in Chrome served by this process on 127.0.0.1. A
    /// run started there runs through admission, spend and the egress gate as the
    /// command line's do, and goes on when the tab is closed. It holds the store until
    /// the page's Quit or Ctrl-C, which cancel and account for the runs going.
    Play(play::Args),
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

/// The same, for a complaint whose own line breaks are lines to print: the models
/// commands', whose complaints hold no path and nothing a caller typed, and which may
/// end with lines of a config to add. Each line goes through the escape by itself, so
/// a character that is not printable is still escaped; the first says who speaks.
fn complain_in_lines(complaint: &str) -> ExitCode {
    for (at, line) in complaint.lines().enumerate() {
        let speaker = if at == 0 { "whizbang: " } else { "" };
        eprintln!("{speaker}{}", door::printable(line));
    }
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

/// The models directory in this machine's data folder for whizbang, from the
/// environment of this process. **This is the one place the binary reads it,** and
/// `main` the one place it is named (`wired::only_main_reads_where_the_platforms_data_folder_is`):
/// every other function is given what says the place, so a test gives its own or
/// none, and none can be pointed at the data folder of whoever runs it. `main` does
/// not call it: it hands it on, and it is called only if a models directory is to
/// be opened and neither a flag nor a config named one.
fn platforms_models_dir() -> Option<std::path::PathBuf> {
    wiring::platform_models_dir(
        cfg!(target_os = "macos"),
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("XDG_DATA_HOME").as_deref(),
    )
}

/// Where the Hub's command-line sign-in keeps its token, from the environment of this
/// process: the variables `HF_TOKEN_PATH`, `HF_HOME`, `XDG_CACHE_HOME` and `HOME`, as
/// the Hub's documentation has them (`whizbang::sign_in::token_file`; plan 1C, D12).
/// **This is the one function that names the real place,** and `main` the one place
/// that names it (`just check` refuses its name anywhere else:
/// `scripts/check-sign-in.sh`). `main` does not call it: it hands it on, and it is
/// called only by `get` of a model whose download needs the sign-in's token. The
/// variables that hold a token itself, which the Hub's own tools read in place of the
/// file, are not read.
fn hub_sign_in_token_file() -> Option<std::path::PathBuf> {
    whizbang::sign_in::token_file(
        std::env::var_os("HF_TOKEN_PATH").as_deref(),
        std::env::var_os("HF_HOME").as_deref(),
        std::env::var_os("XDG_CACHE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
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
        Command::Rascal { command } => {
            match rascal::run(&command, std::sync::Arc::new(platforms_models_dir)) {
                Ok(text) => emit(text.as_bytes()),
                Err(complaint) => complain(&complaint),
            }
        }
        Command::Squint { command } => {
            match squint::run(&command, std::sync::Arc::new(platforms_models_dir)) {
                Ok(text) => emit(text.as_bytes()),
                Err(complaint) => complain(&complaint),
            }
        }
        Command::Try(args) => match tool_try::run(&args) {
            Ok(text) => emit(text.as_bytes()),
            Err(complaint) => complain(&complaint),
        },
        Command::Eval { command } => match eval::run(&command) {
            Ok(text) => emit(text.as_bytes()),
            Err(complaint) => complain(&complaint),
        },
        // A check that finds something wrong prints its whole report, then fails.
        Command::Lab { command } => {
            match lab::run(&command, std::sync::Arc::new(platforms_models_dir)) {
                Ok((text, None)) => emit(text.as_bytes()),
                Ok((text, Some(complaint))) => {
                    let _ = emit(text.as_bytes());
                    complain(&complaint)
                }
                Err(complaint) => complain(&complaint),
            }
        }
        Command::Spend { command } => match spend::run(&command) {
            Ok(text) => emit(text.as_bytes()),
            Err(complaint) => complain(&complaint),
        },
        Command::Models { command } => {
            match models::run(
                &command,
                std::sync::Arc::new(platforms_models_dir),
                std::sync::Arc::new(hub_sign_in_token_file),
            ) {
                Ok(text) => emit(text.as_bytes()),
                Err(complaint) => complain_in_lines(&complaint),
            }
        }
        Command::Backends { command } => {
            match backends::run(&command, std::sync::Arc::new(platforms_models_dir)) {
                Ok(text) => emit(text.as_bytes()),
                Err(complaint) => complain_in_lines(&complaint),
            }
        }
        // Setup reads two things more of this process's environment: `PATH`, where
        // `uv` is looked for when `--uv` names none, and the variables `uv`'s own
        // environment is chosen from (the locale, and where `uv` keeps its Pythons).
        Command::Serve(args) => {
            match serve::run(&args, std::sync::Arc::new(platforms_models_dir)) {
                Ok(text) => emit(text.as_bytes()),
                Err(complaint) => complain_in_lines(&complaint),
            }
        }
        Command::Play(args) => match play::run(&args, std::sync::Arc::new(platforms_models_dir)) {
            Ok(text) => emit(text.as_bytes()),
            Err(complaint) => complain_in_lines(&complaint),
        },
        Command::Setup(args) => match setup::run(
            &args,
            std::sync::Arc::new(platforms_models_dir),
            std::env::var_os("PATH"),
            std::env::vars_os().collect(),
        ) {
            Ok(text) => emit(text.as_bytes()),
            Err(complaint) => complain_in_lines(&complaint),
        },
    }
}
