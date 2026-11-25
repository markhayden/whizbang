//! Replay (plan m7, task R1; ADR 0049): a stored run made again as a new run of the
//! store's one executor ([`super::Executor::start_replay`]).
//!
//! **What is replayed.** A root run of an action this build ships that ended (completed,
//! cancelled, error or invalid; not `interrupted`, and not one whose record lost its
//! admission). A child is replayed as part of its root, never alone. Anything else is
//! refused before anything is prepared, naming why ([`Refusal::NotReplayed`]).
//!
//! **The call** is made from the root's record: its action, its arguments as the record
//! holds them (defaults applied, kept whole), and its inputs by identity, in the order
//! the record lists them. A setting whose text is exactly as long as a setting may be
//! may have been cut, and a family that holds one is refused, naming it. Offline, a
//! family whose settings name a tool file is refused, naming the file: a tool file is
//! code, and a replay never takes it from a record.
//!
//! **Its request id** is looked up by the fingerprint of the replay request itself
//! ([`Replay::fingerprint`], `replay/1`): its mode and its source, never the call made
//! from the source. So the same replay asked again is answered with its run before the
//! source is read, even after the source run was deleted or the config changed; and an
//! ordinary call's fingerprint (`request/1`) is never a replay's.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::artifact::{ArtifactId, Contents};
use crate::bundle::{Manifest, Role, Use};
use crate::capability::recorded::{self, Held, Recorded};
use crate::identity::{Digest, Encoder, Fields};
use crate::store::{Entry, Generation, Kind};

use super::record::{
    ArgumentValue, DIAGNOSTICS_PREFIX, FromBundle, INTERRUPTED, OUTPUTS_PREFIX, RECORD_NAME,
    ReplayMode, Replayed, RunRecord,
};
use super::{
    Call, CallError, Capability, DESCENDANTS_LIMIT, ModelIdentity, Refusal, Registry, RequestId,
    RunId, StoreFailure, Value,
};

/// What a replay replays: a run of the store, or a step of a bundle the store holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Run(RunId),
    Bundle { digest: Digest, step: u32 },
}

/// A request to replay: its source, its mode, and the caller's id for it when it gave one.
#[derive(Debug, Clone)]
pub struct Replay {
    source: Source,
    mode: ReplayMode,
    id: Option<RequestId>,
}

impl Replay {
    /// An offline replay of this source.
    pub fn of(source: Source) -> Replay {
        Replay {
            source,
            mode: ReplayMode::Offline,
            id: None,
        }
    }

    /// The same replay, live: made again for real, as a new run under the current
    /// config.
    pub fn live(mut self) -> Replay {
        self.mode = ReplayMode::Live;
        self
    }

    /// The same replay under this request id.
    pub fn with_id(mut self, id: RequestId) -> Replay {
        self.id = Some(id);
        self
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn mode(&self) -> ReplayMode {
        self.mode
    }

    pub fn id(&self) -> Option<&RequestId> {
        self.id.as_ref()
    }

    /// The fingerprint a replay's request id is held to, kind `replay/1`, in the encoding
    /// ADR 0001 gives every identity: the mode (0 offline, 1 live); then the source, a
    /// run as 0 and its id, a bundle's step as 1, the bundle's digest and the step. It is
    /// of the replay request alone, so it is had before the source is read.
    pub fn fingerprint(&self) -> Digest {
        let mut fields = Encoder::new("replay/1");
        fields.unsigned(match self.mode {
            ReplayMode::Offline => 0,
            ReplayMode::Live => 1,
        });
        match &self.source {
            Source::Run(run) => {
                fields.unsigned(0);
                fields.bytes(run.as_str().as_bytes());
            }
            Source::Bundle { digest, step } => {
                fields.unsigned(1);
                fields.digest(digest);
                fields.unsigned(u64::from(*step));
            }
        }
        fields.finish()
    }

    /// What every run of the replayed family records of this replay.
    pub(super) fn replayed(&self, run: &str) -> Replayed {
        Replayed {
            run: run.to_owned(),
            bundle: match &self.source {
                Source::Run(_) => None,
                Source::Bundle { digest, step } => Some(FromBundle {
                    digest: *digest,
                    step: *step,
                }),
            },
            mode: self.mode,
        }
    }
}

/// A replay, read from its source: the call made from the root's record, the root's id,
/// the config its settings came from, each model the family's requests were answered
/// by, and, offline, the answers they recorded; and, for a step of a bundle, the name of
/// each input under which the replayed run finds it among the bundle's files.
pub(super) struct Read {
    pub(super) call: Call,
    pub(super) root: String,
    pub(super) config: Option<Digest>,
    pub(super) models: Vec<(Capability, ModelIdentity)>,
    pub(super) recorded: Option<Arc<Recorded>>,
    pub(super) names: Vec<(String, Entry)>,
}

/// What a family is read from, through one generation: the store's own runs, or the
/// entry of a bundle the store holds (ADR 0048, "Importing one").
pub struct Stored<'a> {
    contents: &'a (dyn Contents + Send + Sync),
    generation: &'a Generation,
    bundle: Option<(String, Manifest)>,
}

impl<'a> Stored<'a> {
    /// The store's own runs.
    pub fn of_store(
        contents: &'a (dyn Contents + Send + Sync),
        generation: &'a Generation,
    ) -> Stored<'a> {
        Stored {
            contents,
            generation,
            bundle: None,
        }
    }

    /// The runs of the bundle of this digest the store holds, or why not.
    pub fn of_bundle(
        contents: &'a (dyn Contents + Send + Sync),
        generation: &'a Generation,
        digest: &Digest,
    ) -> Result<Stored<'a>, String> {
        let manifest = match crate::bundle::manifest_of(contents, generation, digest) {
            Some(Ok(manifest)) => manifest,
            Some(Err(why)) => return Err(format!("its manifest cannot be read: {why}")),
            None => return Err(format!("the store holds no bundle {digest}")),
        };
        Ok(Stored {
            contents,
            generation,
            bundle: Some((crate::bundle::entry(digest), manifest)),
        })
    }

    /// The manifest of the bundle, for a bundle's runs.
    pub fn manifest(&self) -> Option<&Manifest> {
        self.bundle.as_ref().map(|(_, manifest)| manifest)
    }

    /// The entry of a name of a run: in the run, or under the bundle's entry.
    fn entry(&self, run: &str, name: &str) -> Option<&'a Entry> {
        match &self.bundle {
            None => self.generation.run(run).and_then(|held| held.get(name)),
            Some((entry, _)) => self.generation.get(&format!("{entry}/runs/{run}/{name}")),
        }
    }

    /// The entry of the file of a run's input or output, at its port and place.
    fn file(&self, run: &str, role: Role, port: &str, place: u32) -> Option<&'a Entry> {
        let entry = match &self.bundle {
            None if role == Role::Output => {
                let name = format!("{OUTPUTS_PREFIX}{port}/{place}");
                self.generation.run(run).and_then(|held| held.get(&name))
            }
            None => None,
            Some((entry, manifest)) => {
                let used = Use {
                    run: run.to_owned(),
                    role,
                    port: port.to_owned(),
                    place,
                };
                let file = manifest
                    .artifacts
                    .iter()
                    .find(|file| file.of.contains(&used))?;
                self.generation
                    .get(&format!("{entry}/files/{}", file.file.hex()))
            }
        };
        entry.filter(|entry| entry.kind == Kind::Output)
    }

    fn record_of(&self, run: &str) -> Result<Option<RunRecord>, CallError> {
        let Some(entry) = self.entry(run, RECORD_NAME) else {
            return Ok(None);
        };
        let bytes = (self.contents.read_record(entry))
            .map_err(|error| CallError::Store(StoreFailure::new(error)))?;
        RunRecord::from_bytes(&bytes).map(Some).map_err(|why| {
            not_replayed(
                run,
                &format!("its record is not one this build reads: {why}"),
            )
        })
    }
}

impl Held for Stored<'_> {
    fn diagnostic(&self, run: &str, name: &str) -> Result<Option<Vec<u8>>, String> {
        let Some(entry) = self.entry(run, name) else {
            return Ok(None);
        };
        (self.contents.read_record(entry))
            .map(Some)
            .map_err(|why| format!("the diagnostic {name} of the run {run} cannot be read: {why}"))
    }

    fn output(&self, run: &str, port: &str, place: u32) -> Result<Option<Vec<u8>>, String> {
        let Some(entry) = self.file(run, Role::Output, port, place) else {
            return Ok(None);
        };
        (self.contents.read(entry))
            .map(Some)
            .map_err(|why| format!("the output {port} of the run {run} cannot be read: {why}"))
    }
}

impl Records for Stored<'_> {
    fn record(&self, run: &str) -> Result<Option<RunRecord>, String> {
        self.record_of(run).map_err(|error| error.to_string())
    }

    fn kept(&self, run: &str, name: &str) -> Result<Option<Vec<u8>>, String> {
        self.diagnostic(run, name)
    }
}

fn not_replayed(run: &str, why: &str) -> CallError {
    CallError::Refused(Refusal::NotReplayed {
        run: run.to_owned(),
        why: why.to_owned(),
    })
}

/// What the reading of a replay is told by its executor: each action this build ships,
/// with its version; the actions' descriptors, for the choices of a live replay; and
/// the tool files the current config names, with their revisions.
pub(super) struct Told<'a> {
    pub(super) shipped: &'a BTreeMap<String, String>,
    pub(super) registry: &'a Registry,
    pub(super) tool_files: &'a BTreeMap<String, String>,
}

/// Reads a replay from its source through this generation: the root's family, every rule
/// of "What is replayed" held, and the call made from the root's record. Whether a step
/// of a bundle replays is decided here from its records, as for a run of the store, and
/// never from what the bundle's manifest says of it (ADR 0049).
pub(super) fn read(
    replay: &Replay,
    contents: &(dyn Contents + Send + Sync),
    generation: &Generation,
    told: &Told<'_>,
) -> Result<Read, CallError> {
    let (stored, root) = match &replay.source {
        Source::Run(run) => {
            if generation.run(run.as_str()).is_none() {
                return Err(CallError::Refused(Refusal::NoSuchRun { run: run.clone() }));
            }
            (
                Stored::of_store(contents, generation),
                run.as_str().to_owned(),
            )
        }
        Source::Bundle { digest, step } => {
            let stored = Stored::of_bundle(contents, generation, digest)
                .map_err(|why| not_replayed("-", &why))?;
            let manifest = stored
                .manifest()
                .ok_or_else(|| not_replayed("-", "no manifest"))?;
            let Some(at) = manifest.steps.get(*step as usize) else {
                return Err(not_replayed(
                    "-",
                    &format!(
                        "the bundle {digest} has {} steps, and no step {step}",
                        manifest.steps.len()
                    ),
                ));
            };
            let root = at.run.clone();
            (stored, root)
        }
    };
    let Some(record) = stored.record_of(&root)? else {
        return Err(not_replayed(
            &root,
            "it has not ended: a run is replayed once it has its record",
        ));
    };
    if let Some(parent) = &record.identity().parent {
        return Err(not_replayed(
            &root,
            &format!(
                "it is a run another run called, which is replayed as part of the run it is \
                 under; replay the run {parent}, or the run that called that"
            ),
        ));
    }
    let asked = record.asked().clone();
    let (Some(action), Some(version)) = (&asked.action, &asked.version) else {
        return Err(not_replayed(&root, "its record lost its admission"));
    };
    match told.shipped.get(action) {
        Some(shipped) if shipped == version => {}
        Some(shipped) => {
            return Err(not_replayed(
                &root,
                &format!("this build ships {action} at version {shipped}, not {version}"),
            ));
        }
        None => {
            return Err(not_replayed(
                &root,
                &format!(
                    "{action} is not an action this build ships: a recipe file's or a tool \
                     file's action is never replayed"
                ),
            ));
        }
    }
    // The family, the root first and each run before the runs it called.
    let mut runs = vec![record];
    let mut at = 0;
    while at < runs.len() {
        let steps: Vec<String> = (runs[at].steps().iter())
            .map(|step| step.run().to_owned())
            .collect();
        for child in steps {
            if runs.len() > DESCENDANTS_LIMIT {
                return Err(not_replayed(
                    &root,
                    "its family holds more runs than one may",
                ));
            }
            match stored.record_of(&child)? {
                Some(record) => runs.push(record),
                None => {
                    return Err(not_replayed(
                        &root,
                        &format!("the run {child} of its family has no record in the store"),
                    ));
                }
            }
        }
        at += 1;
    }
    for record in &runs {
        let ended = record.outcome();
        if ended.error.as_ref().is_some_and(|e| e.kind == INTERRUPTED) {
            let run = &record.identity().run;
            return Err(not_replayed(
                &root,
                &format!(
                    "the run {run} of its family was interrupted: the process that ran it ended \
                     first, and the steps it lost cannot be answered"
                ),
            ));
        }
    }
    if let Some((run, name)) = recorded::at_the_limit(&runs).first() {
        return Err(not_replayed(
            &root,
            &format!(
                "the setting {name} of the run {run} is exactly as long as a setting may be, \
                 so it may have been cut"
            ),
        ));
    }
    let live = replay.mode == ReplayMode::Live;
    for record in &runs {
        let run = &record.identity().run;
        for setting in &record.asked().settings {
            let Some(name) = setting.name().strip_prefix(recorded::TOOL_FILE_SETTING) else {
                continue;
            };
            if !live {
                return Err(not_replayed(
                    &root,
                    &format!(
                        "the run {run} of its family called the tool file {name}: a tool file \
                         is code, and an offline replay never runs one"
                    ),
                ));
            }
            let noted = match setting.value() {
                ArgumentValue::Text(text) => text.as_str(),
                _ => "",
            };
            if told.tool_files.get(name).map(String::as_str) != Some(noted) {
                return Err(not_replayed(
                    &root,
                    &format!(
                        "the run {run} of its family called the tool file {name} at {noted}, \
                         and the config names no tool file of that name with that digest"
                    ),
                ));
            }
        }
    }
    let root_record = &runs[0];
    let mut call = Call::new(action);
    for argument in &root_record.asked().arguments {
        let value = match &argument.value {
            ArgumentValue::Number(number) => Value::Number(*number),
            ArgumentValue::Integer(number) => Value::Integer(*number),
            ArgumentValue::Boolean(boolean) => Value::Boolean(*boolean),
            ArgumentValue::Text(text) => Value::Text(text.clone()),
        };
        call = call.argument(&argument.name, value);
    }
    // Each input by identity; from a bundle, each found among the bundle's files under
    // the artifact's ordinary name, for the run started from it alone.
    let mut names = Vec::new();
    let mut places: BTreeMap<String, u32> = BTreeMap::new();
    for input in root_record.inputs() {
        let place = places.entry(input.port.clone()).or_default();
        if stored.bundle.is_some() {
            let Some(entry) = stored.file(&root, Role::Input, &input.port, *place) else {
                return Err(not_replayed(
                    &root,
                    &format!(
                        "its input {} {} is withheld: the bundle does not carry its file",
                        input.port, place
                    ),
                ));
            };
            let name = crate::artifact::name(input.artifact_type, ArtifactId(input.artifact));
            names.push((name, entry.clone()));
        }
        *place += 1;
        call = call.input(&input.port, ArtifactId(input.artifact));
    }
    let mut models: Vec<(Capability, ModelIdentity)> = Vec::new();
    for record in &runs {
        for model in record.models() {
            let identity = ModelIdentity::new(&model.backend, &model.model, &model.version);
            if !models.contains(&(model.capability, identity.clone())) {
                models.push((model.capability, identity));
            }
        }
    }
    if live {
        call = with_choices(call, &root, &runs, &models, told.registry)?;
    }
    let recorded = (!live).then(|| Arc::new(Recorded::of_family(&runs, &stored)));
    Ok(Read {
        call,
        root,
        config: root_record.asked().config,
        models,
        recorded,
        names,
    })
}

/// A live replay's call with each choice the root's run took written explicitly: the
/// alias its settings name for each capability its action chooses by an argument
/// (`backends.<capability>.<alias>.…`). A capability the family asked whose settings do
/// not name exactly one alias is refused, naming it.
fn with_choices(
    mut call: Call,
    root: &str,
    runs: &[RunRecord],
    models: &[(Capability, ModelIdentity)],
    registry: &Registry,
) -> Result<Call, CallError> {
    let Some(action) = registry.get(call.action()) else {
        return Ok(call);
    };
    for need in action.descriptor().needs() {
        let Some(argument) = need.chooser() else {
            continue;
        };
        if call.arguments().contains_key(argument) {
            continue;
        }
        let capability = need.capability();
        let prefix = format!("backends.{}.", capability.name());
        let mut aliases: Vec<&str> = (runs[0].asked().settings.iter())
            .filter_map(|setting| setting.name().strip_prefix(&prefix)?.split('.').next())
            .collect();
        aliases.dedup();
        match aliases.as_slice() {
            [alias] => call = call.argument(argument, Value::text(alias)),
            _ if models.iter().any(|(c, _)| *c == capability) => {
                return Err(not_replayed(
                    root,
                    &format!(
                        "its settings name {} choices of a {} backend, and a live replay \
                         makes the call again under the one it ran under",
                        aliases.len(),
                        capability.name()
                    ),
                ));
            }
            _ => {}
        }
    }
    Ok(call)
}

/// The note a replay's root keeps of where it stopped, as [`recorded::NOTE`] names it
/// among its diagnostics.
pub fn note_name() -> String {
    format!("{DIAGNOSTICS_PREFIX}{}", recorded::NOTE)
}

// ---- the comparison (plan m7, task R1, item 5) ---------------------------------------------

/// The most differences a report lists; the rest are counted.
pub const DIFFERENCES_LIMIT: usize = 256;

/// One difference between a replayed family and the family it replays: the place of the
/// run it is at (`the run`, `the run, step 2`, …), what differs, and each side, cut.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Difference {
    pub place: String,
    pub what: String,
    pub original: String,
    pub replay: String,
}

/// What a replay came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// Everything compared is the same.
    Reproduced,
    /// Each difference by place, first first, at most [`DIFFERENCES_LIMIT`], and how
    /// many more there were.
    Differs {
        differences: Vec<Difference>,
        more: usize,
    },
    /// The replay stopped where a request diverged or could not be answered.
    Stopped(recorded::Stop),
}

impl Report {
    /// The report in a word: `reproduced`, `differs`, `diverged` or `unanswerable`.
    pub fn name(&self) -> &'static str {
        match self {
            Report::Reproduced => "reproduced",
            Report::Differs { .. } => "differs",
            Report::Stopped(stop) => stop.kind.name(),
        }
    }
}

/// One run of a family as it is compared: its place, its record and what it kept.
pub struct Placed {
    pub place: String,
    pub record: RunRecord,
    pub kept: BTreeMap<String, Vec<u8>>,
}

/// What a family's runs are read from: the store's runs, or a bundle's entry.
pub trait Records {
    fn record(&self, run: &str) -> Result<Option<RunRecord>, String>;
    fn kept(&self, run: &str, name: &str) -> Result<Option<Vec<u8>>, String>;
}

/// A family read by place: the root, then each run's steps in their order, at any depth.
pub fn placed(records: &dyn Records, root: &str) -> Result<Vec<Placed>, String> {
    let mut runs: Vec<Placed> = Vec::new();
    let mut queue = vec![("the run".to_owned(), root.to_owned())];
    let mut at = 0;
    while at < queue.len() {
        if queue.len() > DESCENDANTS_LIMIT + 1 {
            return Err("the family holds more runs than one may".to_owned());
        }
        let (place, run) = queue[at].clone();
        let record = records
            .record(&run)?
            .ok_or_else(|| format!("the run {run} has no record in the store"))?;
        for (number, step) in record.steps().iter().enumerate() {
            queue.push((
                format!("{place}, step {}", number + 1),
                step.run().to_owned(),
            ));
        }
        let mut kept = BTreeMap::new();
        for diagnostic in record.diagnostics() {
            let name = diagnostic.name();
            if name == super::record::JOURNAL_NAME || name == note_name() {
                continue;
            }
            if let Some(bytes) = records.kept(&run, name)? {
                kept.insert(name.to_owned(), bytes);
            }
        }
        runs.push(Placed {
            place,
            record,
            kept,
        });
        at += 1;
    }
    Ok(runs)
}

/// The groups of a record that are compared, by their names in the record's JSON. Times,
/// run ids and request ids are not compared; nor are what was spent, where data went,
/// the models' devices, or the settings, which a live replay reports apart.
const COMPARED: [&str; 9] = [
    "inputs", "outputs", "outcome", "steps", "judge", "plan", "started", "findings", "attempts",
];

/// What two families differ in, run matched to run by place, first first: every
/// difference, in order. A reference to a run of either family is compared through the
/// match: each run's id is read as its place.
pub fn differences(original: &[Placed], replay: &[Placed]) -> Vec<Difference> {
    let names = |family: &[Placed]| -> Vec<(String, String)> {
        (family.iter())
            .map(|run| {
                (
                    run.record.identity().run.clone(),
                    format!("<{}>", run.place),
                )
            })
            .collect()
    };
    let (was_names, now_names) = (names(original), names(replay));
    let placed = |text: &str, names: &[(String, String)]| -> String {
        let mut text = text.to_owned();
        for (id, place) in names {
            text = text.replace(id.as_str(), place);
        }
        text
    };
    let mut found = Vec::new();
    let mut differ = |place: &str, what: &str, was: String, now: String| {
        if was != now {
            found.push(Difference {
                place: place.to_owned(),
                what: what.to_owned(),
                original: cut(&was),
                replay: cut(&now),
            });
        }
    };
    let replays: BTreeMap<&str, &Placed> =
        replay.iter().map(|run| (run.place.as_str(), run)).collect();
    for was in original {
        let place = was.place.as_str();
        let Some(now) = replays.get(place) else {
            differ(place, "run", "a run".to_owned(), "none".to_owned());
            continue;
        };
        let json = |run: &Placed| -> serde_json::Value {
            serde_json::from_slice(&run.record.to_bytes()).unwrap_or_default()
        };
        let (was_json, now_json) = (json(was), json(now));
        let field = |json: &serde_json::Value, path: &[&str]| -> String {
            let mut at = json;
            for key in path {
                at = at.get(key).unwrap_or(&serde_json::Value::Null);
            }
            let mut at = at.clone();
            unspent(&mut at);
            at.to_string()
        };
        for path in [
            ["asked", "action"],
            ["asked", "version"],
            ["asked", "arguments"],
        ] {
            differ(
                place,
                path[1],
                placed(&field(&was_json, &path), &was_names),
                placed(&field(&now_json, &path), &now_names),
            );
        }
        for group in COMPARED {
            differ(
                place,
                group,
                placed(&field(&was_json, &[group]), &was_names),
                placed(&field(&now_json, &[group]), &now_names),
            );
        }
        let names: std::collections::BTreeSet<&String> =
            was.kept.keys().chain(now.kept.keys()).collect();
        for name in names {
            let shown = |kept: Option<&Vec<u8>>, ids: &[(String, String)]| match kept {
                None => "nothing".to_owned(),
                Some(bytes) => match std::str::from_utf8(bytes) {
                    Ok(text) => placed(text, ids),
                    Err(_) => Digest::of_bytes(bytes).to_string(),
                },
            };
            differ(
                place,
                &format!("diagnostic {name}"),
                shown(was.kept.get(name), &was_names),
                shown(now.kept.get(name), &now_names),
            );
        }
    }
    let places: std::collections::BTreeSet<&str> =
        original.iter().map(|run| run.place.as_str()).collect();
    for now in replay
        .iter()
        .filter(|run| !places.contains(run.place.as_str()))
    {
        differ(&now.place, "run", "none".to_owned(), "a run".to_owned());
    }
    // First first: in the order the runs ended, each run's steps before the run itself,
    // so the first difference is where the replay first went otherwise.
    let rank = ended_order(original.iter().chain(replay).map(|run| run.place.as_str()));
    found.sort_by_key(|difference| rank.get(difference.place.as_str()).copied().unwrap_or(0));
    found
}

/// Each place's rank in the order its runs end: a run's steps, in their order, before it.
fn ended_order<'a>(places: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    fn visit<'a>(
        at: &'a str,
        places: &std::collections::BTreeSet<&'a str>,
        rank: &mut BTreeMap<&'a str, usize>,
    ) {
        let prefix = format!("{at}, step ");
        let mut steps: Vec<(u64, &'a str)> = (places.iter())
            .filter_map(|place| {
                let number = place.strip_prefix(prefix.as_str())?;
                number.parse().ok().map(|n| (n, *place))
            })
            .collect();
        steps.sort();
        for (_, step) in steps {
            visit(step, places, rank);
        }
        let next = rank.len();
        rank.entry(at).or_insert(next);
    }
    let places: std::collections::BTreeSet<&str> = places.collect();
    let mut rank = BTreeMap::new();
    if let Some(root) = places.iter().find(|place| !place.contains(", step ")) {
        visit(root, &places, &mut rank);
    }
    rank
}

/// A group of a record without what was spent: a replay offline spends nothing, and
/// what a run spends is not what it made.
fn unspent(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|key, _| !key.ends_with("_micros"));
            map.values_mut().for_each(unspent);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(unspent),
        _ => {}
    }
}

/// The settings a live replay ran under that differ from the original root's, by name.
pub fn settings_differ(original: &RunRecord, replay: &RunRecord) -> Vec<Difference> {
    let of = |record: &RunRecord| -> BTreeMap<String, String> {
        (record.asked().settings.iter())
            .map(|setting| {
                let value = serde_json::to_string(setting.value()).unwrap_or_default();
                (setting.name().to_owned(), value)
            })
            .collect()
    };
    let (was, now) = (of(original), of(replay));
    let names: std::collections::BTreeSet<&String> = was.keys().chain(now.keys()).collect();
    names
        .into_iter()
        .filter(|name| was.get(*name) != now.get(*name))
        .map(|name| Difference {
            place: "the run".to_owned(),
            what: format!("setting {name}"),
            original: cut(was.get(name).map_or("none", String::as_str)),
            replay: cut(now.get(name).map_or("none", String::as_str)),
        })
        .collect()
}

/// A side of a difference as a report shows it: at most 200 characters, and three dots.
fn cut(text: &str) -> String {
    match text.char_indices().nth(200) {
        Some((at, _)) => format!("{}...", &text[..at]),
        None => text.to_owned(),
    }
}

/// The report of a replayed family against the family it replays: where it stopped,
/// when its root noted a stop; else reproduced, or each difference.
pub fn report(original: &[Placed], replay: &[Placed], note: Option<&[u8]>) -> Report {
    if let Some(stop) = note
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(recorded::Stop::read)
    {
        return Report::Stopped(stop);
    }
    let mut differences = differences(original, replay);
    if differences.is_empty() {
        return Report::Reproduced;
    }
    let more = differences.len().saturating_sub(DIFFERENCES_LIMIT);
    differences.truncate(DIFFERENCES_LIMIT);
    Report::Differs { differences, more }
}
