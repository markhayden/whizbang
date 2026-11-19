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
//! run's record holds as `identity.request`, and [`by_request`] reads it back. The id
//! alone is not trusted (plan m3, task L2): a run is a cell's attempt only when its
//! action is the cell's and its admission holds the fingerprint of the very request the
//! cell makes for that attempt, so a run some other caller made under such an id is never
//! taken for a cell's; and no caller can give an id that starts `cell.`
//! ([`whizbang_core::contract::RESERVED_REQUEST_PREFIX`]). An attempt whose id a run of
//! another request holds already (its admission says another fingerprint) is passed over
//! for the next; one held by a run of the same request, or by a run whose admission cannot
//! be read, is not, and the executor answers it.
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
use whizbang_core::contract::{Admission, Answer, Executor, Request, RequestId, RunId};
use whizbang_core::identity::Digest;
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

/// What a run's record, and its admission when it has one, say of the cell it was: its
/// identity and its attempt.
pub type Recognise<'a> = &'a dyn Fn(&RunRecord, Option<&Admission>) -> Option<(String, u32)>;

/// The most attempt numbers passed over for a cell whose ids runs of other requests hold.
pub const PASSED_OVER_LIMIT: u32 = 64;

/// Each cell's attempts, by identity: the attempt, its run and how it ended, in order.
pub type Attempts = BTreeMap<String, Vec<(u32, String, Ended)>>;

/// Which cells a run of the work starts beside those never run and those interrupted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resume {
    /// A failed cell is kept as failed and not run again (E1's rule).
    #[default]
    KeepFailed,
    /// A cell whose last attempt failed is given one new attempt, its next.
    RetryFailed,
}

/// What a run of the work started: attempts in all, and of them those that retried a
/// failed cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Started {
    pub started: usize,
    pub retried: usize,
}

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
    let reserved = whizbang_core::contract::RESERVED_REQUEST_PREFIX;
    RequestId::parse(&format!("{reserved}{scope}.{identity}.{attempt}"))
}

/// The cell of a scope a record's request id names: its scope, identity and attempt.
pub fn named_by_request(record: &RunRecord) -> Option<(String, String, u32)> {
    let id = record.identity().request.as_deref()?;
    let mut parts = (id.strip_prefix(whizbang_core::contract::RESERVED_REQUEST_PREFIX)?).split('.');
    let (scope, identity, attempt) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    Some((scope.to_owned(), identity.to_owned(), attempt.parse().ok()?))
}

/// Recognises the cells of one scope by their request ids: a run whose id names a cell of
/// `cells` and an attempt, whose action is the one that attempt calls, and whose admission
/// holds that request's fingerprint. Any other run under such an id is no attempt.
pub fn by_request<'a>(
    scope: &'a str,
    cells: &'a [&'a dyn Work],
) -> impl Fn(&RunRecord, Option<&Admission>) -> Option<(String, u32)> + 'a {
    let by_identity: BTreeMap<String, &dyn Work> =
        cells.iter().map(|cell| (cell.identity(), *cell)).collect();
    move |record, admission| {
        let (of, identity, attempt) = named_by_request(record)?;
        let cell = by_identity.get(&identity).filter(|_| of == scope)?;
        let request = cell.request(attempt).ok()??;
        let same_id = request.id().map(RequestId::as_str) == record.identity().request.as_deref();
        let same_action = record.asked().action.as_deref() == Some(request.call().action());
        let same_request = admission.is_some_and(|a| a.fingerprint() == request.fingerprint());
        (same_id && same_action && same_request).then_some((identity, attempt))
    }
}

/// Every run the store at `root` holds whose record could be read, with how it ended.
pub fn each_run(root: &Path, each: &mut dyn FnMut(&RunRecord, &str, Ended)) -> Result<(), String> {
    each_admitted_run(root, &mut |record, _, id, ended| each(record, id, ended))
}

/// What [`each_admitted_run`] is given each run with: its record, its admission when it
/// could be read, its id and how it ended.
pub type EachAdmitted<'a> = dyn FnMut(&RunRecord, Option<&Admission>, &str, Ended) + 'a;

/// The same, with each run's admission when it could be read.
pub fn each_admitted_run(root: &Path, each: &mut EachAdmitted<'_>) -> Result<(), String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    for id in generation.runs().keys() {
        let stored = whizbang_core::contract::record::stored(&generation, id, |e| reader.read(e));
        let Some(Ok(record)) = stored else { continue };
        let admitted =
            whizbang_core::contract::request::admitted(&generation, id, |e| reader.read(e));
        let admission = admitted.and_then(Result::ok);
        let outcome = record.outcome();
        let ended = match outcome.status {
            _ if !outcome.lost.is_empty() => Ended::Interrupted,
            Status::Completed => Ended::Completed,
            Status::Cancelled => Ended::Interrupted,
            Status::Error | Status::Invalid => Ended::Failed,
        };
        each(&record, admission.as_ref(), id, ended);
    }
    Ok(())
}

/// Every request id a run the store at `root` holds was made under, with the fingerprint
/// of the request its admission says, for each run whose admission could be read.
fn taken_ids(root: &Path) -> Result<BTreeMap<String, Vec<Digest>>, String> {
    let mut taken: BTreeMap<String, Vec<Digest>> = BTreeMap::new();
    each_admitted_run(root, &mut |record, admission, _, _| {
        if let Some(id) = record.identity().request.clone() {
            let held = taken.entry(id).or_default();
            held.extend(admission.map(Admission::fingerprint));
        }
    })?;
    Ok(taken)
}

/// Whether a run of another request holds the id of `request`: one whose admission says
/// another fingerprint. A run of this very request, or one whose admission could not be
/// read, does not: the executor answers it (`Repeat` for this request) rather than a new
/// attempt being started and paid for (plan m3, task L3).
fn held_by_another(taken: &BTreeMap<String, Vec<Digest>>, request: &Request) -> bool {
    let Some(held) = request.id().and_then(|id| taken.get(id.as_str())) else {
        return false;
    };
    held.iter().any(|f| *f != request.fingerprint())
}

/// Every attempt the store at `root` holds of a cell `recognise` knows, by identity.
pub fn attempts(root: &Path, recognise: Recognise<'_>) -> Result<Attempts, String> {
    let mut found: Attempts = BTreeMap::new();
    each_admitted_run(root, &mut |record, admission, id, ended| {
        if let Some((identity, attempt)) = recognise(record, admission) {
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
    let started = run_resuming(cells, recognise, running, at_once, what, Resume::KeepFailed)?;
    Ok(started.started)
}

/// [`run`], with what is done with a failed cell said by `resume`.
pub fn run_resuming(
    cells: &[&dyn Work],
    recognise: Recognise<'_>,
    running: &Running<'_>,
    at_once: usize,
    what: &str,
    resume: Resume,
) -> Result<Started, String> {
    let earlier = attempts(running.root, recognise)?;
    let taken = taken_ids(running.root)?;
    let mut todo = Vec::new();
    for cell in cells {
        let last = earlier.get(&cell.identity()).and_then(|list| list.last());
        let (first, retry) = match (last, resume) {
            (None, _) => (1, false),
            (Some((attempt, _, Ended::Interrupted)), _) => (attempt + 1, false),
            (Some((attempt, _, Ended::Failed)), Resume::RetryFailed) => (attempt + 1, true),
            (Some(_), _) => continue,
        };
        // An attempt whose id a run of another request holds is passed over.
        let mut found = None;
        for attempt in (first..).take(PASSED_OVER_LIMIT as usize) {
            match cell.request(attempt)? {
                Some(request) if held_by_another(&taken, &request) => {}
                other => {
                    found = Some(other);
                    break;
                }
            }
        }
        match found {
            Some(Some(request)) => todo.push((*cell, request, retry)),
            Some(None) => {}
            None => {
                return Err(format!(
                    "a cell's next {PASSED_OVER_LIMIT} request ids are held by runs of other \
                     requests; nothing was run"
                ));
            }
        }
    }
    let mut forecast: u64 = 0;
    for (cell, _, _) in &todo {
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
    for (cell, request, _) in &todo {
        if prepared.insert(cell.group()) {
            (running.executor.prepare(request.call()))
                .map_err(|why| format!("the {what} cannot run, nothing was run: {why}"))?;
        }
    }
    let mut started = Started::default();
    let mut open: VecDeque<RunId> = VecDeque::new();
    let mut failure = None;
    for (_, request, retry) in todo {
        if (running.stop)() {
            break;
        }
        while open.len() >= at_once.max(1) {
            if let Some(run) = open.pop_front() {
                running.executor.wait(&run).map_err(|e| e.to_string())?;
            }
        }
        match running.executor.start(&request) {
            Ok(answer) => {
                if let Answer::Started(run) = answer {
                    open.push_back(run);
                }
                started.started += 1;
                started.retried += usize::from(retry);
            }
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
