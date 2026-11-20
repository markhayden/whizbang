//! What `whizbang lab` does (plan m3, tasks L1 and L2; ADR 0038): the lab's files read and
//! checked (`whizbang_eval::lab`), and a lab's cells run on the shared harness
//! ([`crate::harness`]), each a run of its recipe through the executor, recognised by its
//! request id, whose scope is the lab's name and protocol, with its action and its
//! fingerprint; a reference arm's cell runs nothing.
//!
//! **A round** (`whizbang lab run`, [`prepare`] then [`round`]): the matrix and the
//! protocol and corpus beside it read and checked; the protocol held frozen against the
//! store's runs; each arm's filler and finder resolved from the config ([`resolver`]), a
//! backend the config does not give, or that states no price, refused; the cells made,
//! two of one identity refused; then every picture the matrix binds read from the
//! corpus's folder, held to the digest the manifest pins, and brought into the store as
//! the type the recipe's input declares (a picture as an image, a mask or a location as
//! its canonical file); then the cells run, the round's forecast set against what the
//! allowance has left before the first. What is printed is counts alone: no corpus id,
//! no path, nothing of a picture.
//!
//! An arm's recipe is one this build ships, or one of the config's own recipe files
//! (`[recipes] paths`), by name.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use whizbang_core::artifact::{ArtifactId, ArtifactType};
use whizbang_core::capability::Installed;
use whizbang_core::config::Setting;
use whizbang_core::contract::{Call, Capability, Recovery, Registry, Request, RunState, Value};
use whizbang_core::identity::Projection;
use whizbang_core::recipe::{Declared, RecipeAction, SET};
use whizbang_eval::lab::cell::{self, Backend, Cell, Work};
use whizbang_eval::lab::corpus::Corpus;
use whizbang_eval::lab::matrix::{Checks, Matrix, Validated};
use whizbang_eval::lab::protocol::{self, Protocol};
use whizbang_eval::lab::{decode, digest_of, read_relative};

use crate::harness::{self, Ended, Resume, Running};
use crate::wiring::{Configured, Over};

/// The most bytes of a lab's file (a manifest, a protocol, a matrix) that are read.
pub const FILE_LIMIT: u64 = 1024 * 1024;

/// A recipe this build ships, by name, as an action over `registry`.
pub fn recipe(registry: &Registry, name: &str) -> Result<RecipeAction, String> {
    recipe_of(registry, &[], name)
}

/// A recipe this build ships, or else one of `own` (the texts of the config's recipe
/// files), by name, as an action over `registry`.
pub fn recipe_of(registry: &Registry, own: &[String], name: &str) -> Result<RecipeAction, String> {
    let shipped = whizbang_rascal::recipes::SHIPPED.iter().copied();
    for text in shipped.chain(own.iter().map(String::as_str)) {
        let read = whizbang_core::recipe::Recipe::read(text).map_err(|e| e.to_string())?;
        if read.name == name {
            return whizbang_rascal::recipes::action_of(text, registry).map_err(|e| e.to_string());
        }
    }
    Err(format!(
        "{} is no recipe this build ships or the config lists",
        name.escape_debug()
    ))
}

/// The text of a constant set this build ships.
pub fn constants(name: &str) -> Option<String> {
    whizbang_rascal::constants::text_of(name).map(str::to_owned)
}

/// One cell of a lab as the harness runs it: its work, the lab's scope, and the identity
/// in the store of each corpus picture it binds.
pub struct LabCell<'a> {
    pub cell: &'a Cell,
    pub scope: &'a str,
    pub pictures: &'a BTreeMap<String, ArtifactId>,
}

impl LabCell<'_> {
    fn picture(&self, id: &str) -> Result<ArtifactId, String> {
        (self.pictures.get(id).copied())
            .ok_or_else(|| format!("the picture {id} is not in the store; nothing was run"))
    }

    /// What a reference arm's cell hands back: the picture it binds. None for a recipe's.
    pub fn handed_back(&self) -> Result<Option<ArtifactId>, String> {
        match &self.cell.work {
            Work::Reference { picture } => self.picture(picture).map(Some),
            Work::Recipe { .. } => Ok(None),
        }
    }
}

impl harness::Work for LabCell<'_> {
    fn identity(&self) -> String {
        self.cell.identity.clone()
    }

    fn request(&self, attempt: u32) -> Result<Option<Request>, String> {
        let Work::Recipe {
            recipe,
            inputs,
            texts,
            set,
        } = &self.cell.work
        else {
            return Ok(None);
        };
        let mut call = Call::new(recipe).input("image", self.picture(&self.cell.image)?);
        for (port, id) in inputs {
            call = call.input(port, self.picture(id)?);
        }
        for (name, text) in texts {
            call = call.argument(name, Value::text(text));
        }
        if !set.is_empty() {
            call = call.argument(SET, Value::text(set));
        }
        let id = harness::request_id(self.scope, &self.cell.identity, attempt)?;
        Ok(Some(Request::new(call).with_id(id)))
    }

    fn forecast(&self) -> Result<u64, String> {
        Ok(self.cell.forecast_micros)
    }

    fn group(&self) -> String {
        self.cell.arm.clone()
    }
}

/// Every scope of a cell whose run the store at `root` holds.
pub fn scopes(root: &Path) -> Result<BTreeSet<String>, String> {
    let mut found = BTreeSet::new();
    harness::each_run(root, &mut |record, _, _| {
        if let Some((scope, _, _)) = harness::named_by_request(record) {
            found.insert(scope);
        }
    })?;
    Ok(found)
}

/// Refused when the store at `root` holds runs of this lab under another protocol.
pub fn frozen(root: &Path, name: &str, protocol_digest: &str) -> Result<(), String> {
    let seen = scopes(root)?;
    protocol::frozen(seen.iter().map(String::as_str), name, protocol_digest)
}

/// The bytes of a lab's file given on the command line, at most [`FILE_LIMIT`], read
/// through no link from its folder down; a refusal names it by its file name alone.
fn file_given(path: &Path) -> Result<Vec<u8>, String> {
    let folder = (path.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = (path.file_name().and_then(|n| n.to_str()))
        .ok_or("a lab's file is named by a path that ends in a file name of UTF-8")?;
    read_relative(folder, name, FILE_LIMIT)
}

/// The text of a lab's file `relative` beside `beside`.
fn text_beside(beside: &Path, relative: &str) -> Result<String, String> {
    let folder = (beside.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let bytes = read_relative(folder, relative, FILE_LIMIT)?;
    String::from_utf8(bytes).map_err(|_| format!("{relative} is not UTF-8"))
}

/// `whizbang lab corpus verify`: the manifest's pictures under `root`, each checked
/// against its digest. The report, and whether every picture was there as pinned.
pub fn verify(manifest: &Path, root: &Path) -> Result<(String, bool), String> {
    let bytes = file_given(manifest)?;
    let text = String::from_utf8(bytes).map_err(|_| "the corpus is not UTF-8".to_owned())?;
    let corpus = Corpus::parse(&text)?;
    let found = corpus.verify(root, crate::import::MAX_FILE_BYTES)?;
    Ok(whizbang_eval::lab::corpus::report(&found))
}

// --- a round (plan m3, task L2) -----------------------------------------------------------------

/// The resolver of a round's backends: each from the config's offers, by the name an arm
/// gives or the capability's default (`cell::Resolve`).
pub type Resolver = Box<dyn Fn(&str, Option<&str>) -> Result<Backend, String>>;

/// The backends of a round, resolved from what the config offers (`Over::configured`):
/// the backend's model, its revision (the installation of a local model, the declared
/// version of a remote one), its settings, its provider (its kind when the config names
/// none) and its address, and the most one request may cost, which is its forecast. A
/// capability other than `fill` and `segment`, a backend the config does not give, one
/// that is not ready, and one that names a recipient and states no price are refused.
pub fn resolver(over: &Over) -> Result<Resolver, String> {
    let configured = (over.configured()?)
        .ok_or("a round's filler and finder are the config's: name it with --config")?;
    Ok(Box::new(move |capability, named| {
        backend_of(&configured, capability, named)
    }))
}

fn backend_of(
    configured: &Configured,
    capability: &str,
    named: Option<&str>,
) -> Result<Backend, String> {
    let wanted = match capability {
        "fill" => Capability::Fill,
        "segment" => Capability::Segment,
        other => return Err(format!("a lab resolves no {other} backend")),
    };
    let config = configured.loaded.config();
    let offers = &configured.offers;
    let name = (named.or_else(|| offers.default_of(wanted))).ok_or_else(|| {
        format!("the config gives no default {capability} backend; nothing was run")
    })?;
    let shown = name.escape_debug();
    let Some((_, backend)) = config.backends(wanted).find(|(n, _)| *n == name) else {
        return Err(format!(
            "the config gives no {capability} backend {shown}; nothing was run"
        ));
    };
    if let Some(why) = offers.not_ready(wanted, name) {
        return Err(format!(
            "the {capability} backend {shown} is not ready: {why}; nothing was run"
        ));
    }
    let resolved = (offers.roster().resolve(wanted, Some(name), &[]))
        .map_err(|why| format!("the {capability} backend {shown}: {why}; nothing was run"))?;
    let declaration = resolved.declaration();
    let most = declaration.most();
    if most.is_unbounded() {
        return Err(format!(
            "the {capability} backend {shown} states no price, so a round's forecast cannot \
             count it; nothing was run"
        ));
    }
    let model = declaration.model();
    let revision = if declaration.is_local() {
        match configured.inventory.look(model.model()) {
            Installed::Ready(installation) => installation.to_string(),
            Installed::NotReady(why) => {
                return Err(format!(
                    "the {capability} backend {shown} is not ready: {why}; nothing was run"
                ));
            }
        }
    } else {
        model.version().to_owned()
    };
    let settings = (backend.settings())
        .map(|(key, setting)| {
            let value = match setting {
                Setting::Text(text) => text.clone(),
                Setting::Integer(n) => n.to_string(),
                Setting::Boolean(b) => b.to_string(),
            };
            (key.to_owned(), value)
        })
        .collect();
    Ok(Backend {
        name: model.model().to_owned(),
        revision,
        settings,
        provider: backend.provider().unwrap_or(backend.kind()).to_owned(),
        endpoint: backend.url().unwrap_or_default(),
        price_micros: most.micros(),
    })
}

/// What `whizbang lab run` is given.
pub struct Asked<'a> {
    /// The matrix; its corpus and protocol are beside it.
    pub matrix: &'a Path,
    /// The folder the corpus's files are relative to.
    pub corpus: &'a Path,
    /// The store.
    pub root: &'a Path,
    pub over: &'a Over,
    pub recovery: Recovery,
}

/// A round ready to run: the matrix as checked, its cells, the identity in the store of
/// each picture it binds, and the scope of its runs.
pub struct Prepared {
    pub validated: Validated,
    pub cells: Vec<Cell>,
    pub pictures: BTreeMap<String, ArtifactId>,
    pub scope: String,
}

/// Everything before the first cell runs: the module's description says what, in order.
pub fn prepare(asked: &Asked<'_>) -> Result<Prepared, String> {
    let text = String::from_utf8(file_given(asked.matrix)?)
        .map_err(|_| "the matrix is not UTF-8".to_owned())?;
    let matrix = Matrix::parse(&text)?;
    let protocol_text = text_beside(asked.matrix, &matrix.protocol)?;
    let (protocol, digest): (Protocol, String) = Protocol::parse(protocol_text.as_bytes())?;
    let corpus = Corpus::parse(&text_beside(asked.matrix, &matrix.corpus)?)?;
    let registry = crate::wiring::registry_over(asked.over, false)?;
    let own = asked.over.recipe_files()?;
    let load = |name: &str| recipe_of(&registry, &own, name);
    let validated = matrix.validate(&protocol, &digest, &corpus, &registry, &load)?;
    frozen(asked.root, &protocol.name, &digest)?;
    let resolve = resolver(asked.over)?;
    let cells = cell::cells(&validated, &registry, &load, &*resolve, &constants)?;
    let pictures = bring_in(&validated, &load, asked)?;
    let scope = protocol::scope(&protocol.name, &digest);
    Ok(Prepared {
        validated,
        cells,
        pictures,
        scope,
    })
}

/// Every picture the matrix binds, with the type it is brought in as: the type each arm's
/// recipe declares for the input, an image for the picture itself and for a reference.
fn bound(
    validated: &Validated,
    load: &dyn Fn(&str) -> Result<RecipeAction, String>,
) -> Result<BTreeMap<String, ArtifactType>, String> {
    let mut wanted: BTreeMap<String, ArtifactType> = BTreeMap::new();
    let mut want = |id: &str, kind: ArtifactType| match wanted.insert(id.to_owned(), kind) {
        Some(other) if other != kind => Err(format!(
            "picture {id} is bound as a {other} and as a {kind}; nothing was run"
        )),
        _ => Ok(()),
    };
    let mut recipes = BTreeMap::new();
    for arm in &validated.arms {
        if let Checks::Recipe(recipe) = arm {
            recipes.insert(recipe.name.clone(), load(&recipe.name)?);
        }
    }
    for image in &validated.matrix.image {
        want(&image.id, ArtifactType::Image)?;
        for (input, id) in &image.inputs {
            for arm in &validated.arms {
                let kind = match arm {
                    Checks::Reference(of) if of == input => Some(ArtifactType::Image),
                    Checks::Reference(_) => None,
                    Checks::Recipe(recipe) => (recipes[&recipe.name].recipe().inputs.get(input))
                        .and_then(|declared| match declared.declared {
                            Declared::Artifact(kind) => Some(kind),
                            Declared::Text => None,
                        }),
                };
                if let Some(kind) = kind {
                    want(id, kind)?;
                }
            }
        }
    }
    Ok(wanted)
}

/// Brings into the store every picture the matrix binds, each read from the corpus's
/// folder through no link and held to the digest the manifest pins: a picture as an image
/// of the projection the manifest says (flat when it says none), anything else as its
/// canonical file of the type wanted.
fn bring_in(
    validated: &Validated,
    load: &dyn Fn(&str) -> Result<RecipeAction, String>,
    asked: &Asked<'_>,
) -> Result<BTreeMap<String, ArtifactId>, String> {
    let wanted = bound(validated, load)?;
    // A projection is a picture's: said of a file brought in as anything else, it is
    // refused before anything is brought in.
    for (id, kind) in &wanted {
        let said = (validated.corpus.picture(id)).is_some_and(|p| p.projection.is_some());
        if said && *kind != ArtifactType::Image {
            return Err(format!(
                "picture {id}: a projection is said of a picture, and it is bound as a \
                 {kind}; nothing was run"
            ));
        }
    }
    let mut pictures = BTreeMap::new();
    for (id, kind) in wanted {
        let picture =
            (validated.corpus.picture(&id)).ok_or_else(|| format!("{id} is not in the corpus"))?;
        let bytes = read_relative(asked.corpus, &picture.file, crate::import::MAX_FILE_BYTES)
            .map_err(|why| format!("picture {id}: {why}; nothing was run"))?;
        if digest_of(&bytes) != picture.sha256 {
            return Err(format!(
                "picture {id}: its file is not the one the corpus pins (another SHA-256); \
                 nothing was run"
            ));
        }
        let shown = format!("picture {id}");
        let recovery = asked.recovery.clone();
        // The corpus's word, else flat, as before: a file's own metadata is not read here.
        let projection = match picture.projection.as_deref() {
            Some("equirectangular") => Projection::Equirectangular,
            _ => Projection::Flat,
        };
        let made = match kind {
            ArtifactType::Image => crate::import::picture_of_bytes_as(
                &bytes,
                &shown,
                Some(&picture.sha256),
                asked.root,
                recovery,
                projection,
            ),
            other => crate::import::artifact_of_bytes(&bytes, &shown, other, asked.root, recovery),
        }?;
        pictures.insert(id, made);
    }
    Ok(pictures)
}

/// What a round's run did, and what the store now holds of it: counts alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub cells: usize,
    /// Cells of a reference arm, which run nothing.
    pub handed_back: usize,
    /// Attempts started by this run of the round: runs it made. A request the store had
    /// already answered is not among them (plan m4, task A1, item 8).
    pub started: usize,
    /// Requests of this run of the round that the store had already answered with the run
    /// an earlier one made under the same id: nothing was started for them. Printed apart,
    /// and only when there are any.
    pub answered_before: usize,
    /// Of those, the cells retried after a failed attempt: none unless the run was asked
    /// to (`--retry-failed`), when it is said.
    pub retried: Option<usize>,
    /// Failed attempts the store keeps of cells a later attempt then ran: each a record of
    /// a failure, never removed.
    pub failed_before: usize,
    /// Of the other cells, those whose last attempt completed, failed or was interrupted,
    /// and those with none.
    pub completed: usize,
    pub failed: usize,
    pub interrupted: usize,
    pub not_run: usize,
    /// The first failed cell in the matrix's order, as `<arm> on <image>: <why>`, the why
    /// being its run's own words: so a round every cell of which failed says why at its end.
    pub first_failed: Option<String>,
}

impl Report {
    pub fn text(&self) -> String {
        let answered = match self.answered_before {
            0 => String::new(),
            answered => format!("answered by an earlier run, nothing started: {answered}\n"),
        };
        let mut text = format!(
            "cells: {}\nhanded back by a reference arm: {}\nstarted now: {}\n{answered}\
             completed: {}\nfailed: {}\ninterrupted: {}\nnot run: {}\n",
            self.cells,
            self.handed_back,
            self.started,
            self.completed,
            self.failed,
            self.interrupted,
            self.not_run
        );
        if let Some(retried) = self.retried {
            text.push_str(&format!("retried now: {retried}\n"));
        }
        if self.failed_before > 0 {
            let kept = self.failed_before;
            text.push_str(&format!("failed attempts kept from earlier: {kept}\n"));
        }
        if let Some(why) = &self.first_failed {
            text.push_str(&format!("first failed: {why}\n"));
        }
        text
    }

    /// Why `lab run` fails after printing the report: some cells' last attempt failed. It
    /// names the flag that gives each of them one new attempt.
    pub fn failure(&self) -> Option<String> {
        (self.failed > 0).then(|| {
            format!(
                "{} cells failed; `whizbang lab run --retry-failed` gives each failed cell one \
                 new attempt, its failed one kept",
                self.failed
            )
        })
    }
}

/// Runs a prepared round's cells still to run on the shared harness, one at a time, and
/// reports on every cell as the store then holds it. Its forecast passing what the
/// allowance has left is refused before its first cell. A failed cell is kept and not
/// run again.
pub fn round(prepared: &Prepared, running: &Running<'_>) -> Result<Report, String> {
    round_resuming(prepared, running, Resume::KeepFailed)
}

/// [`round`], with what is done with a failed cell said by `resume`: under
/// [`Resume::RetryFailed`] each cell of a recipe whose last attempt failed is given one new
/// attempt in this run, forecast and reserved as any other, its failed attempt kept.
pub fn round_resuming(
    prepared: &Prepared,
    running: &Running<'_>,
    resume: Resume,
) -> Result<Report, String> {
    round_saying(prepared, running, resume, &mut |_| {})
}

/// [`round_resuming`], saying what the round is doing while it runs (plan m4, task A1,
/// item 7): a line to `say` as each cell starts and as it ends, and one for a cell whose
/// request the store had already answered. `whizbang lab run` writes them to the error
/// stream, so a round of a hundred fills is not silent until its counts. Each line is
/// `cell <n> of <m>: <arm> on <image>: ...`, where `m` is the cells this run of the round
/// sends a request for; a reference arm's cell runs nothing and is not said.
pub fn round_saying(
    prepared: &Prepared,
    running: &Running<'_>,
    resume: Resume,
    say: &mut dyn FnMut(&str),
) -> Result<Report, String> {
    let how = Round {
        resume,
        recognise: None,
    };
    round_as(prepared, running, &how, say)
}

/// How a round is run beside its cells.
pub struct Round<'a> {
    /// What is done with a failed cell.
    pub resume: Resume,
    /// What takes a run of the store for a cell's attempt: with none, the lab's own
    /// ([`harness::by_request`] under the round's scope), which `whizbang lab run` always
    /// uses. A test gives another to stand for a store whose earlier attempts are not
    /// recognised, as when their admissions cannot be read.
    pub recognise: Option<harness::Recognise<'a>>,
}

/// How a run's state reads in a line of [`round_saying`], and as an attempt's end.
fn ended_of(state: &RunState) -> (&'static str, Option<Ended>) {
    match state {
        RunState::Ended(outcome) => match outcome.name() {
            "completed" => ("completed", Some(Ended::Completed)),
            "cancelled" => ("cancelled", Some(Ended::Interrupted)),
            _ => ("failed", Some(Ended::Failed)),
        },
        RunState::NotEnded { .. } | RunState::Abandoned { .. } => {
            ("not ended", Some(Ended::Interrupted))
        }
        RunState::Removed { .. } => ("no longer in the store", None),
    }
}

/// [`round_saying`], run as `how` says.
pub fn round_as(
    prepared: &Prepared,
    running: &Running<'_>,
    how: &Round<'_>,
    say: &mut dyn FnMut(&str),
) -> Result<Report, String> {
    let labs: Vec<LabCell<'_>> = (prepared.cells.iter())
        .map(|cell| LabCell {
            cell,
            scope: &prepared.scope,
            pictures: &prepared.pictures,
        })
        .collect();
    let work: Vec<&dyn harness::Work> = labs.iter().map(|c| c as &dyn harness::Work).collect();
    let own = harness::by_request(&prepared.scope, &work);
    let recognise: harness::Recognise<'_> = how.recognise.unwrap_or(&own);
    let named: BTreeMap<&str, &Cell> = (prepared.cells.iter())
        .map(|cell| (cell.identity.as_str(), cell))
        .collect();
    // A cell's arm and image, escaped: what a line names it by.
    let name = |identity: &str| match named.get(identity) {
        Some(cell) => format!("{} on {}", cell.arm, cell.image)
            .escape_debug()
            .to_string(),
        None => "a cell".to_owned(),
    };
    // A cell whose request an earlier run answered, with how that run ended.
    let mut answered: BTreeMap<String, (Option<Ended>, String)> = BTreeMap::new();
    let mut said = |event: harness::Said<'_>| match event {
        harness::Said::Started {
            at,
            of,
            identity,
            attempt,
            run,
        } => say(&format!(
            "cell {at} of {of}: {}: started, attempt {attempt}, run {run}",
            name(identity)
        )),
        harness::Said::Ended {
            at,
            of,
            identity,
            state,
        } => say(&format!(
            "cell {at} of {of}: {}: {}",
            name(identity),
            ended_of(state).0
        )),
        harness::Said::Repeat {
            at,
            of,
            identity,
            earlier,
        } => {
            let (how, ended) = ended_of(earlier.state());
            let which = match how {
                "not ended" => "which has not ended".to_owned(),
                "no longer in the store" => "which is no longer in the store".to_owned(),
                how => format!("which {how}"),
            };
            answered.insert(identity.to_owned(), (ended, earlier.run().to_string()));
            say(&format!(
                "cell {at} of {of}: {}: answered by an earlier run, {}, {which}; nothing started",
                name(identity),
                earlier.run()
            ));
        }
    };
    let started =
        harness::run_saying(&work, recognise, running, 1, "round", how.resume, &mut said)?;
    let now = harness::attempts(running.root, recognise)?;
    let mut report = Report {
        cells: prepared.cells.len(),
        started: started.started,
        answered_before: started.repeats,
        retried: (how.resume == Resume::RetryFailed).then_some(started.retried),
        ..Report::default()
    };
    for cell in &prepared.cells {
        if matches!(cell.work, Work::Reference { .. }) {
            report.handed_back += 1;
            continue;
        }
        let list = now
            .get(&cell.identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let (last, before) = match list.split_last() {
            Some((last, before)) => (Some(last), before),
            None => (None, &[][..]),
        };
        report.failed_before += (before.iter())
            .filter(|(_, _, ended)| *ended == Ended::Failed)
            .count();
        // A cell with no attempt the round recognises, whose request an earlier run
        // answered, is counted by how that run ended: it ran, though not in this leg.
        let last = match (last, answered.get(&cell.identity)) {
            (None, Some((Some(ended), run))) => {
                match ended {
                    Ended::Completed => report.completed += 1,
                    Ended::Failed => {
                        report.failed += 1;
                        // The first failed cell is said with its run's words, whichever
                        // leg's run it was.
                        if report.first_failed.is_none() {
                            let why = why_failed(running.root, run)?;
                            report.first_failed =
                                Some(format!("{} on {}: {why}", cell.arm, cell.image));
                        }
                    }
                    Ended::Interrupted => report.interrupted += 1,
                }
                continue;
            }
            (last, _) => last,
        };
        match last {
            None => report.not_run += 1,
            Some((_, _, Ended::Completed)) => report.completed += 1,
            Some((_, run, Ended::Failed)) => {
                report.failed += 1;
                if report.first_failed.is_none() {
                    let why = why_failed(running.root, run)?;
                    report.first_failed = Some(format!("{} on {}: {why}", cell.arm, cell.image));
                }
            }
            Some((_, _, Ended::Interrupted)) => report.interrupted += 1,
        }
    }
    Ok(report)
}

/// What the record of the run `id` says of why it failed: what it was given found invalid,
/// else its error, each as printable text on one line.
fn why_failed(root: &Path, id: &str) -> Result<String, String> {
    let mut why = None;
    harness::each_run(root, &mut |record, run, _| {
        if run == id {
            let outcome = record.outcome();
            why = (outcome.invalid.clone())
                .or_else(|| (outcome.error.as_ref()).map(|e| format!("{}: {}", e.kind, e.message)));
        }
    })?;
    let why = why.unwrap_or_else(|| "its run says no reason".to_owned());
    Ok(why.escape_debug().to_string())
}

/// `whizbang lab decode`: the picks decoded against the frozen assignment under the
/// protocol, as counts and an outcome a group (`whizbang_eval::lab::decode`). The
/// assignment is first held to what the protocol's seed draws (plan m3, task L3), and the
/// picks to the assignment file's digest.
pub fn decode_files(protocol: &Path, assignment: &Path, picks: &Path) -> Result<String, String> {
    let (protocol, digest) = Protocol::parse(&file_given(protocol)?)?;
    let assignment_bytes = file_given(assignment)?;
    let frozen = decode::Assignment::parse(&assignment_bytes)?;
    whizbang_eval::lab::pairs::check_drawn(&protocol, &frozen)?;
    let picks = decode::Picks::parse(&file_given(picks)?)?;
    let decoded = decode::decode(
        &protocol,
        &digest,
        &frozen,
        &digest_of(&assignment_bytes),
        &picks,
    )?;
    Ok(decoded.text())
}
