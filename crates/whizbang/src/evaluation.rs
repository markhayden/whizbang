//! What `whizbang eval` does around the evaluation sets (plan 2C, task E1): a set's
//! pictures fetched by hash and brought into a store, and a matrix of cells run as runs
//! of `check` through the executor, resumably.
//!
//! # Fetching
//!
//! A set names where its pictures are (`images_repo = "github.com/<owner>/<repo>"`) and
//! the revision (`images_rev`). Each picture is asked for at
//! `<base>/<owner>/<repo>/<revision>/<set id>/<path>`, where the base is
//! [`RAW_ORIGIN`], by a GET that carries nothing but that address: no credential, no
//! body, nothing of the owner's. Before anything is asked, the base's origin must be one
//! the config approves as a recipient (`[egress.recipients]`), or nothing is fetched; the
//! download client is then made over that one origin alone, so a redirect anywhere else
//! is refused by the client too. What arrives is held to [`PICTURE_LIMIT`] bytes, and is
//! imported only when its SHA-256 is the one the item's job pins: a picture whose digest
//! is another is refused and nothing of it is imported. A picture the store holds
//! already (an image brought in from a file of that digest) is not asked for again.
//!
//! # Running a matrix
//!
//! Every cell is one run of `check`, with the cell in the run's arguments (`cell`): the
//! matrix's name and identity, the set, the item, the judge's backend and model, the arm,
//! the repetition and the attempt. A matrix run again over the same store resumes: a cell
//! with a completed attempt is not run again; a cell whose last attempt failed (an error,
//! or what it was given found invalid) is reported as failed and is not run again, so a
//! failure is never replaced by a later success; and a cell whose last attempt was
//! interrupted (cancelled, or ended for it by the next open after its process died) is
//! run again as its next attempt.
//!
//! **Spend.** Each run is admitted as any call of `check` is, reserving its own bound; at
//! most the matrix's `concurrency` runs are started and not yet waited for at once, so at
//! most that many reservations are held. Before the first run, the matrix's forecast
//! (`whizbang_eval::matrix::forecast`) of the cells still to run is set against what the
//! store's allowance has left, and a matrix whose forecast passes it is refused before
//! anything runs.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::record::{ArgumentValue, RunRecord, Status};
use whizbang_core::contract::{
    Answer, Call, Cancellation, Executor, Recovery, Request, RunId, Secrets, Value,
};
use whizbang_core::egress::{
    Address, CheckedDownloader, DownloadRequest, Downloaded, Downloader, NotAllowed, Origin,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::matrix::{self, Cell, Matrix};
use whizbang_eval::sets::{self, Judging, Set};
use whizbang_squint::job::{Job, Role as JobRole};

use crate::door::{CHECK_OUTPUT_TOKENS, printable};
use crate::import;

/// The base the pictures of a set are fetched from: the raw files of a public repository
/// at a revision (plan 2C, "Origins (E)").
pub const RAW_ORIGIN: &str = "https://raw.githubusercontent.com";

/// The most bytes of one picture that are fetched: the largest file an import takes.
pub const PICTURE_LIMIT: u64 = import::MAX_FILE_BYTES;

/// The name of `check`'s argument that holds a run's cell.
pub const CELL_ARGUMENT: &str = "cell";

/// What the binary knows of judging that a set is checked against: the rule files
/// `check` ships, and whether a job file is one it takes under its use case.
pub fn judging() -> Judging<'static> {
    fn rules(base: &str) -> Option<&'static str> {
        (whizbang_squint::use_case::TABLE.iter())
            .find(|(name, _)| *name == base)
            .map(|(_, text)| *text)
    }
    fn job(text: &str) -> Result<(), String> {
        let job = Job::from_toml(text)?;
        whizbang_squint::use_case::resolve(&job).map(drop)
    }
    Judging {
        rules: &rules,
        job: &job,
    }
}

/// The repository the sets' pictures are in: a set that names any other is not fetched.
pub const SETS_REPOSITORY: &str = "github.com/markhayden/wafflery-evalsets";

/// The address of one picture of a set: `<base>/<owner>/<repo>/<revision>/<set id>/<path>`.
/// The set must name [`SETS_REPOSITORY`] and a revision of 40 hex digits; the path is the
/// job's, which the set's check holds to plain names.
pub fn picture_address(base: &str, set: &Set, path: &str) -> Result<String, String> {
    let meta = &set.manifest.set;
    let plain = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    // The one repository the sets' pictures are fetched from, whatever else a set names.
    let Some(repo) = meta.images_repo.strip_prefix("github.com/") else {
        return Err(format!("{}: images_repo is not {SETS_REPOSITORY}", meta.id));
    };
    if meta.images_repo != SETS_REPOSITORY {
        return Err(format!("{}: images_repo is not {SETS_REPOSITORY}", meta.id));
    }
    let rev = meta.images_rev.as_deref().unwrap_or_default();
    if rev.len() != 40 || !rev.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "{}: images_rev is not a revision of 40 hex digits",
            meta.id
        ));
    }
    if !path.split('/').all(plain) || !meta.id.split('/').all(plain) {
        return Err(format!("{}: a picture's path is not plain names", meta.id));
    }
    Ok(format!(
        "{}/{repo}/{rev}/{}/{path}",
        base.trim_end_matches('/'),
        meta.id
    ))
}

/// What makes the download client over the origins it is handed: the network's
/// (`whizbang_backends::net::downloader`), or a test's fake behind a checked downloader.
pub type MakeDownloader<'a> =
    &'a dyn Fn(&[Origin], &Secrets) -> Result<CheckedDownloader, NotAllowed>;

/// What `eval sets fetch` is given beside the set: the base to fetch from, the origins
/// the config approves, its secrets, and what makes the download client.
pub struct Fetching<'a> {
    pub base: &'a str,
    pub approved: &'a [Origin],
    pub secrets: &'a Secrets,
    pub make: MakeDownloader<'a>,
}

/// The pictures of a set the store does not hold yet, each once: its pin, and the item
/// and image a complaint names it by.
fn wanted(set: &Set, root: &Path) -> Result<(Vec<sets::Picture>, usize), String> {
    let held = import::pictures_by_file(root)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut wanted = Vec::new();
    let mut present = 0;
    for picture in set.pictures()? {
        if !seen.insert(picture.sha256.clone()) {
            continue;
        }
        let digest = Digest::from_hex(&picture.sha256)
            .ok_or_else(|| format!("{}: a pin is no SHA-256", set.id()))?;
        if held.contains_key(&digest) {
            present += 1;
        } else {
            wanted.push(picture);
        }
    }
    Ok((wanted, present))
}

/// `whizbang eval sets fetch`: every picture of the set the store does not hold, fetched
/// from the base by its pin and brought in as a picture. The module's description says
/// what is checked, and when.
pub fn fetch(
    set: &Set,
    root: &Path,
    recovery: &Recovery,
    how: &Fetching<'_>,
) -> Result<String, String> {
    let origin = Origin::of(how.base, how.secrets)
        .map_err(|why| format!("the base to fetch from is no origin: {why}"))?;
    if !how.approved.contains(&origin) {
        return Err(format!(
            "{origin} is not an origin the config approves as a recipient \
             (`[egress.recipients]`), so nothing was fetched"
        ));
    }
    let (wanted, present) = wanted(set, root)?;
    let downloader = (how.make)(std::slice::from_ref(&origin), how.secrets)
        .map_err(|why| format!("the download client could not be made: {why}"))?;
    let mut fetched = 0;
    for picture in &wanted {
        let named = format!(
            "{}: item {} image {}",
            set.id(),
            picture.item,
            picture.image
        );
        let address = picture_address(how.base, set, &picture.path)?;
        let address = Address::of(&address, how.secrets)
            .map_err(|why| format!("{named}: no address: {why}"))?;
        let bytes = download(&downloader, address).map_err(|why| format!("{named}: {why}"))?;
        if Digest::of_bytes(&bytes).hex() != picture.sha256 {
            return Err(format!(
                "{named}: what arrived is not the picture its job pins (another SHA-256), so \
                 nothing of it was imported"
            ));
        }
        import::picture_of_bytes(
            &bytes,
            &named,
            Some(&picture.sha256),
            root,
            recovery.clone(),
        )?;
        fetched += 1;
    }
    Ok(format!(
        "{}: fetched {fetched} pictures, {present} already there\n",
        printable(set.id())
    ))
}

/// One picture's bytes, at most [`PICTURE_LIMIT`]: one byte more is refused.
fn download(downloader: &CheckedDownloader, from: Address) -> Result<Vec<u8>, String> {
    let request = DownloadRequest::new(from, PICTURE_LIMIT + 1).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    let mut over = false;
    let ended = downloader.download(&request, &Cancellation::new(), &mut |piece| {
        bytes.extend_from_slice(piece);
        over = bytes.len() as u64 > PICTURE_LIMIT;
        !over
    });
    match ended.map_err(|fault| format!("the download failed: {fault}"))? {
        Downloaded::Moved(_) => {
            Err("the picture was moved elsewhere, and is not followed".to_owned())
        }
        Downloaded::Bytes(_) if over => Err(format!("the picture is over {PICTURE_LIMIT} bytes")),
        Downloaded::Bytes(_) => Ok(bytes),
    }
}

/// `whizbang eval sets fetch --from <dir>`: the same pictures, from a folder the owner
/// placed by hand, laid out as the repository is (`<dir>/<set id>/<path>`), by the same
/// pins, each read through no link. Nothing is fetched.
pub fn from_folder(
    set: &Set,
    dir: &Path,
    root: &Path,
    recovery: &Recovery,
) -> Result<String, String> {
    let (wanted, present) = wanted(set, root)?;
    for picture in &wanted {
        // Read through no link, from the folder down (`sets::read_under`).
        let relative = format!("{}/{}", set.id(), picture.path);
        let bytes = sets::read_under(dir, &relative, PICTURE_LIMIT)?;
        let shown = dir.join(&relative).display().to_string();
        import::picture_of_bytes(
            &bytes,
            &shown,
            Some(&picture.sha256),
            root,
            recovery.clone(),
        )?;
    }
    Ok(format!(
        "{}: imported {} pictures from the folder, {present} already there\n",
        printable(set.id()),
        wanted.len()
    ))
}

// --- the runner ---------------------------------------------------------------------------------

/// A run's cell as `check`'s argument `cell` holds it: the cell, the matrix's name and the
/// first 16 hex digits of its identity, and the attempt from 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellArgument {
    pub matrix: String,
    pub identity: String,
    #[serde(flatten)]
    pub cell: Cell,
    pub attempt: u32,
}

/// One cell, ready to run: its job and its images' identities in the job's order.
#[derive(Debug, Clone)]
pub struct Planned {
    pub cell: Cell,
    job: Job,
    images: Vec<ArtifactId>,
}

/// The matrix's cells over the sets under `sets_root`, each with its job and its pictures
/// as the store at `root` holds them; and each set's labels as the set has them now, by
/// item. A picture the store does not hold is refused, naming the set to fetch.
#[allow(clippy::type_complexity)]
pub fn plan(
    matrix: &Matrix,
    sets_root: &Path,
    root: &Path,
) -> Result<(Vec<Planned>, BTreeMap<(String, String), sets::Expected>), String> {
    let held = import::pictures_by_file(root)?;
    let mut items = Vec::new();
    let mut jobs = BTreeMap::new();
    let mut labels = BTreeMap::new();
    for id in &matrix.matrix.sets {
        let set = sets::find(sets_root, id)?;
        for label in set.labels.iter().flat_map(|labels| &labels.label) {
            labels.insert((id.clone(), label.item.clone()), label.expected);
        }
        let mut its = Vec::new();
        for (item, text) in &set.jobs {
            let job = Job::from_toml(text).map_err(|why| format!("{id}: item {item}: {why}"))?;
            let mut images = Vec::new();
            for image in &job.images {
                let pin = image
                    .source
                    .as_ref()
                    .and_then(|source| source.sha256.as_deref());
                let found = pin
                    .and_then(|pin| Digest::from_hex(&pin.to_ascii_lowercase()))
                    .and_then(|digest| held.get(&digest));
                let Some(found) = found else {
                    return Err(format!(
                        "{id}: the picture of item {item} image {} is not in the store: fetch \
                         the set first (`whizbang eval sets fetch --set {id}`)",
                        image.id
                    ));
                };
                images.push(*found);
            }
            // An arm that measures tiles for this use case cannot take a job that relates
            // a pair: the matrix is refused before anything runs.
            let paired = (job.images.iter()).any(|image| image.role != JobRole::Candidate);
            for arm in &matrix.matrix.arms {
                let tiles = arm
                    .detail_for(&job.use_case)
                    .is_some_and(|d| d.split(',').any(|d| d == whizbang_squint::details::TILES));
                if tiles && paired {
                    return Err(format!(
                        "{id}: item {item}: arm {arm}: {}; nothing was run",
                        whizbang_squint::details::tiles_refused(&job.use_case)
                    ));
                }
            }
            jobs.insert((id.clone(), item.clone()), (job, images));
            its.push(item.clone());
        }
        items.push((id.clone(), its));
    }
    let planned = matrix
        .cells(&items)
        .into_iter()
        .map(|cell| {
            let (job, images) = jobs[&(cell.set.clone(), cell.item.clone())].clone();
            Planned { cell, job, images }
        })
        .collect();
    Ok((planned, labels))
}

/// How one attempt of a cell ended, as its run's record says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ended {
    Completed,
    /// An error, or what it was given found invalid: reported, and not run again.
    Failed,
    /// Cancelled, or ended by the next open after its process died: run again.
    Interrupted,
}

/// Each cell's attempts: the attempt, its run and how it ended, in order.
pub type Attempts = BTreeMap<Cell, Vec<(u32, String, Ended)>>;

/// Every attempt of each cell of the matrix of this identity the store at `root` holds:
/// its attempt, its run and how it ended, in the order of the attempts.
pub fn attempts(root: &Path, identity: &str) -> Result<Attempts, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let mut found: Attempts = BTreeMap::new();
    for id in generation.runs().keys() {
        let stored = whizbang_core::contract::record::stored(&generation, id, |e| reader.read(e));
        let Some(Ok(record)) = stored else { continue };
        let Some(argument) = cell_of(&record) else {
            continue;
        };
        if argument.identity != identity {
            continue;
        }
        let outcome = record.outcome();
        let ended = match outcome.status {
            _ if !outcome.lost.is_empty() => Ended::Interrupted,
            Status::Completed => Ended::Completed,
            Status::Cancelled => Ended::Interrupted,
            Status::Error | Status::Invalid => Ended::Failed,
        };
        found
            .entry(argument.cell)
            .or_default()
            .push((argument.attempt, id.clone(), ended));
    }
    for list in found.values_mut() {
        list.sort();
    }
    Ok(found)
}

/// The cell a run of `check` was, when its record says so.
pub fn cell_of(record: &RunRecord) -> Option<CellArgument> {
    if record.asked().action.as_deref() != Some(whizbang_squint::check::NAME) {
        return None;
    }
    let argument = record
        .asked()
        .arguments
        .iter()
        .find(|a| a.name == CELL_ARGUMENT)?;
    match &argument.value {
        ArgumentValue::Text(text) => serde_json::from_str(text).ok(),
        _ => None,
    }
}

/// The call of `check` one attempt of a cell is.
fn call_of(planned: &Planned, argument: &CellArgument) -> Result<Call, String> {
    let arm: matrix::Arm = planned.cell.arm.parse()?;
    let mut call = Call::new(whizbang_squint::check::NAME)
        .argument("use_case", Value::text(&planned.job.use_case))
        .argument("job", Value::text(&planned.job.to_json()))
        .argument("max_output_tokens", Value::Integer(CHECK_OUTPUT_TOKENS))
        .argument("tools", Value::text(&arm.offered()))
        .argument("details", Value::text(arm.details()))
        .argument("projection", Value::text(arm.projection()))
        .argument("second_look", Value::text(arm.second_look()))
        .argument(
            CELL_ARGUMENT,
            Value::text(&serde_json::to_string(argument).map_err(|e| e.to_string())?),
        );
    if let Some(detail) = arm.detail_for(&planned.job.use_case) {
        call = call.argument("detail", Value::text(&detail));
    }
    if planned.cell.backend != "default" {
        call = call.argument("judge", Value::text(&planned.cell.backend));
    }
    for image in &planned.images {
        call = call.input("images", *image);
    }
    Ok(call)
}

/// What the runner is given beside the matrix and its planned cells.
pub struct Running<'a> {
    pub executor: &'a dyn Executor,
    pub root: &'a Path,
    /// What the store's allowance has left, in millionths of a US dollar.
    pub left_micros: u64,
    /// Asked before each run is started: true stops the matrix there, and it is resumed
    /// by running it again.
    pub stop: &'a dyn Fn() -> bool,
}

/// What a matrix's run did, and what the store now holds of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub cells: usize,
    /// Attempts started by this run of the matrix.
    pub started: usize,
    /// Cells whose last attempt completed, failed, or was interrupted, or has none.
    pub completed: usize,
    pub failed: usize,
    pub interrupted: usize,
    pub not_run: usize,
    /// Of the completed cells whose verdict was read: the verdicts by word, and how many
    /// agree with their item's label as the set has it now.
    pub verdicts: BTreeMap<String, usize>,
    pub labelled: usize,
    pub agreed: usize,
}

/// Runs every cell still to run, at most `concurrency` at once, and reports on every
/// cell of the matrix as the store then holds it. The forecast of the cells still to
/// run is set against what is left first, and a matrix whose forecast passes it is
/// refused before anything runs; so is one whose first call of a judge and an arm the
/// executor refuses (a judge the config does not give, a tool it does not link).
pub fn run(
    matrix: &Matrix,
    planned: &[Planned],
    labels: &BTreeMap<(String, String), sets::Expected>,
    running: &Running<'_>,
) -> Result<Report, String> {
    let identity: String = matrix.identity().chars().take(16).collect();
    let earlier = attempts(running.root, &identity)?;
    // What is still to run: each cell with no attempt, or whose last was interrupted.
    let mut todo = Vec::new();
    for planned in planned {
        let last = earlier.get(&planned.cell).and_then(|list| list.last());
        match last {
            None => todo.push((planned, 1)),
            Some((attempt, _, Ended::Interrupted)) => todo.push((planned, attempt + 1)),
            Some(_) => {}
        }
    }
    let cells: Vec<Cell> = todo.iter().map(|(p, _)| p.cell.clone()).collect();
    let forecast = matrix::forecast(&cells)?;
    if forecast > running.left_micros {
        return Err(format!(
            "the matrix's forecast for its {} runs still to run is {} millionths of a dollar, \
             more than the {} the store's allowance has left; nothing was run",
            cells.len(),
            forecast,
            running.left_micros
        ));
    }
    let argument = |planned: &Planned, attempt| CellArgument {
        matrix: matrix.matrix.name.clone(),
        identity: identity.clone(),
        cell: planned.cell.clone(),
        attempt,
    };
    // Each judge and arm prepares before anything runs.
    let mut prepared = std::collections::BTreeSet::new();
    for (planned, attempt) in &todo {
        let key = (
            planned.cell.backend.clone(),
            planned.cell.arm.clone(),
            planned.cell.set.clone(),
        );
        if prepared.insert(key) {
            let call = call_of(planned, &argument(planned, *attempt))?;
            running
                .executor
                .prepare(&call)
                .map_err(|why| format!("the matrix cannot run, nothing was run: {why}"))?;
        }
    }
    let at_once = usize::try_from(matrix.matrix.concurrency)
        .unwrap_or(1)
        .max(1);
    let mut started = 0;
    let mut open: std::collections::VecDeque<RunId> = std::collections::VecDeque::new();
    let mut failure = None;
    for (planned, attempt) in &todo {
        if (running.stop)() {
            break;
        }
        // At most `at_once` runs started and not waited for.
        while open.len() >= at_once {
            if let Some(run) = open.pop_front() {
                running.executor.wait(&run).map_err(|e| e.to_string())?;
            }
        }
        let call = call_of(planned, &argument(planned, *attempt))?;
        match running.executor.start(&Request::new(call)) {
            Ok(Answer::Started(run)) => {
                open.push_back(run);
                started += 1;
            }
            Ok(Answer::Ran(_) | Answer::Repeat(_)) => started += 1,
            Err(why) => {
                failure = Some(why.to_string());
                break;
            }
        }
    }
    while let Some(run) = open.pop_front() {
        running.executor.wait(&run).map_err(|e| e.to_string())?;
    }
    if let Some(why) = failure {
        return Err(format!(
            "a run of the matrix was refused, and the matrix stopped; run it again to resume: {why}"
        ));
    }
    let mut report = summary(planned, labels, running.root, &identity)?;
    report.started = started;
    Ok(report)
}

/// What the store holds of each cell of the matrix: its last attempt's end, and the
/// verdicts of the completed cells joined with their labels as the set has them now.
fn summary(
    planned: &[Planned],
    labels: &BTreeMap<(String, String), sets::Expected>,
    root: &Path,
    identity: &str,
) -> Result<Report, String> {
    let now = attempts(root, identity)?;
    let mut report = Report {
        cells: planned.len(),
        ..Report::default()
    };
    for planned in planned {
        let last = now.get(&planned.cell).and_then(|list| list.last());
        let Some((_, run, ended)) = last else {
            report.not_run += 1;
            continue;
        };
        match ended {
            Ended::Failed => report.failed += 1,
            Ended::Interrupted => report.interrupted += 1,
            Ended::Completed => {
                report.completed += 1;
                let stated = RunId::parse(run)
                    .ok()
                    .and_then(|run| crate::door::verdict(root, &run));
                let Some(stated) = stated else { continue };
                *report.verdicts.entry(stated.word.clone()).or_default() += 1;
                let key = (planned.cell.set.clone(), planned.cell.item.clone());
                if let Some(expected) = labels.get(&key) {
                    report.labelled += 1;
                    let word = serde_json::to_value(expected).ok();
                    if word.as_ref().and_then(|w| w.as_str()) == Some(stated.word.as_str()) {
                        report.agreed += 1;
                    }
                }
            }
        }
    }
    Ok(report)
}

impl Report {
    /// The report as the command prints it.
    pub fn text(&self) -> String {
        let verdicts: Vec<String> = (self.verdicts.iter())
            .map(|(word, count)| format!("{} {count}", printable(word)))
            .collect();
        format!(
            "cells: {}\nstarted now: {}\ncompleted: {}\nfailed: {}\ninterrupted: {}\nnot run: {}\n\
             verdicts: {}\nagreeing with their labels: {} of {}\n",
            self.cells,
            self.started,
            self.completed,
            self.failed,
            self.interrupted,
            self.not_run,
            if verdicts.is_empty() {
                "none".to_owned()
            } else {
                verdicts.join(", ")
            },
            self.agreed,
            self.labelled
        )
    }
}
