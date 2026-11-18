//! `whizbang squint check <use-case>` (`SPEC.md` section 11; plan 2A, task S2): wiring
//! from arguments to the door (`whizbang::door::check`), which runs `check` through the
//! executor under the use case and says its verdict.

use clap::Subcommand;
use whizbang::door::{self, CheckAsked};
use whizbang::wiring::{Platform, Uses};
use whizbang_core::contract::{Capability, Recovery};

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
        /// What is measured before the judge looks. `first`: align registers the
        /// candidate onto the source, and the aligned picture, what it found and every
        /// measurement open the conversation, each labelled with the tool and its mark;
        /// the judge may still call view, align and resize. `none`: nothing is measured
        /// first, and the judge is told of view and align and calls them, as squint's
        /// baseline arm C was (with `--projection visual_only`).
        #[arg(long, value_name = "WHEN", default_value = "first", value_parser = ["first", "none"])]
        details: String,
        /// The tools whose details are measured first, by name, separated by commas:
        /// align, so far.
        #[arg(long, value_name = "TOOLS")]
        detail: Option<String>,
        /// How a call of align by the judge is answered: `full`, everything it found,
        /// or `visual_only`, its pictures and a fixed sentence, as squint's baseline arm C.
        #[arg(long, value_name = "WHICH", default_value = "full", value_parser = ["full", "visual_only"])]
        projection: String,
        /// An id for this request, of your choosing: 1 to 128 letters, digits, `_`, `-`,
        /// `.` and `:`. The same request sent again under it is answered with the run
        /// it made the first time, and nothing runs again.
        #[arg(long, value_name = "ID")]
        request_id: Option<String>,
    },
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
            details,
            detail,
            projection,
            at,
            under,
            request_id,
        } => {
            let uses = Uses::One(Capability::Judge, judge.as_deref());
            let executor = executor(at, under, recovery, platform, uses)?;
            let asked = CheckAsked {
                use_case,
                source,
                candidate,
                judge: judge.as_deref(),
                brief: brief.as_deref(),
                request_id: request_id.as_deref(),
                details: if details == "none" {
                    door::Details::None
                } else {
                    door::Details::First
                },
                detail: detail.as_deref(),
                projection,
            };
            let root = at.root().to_path_buf();
            door::check(&executor, &asked, &|run| door::verdict(&root, run))
        }
    }
}
