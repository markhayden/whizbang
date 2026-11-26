//! `whizbang eval`: the evaluation sets and the matrices run over them (plan 2C, task E1):
//! wiring from arguments to `whizbang::evaluation`, which says what each does.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Subcommand;
use whizbang::evaluation::{self, Fetching, Running};
use whizbang::wiring::Uses;
use whizbang_core::config::{DOWNLOAD_LEAST_KIB_PER_SECOND, Loaded};
use whizbang_core::contract::{Capability, Recovery, Secrets};
use whizbang_core::egress::Origin;
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::matrix::Matrix;
use whizbang_eval::sets;

use crate::rascal::{Under, executor, executor_caching};
use crate::store::Where;

/// The most bytes of a matrix file that are read.
const MATRIX_FILE_LIMIT: u64 = 256 * 1024;

#[derive(Subcommand)]
pub enum Command {
    /// The evaluation sets: checking them, and fetching their pictures.
    Sets {
        #[command(subcommand)]
        command: SetsCommand,
    },
    /// Run a matrix: each item of each set it names, under each judge and arm, as many
    /// times as it says, each a run of `squint check` in the store, at most `concurrency`
    /// at once; where a judge or a set lists what it runs, only those cells. Run again, it
    /// resumes: a cell that completed or failed is not run again, and one that was
    /// interrupted is. Refused before anything runs when its forecast passes what the
    /// store's allowance has left, or when a gate file it names is not as it was frozen.
    Matrix {
        /// The matrix file.
        #[arg(long, value_name = "FILE")]
        matrix: PathBuf,
        /// The folder of the evaluation sets: `<use-case>/<set>/set.toml` under it.
        #[arg(long, value_name = "DIR", default_value = "evalsets")]
        root: PathBuf,
        /// Print the forecast and run anyway: each run is admitted against the allowance as
        /// any run is, and the first refused for it ends the matrix, the cells left to run
        /// named by count; run again after a new allowance, it resumes.
        #[arg(long)]
        past_forecast: bool,
        /// Every refusal names only a set's id, an item's place and the kind of problem.
        #[arg(long)]
        held_out: bool,
        #[command(flatten)]
        at: Where,
        #[command(flatten)]
        under: Under,
    },
    /// Run a gate frozen in a file (`SPEC.md` section 8, "Gates"): the matrix it names, each
    /// run holding the gate's digest, then scored against the labels given and decided.
    /// Writes the receipt and prints aggregates only. Run again, it resumes. A gate of
    /// version 3, which its matrix names by digest, runs nothing: it is scored from the
    /// runs `eval matrix` made of its matrix.
    Run {
        /// The gate file; the matrix it names is beside it.
        #[arg(long, value_name = "FILE")]
        gate: PathBuf,
        /// The folder a blind set's labels are read from, laid out as the sets are
        /// (`<folder>/<set id>/labels.toml`), outside the sets' folder; a set that publishes
        /// its labels is scored by its own labels file.
        #[arg(long, value_name = "DIR")]
        labels: PathBuf,
        /// Where the receipt is written: not under the sets' or the labels' folder.
        #[arg(long, value_name = "FILE")]
        receipt: PathBuf,
        /// The folder of the evaluation sets: `<use-case>/<set>/set.toml` under it.
        #[arg(long, value_name = "DIR", default_value = "evalsets")]
        root: PathBuf,
        /// Every refusal names only a set's id, an item's place and the kind of problem.
        #[arg(long)]
        held_out: bool,
        #[command(flatten)]
        at: Where,
        #[command(flatten)]
        under: Under,
    },
    /// Score a tool on rung 2: its read of each item of each set named, each a run in the
    /// store (align, then the tool on the source and the aligned frame), scored against the
    /// sets' labels. Prints each item's number, the area under the ROC curve of the tool's
    /// primary measurement with its 95% interval resampled by family, and the balanced
    /// accuracy at a threshold chosen on one half of the families and read on the other.
    /// An item with no read is an error, apart from the passes and the fails.
    Tool {
        /// The tool: change_map, edge_similarity, or a tool file the config lists whose
        /// images are source and candidate; one that takes the argument job is given each
        /// item's job.
        tool: String,
        /// A set to score on, by its id (`<use-case>/<set>`); give it again for more.
        #[arg(long = "set", value_name = "SET", required = true)]
        sets: Vec<String>,
        /// The folder of the evaluation sets: `<use-case>/<set>/set.toml` under it.
        #[arg(long, value_name = "DIR", default_value = "evalsets")]
        root: PathBuf,
        #[command(flatten)]
        at: Where,
        #[command(flatten)]
        under: Under,
    },
    /// Score judges against each other (--mode judges, under one --arm), or one arm against
    /// another within each judge (--mode arms), from the runs the matrices made: per judge,
    /// arm and scope, accuracy with intervals resampled by family, cost, time and the
    /// submissions' rejections; and the rule asked for, judge-pick/1 or submission/1. Reads
    /// the store only, and writes judges/1 to --out, aggregates only.
    Judges {
        /// A matrix whose runs are scored; give it again for more.
        #[arg(long = "matrix", value_name = "FILE", required = true)]
        matrices: Vec<PathBuf>,
        /// The folder a blind set's labels are read from, laid out as the sets are
        /// (`<folder>/<set id>/labels.toml`), outside the sets' folder; a set that publishes
        /// its labels is scored by its own labels file.
        #[arg(long, value_name = "DIR")]
        labels: PathBuf,
        /// judges: each judge against the best under --arm; arms: within each judge, the
        /// other arm against the control.
        #[arg(long, value_name = "MODE", value_parser = ["judges", "arms"])]
        mode: String,
        /// In judges mode, the one arm every judge is scored under; in arms mode, the
        /// control (the first arm of the first matrix when not given).
        #[arg(long, value_name = "ARM")]
        arm: Option<String>,
        /// The decision rule: judge-pick/1 or judge-pick/2 (judges mode), submission/1 or
        /// submission/2 (arms mode).
        #[arg(long, value_name = "RULE", value_parser = whizbang::judges::RULES)]
        rule: Option<String>,
        /// A set to score alone, by its id; give it again for more. Without it, every set of
        /// the matrices.
        #[arg(long = "set", value_name = "SET")]
        sets: Vec<String>,
        /// Every refusal names only a set's id, an item's place and the kind of problem.
        #[arg(long)]
        held_out: bool,
        /// Where the record is written: not under the sets' or the labels' folder.
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        /// The folder of the evaluation sets: `<use-case>/<set>/set.toml` under it.
        #[arg(long, value_name = "DIR", default_value = "evalsets")]
        root: PathBuf,
        #[command(flatten)]
        at: Where,
    },
    /// What the receipts of the gates run admitted (plan mE, task M1, item 8): each tool,
    /// use case, format, wording or judge a rung 4 gate on blind or held-out data admitted,
    /// with the receipt's digest and date. Reads the receipts only.
    Admitted {
        /// The folder of the receipts, read to four folders down.
        #[arg(long, value_name = "DIR", default_value = "gates")]
        receipts: PathBuf,
    },
    /// The synthetic edit set of the edits' rung 2 (plan m3, task B1): drawing it from a
    /// set's pictures, and scoring a lab round of its recipes from the store.
    Edits {
        #[command(subcommand)]
        command: EditsCommand,
    },
    /// The loop `remove_checked` on the pictures of segment-01 (plan e1, task L1): its
    /// attempts' masks scored against the truth masks, and the contact sheet of its fills.
    Loop {
        #[command(subcommand)]
        command: LoopCommand,
    },
    /// The baseline gate: each recorded decision of a baseline package replayed with a judge
    /// that answers its recorded turns, squint's loop on whizbang's tools and preparation,
    /// and compared with its recorded transcript. Writes the receipt and prints counts only.
    /// With --live, the descriptive live rerun instead: arm C of claude-haiku-5-5 run fresh
    /// on source-fidelity/blind-v3, once, under the config's judge, and its agreement with
    /// the package's recorded run and that run's self-disagreement printed as counts.
    Baseline {
        /// The baseline package's root.
        #[arg(long, value_name = "DIR")]
        package: PathBuf,
        /// The recorded transcripts, as scripts/baseline/convert.py wrote them.
        #[arg(long, value_name = "DIR", required_unless_present = "live")]
        transcripts: Option<PathBuf>,
        /// Where the receipt is written.
        #[arg(long, value_name = "FILE")]
        receipt: PathBuf,
        /// The full whizbang commit replayed, as `git rev-parse HEAD` prints it.
        #[arg(long, value_name = "SHA", required_unless_present = "live")]
        commit: Option<String>,
        /// The live rerun: calls the config's judge, under the store's allowance.
        #[arg(long, conflicts_with_all = ["transcripts", "commit", "pictures"], requires = "store")]
        live: bool,
        /// The live rerun's store, which holds the set's pictures and its runs.
        #[arg(long, value_name = "DIR", requires = "live")]
        store: Option<PathBuf>,
        /// The live rerun's judge: the config's backend of this name, of claude-haiku-5-5.
        #[arg(
            long,
            value_name = "NAME",
            default_value = "default",
            requires = "live"
        )]
        judge: String,
        #[command(flatten)]
        under: Under,
        /// The folder of the evaluation sets the jobs are read from.
        #[arg(long, value_name = "DIR", default_value = "evalsets")]
        sets: PathBuf,
        /// A folder of the sets' pictures, laid out as their repository is
        /// (`<folder>/<set id>/<path>`). Needed when the package does not hold the pictures and
        /// they are not beside the jobs: a decision whose picture is found nowhere is refused.
        #[arg(long, value_name = "DIR")]
        pictures: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum EditsCommand {
    /// Draw the edit set from the set's pictures, each read from a folder laid out as the
    /// set's repository is and held to the SHA-256 its job pins: 24 pictures chosen by the
    /// seed, on each a shaded red ball with a box (for remove, and for remove_prompt named
    /// by a phrase), a scratch with a stroke along it (repair) and a spot with a box
    /// (repair_area), the drawn defect's exact mask kept as truth and the clean picture as
    /// the reference. Writes the pictures, the masks, the locations' canonical files and the
    /// manifest, edits.toml; with --lab, the lab's corpus manifest, protocols and matrices
    /// too. Prints counts, and the forecast of the fills a round of them makes. Reads and
    /// sends nothing else.
    Draw {
        /// A folder of the set's pictures, laid out as their repository is
        /// (`<folder>/<set id>/<path>`).
        #[arg(long, value_name = "DIR")]
        pictures: PathBuf,
        /// The folder the edit set is written to: never inside a git work tree unless --force.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
        /// A folder the lab's files made from the manifest are written to as well.
        #[arg(long, value_name = "DIR")]
        lab: Option<PathBuf>,
        /// The folder of the evaluation sets: `<use-case>/<set>/set.toml` under it.
        #[arg(long, value_name = "DIR", default_value = "evalsets")]
        root: PathBuf,
        /// Write the edit set inside a git work tree.
        #[arg(long)]
        force: bool,
    },
    /// Score the lab round of each recipe of the edit set from the store's runs: by recipe,
    /// the pixels changed outside the result's mask, the IoU of its mask with the drawn
    /// defect's, and the fill's mean error inside the drawn defect against the clean
    /// picture beside a plain blur fill's. Writes m3-edits.md and m3-edits.toml in --out,
    /// aggregates only, and prints counts. Reads the store only.
    Score {
        /// The folder the edit set was drawn to.
        #[arg(long, value_name = "DIR")]
        set: PathBuf,
        /// The folder of the lab's files: the protocols and matrices the round ran.
        #[arg(long, value_name = "DIR", default_value = "labs/m3-edits")]
        labs: PathBuf,
        /// The full whizbang commit the round ran, as `git rev-parse HEAD` prints it.
        #[arg(long, value_name = "SHA")]
        commit: String,
        /// The folder the record is written to.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
        #[command(flatten)]
        at: Where,
    },
    /// Score an edit recipe's gate (plan mE, task M2; `edit-gate/1`, frozen before its
    /// round) from the store: each case's run by its request id; task success from the
    /// checks of the finished edits (rung 3) or from a person's marks (--labels and
    /// --sheet); preservation outside the kept mask, exact; blinded quality from the lab
    /// decode of the picks made on the pairs of these runs (--assignment and --picks);
    /// operational errors, cost and latency. Writes the receipt, edit-receipt/1, and
    /// prints aggregates only. Reads the store only; the owner's files are refused inside
    /// a git work tree.
    Gate {
        /// The gate file; the cases and the protocol it pins are beside it.
        #[arg(long, value_name = "FILE")]
        gate: PathBuf,
        /// A person's marks of the cases' contact sheet, the marks.json it saves.
        #[arg(long, value_name = "FILE", requires = "sheet")]
        labels: Option<PathBuf>,
        /// The sheet's assignment.json, which the marks are read through.
        #[arg(long, value_name = "FILE", requires = "labels")]
        sheet: Option<PathBuf>,
        /// The assignment of the pairs of these runs, as `lab pairs --from-runs` wrote it.
        #[arg(long, value_name = "FILE", requires = "picks")]
        assignment: Option<PathBuf>,
        /// The picks made on those pairs.
        #[arg(long, value_name = "FILE", requires = "assignment")]
        picks: Option<PathBuf>,
        /// Where the receipt is written.
        #[arg(long, value_name = "FILE")]
        receipt: PathBuf,
        #[command(flatten)]
        at: Where,
    },
}

#[derive(Subcommand)]
pub enum LoopCommand {
    /// Score the runs of the loop under one judge whose request ids are the manifest's
    /// (`e-loop-<judge>-<repetition>-<item>`): each attempt's found and shaped masks against
    /// the item's truth mask, read from the folder by its pin and never imported; the
    /// judge's RQ1 answer against mask under-coverage; coverage across each retry. With
    /// --labels, what a person's marks of the contact sheet say of the finished pictures.
    /// Writes loop-score/1, aggregates only, and prints counts. Reads the store only.
    Score {
        /// The loop's manifest, as labs/e-loop/segment-01.toml.
        #[arg(long, value_name = "FILE")]
        manifest: PathBuf,
        /// A folder of the pictures, laid out as their repository is
        /// (`<folder>/<set id>/<path>`).
        #[arg(long, value_name = "DIR")]
        set: PathBuf,
        /// The judge, by its name in the config's `[backends.judge]` tables.
        #[arg(long, value_name = "NAME")]
        judge: String,
        /// How many repetitions of each entry were run: each is a requested run, and one
        /// the store does not hold is counted as missing.
        #[arg(long, value_name = "1..9")]
        repetitions: u32,
        /// A person's marks of the contact sheet, the marks.json it saves.
        #[arg(long, value_name = "FILE", requires = "sheet")]
        labels: Option<PathBuf>,
        /// The sheet's assignment.json, which the marks are read through.
        #[arg(long, value_name = "FILE", requires = "labels")]
        sheet: Option<PathBuf>,
        /// The file the score is written to.
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        #[command(flatten)]
        at: Where,
    },
    /// Write the contact sheet: for every completed fill of those runs, the picture the
    /// loop was given and the filled picture side by side, in an order drawn from a seed
    /// drawn for this sheet, with no verdict, coverage, attempt or judge on the page, and
    /// marks to set. Never inside a git work tree, nor in a folder that is not empty.
    /// Prints counts. With --cases, the sheet of any recipe's runs (plan mE, task M2): each
    /// case's finished picture beside the picture its run was given, both from the store.
    Sheet {
        /// The loop's manifest, as labs/e-loop/segment-01.toml.
        #[arg(long, value_name = "FILE", required_unless_present = "cases")]
        manifest: Option<PathBuf>,
        /// A folder of the pictures, laid out as their repository is.
        #[arg(long, value_name = "DIR", required_unless_present = "cases")]
        set: Option<PathBuf>,
        /// The judge, by its name in the config's `[backends.judge]` tables.
        #[arg(long, value_name = "NAME", required_unless_present = "cases")]
        judge: Option<String>,
        /// The cases (`edit-cases/1`): any action's runs, by request id.
        #[arg(long, value_name = "FILE", conflicts_with_all = ["manifest", "set", "judge"])]
        cases: Option<PathBuf>,
        /// The folder the page is written in: new or empty, never inside a git work tree.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
        #[command(flatten)]
        at: Where,
    },
}

#[derive(Subcommand)]
pub enum SetsCommand {
    /// Check every set under the folder: what code can check, offline. Each set's
    /// manifest, families, labels, corrections and jobs are read strictly; its rules are
    /// the ones `check` ships; its labels are right for its split; its seal holds; and a
    /// family is in one split only.
    Check {
        /// The folder of the evaluation sets.
        #[arg(long, value_name = "DIR", default_value = "evalsets")]
        root: PathBuf,
        /// Every refusal names only a set's id, an item's place and the kind of problem.
        #[arg(long)]
        held_out: bool,
    },
    /// Bring a set's pictures into the store, each checked against the SHA-256 its job
    /// pins: fetched from the raw files of the repository and revision the set names, an
    /// origin the config must approve as a recipient; or, with --from, read from a folder
    /// laid out as that repository is.
    Fetch {
        /// The set, by its id: `<use-case>/<set>`.
        #[arg(long, value_name = "ID")]
        set: String,
        /// The folder of the evaluation sets.
        #[arg(long, value_name = "DIR", default_value = "evalsets")]
        root: PathBuf,
        /// A folder that holds the pictures as the repository does
        /// (`<folder>/<set id>/<path>`): nothing is fetched.
        #[arg(long, value_name = "DIR")]
        from: Option<PathBuf>,
        /// Every refusal names only the set's id, an item's place and the kind of problem.
        #[arg(long)]
        held_out: bool,
        #[command(flatten)]
        at: Where,
        #[command(flatten)]
        under: Under,
    },
}

/// Runs an eval command: what to print on success, or the complaint.
/// A matrix's executor is given no platform's data folder: a local model's directory is
/// named by `--models-dir` or the config.
pub fn run(command: &Command) -> Result<String, String> {
    let ran = run_command(command);
    match (held_out_of(command), ran) {
        (Some((root, ids)), Err(why)) => Err(whizbang::held_out::shown(&root, &ids, &why)),
        (_, ran) => ran,
    }
}

/// Under `--held-out`, the sets' folder and the ids of the sets the command reads.
fn held_out_of(command: &Command) -> Option<(PathBuf, Vec<String>)> {
    // The matrix read as `eval matrix` reads it, at most its limit of bytes.
    let sets_of = |file: &Path| -> Vec<String> {
        use std::io::Read;
        let mut text = String::new();
        let read = std::fs::File::open(file)
            .and_then(|opened| opened.take(MATRIX_FILE_LIMIT + 1).read_to_string(&mut text));
        if read.is_err() || text.len() as u64 > MATRIX_FILE_LIMIT {
            return Vec::new();
        }
        Matrix::parse(&text)
            .map(|m| m.matrix.sets)
            .unwrap_or_default()
    };
    match command {
        Command::Sets {
            command:
                SetsCommand::Check {
                    root,
                    held_out: true,
                },
        } => Some((root.clone(), whizbang::held_out::ids_under(root))),
        Command::Sets {
            command:
                SetsCommand::Fetch {
                    set,
                    root,
                    held_out: true,
                    ..
                },
        } => Some((root.clone(), vec![set.clone()])),
        Command::Matrix {
            matrix,
            root,
            held_out: true,
            ..
        } => Some((root.clone(), sets_of(matrix))),
        Command::Run {
            gate,
            root,
            held_out: true,
            ..
        } => {
            let ids = (whizbang::gating::frozen(gate).ok())
                .map(|frozen| frozen.matrix.matrix.sets)
                .unwrap_or_default();
            Some((root.clone(), ids))
        }
        Command::Judges {
            matrices,
            root,
            held_out: true,
            ..
        } => Some((
            root.clone(),
            matrices.iter().flat_map(|m| sets_of(m)).collect(),
        )),
        _ => None,
    }
}

fn run_command(command: &Command) -> Result<String, String> {
    let recovery = whizbang::wiring::recovery();
    match command {
        Command::Admitted { receipts } => {
            let (admitted, read) = whizbang::admitted::read(receipts)?;
            Ok(whizbang::admitted::text(&admitted, read))
        }
        Command::Sets {
            command: SetsCommand::Check { root, .. },
        } => {
            let report = sets::check(root, &evaluation::judging())?;
            Ok(format!(
                "sets: {}\nitems: {}\npictures pinned: {}\n",
                report.sets, report.items, report.pictures
            ))
        }
        Command::Sets {
            command:
                SetsCommand::Fetch {
                    set,
                    root,
                    from,
                    held_out,
                    at,
                    under,
                },
        } => {
            let set = sets::find(root, set)?;
            if let Some(from) = from {
                if *held_out {
                    return whizbang::held_out::from_folder(&set, from, at.root(), &recovery);
                }
                return evaluation::from_folder(&set, from, at.root(), &recovery);
            }
            let (approved, secrets, least) = config_of(under)?;
            let make = move |origins: &[Origin], secrets: &Secrets| {
                whizbang_backends::net::downloader(least, origins, secrets)
            };
            let how = Fetching {
                base: evaluation::RAW_ORIGIN,
                approved: &approved,
                secrets: &secrets,
                make: &make,
            };
            evaluation::fetch(&set, at.root(), &recovery, &how)
        }
        Command::Matrix {
            matrix,
            root,
            past_forecast,
            at,
            under,
            ..
        } => run_matrix(matrix, root, at, under, recovery, *past_forecast),
        Command::Judges {
            matrices,
            labels,
            mode,
            arm,
            rule,
            sets,
            out,
            root,
            at,
            ..
        } => {
            use whizbang::judges::{self, Mode, Reading, Rule};
            let reading = Reading {
                matrices,
                store: at.root(),
                sets_root: root,
                labels_root: labels,
                out,
                mode: if mode == "arms" {
                    Mode::Arms
                } else {
                    Mode::Judges
                },
                arm: arm.as_deref(),
                rule: rule.as_deref().map(Rule::named).transpose()?,
                sets,
                commit: crate::COMMIT,
            };
            judges::run(&reading).map(|written| judges::text(&written))
        }
        Command::Edits {
            command:
                EditsCommand::Draw {
                    pictures,
                    out,
                    lab,
                    root,
                    force,
                },
        } => {
            if !force && whizbang::edits::inside_a_work_tree(out) {
                return Err(
                    "the edit set's folder is inside a git work tree; its pictures are not \
                     committed (--force writes them there anyway); nothing was drawn"
                        .to_owned(),
                );
            }
            let set = sets::find(root, whizbang::edits::SET)?;
            let sources = whizbang::edits::sources_of(&set, pictures)?;
            let drawn = whizbang::edits::draw(
                set.id(),
                &sources,
                whizbang::edits::COUNT,
                whizbang::edits::SEED,
            )?;
            whizbang::edits::write(&drawn, out)?;
            if let Some(lab) = lab {
                let failed = |why: std::io::Error| format!("the lab's files: {why}");
                std::fs::create_dir_all(lab).map_err(failed)?;
                for (name, text) in whizbang::edits::lab_files(&drawn.set) {
                    std::fs::write(lab.join(name), text).map_err(failed)?;
                }
            }
            Ok(whizbang::edits::drawn_text(&drawn))
        }
        Command::Edits {
            command:
                EditsCommand::Score {
                    set,
                    labs,
                    commit,
                    out,
                    at,
                },
        } => {
            let scoring = whizbang::edits::Scoring {
                set,
                labs,
                root: at.root(),
                commit,
            };
            let record = whizbang::edits::score(&scoring)?;
            whizbang::edits::write_record(&record, out)?;
            Ok(record.text())
        }
        Command::Edits {
            command: EditsCommand::Gate { .. },
        } => edits_gate(command),
        Command::Loop { command } => eval_loop(command),
        Command::Run {
            gate,
            labels,
            receipt,
            root,
            at,
            under,
            ..
        } => {
            let frozen = whizbang::gating::frozen(gate)?;
            let gating = whizbang::gating::Gating {
                file: gate,
                sets_root: root,
                labels_root: labels,
                receipt,
            };
            if frozen.scored_from_its_matrix() {
                let written = whizbang::gating::score(&gating, at.root())?;
                return Ok(whizbang::gating::text(&written));
            }
            judges_are_of_their_models(&frozen.matrix, under)?;
            let executor = executor(at, under, recovery, None, uses_of(&frozen.matrix))?;
            let running = Running {
                executor: &executor,
                root: at.root(),
                left_micros: left(at.root())?,
                stop: &|| false,
            };
            let written = whizbang::gating::run(&gating, &running)?;
            Ok(whizbang::gating::text(&written))
        }
        Command::Tool {
            tool,
            sets,
            root,
            at,
            under,
        } => {
            let (executor, over) = executor_caching(at, under, recovery, None, Uses::Nothing)?;
            let files = over.tool_files()?;
            let declared = whizbang::trying::declared_in(tool, files.as_deref())?;
            let scored = whizbang::trying::eval_tool(
                &executor,
                at.root(),
                root,
                &declared,
                sets,
                whizbang::trying::RESAMPLES,
            );
            scored.map(|(_, text)| text)
        }
        Command::Baseline {
            package,
            transcripts,
            receipt,
            commit,
            sets,
            pictures,
            live,
            store,
            judge,
            under,
        } => {
            if *live {
                let at = Where::at(store.as_deref().unwrap_or(Path::new("")));
                return live_rerun(package, receipt, sets, judge, &at, under, recovery);
            }
            let (Some(transcripts), Some(commit)) = (transcripts, commit) else {
                return Err("the replay is given --transcripts and --commit".to_owned());
            };
            baseline(
                package,
                transcripts,
                receipt,
                commit,
                sets,
                pictures.as_deref(),
            )
        }
    }
}

/// `eval loop score` and `eval loop sheet`.
fn eval_loop(command: &LoopCommand) -> Result<String, String> {
    match command {
        LoopCommand::Score {
            manifest,
            set,
            judge,
            repetitions,
            labels,
            sheet,
            out,
            at,
        } => {
            let scoring = whizbang::loop_score::Scoring {
                manifest,
                set,
                root: at.root(),
                judge,
                repetitions: *repetitions,
                labels: labels.as_deref().zip(sheet.as_deref()),
            };
            let score = whizbang::loop_score::score(&scoring)?;
            score.write(out)?;
            Ok(score.text())
        }
        LoopCommand::Sheet {
            cases: Some(cases),
            out,
            at,
            ..
        } => {
            let asked = whizbang::loop_sheet::AskedCases {
                cases,
                root: at.root(),
                out,
            };
            whizbang::loop_sheet::sheet_of_cases(&asked).map(|written| written.text())
        }
        LoopCommand::Sheet {
            manifest: Some(manifest),
            set: Some(set),
            judge: Some(judge),
            out,
            at,
            ..
        } => {
            let asked = whizbang::loop_sheet::Asked {
                manifest,
                set,
                root: at.root(),
                judge,
                out,
            };
            whizbang::loop_sheet::sheet(&asked).map(|written| written.text())
        }
        LoopCommand::Sheet { .. } => {
            Err("eval loop sheet takes --manifest, --set and --judge, or --cases".to_owned())
        }
    }
}

/// `eval edits gate` (plan mE, task M2).
fn edits_gate(command: &Command) -> Result<String, String> {
    let Command::Edits {
        command:
            EditsCommand::Gate {
                gate,
                labels,
                sheet,
                assignment,
                picks,
                receipt,
                at,
            },
    } = command
    else {
        return Err("not an edit gate".to_owned());
    };
    let gating = whizbang::edits_gate::Gating {
        gate,
        root: at.root(),
        marks: labels.as_deref().zip(sheet.as_deref()),
        picks: assignment.as_deref().zip(picks.as_deref()),
        receipt,
        commit: crate::COMMIT,
    };
    whizbang::edits_gate::score(&gating).map(|receipt| receipt.text())
}

/// `eval baseline`: the gate, in a build made with the replay.
#[cfg(feature = "baseline-replay")]
fn baseline(
    package: &Path,
    transcripts: &Path,
    receipt: &Path,
    commit: &str,
    sets: &Path,
    pictures: Option<&Path>,
) -> Result<String, String> {
    let asked = whizbang::baseline::Asked {
        package,
        transcripts,
        receipt,
        sets,
        pictures,
        commit,
    };
    whizbang::baseline::run(&asked).map(|tally| tally.text())
}

/// `eval baseline` in a build made without the replay: it says so, and how to build it.
#[cfg(not(feature = "baseline-replay"))]
fn baseline(
    _package: &Path,
    _transcripts: &Path,
    _receipt: &Path,
    _commit: &str,
    _sets: &Path,
    _pictures: Option<&Path>,
) -> Result<String, String> {
    Err(
        "this build was made without the baseline replay; build it with \
         `cargo build --release -p whizbang --features baseline-replay`"
            .to_owned(),
    )
}

/// The origins the config approves as recipients, each recipient's and those it may pass
/// data on to; its secrets; and the least rate of a download.
fn config_of(under: &Under) -> Result<(Vec<Origin>, Secrets, u64), String> {
    let asked = under.asked();
    let Some(config) = asked.config else {
        return Ok((Vec::new(), Secrets::none(), DOWNLOAD_LEAST_KIB_PER_SECOND));
    };
    let loaded = Loaded::of_files(config, asked.env_file)
        .map_err(|refused| format!("the config cannot be used: {refused}"))?;
    let read = loaded.config();
    let approved = read
        .recipients()
        .flat_map(|(_, approval)| {
            std::iter::once(approval.origin().clone()).chain(approval.upstreams().to_vec())
        })
        .collect();
    let least = read
        .download_least_kib_per_second()
        .unwrap_or(DOWNLOAD_LEAST_KIB_PER_SECOND);
    Ok((approved, loaded.kept_out(), least))
}

fn run_matrix(
    file: &Path,
    root: &Path,
    at: &Where,
    under: &Under,
    recovery: Recovery,
    past_forecast: bool,
) -> Result<String, String> {
    use std::io::Read;
    let mut text = String::new();
    let shown = whizbang::door::printable(&file.display().to_string());
    let opened = std::fs::File::open(file).map_err(|e| format!("{shown}: {e}"))?;
    (opened.take(MATRIX_FILE_LIMIT + 1).read_to_string(&mut text))
        .map_err(|e| format!("{shown}: {e}"))?;
    if text.len() as u64 > MATRIX_FILE_LIMIT {
        return Err(format!(
            "{shown}: a matrix file is at most {MATRIX_FILE_LIMIT} bytes"
        ));
    }
    let matrix = Matrix::parse(&text).map_err(|why| format!("{shown}: {why}"))?;
    // The gates frozen on it hold as frozen, before anything runs (plan mE, task M1).
    let (gates, _) =
        whizbang::gating::gates_on(file, &matrix).map_err(|why| format!("{shown}: {why}"))?;
    whizbang::gating::versions_hold(root, &gates).map_err(|why| format!("{shown}: {why}"))?;
    judges_are_of_their_models(&matrix, under)?;
    let (planned, labels) = evaluation::plan(&matrix, root, at.root())?;
    let left_micros = left(at.root())?;
    let executor = executor(at, under, recovery, None, uses_of(&matrix))?;
    let running = Running {
        executor: &executor,
        root: at.root(),
        left_micros,
        stop: &|| false,
    };
    if past_forecast {
        let (forecast, report) =
            evaluation::run_past_forecast(&matrix, &planned, &labels, &running)?;
        return Ok(format!(
            "forecast of the runs still to run: {forecast} millionths of a dollar\n{}",
            report.text()
        ));
    }
    let report = evaluation::run(&matrix, &planned, &labels, &running)?;
    Ok(report.text())
}

/// `eval baseline --live`: the live rerun under the config's judge, which must be of the
/// model it reruns.
fn live_rerun(
    package: &Path,
    receipt: &Path,
    sets: &Path,
    judge: &str,
    at: &Where,
    under: &Under,
    recovery: Recovery,
) -> Result<String, String> {
    use whizbang::baseline_live::{LIVE_MODEL, LIVE_SET, Live};
    let matrix = whizbang::baseline_live::matrix(LIVE_SET, LIVE_MODEL, judge)?;
    judges_are_of_their_models(&matrix, under)?;
    let executor = executor(at, under, recovery, None, uses_of(&matrix))?;
    let running = Running {
        executor: &executor,
        root: at.root(),
        left_micros: left(at.root())?,
        stop: &|| false,
    };
    let live = Live {
        package,
        receipt,
        sets,
        set: LIVE_SET,
        model: LIVE_MODEL,
        matrix: &matrix,
    };
    whizbang::baseline_live::run(&live, &running).map(|rerun| rerun.text())
}

/// What a matrix's executor is built to use: its one judge, or every backend.
fn uses_of(matrix: &Matrix) -> Uses<'_> {
    match matrix.matrix.judges.as_slice() {
        [one] if one.backend != "default" => Uses::One(Capability::Judge, Some(&one.backend)),
        [_] => Uses::One(Capability::Judge, None),
        _ => Uses::Every,
    }
}

/// Each judge the matrix names is a backend of the config of the model it says.
fn judges_are_of_their_models(matrix: &Matrix, under: &Under) -> Result<(), String> {
    let asked = under.asked();
    let Some(config) = asked.config else {
        return Err("a matrix's judges are the config's: name it with --config".to_owned());
    };
    let loaded = Loaded::of_files(config, asked.env_file)
        .map_err(|refused| format!("the config cannot be used: {refused}"))?;
    let read = loaded.config();
    for judge in &matrix.matrix.judges {
        let name = match judge.backend.as_str() {
            "default" => read.default_of(Capability::Judge).unwrap_or("default"),
            other => other,
        };
        let model = read
            .backends(Capability::Judge)
            .find(|(named, _)| *named == name)
            .and_then(|(_, backend)| backend.model());
        if model != Some(judge.model.as_str()) {
            return Err(format!(
                "the matrix's judge {} is to be of the model {}, and the config's judge of that \
                 name is not",
                whizbang::door::printable(&judge.backend),
                whizbang::door::printable(&judge.model)
            ));
        }
    }
    Ok(())
}

/// What the allowance of the store at `root` has left, in millionths of a US dollar.
pub(crate) fn left(root: &Path) -> Result<u64, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let folded = whizbang_core::spend::fold::fold(&generation, |entry| reader.read(entry));
    Ok(folded.balance().left.micros())
}
