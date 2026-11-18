//! `whizbang squint check <use-case>` (`SPEC.md` section 11; plan 2A, task S2): wiring
//! from arguments to the door (`whizbang::door::check`), which runs `check` through the
//! executor under the use case and says its verdict.

use clap::Subcommand;
use whizbang::door::{self, CheckAsked};
use whizbang::wiring::Platform;
use whizbang_core::config::Config;
use whizbang_core::contract::{Recovery, Secrets};

use crate::rascal::{Under, executor};
use crate::store::Where;

const EXIT_CODES: &str = "Exit codes:
  0  the run completed: its verdict, pass, fail or unsure, is printed with its reason
  1  nothing ran, or the run did not complete: the complaint says which, and names the run
  2  the command line was not understood
  3  what was to be printed could not be written
  101  the command itself failed (a panic outside an action): one fixed line is printed";

#[derive(Subcommand)]
pub enum Command {
    /// Have the configured judge check an edit of an image against its source, under
    /// a use case: `source-fidelity`, whether the edit kept what mattered in the
    /// source. The judge disposes of each item of the use case's checklist; the
    /// verdict is worked out from those, and printed with its reason and the run.
    #[command(after_help = EXIT_CODES)]
    Check {
        /// The use case: source-fidelity.
        #[arg(value_parser = [whizbang_squint::source_fidelity::NAME])]
        use_case: String,
        /// The source image, by its identity in the store: sha256: and 64 hex digits,
        /// as `whizbang store import` prints it.
        #[arg(long)]
        source: String,
        /// The edit of it, by its identity in the store.
        #[arg(long)]
        candidate: String,
        /// The judge, by its name in the config's `[backends.judge]` tables. With
        /// none, the config's default judge answers.
        #[arg(long, value_name = "NAME")]
        judge: Option<String>,
        /// What the edit was to do and what it had to keep, in words, such as "change
        /// only the sky; the parrot's tail keeps its colour". The judge holds the edit
        /// to it: what it names as kept and is not is a failure. With none, nothing is
        /// named as kept, and a change is noted but does not fail the check.
        #[arg(long, value_name = "TEXT")]
        brief: Option<String>,
        #[command(flatten)]
        at: Where,
        #[command(flatten)]
        under: Under,
        /// An id for this request, of your choosing: 1 to 128 letters, digits, `_`, `-`,
        /// `.` and `:`. The same request sent again under it is answered with the run
        /// it made the first time, and nothing runs again.
        #[arg(long, value_name = "ID")]
        request_id: Option<String>,
    },
}

/// Whether a text is a variable's name as a person writes one: an upper-case letter,
/// then upper-case letters, digits and `_`, 64 characters at most. A complaint names a
/// variable only so: a key written where its name belongs is never said back.
fn plainly_a_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_uppercase())
        && text.len() <= 64
        && bytes.all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// The complaint of a command whose config names a variable for a credential and that
/// was given no env file: before anything is read but the config, naming each variable
/// that is plainly a name.
fn no_env_file(under: &Under) -> Option<String> {
    let (Some(config), None) = (under.asked().config, under.asked().env_file) else {
        return None;
    };
    let config = Config::read(config, &Secrets::none()).ok()?;
    let variables = config.variables();
    if variables.is_empty() {
        return None;
    }
    let named: Vec<&str> = variables
        .iter()
        .copied()
        .filter(|variable| plainly_a_name(variable))
        .collect();
    let which = if named.len() == variables.len() {
        named.join(", ")
    } else {
        "a variable that is not shown".to_owned()
    };
    Some(format!(
        "the config names {which} for a credential, and no env file was given: name the \
         env file that sets it with --env-file; nothing was read but the config and \
         nothing ran"
    ))
}

/// Runs a squint command: what to print on success, or the complaint.
pub fn run(command: &Command, platform: Platform) -> Result<String, String> {
    run_at(command, whizbang::wiring::recovery(), Some(platform))
}

fn run_at(
    command: &Command,
    recovery: Recovery,
    platform: Option<Platform>,
) -> Result<String, String> {
    match command {
        Command::Check {
            use_case,
            source,
            candidate,
            judge,
            brief,
            at,
            under,
            request_id,
        } => {
            if let Some(complaint) = no_env_file(under) {
                return Err(complaint);
            }
            let executor = executor(at, under, recovery, platform)?;
            let asked = CheckAsked {
                use_case,
                source,
                candidate,
                judge: judge.as_deref(),
                brief: brief.as_deref(),
                request_id: request_id.as_deref(),
            };
            let root = at.root().to_path_buf();
            door::check(&executor, &asked, &|run| door::verdict(&root, run))
        }
    }
}
