//! The shared harness (plan m3, task L1, item 0; `SPEC.md` section 7, "on the shared
//! harness"): E1's runner of a matrix, made to run cells of any action. A cell is an
//! action, its arguments and a cell identity ([`Work`]); an attempt of it is one run, and is
//! recognised by that identity in the run's record, whatever the action; and a cell's
//! forecast is asked of its kind: a judge's from the study's rates, a fill's from its price
//! per call, nothing for a cell that runs nothing (a lab's reference arm).
//!
//! How a record names its cell is the caller's ([`Recognise`]): a run of `check` holds its
//! cell in its argument `cell`, as E1's matrices always have, so their stores resume as
//! they did; a cell of any other action is given a request id, [`request_id`], which the
//! run's record holds as `identity.request`, and [`by_request`] reads it back.
//!
//! **Resuming.** A cell with a completed attempt is not run again; a cell whose last
//! attempt failed (an error, or what it was given found invalid) is kept as failed and is
//! not run again, so a failure is never replaced by a later success; a cell whose last
//! attempt was interrupted (cancelled, or ended for it by the next open after its process
//! died) is run again as its next attempt.
//!
//! **Spend.** Before the first run, the forecast of the cells still to run is set against
//! what the store's allowance has left, and work whose forecast passes it is refused before
//! anything runs. Each run is then admitted as any call is, reserving its own bound; at
//! most `at_once` runs are started and not yet waited for, so at most that many
//! reservations are held.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use whizbang_core::contract::record::{RunRecord, Status};
use whizbang_core::contract::{Answer, Executor, Request, RequestId, RunId};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;

/// How one attempt of a cell ended, as its run's record says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ended {
    Completed,
    /// An error, or what it was given found invalid: reported, and not run again.
    Failed,
    /// Cancelled, or ended by the next open after its process died: run again.
    Interrupted,
}

/// One cell, as the harness runs it.
pub trait Work {
    /// The cell's identity: what its attempts are recognised by.
    fn identity(&self) -> String;
    /// The request one attempt of the cell is, from 1; none for a cell that runs nothing.
    fn request(&self, attempt: u32) -> Result<Option<Request>, String>;
    /// What one attempt is forecast to cost, in millionths of a US dollar.
    fn forecast(&self) -> Result<u64, String>;
    /// Cells of one group need one preparation: the first of each is prepared before
    /// anything runs.
    fn group(&self) -> String;
}

/// What a run's record says of the cell it was: its identity and its attempt.
pub type Recognise<'a> = &'a dyn Fn(&RunRecord) -> Option<(String, u32)>;

/// Each cell's attempts, by identity: the attempt, its run and how it ended, in order.
pub type Attempts = BTreeMap<String, Vec<(u32, String, Ended)>>;

/// What the harness is given beside the cells.
pub struct Running<'a> {
    pub executor: &'a dyn Executor,
    pub root: &'a Path,
    /// What the store's allowance has left, in millionths of a US dollar.
    pub left_micros: u64,
    /// Asked before each run is started: true stops there, and the work is resumed by
    /// running it again.
    pub stop: &'a dyn Fn() -> bool,
}

/// The most characters of a scope: what is left of a request id for a cell's identity.
pub const SCOPE_LIMIT: usize = 48;

/// The request id of one attempt of a cell of any action: `cell.<scope>.<identity>.<attempt>`.
/// The scope says whose cells they are (a lab and its protocol); it and the identity are
/// letters, digits and `-`.
pub fn request_id(scope: &str, identity: &str, attempt: u32) -> Result<RequestId, String> {
    let plain = |text: &str| {
        !text.is_empty() && (text.chars()).all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    if !plain(scope) || scope.len() > SCOPE_LIMIT || !plain(identity) {
        return Err(format!(
            "a cell's scope is 1 to {SCOPE_LIMIT} letters, digits and -, and so is its identity"
        ));
    }
    RequestId::parse(&format!("cell.{scope}.{identity}.{attempt}"))
}

/// The cell of a scope a record's request id names: its scope, identity and attempt.
pub fn named_by_request(record: &RunRecord) -> Option<(String, String, u32)> {
    let id = record.identity().request.as_deref()?;
    let mut parts = id.strip_prefix("cell.")?.split('.');
    let (scope, identity, attempt) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    Some((scope.to_owned(), identity.to_owned(), attempt.parse().ok()?))
}

/// Recognises the cells of one scope by their request ids.
pub fn by_request(scope: &str) -> impl Fn(&RunRecord) -> Option<(String, u32)> + '_ {
    move |record| match named_by_request(record) {
        Some((of, identity, attempt)) if of == scope => Some((identity, attempt)),
        _ => None,
    }
}

/// Every run the store at `root` holds whose record could be read, with how it ended.
pub fn each_run(root: &Path, each: &mut dyn FnMut(&RunRecord, &str, Ended)) -> Result<(), String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    for id in generation.runs().keys() {
        let stored = whizbang_core::contract::record::stored(&generation, id, |e| reader.read(e));
        let Some(Ok(record)) = stored else { continue };
        let outcome = record.outcome();
        let ended = match outcome.status {
            _ if !outcome.lost.is_empty() => Ended::Interrupted,
            Status::Completed => Ended::Completed,
            Status::Cancelled => Ended::Interrupted,
            Status::Error | Status::Invalid => Ended::Failed,
        };
        each(&record, id, ended);
    }
    Ok(())
}

/// Every attempt the store at `root` holds of a cell `recognise` knows, by identity.
pub fn attempts(root: &Path, recognise: Recognise<'_>) -> Result<Attempts, String> {
    let mut found: Attempts = BTreeMap::new();
    each_run(root, &mut |record, id, ended| {
        if let Some((identity, attempt)) = recognise(record) {
            found
                .entry(identity)
                .or_default()
                .push((attempt, id.to_owned(), ended));
        }
    })?;
    for list in found.values_mut() {
        list.sort();
    }
    Ok(found)
}

/// Runs every cell still to run, at most `at_once` at once, and says how many attempts it
/// started. `what` names the work in a refusal (`matrix`, `round`). The forecast of the
/// cells still to run is set against what is left first, and work whose forecast passes
/// it is refused before anything runs; so is work whose first call of a group the
/// executor refuses.
pub fn run(
    cells: &[&dyn Work],
    recognise: Recognise<'_>,
    running: &Running<'_>,
    at_once: usize,
    what: &str,
) -> Result<usize, String> {
    let earlier = attempts(running.root, recognise)?;
    let mut todo = Vec::new();
    for cell in cells {
        let last = earlier.get(&cell.identity()).and_then(|list| list.last());
        let attempt = match last {
            None => 1,
            Some((attempt, _, Ended::Interrupted)) => attempt + 1,
            Some(_) => continue,
        };
        if let Some(request) = cell.request(attempt)? {
            todo.push((*cell, request));
        }
    }
    let mut forecast: u64 = 0;
    for (cell, _) in &todo {
        forecast = forecast.saturating_add(cell.forecast()?);
    }
    if forecast > running.left_micros {
        return Err(format!(
            "the {what}'s forecast for its {} runs still to run is {forecast} millionths of a \
             dollar, more than the {} the store's allowance has left; nothing was run",
            todo.len(),
            running.left_micros
        ));
    }
    let mut prepared = BTreeSet::new();
    for (cell, request) in &todo {
        if prepared.insert(cell.group()) {
            (running.executor.prepare(request.call()))
                .map_err(|why| format!("the {what} cannot run, nothing was run: {why}"))?;
        }
    }
    let mut started = 0;
    let mut open: VecDeque<RunId> = VecDeque::new();
    let mut failure = None;
    for (_, request) in todo {
        if (running.stop)() {
            break;
        }
        while open.len() >= at_once.max(1) {
            if let Some(run) = open.pop_front() {
                running.executor.wait(&run).map_err(|e| e.to_string())?;
            }
        }
        match running.executor.start(&request) {
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
    match failure {
        Some(why) => Err(format!(
            "a run of the {what} was refused, and the {what} stopped; run it again to resume: {why}"
        )),
        None => Ok(started),
    }
}
