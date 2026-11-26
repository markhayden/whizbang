//! `whizbang eval edits gate` (plan mE, task M2, item 2): an edit recipe's gate
//! (`whizbang_eval::edit_gate`, which says how each case counts and how it is decided)
//! scored from the store, and its receipt.
//!
//! **Each case's run** is the run the store holds under the case's request id, of the
//! cases' action ([`case_runs`]): any action's, a recipe's or the loop's. Its finished
//! picture is its output `image`, its kept mask its output `mask` (the loop hands back its
//! kept attempt's), and the picture it was given its input `image`; nothing is read from a
//! folder of pictures.
//!
//! **What is read of a finished run**, from the store alone: preservation, every pixel
//! where the kept mask weighs nothing compared with the given picture's; the settled cost
//! of the run and of every run nested under it; its time, and the time of every backend
//! request made under it. A mask on a cut's canvas (`seam` hands back its ramp's) is mapped
//! onto the panorama through the plan of the cut its run made; a mask in another space that
//! cannot be mapped stops the scoring. Cost is what every run under the case's request id
//! and under them settled, and what no answer settled, a failed run's too. Task success
//! from a check (rung 3): every run of `source-fidelity`, completed or failed, given the
//! finished picture then the given picture, as the loop writes them, the job the loop
//! writes for the case's `what`, under the gate's judge when it names one; a success only
//! when there is one and every one passed. An edit that changed nothing is a success only
//! by a check made under its own run.
//! From a person's marks (`--labels`, the marks the contact sheet of these cases saves,
//! with `--sheet`, its assignment: `whizbang eval loop sheet --cases`): a success when the
//! picture's mark says the named thing is gone and nothing else was damaged.
//!
//! **Blinded quality** is the lab decode of the picks (`--picks`) made on the pairs drawn
//! from these runs (`--assignment`, as `whizbang lab pairs --from-runs` writes it) under
//! the protocol the gate pins, the assignment held to what the protocol's seed draws and
//! each pair's recipe side to its case's run.
//!
//! A mark file, a sheet's assignment, the picks and the pairs' assignment are the owner's,
//! and are refused inside a git work tree, where they could be committed. The receipt holds
//! aggregates only: no case, family, run, picture, phrase or mark.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType};
use whizbang_core::contract::record::{ArgumentValue, Decision, RunRecord, Status, Timed};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Digest};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::edit_gate::{
    self, Admits, Case, Cases, Decided, EditGate, Finished, GroupRead, Observation, Observed,
};
use whizbang_eval::lab::decode::{self, Side};
use whizbang_eval::lab::protocol::Protocol;
use whizbang_eval::lab::{digest_of, read_relative};

use crate::harness::{self, Ended};
use crate::loop_score::{self, Assignment, Mark};

/// The check whose verdict is task success on rung 3.
pub const CHECK: &str = whizbang_rascal::checked::CHECK;

/// The most runs walked down from a case's run.
const NESTING_LIMIT: usize = 4_096;

/// A gate file and the files it pins, each held to its digest.
pub struct Frozen {
    pub gate: EditGate,
    /// The SHA-256 of the gate file's bytes, in hex.
    pub digest: String,
    pub cases: Cases,
    pub protocol: Protocol,
    pub protocol_digest: String,
}

/// The bytes of a file beside `file`.
fn beside(file: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let folder = (file.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    read_relative(folder, relative, loop_score::FILE_LIMIT)
}

/// Reads a gate file, its cases and its protocol, refusing either when it is not the file
/// the gate pins.
pub fn frozen(file: &Path) -> Result<Frozen, String> {
    let bytes = loop_score::read_limited(file, "the edit gate")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "the edit gate is not UTF-8")?;
    let gate = EditGate::parse(text)?;
    let cases_file = file.with_file_name(&gate.cases);
    let folder = cases_file.parent().unwrap_or(Path::new("."));
    if gate.rung == 4 && crate::edits::inside_a_work_tree(folder) {
        return Err(
            "a rung 4 gate's cases are of a fresh corpus, the owner's, read only from outside a \
             git work tree; nothing was scored"
                .to_owned(),
        );
    }
    let cases_bytes = beside(file, &gate.cases)?;
    if digest_of(&cases_bytes) != gate.cases_sha256 {
        return Err("the cases are not the file the gate pins (another SHA-256)".to_owned());
    }
    let cases = Cases::parse(std::str::from_utf8(&cases_bytes).map_err(|_| "cases not UTF-8")?)?;
    if cases.action != gate.recipe {
        return Err("the cases are runs of another action than the gate's recipe".to_owned());
    }
    let unsaid = cases.case.iter().any(|c| c.what.is_none());
    if gate.success.from == edit_gate::ReadFrom::Check && unsaid {
        return Err(
            "a gate scored from checks needs each case's what: its check's job is the one the \
             loop writes for it"
                .to_owned(),
        );
    }
    let protocol_bytes = beside(file, &gate.quality.protocol)?;
    let (protocol, protocol_digest) = Protocol::parse(&protocol_bytes)?;
    if protocol_digest != gate.quality.protocol_sha256 {
        return Err("the protocol is not the file the gate pins (another SHA-256)".to_owned());
    }
    Ok(Frozen {
        gate,
        digest: Digest::of_bytes(&bytes).hex(),
        cases,
        protocol,
        protocol_digest,
    })
}

/// One case and what the store holds of its run.
#[derive(Debug, Clone)]
pub struct CaseRun<'a> {
    pub case: &'a Case,
    /// The run's id, when the store holds one; the first, when it holds more.
    pub run: Option<String>,
    /// Every run under the case's request id.
    pub all: Vec<String>,
    /// How it ended: an error unless every run under its id completed.
    pub finished: bool,
    pub given: Option<ArtifactId>,
    pub image: Option<ArtifactId>,
    pub mask: Option<ArtifactId>,
    pub end_ms: u64,
}

/// What one run of the store says of its place, its spend, its time, and, for a cut, the
/// panorama it cut and the plan it handed back.
struct Seen {
    parent: Option<String>,
    spent_micros: u64,
    requests_ms: u64,
    cut: Option<(ArtifactId, ArtifactId)>,
}

/// A run of the check, as much as is needed to match it to an edit: its two pictures in
/// order (the candidate, then the source), the SHA-256 of its job, its judge, and whether
/// it passed. Nothing else of its record is kept.
struct CheckSeen {
    run: String,
    images: [ArtifactId; 2],
    job: Option<Digest>,
    judge: Option<String>,
    passed: bool,
}

/// What the store holds of the cases' runs, and of every run, for the walk down.
pub struct Held<'a> {
    pub runs: Vec<CaseRun<'a>>,
    records: BTreeMap<String, RunRecord>,
    seen: BTreeMap<String, Seen>,
    checks: Vec<CheckSeen>,
}

fn port(record: &RunRecord, name: &str) -> Option<ArtifactId> {
    let output = record.outputs().iter().find(|o| o.port == name);
    output.map(|o| ArtifactId(o.artifact))
}

fn text_argument<'a>(record: &'a RunRecord, name: &str) -> Option<&'a str> {
    (record.asked().arguments.iter()).find_map(|a| match &a.value {
        ArgumentValue::Text(text) if a.name == name => Some(text.as_str()),
        _ => None,
    })
}

fn input(record: &RunRecord, name: &str) -> Option<ArtifactId> {
    let found = record.inputs().iter().find(|i| i.port == name);
    found.map(|i| ArtifactId(i.artifact))
}

/// A check of two pictures, kept small; any other run of the check is passed over.
fn check_of(record: &RunRecord, id: &str) -> Option<CheckSeen> {
    if record.outcome().status == Status::Cancelled {
        return None;
    }
    let images: Vec<ArtifactId> = (record.inputs().iter())
        .filter(|i| i.port == "images")
        .map(|i| ArtifactId(i.artifact))
        .collect();
    let images: [ArtifactId; 2] = images.try_into().ok()?;
    Some(CheckSeen {
        run: id.to_owned(),
        images,
        job: text_argument(record, "job").map(|j| Digest::of_bytes(j.as_bytes())),
        judge: text_argument(record, "judge").map(str::to_owned),
        passed: record.outcome().status == Status::Completed
            && matches!(record.verdict(), Some((Decision::Pass, _))),
    })
}

/// Each case's run in the store at `root`, in the cases' order. A run under a case's
/// request id of another action is refused; a case with more than one run under its id
/// is finished only when every one completed, so a failure is never replaced by a later
/// success.
pub fn case_runs<'a>(root: &Path, cases: &'a Cases) -> Result<Held<'a>, String> {
    let wanted: BTreeMap<&str, &Case> = cases.case.iter().map(|c| (c.run.as_str(), c)).collect();
    let mut under: BTreeMap<&str, Vec<(String, Ended)>> = BTreeMap::new();
    let mut records = BTreeMap::new();
    let mut seen = BTreeMap::new();
    let mut checks = Vec::new();
    let mut foreign = false;
    harness::each_run(root, &mut |record, id, ended| {
        let requests_ms: u64 = (record.time().durations.iter())
            .filter(|d| d.of == Timed::Request)
            .map(|d| d.ms)
            .sum();
        // Settled, and what no answer settled, as a gate's cost counts it.
        let spent_micros: u64 = (record.spend().requests.iter())
            .map(|p| p.settled_micros() + p.unresolved_micros())
            .sum();
        let action = record.asked().action.as_deref();
        let cut = (action == Some(whizbang_rascal::cut::NAME))
            .then(|| input(record, "image").zip(port(record, "plan")))
            .flatten();
        let parent = record.identity().parent.clone();
        seen.insert(
            id.to_owned(),
            Seen {
                parent,
                spent_micros,
                requests_ms,
                cut,
            },
        );
        if action == Some(CHECK) {
            checks.extend(check_of(record, id));
        }
        let request = record.identity().request.as_deref();
        if let Some((key, _)) = request.and_then(|r| wanted.get_key_value(r)) {
            if action != Some(cases.action.as_str()) {
                foreign = true;
            }
            under.entry(*key).or_default().push((id.to_owned(), ended));
            records.insert(id.to_owned(), record.clone());
        }
    })?;
    if foreign {
        return Err("a run under a case's request id is of another action".to_owned());
    }
    let mut runs = Vec::new();
    let mut givens = std::collections::BTreeSet::new();
    for case in &cases.case {
        let list = (under.get(case.run.as_str()))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let finished = !list.is_empty() && list.iter().all(|(_, e)| *e == Ended::Completed);
        let first = list.first().map(|(id, _)| id.clone());
        let record = first.as_ref().and_then(|id| records.get(id));
        let given = record.and_then(|r| input(r, "image"));
        givens.extend(given);
        runs.push(CaseRun {
            case,
            run: first,
            all: list.iter().map(|(id, _)| id.clone()).collect(),
            finished,
            given,
            image: record.and_then(|r| port(r, "image")),
            mask: record.and_then(|r| port(r, "mask")),
            end_ms: record.map_or(0, |r| r.time().end_ms),
        });
    }
    // Only the checks of a picture a case's run was given are kept.
    checks.retain(|c| givens.contains(&c.images[1]));
    Ok(Held {
        runs,
        records,
        seen,
        checks,
    })
}

impl Held<'_> {
    /// Every run under each of `runs`, those runs among them.
    fn tree(&self, runs: &[String]) -> Vec<String> {
        let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (id, s) in &self.seen {
            if let Some(parent) = &s.parent {
                (children.entry(parent.as_str()).or_default()).push(id.as_str());
            }
        }
        let mut out = Vec::new();
        let mut next: Vec<&str> = runs.iter().map(String::as_str).collect();
        while let Some(at) = next.pop() {
            if out.len() >= NESTING_LIMIT {
                break;
            }
            out.push(at.to_owned());
            next.extend(children.get(at).into_iter().flatten().copied());
        }
        out
    }

    /// What the runs under a case's request id and every run under them spent, settled
    /// and unresolved, and the time their backend requests took.
    fn spent(&self, runs: &[String]) -> (u64, u64) {
        let mut out = (0, 0);
        for at in self.tree(runs) {
            if let Some(s) = self.seen.get(&at) {
                out.0 += s.spent_micros;
                out.1 += s.requests_ms;
            }
        }
        out
    }

    /// The plan of the cut a run under `run` made of `given`, when there is one.
    fn plan_of(&self, run: &str, given: ArtifactId) -> Option<ArtifactId> {
        let plans: Vec<ArtifactId> = (self.tree(&[run.to_owned()]).into_iter())
            .filter_map(|at| self.seen.get(&at).and_then(|s| s.cut))
            .filter(|(cut, _)| *cut == given)
            .map(|(_, plan)| plan)
            .collect();
        (plans.len() == 1).then(|| plans[0])
    }

    /// Whether the edit passed its checks: those of `image` as the candidate and `given` as
    /// the source, in that order (as the loop writes them), given the job the loop writes
    /// for the case's phrase under either version of its template, under the judge when one is named. Every such check must
    /// have passed, however many there are; none when there is none. An edit that changed
    /// nothing is not a success unless a check of it was made under its own run.
    fn checked(
        &self,
        run: &CaseRun<'_>,
        image: ArtifactId,
        given: ArtifactId,
        judge: Option<&str>,
    ) -> Option<bool> {
        let what = run.case.what.as_deref();
        let found: Vec<&CheckSeen> = (self.checks.iter())
            .filter(|c| c.images == [image, given] && is_the_loops_job(c.job, what))
            .filter(|c| judge.is_none_or(|j| c.judge.as_deref() == Some(j)))
            .collect();
        if image == given {
            let own = self.tree(&run.all);
            let ours: Vec<&&CheckSeen> = found.iter().filter(|c| own.contains(&c.run)).collect();
            return Some(!ours.is_empty() && ours.iter().all(|c| c.passed));
        }
        (!found.is_empty()).then(|| found.iter().all(|c| c.passed))
    }
}

/// Whether a check given the job of digest `job` was given the job the loop writes for
/// the phrase `what`, under either version of its template (plan mE, task L2, item 1): a
/// check made by a build before that task was given version 1. No phrase, no job.
pub fn is_the_loops_job(job: Option<Digest>, what: Option<&str>) -> bool {
    let (Some(job), Some(what)) = (job, what) else {
        return false;
    };
    (whizbang_rascal::checked::jobs(what).iter()).any(|one| Digest::of_bytes(one.as_bytes()) == job)
}

fn pixels_of(reader: &Reader, id: ArtifactId) -> Result<CanonicalPixels, String> {
    let limits = DecodeLimits::default();
    match artifact::get_anywhere(reader, ArtifactType::Image, id, &limits) {
        Ok(Artifact::Image(image)) => image.pixels(&limits).map_err(|e| e.to_string()),
        _ => Err("a picture of a case's run is not an image the store holds".to_owned()),
    }
}

/// The kept mask's weights over the given picture: the mask's own, when it is of the
/// picture's size; mapped through the wrap of the cut whose plan `plan` is, when it is on
/// that cut's canvas (as `seam` hands back its ramp's mask): the canvas's left half on the
/// panorama's last `s` columns, its right half on the first `s`, nothing elsewhere. A mask
/// in any other space is refused, and nothing is scored.
fn weights_over(
    reader: &Reader,
    given: &CanonicalPixels,
    mask: ArtifactId,
    plan: Option<ArtifactId>,
) -> Result<Vec<f32>, String> {
    let limits = DecodeLimits::default();
    let read = artifact::get_anywhere(reader, ArtifactType::Mask, mask, &limits);
    let mask = (read.map_err(|e| e.to_string())?)
        .into_mask()
        .map_err(|e| e.to_string())?;
    let (w, h) = (given.width(), given.height());
    let space = mask.space();
    let weight = |x: u32, y: u32| mask.weight(x, y).unwrap_or(0.0);
    if (space.width(), space.height()) == (w, h) {
        return Ok((0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| weight(x, y))
            .collect());
    }
    let refused = "a case's kept mask is in another space than its picture, and no cut of its \
                   run maps it there; nothing was scored";
    let plan = plan.ok_or(refused)?;
    let Ok(Artifact::Plan(plan)) =
        artifact::get_anywhere(reader, ArtifactType::Plan, plan, &limits)
    else {
        return Err(refused.to_owned());
    };
    let s = plan.strip();
    let canvas = whizbang_rascal::cut::canvas_of(given, s).map_err(|_| refused)?;
    let on_the_cut = plan.binding().source() == given.digest()
        && (space.width(), space.height()) == (2 * s, h)
        && mask.image() == canvas.digest()
        && 2 * s <= w;
    if !on_the_cut {
        return Err(refused.to_owned());
    }
    let mut out = Vec::with_capacity(w as usize * h as usize);
    for y in 0..h {
        for x in 0..w {
            let column = if x >= w - s {
                Some(x - (w - s))
            } else if x < s {
                Some(s + x)
            } else {
                None
            };
            out.push(column.map_or(0.0, |c| weight(c, y)));
        }
    }
    Ok(out)
}

/// The pixels outside the mask's support that differ from the given picture's: every
/// pixel when the finished picture is of another size.
fn changed_outside(
    reader: &Reader,
    given: &CanonicalPixels,
    image: ArtifactId,
    weights: &[f32],
) -> Result<u64, String> {
    let edited = pixels_of(reader, image)?;
    if (edited.width(), edited.height()) != (given.width(), given.height()) {
        return Ok(u64::from(given.width()) * u64::from(given.height()));
    }
    crate::edits::changed_outside(given.rgba(), edited.rgba(), weights)
}

/// What each case's run did, as the gate counts it.
pub fn observe(
    root: &Path,
    held: &Held<'_>,
    gate: &EditGate,
    marks: Option<&BTreeMap<String, Mark>>,
) -> Result<Vec<Observation>, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for run in &held.runs {
        let family = run.case.family.clone();
        let (spent, backend_ms) = held.spent(&run.all);
        let spent_micros = (!run.all.is_empty()).then_some(spent);
        let (Some(id), true, Some(given), Some(image), Some(mask)) =
            (&run.run, run.finished, run.given, run.image, run.mask)
        else {
            out.push(Observation {
                family,
                observed: Observed::Error,
                spent_micros,
            });
            continue;
        };
        let given_pixels = pixels_of(&reader, given)?;
        let weights = weights_over(&reader, &given_pixels, mask, held.plan_of(id, given))?;
        let record = &held.records[id];
        let total = (record.time().start_ms).map_or(0, |s| record.time().end_ms.saturating_sub(s));
        let success = match marks {
            Some(marks) => marks.get(id).map(|m| !m.visible && !m.damaged),
            None => held.checked(run, image, given, gate.success.judge.as_deref()),
        };
        out.push(Observation {
            family,
            observed: Observed::Finished(Finished {
                success,
                changed_outside: changed_outside(&reader, &given_pixels, image, &weights)?,
                local_ms: total.saturating_sub(backend_ms),
                backend_ms,
                pixels: u64::from(given_pixels.width()) * u64::from(given_pixels.height()),
            }),
            spent_micros,
        });
    }
    Ok(out)
}

/// The owner's file at `file`, refused inside a git work tree.
fn owners(file: &Path, what: &str) -> Result<Vec<u8>, String> {
    let folder = (file.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if crate::edits::inside_a_work_tree(folder) {
        return Err(format!(
            "{what} is inside a git work tree, where it could be committed; it is read only \
             from outside one; nothing was scored"
        ));
    }
    loop_score::read_limited(file, what)
}

/// The marks of a sheet of these cases (`eval loop sheet --cases`), by run. Refused: marks
/// of another sheet; an assignment whose pictures are not these cases' finished runs; a
/// picture marked twice, or one the assignment does not hold.
pub fn marks_of(
    marks: &[u8],
    assignment: &Assignment,
    runs: &[CaseRun<'_>],
) -> Result<BTreeMap<String, Mark>, String> {
    let file: loop_score::MarksFile =
        serde_json::from_slice(marks).map_err(|_| "the marks file is not one the sheet saves")?;
    if file.marks_version != 1 || file.sheet != loop_score::sheet_id(assignment.seed) {
        return Err("the marks file was saved from another sheet than this assignment's".into());
    }
    let mut by_key = BTreeMap::new();
    for one in &assignment.pictures {
        let ours = (runs.iter()).any(|r| {
            r.finished && r.run.as_deref() == Some(one.run.as_str()) && r.case.id == one.item
        });
        let key = loop_score::key(assignment.seed, &one.run, one.attempt);
        if !ours || one.attempt != 0 || one.key != key {
            return Err("the sheet's assignment is not of these cases' finished runs".to_owned());
        }
        by_key.insert(one.key.clone(), one.run.clone());
    }
    let mut out = BTreeMap::new();
    for mark in &file.marks {
        let run = (by_key.get(&mark.picture))
            .ok_or("the marks file marks a picture the sheet's assignment does not hold")?;
        let given = Mark {
            visible: mark.visible,
            damaged: mark.damaged,
        };
        if out.insert(run.clone(), given).is_some() {
            return Err("the marks file marks one picture twice".to_owned());
        }
    }
    Ok(out)
}

/// The decode of the picks made on the pairs of these runs: the pairs picked, and each
/// group. Refused: an assignment the protocol's seed does not draw, or whose recipe side is
/// not each case's run; picks made on another assignment.
pub fn quality_of(
    frozen: &Frozen,
    assignment: &[u8],
    picks: &[u8],
    runs: &[CaseRun<'_>],
) -> Result<(u32, Vec<GroupRead>), String> {
    let drawn = decode::Assignment::parse(assignment)?;
    whizbang_eval::lab::pairs::check_drawn(&frozen.protocol, &drawn)?;
    for pair in &drawn.pair {
        let a = if pair.a_side == Side::Left {
            &pair.left
        } else {
            &pair.right
        };
        let ours = (runs.iter()).any(|r| {
            r.case.id == pair.id && r.finished && r.run.as_deref() == Some(a.cell.as_str())
        });
        if !ours {
            return Err("the pairs are not of this round's runs of these cases".to_owned());
        }
    }
    let picks = decode::Picks::parse(picks)?;
    let decoded = decode::decode(
        &frozen.protocol,
        &frozen.protocol_digest,
        &drawn,
        &digest_of(assignment),
        &picks,
    )?;
    let groups: Vec<GroupRead> = (decoded.groups.iter())
        .map(|g| GroupRead {
            group: g.group.clone(),
            pairs: g.pairs,
            decided: g.decided,
            wins: g.wins,
            losses: g.losses,
            same: g.same,
            neither: g.neither,
            outcome: g.outcome.word().to_owned(),
        })
        .collect();
    Ok((groups.iter().map(|g| g.pairs).sum(), groups))
}

/// What `eval edits gate` reads beside the gate.
pub struct Gating<'a> {
    pub gate: &'a Path,
    pub root: &'a Path,
    /// A person's marks and the sheet's assignment.
    pub marks: Option<(&'a Path, &'a Path)>,
    /// The pairs' assignment and the picks made on it.
    pub picks: Option<(&'a Path, &'a Path)>,
    pub receipt: &'a Path,
    pub commit: &'a str,
}

/// The receipt, `edit-receipt/1`: what was decided, and of what, in aggregates.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Receipt {
    pub receipt: String,
    pub gate: String,
    pub gate_sha256: String,
    pub recipe: String,
    pub rung: u8,
    pub corpus: String,
    pub cases_sha256: String,
    pub protocol_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignment_sha256: Option<String>,
    pub marked: bool,
    /// The day the last case's run ended.
    pub date: String,
    pub whizbang: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admits: Option<Admits>,
    #[serde(flatten)]
    pub decided: Decided,
}

/// Scores the gate from the store and writes its receipt.
pub fn score(gating: &Gating<'_>) -> Result<Receipt, String> {
    let frozen = frozen(gating.gate)?;
    let gate = &frozen.gate;
    if gate.success.from == edit_gate::ReadFrom::Marks && gating.marks.is_none() {
        return Err("this gate reads task success from marks: give --labels and --sheet".into());
    }
    if gate.success.from == edit_gate::ReadFrom::Check && gating.marks.is_some() {
        return Err("this gate reads task success from checks, not marks".to_owned());
    }
    let held = case_runs(gating.root, &frozen.cases)?;
    let marks = match gating.marks {
        Some((marks, sheet)) => {
            let sheet = owners(sheet, "the sheet's assignment")?;
            let assignment: Assignment = serde_json::from_slice(&sheet)
                .map_err(|_| "the sheet's assignment is not one the sheet writes")?;
            Some(marks_of(
                &owners(marks, "the marks file")?,
                &assignment,
                &held.runs,
            )?)
        }
        None => None,
    };
    let (quality, assignment_sha256) = match gating.picks {
        Some((assignment, picks)) => {
            let bytes = owners(assignment, "the pairs' assignment")?;
            let read = quality_of(&frozen, &bytes, &owners(picks, "the picks")?, &held.runs)?;
            (Some(read), Some(digest_of(&bytes)))
        }
        None => (None, None),
    };
    let observations = observe(gating.root, &held, gate, marks.as_ref())?;
    let decided = edit_gate::decide(gate, &observations, quality)?;
    let last = held.runs.iter().map(|r| r.end_ms).max().unwrap_or(0);
    let receipt = Receipt {
        receipt: edit_gate::RECEIPT.to_owned(),
        gate: gate.name.clone(),
        gate_sha256: frozen.digest.clone(),
        recipe: gate.recipe.clone(),
        rung: gate.rung,
        corpus: gate.corpus.clone(),
        cases_sha256: gate.cases_sha256.clone(),
        protocol_sha256: frozen.protocol_digest.clone(),
        assignment_sha256,
        marked: marks.is_some(),
        date: if last == 0 {
            "none".to_owned()
        } else {
            crate::gating::day_of(last)
        },
        whizbang: gating.commit.to_owned(),
        admits: gate.admits.clone(),
        decided,
    };
    let text = serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())? + "\n";
    crate::gating::write_new(gating.receipt, text.as_bytes())?;
    Ok(receipt)
}

fn shown(value: Option<f64>) -> String {
    value.map_or("-".to_owned(), |v| format!("{v:.3}"))
}

impl Receipt {
    /// What the command prints: aggregates only.
    pub fn text(&self) -> String {
        let d = &self.decided;
        format!(
            "gate: {} ({}), rung {}\ncases: {}, families: {}, finished: {}, errors: {}\n\
             task success: {} of {} ({} unscored), lower bound {}\npreserved: {} of {}\n\
             pairs picked: {}\nmean spent: {} micros of {}\nmedian local time: {} of the \
             record's\noutcome: {}{}\n",
            self.gate,
            &self.gate_sha256[..16.min(self.gate_sha256.len())],
            self.rung,
            d.cases,
            d.families,
            d.finished,
            d.errors,
            d.success.successes,
            d.success.of,
            d.success.unscored,
            shown(d.success.lower),
            d.preservation.preserved,
            d.preservation.finished,
            d.quality.picked,
            shown(d.cost.mean_spent_micros),
            d.cost.limit_micros,
            shown(d.latency.median_ratio),
            d.outcome,
            if d.why.is_empty() {
                String::new()
            } else {
                format!(" ({})", d.why.join(", "))
            }
        )
    }
}
