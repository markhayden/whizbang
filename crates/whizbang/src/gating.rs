//! `whizbang eval run --gate <file>` (plan m3, task G1, item 1; `SPEC.md` section 8,
//! "Gates"): a gate frozen in a file is run over the matrix it names, through the matrix
//! runner ([`crate::evaluation`]), and decided ([`whizbang_eval::gate`]).
//!
//! **Frozen.** The gate file names its matrix, beside it, and the matrix's identity; a
//! matrix whose identity is another is refused before anything runs, and so is one whose
//! arms are not exactly the gate's two, or whose most repetitions of a judge are not the
//! gate's. The SHA-256 of the gate file's bytes is in the cell of every run the gate makes
//! (`gate` in `check`'s argument `cell`), and a gate resumes from its own runs alone.
//!
//! **Scoring.** A set that publishes its labels is scored by its own committed labels file.
//! A blind set's labels are read from a folder given on the command line, laid out as the
//! sets are (`<labels>/<set id>/labels.toml`), never at or under the sets' folder as the file
//! system resolves both, and held to the set's seal (`Set::labels_from`): so they are scored
//! without ever being placed in the sets' tree. Nothing is written under the sets'
//! folder or the labels' folder: a receipt asked for under either is refused. Each cell of
//! the gate's arms is one row: its verdict (a run that failed is an error), its item's label
//! and family, what its paid requests settled at and its wall time. An item with no label
//! stops the scoring (`missing_label = "refuse"`), and so does a cell that has not ended,
//! since a gate decided on part of its matrix would be decided on what happened to finish.
//!
//! **The receipt** holds aggregates and digests only: no item, family, label or verdict of
//! one. What is printed is the same.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::contract::record::Decision;
use whizbang_core::identity::Digest;
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::gate::{Gate, Judged, Outcome, RECEIPT_VERSION, Receipt};
use whizbang_eval::matrix::Matrix;
use whizbang_eval::metrics::{Computed, Row};
use whizbang_eval::sets::{self, Expected};

use crate::door::printable;
use crate::evaluation::{self, Ended, Running};

/// The most bytes of a gate file, or of the matrix it names, that are read.
pub const FILE_LIMIT: u64 = 256 * 1024;

/// The name of a set's labels file under the labels' folder.
pub const LABELS_FILE: &str = "labels.toml";

/// A gate and the matrix it names, read and held to each other.
#[derive(Debug, Clone)]
pub struct Frozen {
    pub gate: Gate,
    /// The SHA-256 of the gate file's bytes, in hex.
    pub digest: String,
    pub matrix: Matrix,
}

fn read_limited(file: &Path, what: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let shown = printable(&file.display().to_string());
    let opened = std::fs::File::open(file).map_err(|e| format!("{shown}: {e}"))?;
    let mut bytes = Vec::new();
    (opened.take(FILE_LIMIT + 1).read_to_end(&mut bytes)).map_err(|e| format!("{shown}: {e}"))?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(format!("{shown}: {what} is at most {FILE_LIMIT} bytes"));
    }
    Ok(bytes)
}

/// Reads a gate file and the matrix it names, refusing a matrix that is not the one the
/// gate was frozen with.
pub fn frozen(file: &Path) -> Result<Frozen, String> {
    let shown = printable(&file.display().to_string());
    let bytes = read_limited(file, "a gate file")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| format!("{shown}: not UTF-8"))?;
    let gate = Gate::parse(text).map_err(|why| format!("{shown}: {why}"))?;
    let beside = file.parent().unwrap_or(Path::new("."));
    let matrix_file = beside.join(&gate.run.matrix);
    let matrix_bytes = read_limited(&matrix_file, "a matrix file")?;
    let matrix_text = String::from_utf8(matrix_bytes)
        .map_err(|_| format!("{}: not UTF-8", printable(&gate.run.matrix)))?;
    let matrix = Matrix::parse(&matrix_text)
        .map_err(|why| format!("{}: {why}", printable(&gate.run.matrix)))?;
    if matrix.identity() != gate.run.matrix_identity {
        return Err(format!(
            "{shown}: the matrix's identity is {}, not the {} the gate was frozen with; \
             nothing was run",
            matrix.identity(),
            gate.run.matrix_identity
        ));
    }
    let m = &matrix.matrix;
    let mut arms: Vec<String> = m.arms.iter().map(ToString::to_string).collect();
    arms.sort();
    let mut wanted = vec![
        gate.baseline.control.clone(),
        gate.baseline.treatment.clone(),
    ];
    wanted.sort();
    if arms != wanted {
        return Err(format!(
            "{shown}: the matrix's arms are not exactly the gate's control and treatment"
        ));
    }
    let most = (m.judges.iter()).map(|judge| judge.repeated(m)).max();
    if most != Some(gate.repetitions.count) {
        return Err(format!(
            "{shown}: the gate's repetitions are {}, and the most of any judge of its matrix \
             are {}",
            gate.repetitions.count,
            most.unwrap_or(0)
        ));
    }
    Ok(Frozen {
        gate,
        digest: Digest::of_bytes(&bytes).hex(),
        matrix,
    })
}

/// What scoring reads of the sets: each item's label and family, and each set's labels
/// file by its SHA-256.
#[derive(Debug, Clone, Default)]
pub struct Labelled {
    pub expected: BTreeMap<(String, String), Expected>,
    pub families: BTreeMap<(String, String), String>,
    pub digests: BTreeMap<String, String>,
}

/// Each set's labels, held to the set's seal. A set that publishes its labels is read from
/// its own committed labels file, where it is. A blind set, which publishes none, is read
/// from the labels' folder (`<labels>/<set id>/labels.toml`, through no link below it), and
/// never from under the sets' folder: the labels' folder, as the file system resolves it
/// (so a link to a place in the tree counts as that place), at or below the sets' folder is
/// refused, as is one that cannot be resolved.
pub fn labelled(sets_root: &Path, labels_root: &Path, ids: &[String]) -> Result<Labelled, String> {
    let mut out = Labelled::default();
    for id in ids {
        let set = sets::find(sets_root, id)?;
        let bytes = match &set.manifest.set.labels {
            Some(own) => sets::read_under(&set.dir, own, sets::FILE_LIMIT)?,
            None => {
                let base = blind_labels_base(sets_root, labels_root)?;
                sets::read_under(&base, &format!("{id}/{LABELS_FILE}"), sets::FILE_LIMIT)?
            }
        };
        let labels = set.labels_from(&bytes)?;
        out.digests
            .insert(id.clone(), Digest::of_bytes(&bytes).hex());
        for label in labels.label {
            out.expected
                .insert((id.clone(), label.item.clone()), label.expected);
        }
        for (item, family) in set.families_by_item() {
            out.families.insert((id.clone(), item), family);
        }
    }
    Ok(out)
}

/// The labels' folder a blind set's labels are read from, as the file system resolves it:
/// refused when it cannot be resolved, or when it is at or below the sets' folder.
fn blind_labels_base(sets_root: &Path, labels_root: &Path) -> Result<PathBuf, String> {
    let unresolved = |_| "the sets' folder or the labels' folder cannot be resolved".to_owned();
    let tree = sets_root.canonicalize().map_err(unresolved)?;
    let base = labels_root.canonicalize().map_err(unresolved)?;
    if base.starts_with(&tree) {
        return Err(
            "a blind set's labels are never read from under the sets' folder: give --labels \
             a folder outside it"
                .to_owned(),
        );
    }
    Ok(base)
}

/// Whether a file at `path` would be at or below `root`, both as the file system resolves
/// them. Fails closed: when either cannot be resolved, it is taken to be below.
fn under(path: &Path, root: &Path) -> bool {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let parent = parent.unwrap_or(Path::new("."));
    match (root.canonicalize(), parent.canonicalize()) {
        (Ok(root), Ok(parent)) => parent.starts_with(&root),
        _ => true,
    }
}

/// What a gate is run with beside the runner.
pub struct Gating<'a> {
    pub file: &'a Path,
    pub sets_root: &'a Path,
    pub labels_root: &'a Path,
    pub receipt: &'a Path,
}

/// How one ended cell is scored: its verdict, its cost in dollars and its wall time.
struct Scored {
    computed: Computed,
    cost_usd: f64,
    wall_ms: u64,
}

/// Each run's verdict, cost and wall time, read from its record in the store at `root`.
fn scored_runs(root: &Path, runs: &[(String, Ended)]) -> Result<BTreeMap<String, Scored>, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let mut out = BTreeMap::new();
    for (run, ended) in runs {
        let stored = whizbang_core::contract::record::stored(&generation, run, |e| reader.read(e));
        let Some(Ok(record)) = stored else {
            return Err("a run of the gate has no record that can be read".to_owned());
        };
        let computed = match (ended, record.verdict()) {
            (Ended::Completed, Some((Decision::Pass, _))) => Computed::Pass,
            (Ended::Completed, Some((Decision::Fail, _))) => Computed::Fail,
            (Ended::Completed, Some((Decision::Unsure, _))) => Computed::Unsure,
            _ => Computed::Error,
        };
        let micros: u64 = (record.spend().requests.iter())
            .map(|paid| {
                paid.settled_micros()
                    .saturating_add(paid.unresolved_micros())
            })
            .sum();
        let time = record.time();
        let wall_ms = time
            .start_ms
            .map_or(0, |start| time.end_ms.saturating_sub(start));
        out.insert(
            run.clone(),
            Scored {
                computed,
                cost_usd: micros as f64 / 1_000_000.0,
                wall_ms,
            },
        );
    }
    Ok(out)
}

/// Runs the gate's matrix under its digest, scores it against the labels given, decides it,
/// and writes its receipt. Refused before anything runs when the gate and its matrix do not
/// hold together, a label cannot be read, or the receipt would be written under the sets'
/// or the labels' folder.
pub fn run(gating: &Gating<'_>, running: &Running<'_>) -> Result<Receipt, String> {
    let frozen = frozen(gating.file)?;
    for root in [gating.sets_root, gating.labels_root] {
        if under(gating.receipt, root) {
            return Err(
                "the receipt is not written under the sets' folder or the labels' folder, nor \
                 where either cannot be resolved"
                    .to_owned(),
            );
        }
    }
    let matrix = &frozen.matrix;
    let labelled = labelled(gating.sets_root, gating.labels_root, &matrix.matrix.sets)?;
    let (planned, _) = evaluation::plan(matrix, gating.sets_root, running.root)?;
    let digest = Some(frozen.digest.as_str());
    let report = evaluation::run_under(matrix, &planned, &labelled.expected, running, digest)?;
    let identity: String = matrix.identity().chars().take(16).collect();
    let attempts = evaluation::attempts_under(running.root, &identity, digest)?;
    let mut ended = Vec::new();
    let mut open = 0;
    for planned in &planned {
        match attempts.get(&planned.cell).and_then(|list| list.last()) {
            Some((_, run, end @ (Ended::Completed | Ended::Failed))) => {
                ended.push((planned.cell.clone(), run.clone(), *end));
            }
            _ => open += 1,
        }
    }
    if open > 0 {
        return Err(format!(
            "{open} of the gate's {} cells have not ended; run the gate again to resume. No \
             receipt was written",
            report.cells
        ));
    }
    let runs: Vec<(String, Ended)> = (ended.iter())
        .map(|(_, run, end)| (run.clone(), *end))
        .collect();
    let scored = scored_runs(running.root, &runs)?;
    let gate = &frozen.gate;
    let mut judged: Vec<Judged> = (matrix.matrix.judges.iter())
        .map(|judge| Judged {
            backend: judge.backend.clone(),
            model: judge.model.clone(),
            control: Vec::new(),
            treatment: Vec::new(),
        })
        .collect();
    let mut missing = 0;
    for (cell, run, _) in &ended {
        let key = (cell.set.clone(), cell.item.clone());
        let (Some(expected), Some(family)) =
            (labelled.expected.get(&key), labelled.families.get(&key))
        else {
            missing += 1;
            continue;
        };
        let one = &scored[run];
        let row = Row {
            set: cell.set.clone(),
            item: cell.item.clone(),
            family: family.clone(),
            repetition: cell.repetition,
            expected: *expected,
            computed: one.computed,
            cost_usd: one.cost_usd,
            wall_ms: one.wall_ms,
        };
        let Some(judge) =
            (judged.iter_mut()).find(|j| j.backend == cell.backend && j.model == cell.model)
        else {
            continue;
        };
        if cell.arm == gate.baseline.control {
            judge.control.push(row);
        } else if cell.arm == gate.baseline.treatment {
            judge.treatment.push(row);
        }
    }
    if missing > 0 {
        return Err(format!(
            "{missing} cells are of items with no label, and the gate refuses a missing label; \
             no receipt was written"
        ));
    }
    let receipt = Receipt {
        receipt_version: RECEIPT_VERSION,
        gate_sha256: frozen.digest.clone(),
        matrix: matrix.matrix.name.clone(),
        matrix_identity: matrix.identity(),
        sets: matrix.matrix.sets.clone(),
        labels_sha256: labelled.digests,
        cells: ended.len(),
        control: gate.baseline.control.clone(),
        treatment: gate.baseline.treatment.clone(),
        scored: gate.score(&judged)?,
    };
    let written = serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())? + "\n";
    write_new(gating.receipt, written.as_bytes())?;
    Ok(receipt)
}

/// Writes `bytes` to `path` whole: to a new file beside it first, made by this call and so
/// never through a link, then moved into place, which replaces a link there rather than
/// writing through it.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let refused = |_| "the receipt cannot be written".to_owned();
    let mut beside = PathBuf::from(path);
    beside.as_mut_os_string().push(".partial");
    let mut file = (std::fs::OpenOptions::new().write(true).create_new(true))
        .open(&beside)
        .map_err(refused)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(e) = written.and_then(|()| std::fs::rename(&beside, path)) {
        let _ = std::fs::remove_file(&beside);
        return Err(refused(e));
    }
    Ok(())
}

fn word(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Admitted => "admitted",
        Outcome::Rejected => "rejected",
        Outcome::Inconclusive => "inconclusive",
    }
}

fn share(value: Option<f64>) -> String {
    value.map_or("-".to_owned(), |v| format!("{:.3}", v))
}

/// The receipt as the command prints it: aggregates only.
pub fn text(receipt: &Receipt) -> String {
    let mut out = format!(
        "gate: {}\nmatrix: {} ({})\ncells scored: {}\noutcome: {}\n",
        &receipt.gate_sha256[..16.min(receipt.gate_sha256.len())],
        printable(&receipt.matrix),
        &receipt.matrix_identity[..16.min(receipt.matrix_identity.len())],
        receipt.cells,
        word(receipt.scored.outcome)
    );
    for judge in &receipt.scored.judges {
        let p = &judge.paired;
        let interval = |i: &Option<whizbang_eval::bootstrap::Interval>| {
            i.as_ref().map_or("-".to_owned(), |i| {
                format!("{:.3} [{:.3}, {:.3}]", i.point, i.lower, i.upper)
            })
        };
        out.push_str(&format!(
            "judge {} ({}): {}; items {}, families {}, differing {}; balanced accuracy {} {}, \
             {} {}; gain {}; false-reject increase {}; failed: {}\n",
            printable(&judge.backend),
            printable(&judge.model),
            word(judge.outcome),
            p.items,
            p.families,
            p.differing_families,
            printable(&receipt.control),
            share(p.control.balanced_accuracy),
            printable(&receipt.treatment),
            share(p.treatment.balanced_accuracy),
            interval(&p.gain),
            interval(&p.false_reject_increase),
            if judge.failed.is_empty() {
                "none".to_owned()
            } else {
                judge.failed.join(", ")
            }
        ));
    }
    out
}
