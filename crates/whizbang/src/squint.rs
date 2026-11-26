//! `whizbang squint check <use-case>` (`SPEC.md` section 11; plan 2A, task S2; plan 2C,
//! task A1): wiring from arguments to the door (`whizbang::door::check_job`), which runs
//! the use case's recipe through the executor under a job (plan m3, task Q1: `details`,
//! then `check` as the judge's step), and says its verdict.

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
        #[arg(
            long,
            required_unless_present_any = ["job", "job_json"],
            conflicts_with_all = ["job", "job_json"]
        )]
        candidate: Option<String>,
        /// What the candidate was edited from, by its identity in the store: for
        /// source-fidelity.
        #[arg(long, conflicts_with_all = ["job", "job_json"])]
        source: Option<String>,
        /// What the candidate is held to, by its identity in the store: for
        /// reference-consistency and product-fidelity. Up to 8, each labelled by its
        /// place: reference 1, reference 2, and so on.
        #[arg(long = "reference", value_name = "ID", conflicts_with_all = ["job", "job_json"])]
        references: Vec<String>,
        /// The judge, by its name in the config's `[backends.judge]` tables. With
        /// none, the config's default judge answers.
        #[arg(long, value_name = "NAME")]
        judge: Option<String>,
        /// The job's requirements in words, such as "change only the sky; the parrot's
        /// tail keeps its colour". The judge holds the picture to it. brief-compliance
        /// needs one; with none, nothing is named as kept or as asked for.
        #[arg(long, value_name = "TEXT", conflicts_with_all = ["job", "job_json"])]
        brief: Option<String>,
        /// A squint job file, read whole: its brief, its requirements with their ids,
        /// scopes and evidence, and its images with their roles and labels, each
        /// brought into the store from the file it names, relative to the job file's
        /// folder, and checked against the SHA-256 the job pins.
        #[arg(long, value_name = "FILE", conflicts_with = "job_json")]
        job: Option<PathBuf>,
        /// A job of the contract job/1 as JSON, as a stored run holds it (the argument
        /// `job` of a check's record): its brief, its requirements and its images by role,
        /// whose pictures are given by --image, by their identities in the store, once
        /// each in the job's order. The command the playground copies from a check says
        /// its whole job this way. Held as a job file is: at most 1 MiB, no picture given
        /// inline, and a pinned SHA-256 must be the file the store imported that picture
        /// from.
        #[arg(long, value_name = "JSON", requires = "images")]
        job_json: Option<String>,
        /// With --job-json: each of the job's images, by its identity in the store, in
        /// the job's order.
        #[arg(long = "image", value_name = "ID", requires = "job_json")]
        images: Vec<String>,
        #[command(flatten)]
        at: Where,
        #[command(flatten)]
        under: Under,
        /// What is measured before the judge looks. `none` (the default): nothing is
        /// measured first, the judge is told of view and align and calls them, and a call
        /// of align is answered with its pictures alone; this is the control arm of the
        /// gate of details first, which rejected details first. `first`: align registers
        /// the candidate onto the source, and the aligned picture, what it found and every
        /// measurement open the conversation, each labelled with the tool and its mark; the
        /// judge may still call view, align and resize, and a call of align is answered in
        /// full.
        #[arg(long, value_name = "WHEN", value_parser = door::Details::NAMES)]
        details: Option<String>,
        /// The tools whose details are measured first, by name, separated by commas:
        /// align, and the unverified change_map and edge_similarity, which compare the
        /// source with the frame align returned (align is then run first, named or not):
        /// where the picture changed in structure, and how much of its edges it kept.
        /// Or `tiles`, for a job with one image (reality-checks; brief-compliance with no
        /// reference): the experimental candidate cut by view into four overlapping tiles
        /// at full resolution, each labelled with its place. Without it, align alone.
        /// Taken only with `--details first`: nothing is measured first otherwise.
        #[arg(long, value_name = "TOOLS")]
        detail: Option<String>,
        /// How a call of align by the judge is answered: `full`, everything it found,
        /// or `visual_only`, its pictures and a fixed sentence. Without it, `visual_only`
        /// under `--details none` and `full` under `--details first`, as each arm of the
        /// gate was measured.
        #[arg(long, value_name = "WHICH", value_parser = ["full", "visual_only"])]
        projection: Option<String>,
        /// `on`: when the judge's submission leaves a mandatory item tentative, so that the
        /// verdict would be unsure, it is given one more turn, inside the run's turns and
        /// spend, with crops at full resolution of what it cited for those items, to settle
        /// them alone; the verdict is still worked out from the dispositions. `off` (the
        /// default) runs as before. `strict`: the same, but an item is settled toward pass
        /// only with a location cited for it, and stays tentative otherwise. Experimental.
        #[arg(long, value_name = "WHEN", default_value = "off", value_parser = ["off", "on", "strict"])]
        second_look: String,
        /// The form the judge answers in: `v1`, the default; `v2`, whose items are keyed by
        /// checklist id; or `v3`, v1's list of items with each finding on an item written
        /// inside it, so that it cites the item's own obligations. v2 and v3 read
        /// near-misses that have one reading, record them, and name every problem in
        /// their one repair (ADR 0042). Experimental.
        #[arg(long, value_name = "FORM", value_parser = ["v1", "v2", "v3"])]
        submission: Option<String>,
        /// A tool file the config lists under `[tools] paths`, offered to the judge with
        /// the schema its declaration gives; give it again for more. An experimental tool
        /// is offered only to a run that names it.
        #[arg(long = "tool", value_name = "NAME")]
        tools: Vec<String>,
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
            job_json,
            images,
            details,
            detail,
            projection,
            second_look,
            submission,
            tools,
            at,
            under,
            request_id,
        } => {
            let root = at.root().to_path_buf();
            // What is measured first, by its name or the door's default; the projection
            // stated, or the arm's own.
            let measuring = door::Measuring {
                tools,
                second_look: second_look.as_str(),
                submission: submission.as_deref(),
                ..door::Measuring::stated(details.as_deref(), projection.as_deref())?
                    .measuring(detail.as_deref())?
            };
            // What the command line states is checked, and a job file read and its
            // pictures brought in, before anything runs.
            let (job, images) = match (job, job_json) {
                (Some(file), _) => door::job_file(file, use_case, &root, &recovery)?,
                (None, Some(text)) => door::job_json(text, images, use_case, &root)?,
                (None, None) => {
                    let references: Vec<&str> = references.iter().map(String::as_str).collect();
                    door::shorthand(&CheckAsked {
                        use_case,
                        candidate: candidate.as_deref().unwrap_or_default(),
                        source: source.as_deref(),
                        references: &references,
                        judge: judge.as_deref(),
                        brief: brief.as_deref(),
                        request_id: request_id.as_deref(),
                        details: measuring.details,
                        detail: None,
                        projection: measuring.projection,
                    })?
                }
            };
            // Tiles are of a job with one image: one that relates a pair is refused here,
            // naming its use case, before the config or a judge is looked at.
            let tiles = (detail.as_deref().unwrap_or_default().split(','))
                .any(|d| d.trim() == whizbang_squint::details::TILES);
            let paired = (job.images.iter())
                .any(|image| image.role != whizbang_squint::job::Role::Candidate);
            if tiles && paired {
                return Err(whizbang_squint::details::tiles_refused(use_case));
            }
            let uses = Uses::One(Capability::Judge, judge.as_deref());
            let (executor, _) = executor_caching(at, under, recovery, platform, uses)?;
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
