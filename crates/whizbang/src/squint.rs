//! `whizbang squint check <use-case>` (`SPEC.md` section 11; plan 2A, task S2; plan 2C,
//! task A1): wiring from arguments to the door (`whizbang::door::check_job`), which runs
//! `check` through the executor under the use case and a job, and says its verdict.

use std::path::PathBuf;

use clap::Subcommand;
use whizbang::door::{self, CheckAsked};
use whizbang::wiring::{Platform, Uses};
use whizbang_core::contract::{Capability, Recovery};

use crate::rascal::{Under, executor_caching};
use crate::store::Where;

const EXIT_CODES: &str = "Exit codes:
  0  the run completed: its verdict, pass, fail or unsure, is printed with its reason
  1  nothing ran, or the run did not complete: the complaint says which, and names the run
  2  the command line was not understood
  3  what was to be printed could not be written
  101  the command itself failed (a panic outside an action): one fixed line is printed";

#[derive(Subcommand)]
pub enum Command {
    /// Have the configured judge check a picture under a use case: `source-fidelity`,
    /// whether an edit kept what mattered in its source; `reference-consistency`,
    /// whether a subject of the references appears faithfully; `reality-checks`,
    /// mistakes no real photograph would make; `brief-compliance`, whether it holds
    /// what the brief asked for and nothing it forbade; `product-fidelity`, whether a
    /// product shot keeps the exact product. The judge disposes of each item of the use
    /// case's checklist; the verdict is worked out from those, and printed with its
    /// reason and the run.
    #[command(after_help = EXIT_CODES)]
    Check {
        /// The use case.
        #[arg(value_parser = whizbang_squint::use_case::NAMES)]
        use_case: String,
        /// The picture that is judged, by its identity in the store: sha256: and 64 hex
        /// digits, as `whizbang store import` prints it.
        #[arg(long, required_unless_present = "job", conflicts_with = "job")]
        candidate: Option<String>,
        /// What the candidate was edited from, by its identity in the store: for
        /// source-fidelity.
        #[arg(long, conflicts_with = "job")]
        source: Option<String>,
        /// What the candidate is held to, by its identity in the store: for
        /// reference-consistency and product-fidelity. Up to 8, each labelled by its
        /// place: reference 1, reference 2, and so on.
        #[arg(long = "reference", value_name = "ID", conflicts_with = "job")]
        references: Vec<String>,
        /// The judge, by its name in the config's `[backends.judge]` tables. With
        /// none, the config's default judge answers.
        #[arg(long, value_name = "NAME")]
        judge: Option<String>,
        /// The job's requirements in words, such as "change only the sky; the parrot's
        /// tail keeps its colour". The judge holds the picture to it. brief-compliance
        /// needs one; with none, nothing is named as kept or as asked for.
        #[arg(long, value_name = "TEXT", conflicts_with = "job")]
        brief: Option<String>,
        /// A squint job file, read whole: its brief, its requirements with their ids,
        /// scopes and evidence, and its images with their roles and labels, each
        /// brought into the store from the file it names, relative to the job file's
        /// folder, and checked against the SHA-256 the job pins.
        #[arg(long, value_name = "FILE")]
        job: Option<PathBuf>,
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
        /// align, and the experimental change_map and edge_similarity, which compare the
        /// source with the frame align returned (align is then run first, named or not):
        /// where the picture changed in structure, and how much of its edges it kept.
        /// Without it, align alone.
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
            candidate,
            source,
            references,
            judge,
            brief,
            job,
            details,
            detail,
            projection,
            at,
            under,
            request_id,
        } => {
            let root = at.root().to_path_buf();
            // What the command line states is checked, and a job file read and its
            // pictures brought in, before anything runs.
            let (job, images) = match job {
                Some(file) => door::job_file(file, use_case, &root, &recovery)?,
                None => {
                    let references: Vec<&str> = references.iter().map(String::as_str).collect();
                    door::shorthand(&CheckAsked {
                        use_case,
                        candidate: candidate.as_deref().unwrap_or_default(),
                        source: source.as_deref(),
                        references: &references,
                        judge: judge.as_deref(),
                        brief: brief.as_deref(),
                        request_id: request_id.as_deref(),
                        details: door::Details::First,
                        detail: None,
                        projection: "full",
                    })?
                }
            };
            let uses = Uses::One(Capability::Judge, judge.as_deref());
            let (executor, _) = executor_caching(at, under, recovery, platform, uses)?;
            let measuring = door::Measuring {
                details: if details == "none" {
                    door::Details::None
                } else {
                    door::Details::First
                },
                detail: detail.as_deref(),
                projection,
            };
            let read = |run: &_| door::verdict(&root, run);
            door::check_job(
                &executor,
                &job,
                &images,
                judge.as_deref(),
                request_id.as_deref(),
                &measuring,
                &read,
            )
        }
    }
}
