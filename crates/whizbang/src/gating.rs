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
//!
//! **Frozen in its matrix** (plan mE, task M1, items 3 and 4; a gate file of version 3).
//! `whizbang eval matrix` runs the cells, and `eval run --gate` runs nothing: it scores the
//! gate from the runs of the matrix it names, made under no gate, reading only the cells of
//! its two arms (or of its arm on its two sets) under the judges that run both. The matrix
//! must name the gate's file by the SHA-256 of its bytes (`[gates]`), so the gate was frozen
//! before the matrix's first cell; every gate file the matrix names must read, as frozen,
//! and name the matrix back; and the gate's `[confidence] hypotheses` must be the count of
//! every comparison of every gate the matrix names, one for each judge a gate compares.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::contract::record::Decision;
use whizbang_core::identity::Digest;
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::gate::{
    Compared, Gate, Judged, Outcome, RECEIPT_VERSION, RECEIPT_VERSION_FROZEN, Receipt,
};
use whizbang_eval::matrix::{Cell, Matrix};
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
    /// What a gate of version 3 compares in its matrix: each judge it decides, with the
    /// sets it reads under that judge. Empty for version 2, which compares every judge.
    pub comparisons: Vec<Comparison>,
}

impl Frozen {
    /// Whether the gate is scored from its matrix's runs, running nothing (version 3).
    pub fn scored_from_its_matrix(&self) -> bool {
        self.gate.gate_version >= 3
    }
}

/// One judge a gate of version 3 decides: its backend and model, and the sets it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    pub backend: String,
    pub model: String,
    pub sets: Vec<String>,
}

pub(crate) fn read_limited(file: &Path, what: &str) -> Result<Vec<u8>, String> {
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
    let digest = Digest::of_bytes(&bytes).hex();
    if gate.gate_version >= 3 {
        return frozen_in_matrix(file, gate, digest, matrix, &matrix_file);
    }
    if Some(matrix.identity()) != gate.run.matrix_identity {
        return Err(format!(
            "{shown}: the matrix's identity is {}, not the {} the gate was frozen with; \
             nothing was run",
            matrix.identity(),
            gate.run.matrix_identity.as_deref().unwrap_or_default()
        ));
    }
    // Version 2 runs every cell of its matrix as its own: a matrix that lists its cells, or
    // names gates frozen on it, is version 3's.
    if matrix.lists() || !matrix.gates.is_empty() {
        return Err(format!(
            "{shown}: a matrix that lists the cells it runs, or names its gates, takes gates of \
             version 3; nothing was run"
        ));
    }
    let m = &matrix.matrix;
    let mut arms: Vec<String> = m.arms.iter().map(ToString::to_string).collect();
    arms.sort();
    let (control, treatment) = match gate.compared()? {
        Compared::Arms { control, treatment } => (control, treatment),
        Compared::Sets { .. } => return Err(format!("{shown}: version 2 compares two arms")),
    };
    let mut wanted = vec![control, treatment];
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
        digest,
        matrix,
        comparisons: Vec::new(),
    })
}

/// The judges a gate of version 3 decides in its matrix, each with the sets it reads: under
/// two arms, every judge that runs both on a set, over those sets; under two versions of a
/// set, every judge that runs the arm on both. Refused when the gate names an arm or a set
/// the matrix does not have, or compares nothing.
pub fn comparisons(gate: &Gate, matrix: &Matrix) -> Result<Vec<Comparison>, String> {
    let m = &matrix.matrix;
    let arms: Vec<String> = m.arms.iter().map(ToString::to_string).collect();
    let arm_of = |name: &str| -> Result<String, String> {
        let arm: whizbang_eval::matrix::Arm = name.parse()?;
        let arm = arm.to_string();
        if arms.contains(&arm) && arm == name {
            Ok(arm)
        } else {
            Err(format!(
                "the gate names the arm {}, which its matrix does not name so",
                printable(name)
            ))
        }
    };
    let mut out = Vec::new();
    match gate.compared()? {
        Compared::Arms { control, treatment } => {
            let (control, treatment) = (arm_of(&control)?, arm_of(&treatment)?);
            for judge in &m.judges {
                let sets: Vec<String> = (m.sets.iter())
                    .filter(|set| {
                        matrix.runs(set, judge, &control).is_some()
                            && matrix.runs(set, judge, &treatment).is_some()
                    })
                    .cloned()
                    .collect();
                if !sets.is_empty() {
                    out.push(Comparison {
                        backend: judge.backend.clone(),
                        model: judge.model.clone(),
                        sets,
                    });
                }
            }
        }
        Compared::Sets {
            arm,
            control,
            treatment,
        } => {
            let arm = arm_of(&arm)?;
            for set in [&control, &treatment] {
                if !m.sets.contains(set) {
                    return Err(format!(
                        "the gate names the set {}, which its matrix does not",
                        printable(set)
                    ));
                }
            }
            for judge in &m.judges {
                let both = [&control, &treatment]
                    .iter()
                    .all(|set| matrix.runs(set, judge, &arm).is_some());
                if both {
                    out.push(Comparison {
                        backend: judge.backend.clone(),
                        model: judge.model.clone(),
                        sets: vec![control.clone(), treatment.clone()],
                    });
                }
            }
        }
    }
    if out.is_empty() {
        return Err("the gate compares nothing in its matrix: no judge runs both its sides".into());
    }
    Ok(out)
}

/// The most repetitions of any cell a gate of version 3 reads.
fn most_repetitions(
    gate: &Gate,
    matrix: &Matrix,
    comparisons: &[Comparison],
) -> Result<u32, String> {
    let arms: Vec<String> = match gate.compared()? {
        Compared::Arms { control, treatment } => vec![control, treatment],
        Compared::Sets { arm, .. } => vec![arm],
    };
    let mut most = 0;
    for one in comparisons {
        let judge = (matrix.matrix.judges.iter())
            .find(|j| j.backend == one.backend && j.model == one.model);
        let Some(judge) = judge else { continue };
        for set in &one.sets {
            for arm in &arms {
                most = most.max(matrix.runs(set, judge, arm).unwrap_or(0));
            }
        }
    }
    Ok(most)
}

/// Every gate on the matrix that compares two versions of a set: those two sets, under the
/// sets' folder, hold the same items in the same families by id. `eval matrix` refuses a
/// matrix whose gates fail this before any cell runs, and scoring refuses it again.
pub fn versions_hold(sets_root: &Path, gates: &[Named]) -> Result<(), String> {
    for (file, gate, _) in gates {
        if let Compared::Sets {
            control, treatment, ..
        } = gate.compared()?
        {
            let of = |id: &str| sets::find(sets_root, id).map(|set| set.families_by_item());
            if of(&control)? != of(&treatment)? {
                return Err(format!(
                    "{}: {}",
                    printable(&file.file_name().unwrap_or_default().to_string_lossy()),
                    not_versions(&control, &treatment)
                ));
            }
        }
    }
    Ok(())
}

fn not_versions(control: &str, treatment: &str) -> String {
    format!(
        "the sets {} and {} are not two versions of one set: their items or their families \
         differ by id; nothing was run or scored",
        printable(control),
        printable(treatment)
    )
}

/// A gate file a matrix names: its path, its gate and what it compares.
pub type Named = (PathBuf, Gate, Vec<Comparison>);

/// Every gate file the matrix at `matrix_file` names, read and held to its digest, each of
/// version 3 and naming that matrix back, with its file, its gate and its comparisons; and
/// the hypotheses their comparisons count together, one for each. Each gate must correct for
/// that count and state the most repetitions of any cell it reads. `eval matrix` refuses a
/// matrix whose gates do not hold so before anything runs, and `eval run --gate` scores
/// none of them.
pub fn gates_on(matrix_file: &Path, matrix: &Matrix) -> Result<(Vec<Named>, usize), String> {
    let beside = matrix_file.parent().unwrap_or(Path::new("."));
    let unresolved = |_| "a gate file or its matrix cannot be resolved".to_owned();
    let matrix_resolved = matrix_file.canonicalize().map_err(unresolved)?;
    let mut out = Vec::new();
    let mut hypotheses = 0;
    for (name, digest) in &matrix.gates {
        let shown = printable(name);
        let file = beside.join(name);
        let bytes = read_limited(&file, "a gate file")?;
        if Digest::of_bytes(&bytes).hex() != *digest {
            return Err(format!(
                "the gate file {shown} is not the one the matrix names by its digest: the gates \
                 frozen on the matrix changed after it was written; nothing was run"
            ));
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| format!("{shown}: not UTF-8"))?;
        let gate = Gate::parse(text).map_err(|why| format!("{shown}: {why}"))?;
        if gate.gate_version < 3 {
            return Err(format!(
                "{shown}: a gate the matrix names is of version 3, frozen in its matrix"
            ));
        }
        let named = file
            .parent()
            .unwrap_or(Path::new("."))
            .join(&gate.run.matrix);
        if named.canonicalize().map_err(unresolved)? != matrix_resolved {
            return Err(format!(
                "{shown}: it names another matrix than the one naming it"
            ));
        }
        let compared = comparisons(&gate, matrix).map_err(|why| format!("{shown}: {why}"))?;
        let most = most_repetitions(&gate, matrix, &compared)?;
        if most != gate.repetitions.count {
            return Err(format!(
                "{shown}: the gate's repetitions are {}, and the most of any cell it reads are \
                 {most}",
                gate.repetitions.count
            ));
        }
        hypotheses += compared.len();
        out.push((file, gate, compared));
    }
    // The matrix may count more hypotheses than its gates compare: the judge's rule's beside
    // them (plan mE, task M1, item 6). Then every gate corrects for that count, and resamples
    // as that count needs.
    let counted = match matrix.hypotheses {
        Some(declared) if declared < hypotheses => {
            return Err(format!(
                "the matrix counts {declared} hypotheses, and its gates compare {hypotheses}"
            ));
        }
        Some(declared) => declared,
        None => hypotheses,
    };
    for (file, gate, _) in &out {
        let shown = printable(&file.file_name().unwrap_or_default().to_string_lossy());
        if gate.confidence.hypotheses != counted {
            return Err(format!(
                "{shown}: the gate corrects for {} hypotheses, and the gates on its matrix compare \
                 {counted}; each counts every one",
                gate.confidence.hypotheses
            ));
        }
        let wanted = whizbang_eval::matrix::resamples_for(counted);
        if matrix.hypotheses.is_some() && gate.families.resamples != wanted {
            return Err(format!(
                "{shown}: the gate resamples {} times, and a round of {counted} hypotheses \
                 resamples {wanted}",
                gate.families.resamples
            ));
        }
    }
    Ok((out, hypotheses))
}

/// A gate of version 3 and its matrix: the matrix names this file's digest; every gate it
/// names holds; the gate's hypotheses are their count; and its repetitions are the most of
/// any cell it reads.
fn frozen_in_matrix(
    file: &Path,
    gate: Gate,
    digest: String,
    matrix: Matrix,
    matrix_file: &Path,
) -> Result<Frozen, String> {
    let shown = printable(&file.display().to_string());
    let unresolved = |_| "a gate file or its matrix cannot be resolved".to_owned();
    let this = file.canonicalize().map_err(unresolved)?;
    let beside = matrix_file.parent().unwrap_or(Path::new("."));
    let named = (matrix.gates.iter()).any(|(name, frozen)| {
        *frozen == digest
            && beside
                .join(name)
                .canonicalize()
                .is_ok_and(|that| that == this)
    });
    if !named {
        return Err(format!(
            "{shown}: its matrix does not name this gate file by the SHA-256 of its bytes \
             ({digest}), so it was not frozen before the matrix ran; nothing was scored"
        ));
    }
    // Every gate on the matrix holds, this one among them: its hypotheses are the count of
    // them all, and its repetitions the most of any cell it reads.
    gates_on(matrix_file, &matrix)?;
    let compared = comparisons(&gate, &matrix).map_err(|why| format!("{shown}: {why}"))?;
    Ok(Frozen {
        gate,
        digest,
        matrix,
        comparisons: compared,
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
pub(crate) fn under(path: &Path, root: &Path) -> bool {
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decided {
    pub computed: Computed,
    pub cost_usd: f64,
    pub wall_ms: u64,
    /// When the run ended, in milliseconds since the Unix epoch.
    pub ended_ms: u64,
}

/// Each run's verdict, cost and wall time, read from its record in the store at `root`.
fn scored_runs(root: &Path, runs: &[(String, Ended)]) -> Result<BTreeMap<String, Decided>, String> {
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
            Decided {
                computed,
                cost_usd: micros as f64 / 1_000_000.0,
                wall_ms,
                ended_ms: time.end_ms,
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
    receipt_outside(gating)?;
    if frozen.scored_from_its_matrix() {
        return score_frozen(gating, &frozen, running.root);
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
    let compared = gate.compared()?;
    let (control, treatment) = compared.sides();
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
        if cell.arm == control {
            judge.control.push(row);
        } else if cell.arm == treatment {
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
        control: control.to_owned(),
        treatment: treatment.to_owned(),
        arm: None,
        relabelled: None,
        splits: BTreeMap::new(),
        admits: None,
        date: None,
        scored: gate.score(&judged)?,
    };
    let written = serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())? + "\n";
    write_new(gating.receipt, written.as_bytes())?;
    Ok(receipt)
}

/// Refuses a receipt asked for under the sets' folder or the labels' folder, or where
/// either cannot be resolved.
fn receipt_outside(gating: &Gating<'_>) -> Result<(), String> {
    for root in [gating.sets_root, gating.labels_root] {
        if under(gating.receipt, root) {
            return Err(
                "the receipt is not written under the sets' folder or the labels' folder, nor \
                 where either cannot be resolved"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

/// `eval run --gate` on a gate of version 3: scores it from the runs its matrix made in the
/// store at `root`, running nothing, and writes its receipt.
pub fn score(gating: &Gating<'_>, root: &Path) -> Result<Receipt, String> {
    let frozen = frozen(gating.file)?;
    receipt_outside(gating)?;
    if !frozen.scored_from_its_matrix() {
        return Err("a gate of version 2 runs its own matrix: `eval run --gate` runs it".into());
    }
    score_frozen(gating, &frozen, root)
}

/// The items of each set by id.
fn items_of(sets_root: &Path, ids: &[String]) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut items = Vec::new();
    for id in ids {
        let set = sets::find(sets_root, id)?;
        let its: Vec<String> = set.jobs.iter().map(|(item, _)| item.clone()).collect();
        items.push((id.clone(), its));
    }
    Ok(items)
}

impl Frozen {
    /// The sets a gate of version 3 reads, in order.
    pub fn sets(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for one in &self.comparisons {
            for set in &one.sets {
                if !ids.contains(set) {
                    ids.push(set.clone());
                }
            }
        }
        ids.sort();
        ids
    }

    /// The cells a gate of version 3 reads over these items of its sets: of its two arms, or
    /// of its arm on its two sets, on the sets each judge it compares reads there.
    pub fn cells(&self, items: &[(String, Vec<String>)]) -> Result<Vec<Cell>, String> {
        let compared = self.gate.compared()?;
        let reads = |cell: &Cell| {
            let judge = (self.comparisons.iter())
                .find(|c| c.backend == cell.backend && c.model == cell.model);
            let Some(judge) = judge else { return false };
            let arm = match &compared {
                Compared::Arms { control, treatment } => {
                    cell.arm == *control || cell.arm == *treatment
                }
                Compared::Sets { arm, .. } => cell.arm == *arm,
            };
            arm && judge.sets.contains(&cell.set)
        };
        Ok(self.matrix.cells(items).into_iter().filter(reads).collect())
    }
}

/// What scoring a gate of version 3 reads: each of its sets' items, their labels and
/// families, and how each cell of the gate that has ended was decided. A cell of the gate
/// missing from `ended` has not ended.
#[derive(Debug, Clone, Default)]
pub struct Read {
    pub items: Vec<(String, Vec<String>)>,
    pub labelled: Labelled,
    /// Each set's split, by its id.
    pub splits: BTreeMap<String, String>,
    pub ended: BTreeMap<Cell, Decided>,
}

/// Scores a gate of version 3 from its matrix's runs in the store at `root`, and writes its
/// receipt: the store read ([`Read`]), then scored ([`score_read`]).
fn score_frozen(gating: &Gating<'_>, frozen: &Frozen, root: &Path) -> Result<Receipt, String> {
    let ids = frozen.sets();
    let items = items_of(gating.sets_root, &ids)?;
    let labelled = labelled(gating.sets_root, gating.labels_root, &ids)?;
    let mut splits = BTreeMap::new();
    for id in &ids {
        let set = sets::find(gating.sets_root, id)?;
        splits.insert(id.clone(), set.manifest.set.split.to_string());
    }
    let cells = frozen.cells(&items)?;
    let identity: String = frozen.matrix.identity().chars().take(16).collect();
    let attempts = evaluation::attempts_under(root, &identity, None)?;
    let mut runs = Vec::new();
    for cell in &cells {
        if let Some((_, run, end @ (Ended::Completed | Ended::Failed))) =
            attempts.get(cell).and_then(|list| list.last())
        {
            runs.push((cell.clone(), run.clone(), *end));
        }
    }
    let listed: Vec<(String, Ended)> = (runs.iter())
        .map(|(_, run, end)| (run.clone(), *end))
        .collect();
    let decided = scored_runs(root, &listed)?;
    let read = Read {
        items,
        labelled,
        splits,
        ended: (runs.into_iter())
            .map(|(cell, run, _)| (cell, decided[&run]))
            .collect(),
    };
    let receipt = score_read(frozen, &read)?;
    let written = serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())? + "\n";
    write_new(gating.receipt, written.as_bytes())?;
    Ok(receipt)
}

/// Scores a gate of version 3 on what was read of its sets and its matrix's runs: only the
/// cells of its two arms, or of its arm on its two sets, under the judges it compares, each
/// of which must have ended. Two versions of a set must hold the same items in the same
/// families by id; they are paired item by item, and an item whose label differs between
/// them is counted apart and scored under neither. Reads and writes nothing.
pub fn score_read(frozen: &Frozen, read: &Read) -> Result<Receipt, String> {
    let gate = &frozen.gate;
    let matrix = &frozen.matrix;
    let compared = gate.compared()?;
    let ids = frozen.sets();
    let labelled = &read.labelled;
    let versions = match &compared {
        Compared::Sets {
            control, treatment, ..
        } => Some((control.as_str(), treatment.as_str())),
        Compared::Arms { .. } => None,
    };
    let items_of = |id: &str| -> Vec<String> {
        (read.items.iter())
            .find(|(set, _)| set == id)
            .map(|(_, its)| its.clone())
            .unwrap_or_default()
    };
    if let Some((control, treatment)) = versions {
        let families = |id: &str| -> BTreeMap<String, Option<String>> {
            (items_of(id).into_iter())
                .map(|item| {
                    let family = labelled
                        .families
                        .get(&(id.to_owned(), item.clone()))
                        .cloned();
                    (item, family)
                })
                .collect()
        };
        if families(control) != families(treatment) {
            return Err(not_versions(control, treatment));
        }
    }
    let cells = frozen.cells(&read.items)?;
    let open = cells
        .iter()
        .filter(|c| !read.ended.contains_key(*c))
        .count();
    if open > 0 {
        return Err(format!(
            "{open} of the gate's {} cells have not ended; run its matrix (`whizbang eval \
             matrix`) to finish them. No receipt was written",
            cells.len()
        ));
    }
    let mut judged: Vec<Judged> = (frozen.comparisons.iter())
        .map(|one| Judged {
            backend: one.backend.clone(),
            model: one.model.clone(),
            control: Vec::new(),
            treatment: Vec::new(),
        })
        .collect();
    // Of two versions of a set, an item whose label differs between them is counted apart
    // and scored under neither version's truth.
    let relabelled: Vec<String> = match &versions {
        Some((control, treatment)) => (items_of(control).into_iter())
            .filter(|item| {
                let of = |set: &str| labelled.expected.get(&(set.to_owned(), item.clone()));
                of(control) != of(treatment)
            })
            .collect(),
        None => Vec::new(),
    };
    let mut missing = 0;
    for cell in &cells {
        let key = (cell.set.clone(), cell.item.clone());
        let (Some(expected), Some(family)) =
            (labelled.expected.get(&key), labelled.families.get(&key))
        else {
            missing += 1;
            continue;
        };
        if versions.is_some() && relabelled.contains(&cell.item) {
            continue;
        }
        let one = &read.ended[cell];
        // Two versions of a set are paired item by item: both sides under one name.
        let set = match &versions {
            Some((_, treatment)) => (*treatment).to_owned(),
            None => cell.set.clone(),
        };
        let row = Row {
            set,
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
        let control_side = match &compared {
            Compared::Arms { control, .. } => cell.arm == *control,
            Compared::Sets { control, .. } => cell.set == *control,
        };
        if control_side {
            judge.control.push(row);
        } else {
            judge.treatment.push(row);
        }
    }
    if missing > 0 {
        return Err(format!(
            "{missing} cells are of items with no label, and the gate refuses a missing label; \
             no receipt was written"
        ));
    }
    let (control, treatment) = compared.sides();
    let digests = (labelled.digests.iter())
        .filter(|(id, _)| ids.contains(id))
        .map(|(id, digest)| (id.clone(), digest.clone()))
        .collect();
    Ok(Receipt {
        receipt_version: RECEIPT_VERSION_FROZEN,
        gate_sha256: frozen.digest.clone(),
        matrix: matrix.matrix.name.clone(),
        matrix_identity: matrix.identity(),
        sets: ids.clone(),
        labels_sha256: digests,
        cells: cells.len(),
        control: control.to_owned(),
        treatment: treatment.to_owned(),
        arm: match &compared {
            Compared::Sets { arm, .. } => Some(arm.clone()),
            Compared::Arms { .. } => None,
        },
        relabelled: versions.map(|_| relabelled.len()),
        splits: (read.splits.iter())
            .filter(|(id, _)| ids.contains(id))
            .map(|(id, split)| (id.clone(), split.clone()))
            .collect(),
        admits: gate.admits.clone(),
        // The day the last cell the gate read ended: the receipt is the same however often
        // it is scored.
        date: (cells.iter())
            .filter_map(|cell| read.ended.get(cell))
            .map(|one| one.ended_ms)
            .max()
            .map(day_of),
        scored: gate.score(&judged)?,
    })
}

/// The day, in UTC, of a time in milliseconds since the Unix epoch, as `YYYY-MM-DD`: the
/// civil date by the usual arithmetic on eras of 400 years.
pub fn day_of(ms: u64) -> String {
    let shifted = ms / 86_400_000 + 719_468;
    let (era, day_of_era) = (shifted / 146_097, shifted % 146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
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
    if let Some(arm) = &receipt.arm {
        out.push_str(&format!(
            "two versions of a set under {}: {} against {}\n",
            printable(arm),
            printable(&receipt.treatment),
            printable(&receipt.control)
        ));
    }
    if let Some(relabelled) = receipt.relabelled {
        out.push_str(&format!(
            "items whose label differs between the versions, scored under neither: {relabelled}\n"
        ));
    }
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
