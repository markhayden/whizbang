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
    /// at once. Run again, it resumes: a cell that completed or failed is not run again,
    /// and one that was interrupted is. Refused before anything runs when its forecast
    /// passes what the store's allowance has left.
    Matrix {
        /// The matrix file.
        #[arg(long, value_name = "FILE")]
        matrix: PathBuf,
        /// The folder of the evaluation sets: `<use-case>/<set>/set.toml` under it.
        #[arg(long, value_name = "DIR", default_value = "evalsets")]
        root: PathBuf,
        #[command(flatten)]
        at: Where,
        #[command(flatten)]
        under: Under,
    },
    /// Run a gate frozen in a file (`SPEC.md` section 8, "Gates"): the matrix it names, each
    /// run holding the gate's digest, then scored against the labels given and decided.
    /// Writes the receipt and prints aggregates only. Run again, it resumes.
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
pub enum SetsCommand {
    /// Check every set under the folder: what code can check, offline. Each set's
    /// manifest, families, labels, corrections and jobs are read strictly; its rules are
    /// the ones `check` ships; its labels are right for its split; its seal holds; and a
    /// family is in one split only.
    Check {
        /// The folder of the evaluation sets.
        #[arg(long, value_name = "DIR", default_value = "evalsets")]
        root: PathBuf,
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
    let recovery = whizbang::wiring::recovery();
    match command {
        Command::Sets {
            command: SetsCommand::Check { root },
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
                    at,
                    under,
                },
        } => {
            let set = sets::find(root, set)?;
            if let Some(from) = from {
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
            at,
            under,
        } => run_matrix(matrix, root, at, under, recovery),
        Command::Run {
            gate,
            labels,
            receipt,
            root,
            at,
            under,
        } => {
            let frozen = whizbang::gating::frozen(gate)?;
            judges_are_of_their_models(&frozen.matrix, under)?;
            let executor = executor(at, under, recovery, None, uses_of(&frozen.matrix))?;
            let running = Running {
                executor: &executor,
                root: at.root(),
                left_micros: left(at.root())?,
                stop: &|| false,
            };
            let gating = whizbang::gating::Gating {
                file: gate,
                sets_root: root,
                labels_root: labels,
                receipt,
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
fn left(root: &Path) -> Result<u64, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let folded = whizbang_core::spend::fold::fold(&generation, |entry| reader.read(entry));
    Ok(folded.balance().left.micros())
}
