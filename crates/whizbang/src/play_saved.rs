//! Saved-run comparisons, saved examples and Home's library (plan m7, task U1): what
//! [`Playground`] answers the host's [`Saved`] with.
//!
//! **A comparison** is of two ended root runs of one task, read from their families as
//! the result screen reads a run (`crate::play_result`): what each was asked, side by
//! side, each difference marked; what each resolved (the backend and model of each
//! capability, and its settings); its inputs and outputs by port; how it ended, its time,
//! and its cost and models over the whole family, with unresolved spend and a family not
//! wholly read said as such. It claims no winner and computes no score.
//!
//! **An example** is the store's entry `examples/<id>` (ADR 0048: it belongs to no run,
//! and only its own deletion lets it go), `example/1`: a name and notes a person gave, the
//! task, its arguments as the run's record holds them, the backends and models it named,
//! each input by its identity, and the run it was saved from. Beside it, under
//! `examples/<id>/inputs/<n>`, the canonical file of each input, so its inputs outlive the
//! run's deletion and a prune. Opening one gives back, as an import, an input whose
//! name the store let go of, and answers the draft; it starts nothing.
//!
//! **The library** is every picture the store holds as an input of a root run or an
//! import, whatever door gave it, with what the store knows of it: the name a person gave
//! it in the playground, the runs that took it, when it was first used.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json, json};
use whizbang_core::artifact::{self, ArtifactId, ArtifactType};
use whizbang_core::contract::RunId;
use whizbang_core::contract::record::{self, Argument, ArgumentValue, RunRecord};
use whizbang_core::identity::Digest;
use whizbang_core::store::{EXAMPLES_PREFIX, Generation, IMPORTED_PREFIX, Store};
use whizbang_serve::escaped;
use whizbang_serve::play::Refused;
use whizbang_serve::play::saved::{EXAMPLES_LIMIT, Saved, Show, is_example_id};

use crate::play_result::{self, Given, Resolve, Tasks, record_in, record_of};
use crate::playing::Playground;

/// The version an example is written in.
pub const EXAMPLE_VERSION: &str = "example/1";

fn busy() -> Refused {
    Refused::new(500, "the store could not answer")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

// Comparisons.

/// A run's family: its record and those of the runs it called, at every depth, breadth
/// first, at most [`play_result::TREE_LIMIT`]; and whether every one was read.
fn family(store: &Store, top: RunRecord) -> (Vec<RunRecord>, bool) {
    let Ok(generation) = store.snapshot() else {
        return (vec![top], false);
    };
    let mut records = vec![top];
    let mut whole = true;
    let mut at = 0;
    while at < records.len() {
        let children: Vec<String> = (records[at].steps().iter())
            .map(|step| step.run().to_owned())
            .collect();
        for child in children {
            if records.len() >= play_result::TREE_LIMIT {
                whole = false;
                break;
            }
            match record_in(store, &generation, &child) {
                Some(record) => records.push(record),
                None => whole = false,
            }
        }
        at += 1;
    }
    (records, whole)
}

/// An argument's value as the comparison shows it: its words, escaped.
fn shown(value: &ArgumentValue) -> String {
    match value {
        ArgumentValue::Number(number) => number.to_string(),
        ArgumentValue::Integer(number) => number.to_string(),
        ArgumentValue::Boolean(yes) => yes.to_string(),
        ArgumentValue::Text(text) => escaped(text),
    }
}

/// One row of the comparison: its section, its name, the two values (none where a run
/// has none), and whether they differ.
fn row(section: &str, name: &str, baseline: Option<String>, attempt: Option<String>) -> Json {
    let differs = baseline != attempt;
    json!({"section": section, "name": escaped(name), "baseline": baseline,
           "attempt": attempt, "differs": differs})
}

/// Rows of values by name, in the baseline's order and then the attempt's.
fn rows_by_name(
    section: &str,
    baseline: &[(String, String)],
    attempt: &[(String, String)],
) -> Vec<Json> {
    let mut names: Vec<&str> = baseline.iter().map(|(name, _)| name.as_str()).collect();
    for (name, _) in attempt {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    let value = |list: &[(String, String)], name: &str| {
        let found: Vec<&str> = (list.iter())
            .filter(|(at, _)| at == name)
            .map(|(_, value)| value.as_str())
            .collect();
        (!found.is_empty()).then(|| found.join(", "))
    };
    names
        .into_iter()
        .map(|name| row(section, name, value(baseline, name), value(attempt, name)))
        .collect()
}

/// What each capability was answered by over a family: the config's name of its backend
/// and the model, as "backend · model", in the order first asked.
fn resolved(records: &[RunRecord], resolve: &Resolve<'_>) -> Vec<(String, String)> {
    let mut said: Vec<(String, String)> = Vec::new();
    for record in records {
        for model in record.models() {
            let capability = model.capability.name().to_owned();
            let backend = resolve(&capability, &model.backend, &model.model)
                .unwrap_or_else(|| model.backend.clone());
            let words = escaped(&format!("{backend} · {}", model.model));
            if !said
                .iter()
                .any(|(name, value)| *name == capability && *value == words)
            {
                said.push((capability, words));
            }
        }
    }
    said
}

/// One side of a comparison.
fn side(
    store: &Store,
    root: &std::path::Path,
    record: &RunRecord,
    records: &[RunRecord],
    whole: bool,
    tasks: &Tasks<'_>,
    resolve: &Resolve<'_>,
) -> Json {
    let run = record.identity().run.clone();
    let action = record.asked().action.clone().unwrap_or_default();
    let title = tasks
        .descriptor(&action)
        .map(|d| d.title().unwrap_or(d.name()).to_owned());
    let artifacts = |list: Vec<(String, ArtifactType, String)>| -> Vec<Json> {
        list.into_iter()
            .map(|(port, kind, id)| json!({"port": escaped(&port), "type": kind.name(), "id": id}))
            .collect()
    };
    let inputs = artifacts(
        (record.inputs().iter())
            .map(|input| {
                (
                    input.port.clone(),
                    input.artifact_type,
                    input.artifact.to_string(),
                )
            })
            .collect(),
    );
    let outputs = artifacts(
        (record.outputs().iter())
            .map(|output| {
                (
                    output.port.clone(),
                    output.artifact_type,
                    output.artifact.to_string(),
                )
            })
            .collect(),
    );
    let verdict = RunId::parse(&run)
        .ok()
        .and_then(|id| crate::door::verdict(root, &id))
        .map(|stated| json!({"word": escaped(&stated.word), "why": escaped(&stated.why)}));
    let time = record.time();
    let mut models: Vec<String> = (records.iter())
        .flat_map(|one| one.models().iter())
        .map(|model| escaped(&model.model))
        .collect();
    models.sort();
    models.dedup();
    let _ = store;
    json!({
        "run": run,
        "action": escaped(&action),
        "title": title,
        "outcome": record.outcome().status.name(),
        "verdict": verdict,
        "start_ms": time.start_ms,
        "end_ms": time.end_ms,
        "time_ms": time.start_ms.map(|start| time.end_ms.saturating_sub(start)),
        "inputs": inputs,
        "outputs": outputs,
        "cost": play_result::cost(records, resolve),
        "models": models,
        "runs": records.len(),
        "whole": whole,
    })
}

/// What `GET /api/compare/{baseline}/{attempt}` answers.
pub fn compare_json(
    store: &Store,
    root: &std::path::Path,
    baseline: &RunId,
    attempt: &RunId,
    tasks: &Tasks<'_>,
    resolve: &Resolve<'_>,
) -> Result<Json, Refused> {
    let first = record_of(store, baseline)?;
    let second = record_of(store, attempt)?;
    for record in [&first, &second] {
        if record.identity().parent.is_some() {
            return Err(Refused::new(
                422,
                &format!(
                    "run {} was called by another run: a comparison is of two runs a person started",
                    escaped(&record.identity().run)
                ),
            ));
        }
    }
    let (one, two) = (
        first.asked().action.clone().unwrap_or_default(),
        second.asked().action.clone().unwrap_or_default(),
    );
    if one != two {
        let title = |action: &str| {
            tasks.descriptor(action).map_or(action.to_owned(), |d| {
                d.title().unwrap_or(d.name()).to_owned()
            })
        };
        return Err(Refused::new(
            422,
            &format!(
                "the baseline is {}; this run is {}. A comparison is two runs of one task",
                escaped(&title(&one)),
                escaped(&title(&two))
            ),
        ));
    }
    let (left, left_whole) = family(store, first);
    let (right, right_whole) = family(store, second);
    let (a, b) = (&left[0], &right[0]);
    let arguments = |record: &RunRecord| -> Vec<(String, String)> {
        (record.asked().arguments.iter())
            .map(|argument| (argument.name.clone(), shown(&argument.value)))
            .collect()
    };
    let settings = |record: &RunRecord| -> Vec<(String, String)> {
        (record.asked().settings.iter())
            .map(|setting| (setting.name().to_owned(), shown(setting.value())))
            .collect()
    };
    let ports = |list: Vec<(String, String)>| list;
    let inputs = |record: &RunRecord| {
        ports(
            (record.inputs().iter())
                .map(|input| (input.port.clone(), input.artifact.to_string()))
                .collect(),
        )
    };
    let outputs = |record: &RunRecord| {
        ports(
            (record.outputs().iter())
                .map(|output| (output.port.clone(), output.artifact.to_string()))
                .collect(),
        )
    };
    let mut rows = rows_by_name("asked", &arguments(a), &arguments(b));
    rows.extend(rows_by_name(
        "resolved",
        &resolved(&left, resolve),
        &resolved(&right, resolve),
    ));
    rows.extend(rows_by_name("settings", &settings(a), &settings(b)));
    rows.extend(rows_by_name("inputs", &inputs(a), &inputs(b)));
    rows.extend(rows_by_name("outputs", &outputs(a), &outputs(b)));
    Ok(json!({
        "action": escaped(&one),
        "baseline": side(store, root, a, &left, left_whole, tasks, resolve),
        "attempt": side(store, root, b, &right, right_whole, tasks, resolve),
        "rows": rows,
    }))
}

// Examples.

/// An input of an example: its port, its artifact's type and its identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExampleInput {
    pub port: String,
    #[serde(rename = "type")]
    pub artifact_type: String,
    pub id: String,
}

/// A request of the run an example was saved from: its capability, the config's name of
/// its backend, and its model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExampleModel {
    pub capability: String,
    pub backend: String,
    pub model: String,
}

/// `example/1`: what the store keeps of an example.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Example {
    pub example: String,
    pub id: String,
    pub name: String,
    pub notes: String,
    pub action: String,
    pub arguments: Vec<Argument>,
    /// The config's name of the backend each capability was answered by.
    pub backends: BTreeMap<String, String>,
    pub models: Vec<ExampleModel>,
    pub inputs: Vec<ExampleInput>,
    /// The run it was saved from, which may since have been deleted.
    pub run: String,
    pub at_ms: u64,
}

/// The name of an example's entry.
pub fn example_name(id: &str) -> String {
    format!("{EXAMPLES_PREFIX}{id}")
}

/// The name of the canonical file of an example's input, by its place.
fn input_name(id: &str, at: usize) -> String {
    format!("{EXAMPLES_PREFIX}{id}/inputs/{at}")
}

/// The ids of the examples a generation holds, in order.
pub fn example_ids(generation: &Generation) -> Vec<String> {
    (generation.entries().range(EXAMPLES_PREFIX.to_owned()..))
        .map(|(name, _)| name)
        .take_while(|name| name.starts_with(EXAMPLES_PREFIX))
        .filter_map(|name| name.strip_prefix(EXAMPLES_PREFIX))
        .filter(|rest| !rest.contains('/'))
        .map(str::to_owned)
        .collect()
}

/// The example of this id, when the generation holds one that reads.
pub fn example_in(store: &Store, generation: &Generation, id: &str) -> Option<Example> {
    let entry = generation.get(&example_name(id))?;
    let bytes = store.read_output(entry).ok()?;
    serde_json::from_slice::<Example>(&bytes)
        .ok()
        .filter(|example| example.example == EXAMPLE_VERSION && example.id == id)
}

fn artifact_type(name: &str) -> Option<ArtifactType> {
    ArtifactType::ALL
        .into_iter()
        .find(|kind| kind.name() == name)
}

/// An example as the page lists it, every text escaped: and whether this build still has
/// its task.
fn example_json(example: &Example, tasks: &Tasks<'_>) -> Json {
    let descriptor = tasks.descriptor(&example.action);
    let picture = (example.inputs.iter())
        .find(|input| input.artifact_type == "image")
        .map(|input| input.id.clone());
    json!({
        "id": example.id,
        "name": escaped(&example.name),
        "notes": escaped(&example.notes),
        "task": escaped(&example.action),
        "title": descriptor.map(|d| d.title().unwrap_or(d.name()).to_owned()),
        "kit": descriptor.and_then(|d| crate::play_form::listed_in(d, tasks.kits)),
        "known": descriptor.is_some(),
        "run": example.run,
        "at_ms": example.at_ms,
        "picture": picture,
        "inputs": example.inputs.iter().map(|input| json!({"port": escaped(&input.port),
            "type": input.artifact_type, "id": input.id})).collect::<Vec<_>>(),
        "backends": example.backends.iter().map(|(capability, backend)| {
            (capability.clone(), json!(escaped(backend)))
        }).collect::<Map<String, Json>>(),
    })
}

/// Whether a record's call depends on a setting that may have been cut: a text exactly
/// at the record's limit (`SETTING_TEXT_LIMIT`), which a draft cannot make again.
fn cut_setting(record: &RunRecord) -> Option<String> {
    (record.asked().settings.iter())
        .find(|setting| {
            matches!(setting.value(), ArgumentValue::Text(text)
                if text.chars().count() >= record::SETTING_TEXT_LIMIT)
        })
        .map(|setting| setting.name().to_owned())
}

/// Saves the run `run` as an example: its record read whole, each input's canonical file
/// kept beside it, in one publication.
pub fn save(
    store: &Store,
    run: &RunId,
    name: &str,
    notes: &str,
    resolve: &Resolve<'_>,
    tasks: &Tasks<'_>,
) -> Result<Json, Refused> {
    let top = record_of(store, run)?;
    if top.identity().parent.is_some() {
        return Err(Refused::new(
            422,
            "an example is saved from a run a person started, not from a step of one",
        ));
    }
    let Some(action) = top.asked().action.clone() else {
        return Err(Refused::new(
            422,
            "the run's record does not say what was asked",
        ));
    };
    if let Some(setting) = cut_setting(&top) {
        return Err(Refused::new(
            422,
            &format!(
                "the run's setting {} is at the record's limit and may have been cut: a draft \
                 cannot make the same call, so it is not saved as an example",
                escaped(&setting)
            ),
        ));
    }
    let generation = store.snapshot().map_err(|_| busy())?;
    let held = example_ids(&generation);
    if held.len() >= EXAMPLES_LIMIT {
        return Err(Refused::new(
            409,
            &format!(
                "the store holds {} examples, the most it keeps ({EXAMPLES_LIMIT}): delete one first",
                held.len()
            ),
        ));
    }
    let (records, _) = family(store, top);
    let top = &records[0];
    let used = play_result::backends_used(&records, resolve);
    let models = (records.iter())
        .flat_map(|record| record.models().iter())
        .map(|model| {
            let capability = model.capability.name().to_owned();
            ExampleModel {
                backend: resolve(&capability, &model.backend, &model.model)
                    .unwrap_or_else(|| model.backend.clone()),
                capability,
                model: model.model.clone(),
            }
        })
        .fold(Vec::new(), |mut list: Vec<ExampleModel>, model| {
            if !list.contains(&model) {
                list.push(model);
            }
            list
        });
    // Each input's canonical file, read from its name outside runs.
    let mut files = Vec::new();
    for input in top.inputs() {
        let named = artifact::name(input.artifact_type, ArtifactId(input.artifact));
        let entry = generation.get(&named).ok_or_else(|| {
            Refused::new(
                409,
                "an input of the run is no longer in the store, so no example can name it",
            )
        })?;
        files.push(store.read_output(entry).map_err(|_| busy())?);
    }
    let id = whizbang_serve::play::token::random_hex(8)
        .map_err(|_| Refused::new(500, "the system's randomness could not be read"))?;
    let example = Example {
        example: EXAMPLE_VERSION.to_owned(),
        id: id.clone(),
        name: name.to_owned(),
        notes: notes.to_owned(),
        action,
        arguments: top.asked().arguments.clone(),
        backends: used,
        models,
        inputs: (top.inputs().iter())
            .map(|input| ExampleInput {
                port: input.port.clone(),
                artifact_type: input.artifact_type.name().to_owned(),
                id: input.artifact.to_string(),
            })
            .collect(),
        run: top.identity().run.clone(),
        at_ms: now_ms(),
    };
    let bytes = serde_json::to_vec(&example).map_err(|_| busy())?;
    let mut publication = store.begin().map_err(|_| busy())?;
    publication
        .put(&example_name(&id), &bytes)
        .map_err(|_| Refused::new(500, "the store could not keep the example"))?;
    for (at, file) in files.iter().enumerate() {
        publication
            .put(&input_name(&id, at), file)
            .map_err(|_| Refused::new(500, "the store could not keep the example"))?;
    }
    publication
        .publish()
        .map_err(|_| Refused::new(500, "the store could not keep the example"))?;
    Ok(example_json(&example, tasks))
}

/// Every example the store holds, newest first.
pub fn list(store: &Store, tasks: &Tasks<'_>) -> Result<Json, Refused> {
    let generation = store.snapshot().map_err(|_| busy())?;
    let mut examples: Vec<Example> = (example_ids(&generation).iter())
        .filter_map(|id| example_in(store, &generation, id))
        .collect();
    examples.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then_with(|| a.id.cmp(&b.id)));
    let listed: Vec<Json> = examples
        .iter()
        .map(|one| example_json(one, tasks))
        .collect();
    Ok(json!({"examples": listed, "limit": EXAMPLES_LIMIT}))
}

fn no_example() -> Refused {
    Refused::new(404, "the store holds no example of that id")
}

/// Gives an example another name, other notes, or both.
pub fn rename(
    store: &Store,
    id: &str,
    name: Option<&str>,
    notes: Option<&str>,
    tasks: &Tasks<'_>,
) -> Result<Json, Refused> {
    let generation = store.snapshot().map_err(|_| busy())?;
    let mut example = example_in(store, &generation, id).ok_or_else(no_example)?;
    if let Some(name) = name {
        name.clone_into(&mut example.name);
    }
    if let Some(notes) = notes {
        notes.clone_into(&mut example.notes);
    }
    let bytes = serde_json::to_vec(&example).map_err(|_| busy())?;
    let mut publication = store.begin().map_err(|_| busy())?;
    publication
        .put(&example_name(id), &bytes)
        .map_err(|_| Refused::new(500, "the store could not keep the example"))?;
    publication
        .publish()
        .map_err(|_| Refused::new(500, "the store could not keep the example"))?;
    Ok(example_json(&example, tasks))
}

/// Lets go of an example, its names alone: its run and its pictures stay.
pub fn delete(store: &Store, id: &str) -> Result<Json, Refused> {
    let mut publication = store.begin().map_err(|_| busy())?;
    publication
        .let_go_of_entry(&example_name(id))
        .map_err(|_| no_example())?;
    match publication.publish() {
        Ok(_) => Ok(json!({"id": id, "deleted": true})),
        Err(whizbang_core::store::Error::NoSuchEntry { .. }) => Err(no_example()),
        Err(_) => Err(busy()),
    }
}

/// The draft an example opens. An input whose name the store let go of (its run was
/// deleted, and no other name held it) is given back from the example's own file, as an
/// import, so the draft can take it; nothing else is written, and nothing is started.
pub fn open(store: &Store, id: &str, tasks: &Tasks<'_>) -> Result<Json, Refused> {
    let generation = store.snapshot().map_err(|_| busy())?;
    let example = example_in(store, &generation, id).ok_or_else(no_example)?;
    let Some(descriptor) = tasks.descriptor(&example.action) else {
        return Err(Refused::new(
            422,
            &format!(
                "this build has no task \u{201c}{}\u{201d}: the example can be read and deleted, not opened",
                escaped(&example.action)
            ),
        ));
    };
    let mut given = Vec::new();
    let mut publication = None;
    for (at, input) in example.inputs.iter().enumerate() {
        let (Some(kind), Ok(digest)) = (
            artifact_type(&input.artifact_type),
            input.id.parse::<Digest>(),
        ) else {
            return Err(Refused::new(500, "the example's inputs could not be read"));
        };
        let named = artifact::name(kind, ArtifactId(digest));
        if !artifact::holds(&generation, kind, ArtifactId(digest)) {
            let entry = generation.get(&input_name(id, at)).ok_or_else(|| {
                Refused::new(409, "an input of the example is no longer in the store")
            })?;
            let bytes = store.read_output(entry).map_err(|_| busy())?;
            if publication.is_none() {
                publication = Some(store.begin().map_err(|_| busy())?);
            }
            if let Some(publication) = publication.as_mut() {
                publication.put(&named, &bytes).map_err(|_| busy())?;
                publication.mark_imported(&named).map_err(|_| busy())?;
            }
        }
        given.push(Given {
            port: input.port.clone(),
            artifact_type: kind,
            id: input.id.clone(),
        });
    }
    if let Some(publication) = publication {
        publication
            .publish()
            .map_err(|_| Refused::new(500, "the store could not give back the example's inputs"))?;
    }
    let mut draft = tasks
        .draft_of(
            &example.action,
            &example.arguments,
            &given,
            &example.backends,
            None,
        )
        .ok_or_else(|| Refused::new(422, "the example's inputs do not fit its task"))?;
    // Each backend the draft chose, apart: the host prepares each, and names those this
    // config does not offer now.
    let choices: Vec<Json> = crate::play_form::choosers_of(descriptor)
        .into_iter()
        .filter_map(|(argument, capability)| {
            let value = draft["values"][&argument].as_str()?.to_owned();
            Some(json!({"argument": argument, "capability": capability, "value": value}))
        })
        .collect();
    draft["choices"] = Json::Array(choices);
    draft["example"] = json!({"id": example.id, "name": escaped(&example.name),
                              "notes": escaped(&example.notes), "run": example.run});
    Ok(draft)
}

// The library.

/// What the store knows of one picture of the library.
#[derive(Debug, Clone, Default)]
struct Known {
    name: Option<String>,
    named_ms: Option<u64>,
    imported_ms: Option<u64>,
    imported: bool,
    /// The root runs that took it: each id, start, action and outcome.
    runs: Vec<(String, u64, String, String)>,
}

impl Known {
    fn first_ms(&self) -> Option<u64> {
        let runs = self.runs.iter().map(|(_, at, _, _)| *at);
        runs.chain(self.imported_ms)
            .chain(self.named_ms)
            .filter(|at| *at > 0)
            .min()
    }

    fn last_ms(&self) -> u64 {
        let runs = self.runs.iter().map(|(_, at, _, _)| *at);
        runs.chain(self.imported_ms)
            .chain(self.named_ms)
            .max()
            .unwrap_or(0)
    }

    fn shows(&self, show: Show) -> bool {
        match show {
            Show::All => true,
            Show::Named => self.name.is_some(),
            Show::Inputs => !self.runs.is_empty(),
            Show::Imported => self.imported,
        }
    }
}

/// Every picture of the library, by its identity: from the records of the store's root
/// runs, read once for each generation as history reads them, and from its imports'
/// marks and the playground's names.
fn known(store: &Store, loaded: &crate::stats::Loaded) -> Result<BTreeMap<String, Known>, Refused> {
    let generation = store.snapshot().map_err(|_| busy())?;
    let mut all: BTreeMap<String, Known> = BTreeMap::new();
    for (id, record) in &loaded.all {
        if record.identity().parent.is_some() {
            continue;
        }
        let at = record.time().start_ms.unwrap_or(0);
        let action = record.asked().action.clone().unwrap_or_default();
        let outcome = record.outcome().status.name().to_owned();
        for input in record.inputs() {
            if input.artifact_type != ArtifactType::Image {
                continue;
            }
            let runs = &mut all.entry(input.artifact.to_string()).or_default().runs;
            if !runs.iter().any(|(run, ..)| run == id) {
                runs.push((id.clone(), at, action.clone(), outcome.clone()));
            }
        }
    }
    let marks = format!("{IMPORTED_PREFIX}artifacts/{}/", ArtifactType::Image.name());
    for (name, _) in (generation.entries().range(marks.clone()..))
        .take_while(|(name, _)| name.starts_with(&marks))
    {
        let Some(hex) = name.strip_prefix(&marks) else {
            continue;
        };
        let Some(digest) = Digest::from_hex(hex) else {
            continue;
        };
        let marked = name.strip_prefix(IMPORTED_PREFIX).unwrap_or_default();
        let one = all.entry(digest.to_string()).or_default();
        one.imported = true;
        one.imported_ms = generation.imported(marked).map(|import| import.at_ms);
    }
    let pictures = crate::play_kept::PICTURES;
    for (_, entry) in (generation.entries().range(pictures.to_owned()..))
        .take_while(|(name, _)| name.starts_with(pictures))
    {
        let Some(named) = (store.read_output(entry).ok())
            .and_then(|bytes| serde_json::from_slice::<crate::play_kept::Named>(&bytes).ok())
        else {
            continue;
        };
        let one = all.entry(named.id.clone()).or_default();
        one.name = Some(named.name);
        one.named_ms = Some(named.at_ms);
    }
    // Only what the store still holds as a picture.
    all.retain(|id, _| {
        id.parse::<Digest>().is_ok_and(|digest| {
            artifact::holds(&generation, ArtifactType::Image, ArtifactId(digest))
        })
    });
    for one in all.values_mut() {
        one.runs
            .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    }
    Ok(all)
}

fn picture_json(id: &str, one: &Known) -> Json {
    json!({
        "id": id,
        "name": one.name.as_deref().map(escaped),
        "runs": one.runs.len(),
        "imported": one.imported,
        "first_ms": one.first_ms(),
        "last_ms": one.last_ms(),
    })
}

/// One page of the library, newest used first, and how many pictures each chip holds.
pub fn library(
    store: &Store,
    loaded: &crate::stats::Loaded,
    show: Show,
    after: Option<&str>,
    limit: usize,
) -> Result<Json, Refused> {
    let all = known(store, loaded)?;
    let count = |show: Show| all.values().filter(|one| one.shows(show)).count();
    let counts = json!({"all": count(Show::All), "named": count(Show::Named),
                        "inputs": count(Show::Inputs), "imported": count(Show::Imported)});
    let mut shown: Vec<(&String, &Known)> = all.iter().filter(|(_, one)| one.shows(show)).collect();
    shown.sort_by(|a, b| b.1.last_ms().cmp(&a.1.last_ms()).then_with(|| a.0.cmp(b.0)));
    let start = match after {
        Some(after) => match shown.iter().position(|(id, _)| id.as_str() == after) {
            Some(at) => at + 1,
            None => return Err(Refused::new(400, "after names no picture of this list")),
        },
        None => 0,
    };
    let page: Vec<Json> = (shown.iter().skip(start).take(limit))
        .map(|(id, one)| picture_json(id, one))
        .collect();
    let more = shown.len() > start + page.len();
    Ok(json!({"pictures": page, "more": more, "counts": counts}))
}

/// The most runs a picture's card lists.
pub const CARD_RUNS: usize = 50;

/// One picture of the library: what the page lists of it, its size and bytes, and the
/// runs that took it, newest first, at most [`CARD_RUNS`].
pub fn card(
    store: &Store,
    loaded: &crate::stats::Loaded,
    id: &str,
    tasks: &Tasks<'_>,
) -> Result<Json, Refused> {
    let none = || Refused::new(404, "the store holds no picture of that identity");
    let all = known(store, loaded)?;
    let one = all.get(id).ok_or_else(none)?;
    let digest = id.parse::<Digest>().map_err(|_| none())?;
    let generation = store.snapshot().map_err(|_| busy())?;
    let entry = generation
        .get(&artifact::name(ArtifactType::Image, ArtifactId(digest)))
        .ok_or_else(none)?;
    let limits = whizbang_core::identity::DecodeLimits::default();
    let size = match artifact::get(store, ArtifactType::Image, ArtifactId(digest), &limits) {
        Ok(artifact::Artifact::Image(image)) => {
            Some((image.space().width(), image.space().height()))
        }
        _ => None,
    };
    let mut said = picture_json(id, one);
    said["width"] = json!(size.map(|(width, _)| width));
    said["height"] = json!(size.map(|(_, height)| height));
    said["bytes"] = json!(entry.bytes);
    said["runs_list"] = Json::Array(
        (one.runs.iter().take(CARD_RUNS))
            .map(|(run, at, action, outcome)| {
                let title = tasks
                    .descriptor(action)
                    .map(|d| d.title().unwrap_or(d.name()).to_owned());
                json!({"run": run, "at_ms": at, "action": escaped(action), "title": title,
                       "outcome": outcome})
            })
            .collect(),
    );
    Ok(said)
}

impl Saved for Playground {
    fn compare(&self, baseline: &RunId, attempt: &RunId) -> Result<Json, Refused> {
        let store = self.store()?;
        let root = self
            .root()
            .map(std::path::Path::to_path_buf)
            .ok_or_else(|| Refused::new(503, "the playground keeps no store"))?;
        let rows = crate::backend_status::readiness(self.over()).unwrap_or_default();
        let resolve = |capability: &str, kind: &str, model: &str| {
            crate::playing::resolved(&rows, capability, kind, model)
        };
        compare_json(&store, &root, baseline, attempt, &self.tasks(), &resolve)
    }

    fn examples(&self) -> Result<Json, Refused> {
        list(&*self.store()?, &self.tasks())
    }

    fn save_example(&self, run: &RunId, name: &str, notes: &str) -> Result<Json, Refused> {
        let store = self.store()?;
        let rows = crate::backend_status::readiness(self.over()).unwrap_or_default();
        let resolve = |capability: &str, kind: &str, model: &str| {
            crate::playing::resolved(&rows, capability, kind, model)
        };
        save(&store, run, name, notes, &resolve, &self.tasks())
    }

    fn rename_example(
        &self,
        id: &str,
        name: Option<&str>,
        notes: Option<&str>,
    ) -> Result<Json, Refused> {
        if !is_example_id(id) {
            return Err(no_example());
        }
        rename(&*self.store()?, id, name, notes, &self.tasks())
    }

    fn delete_example(&self, id: &str) -> Result<Json, Refused> {
        if !is_example_id(id) {
            return Err(no_example());
        }
        delete(&*self.store()?, id)
    }

    fn open_example(&self, id: &str) -> Result<Json, Refused> {
        if !is_example_id(id) {
            return Err(no_example());
        }
        open(&*self.store()?, id, &self.tasks())
    }

    fn library(&self, show: Show, after: Option<&str>, limit: usize) -> Result<Json, Refused> {
        let loaded = self.loaded()?;
        library(&*self.store()?, &loaded, show, after, limit)
    }

    fn library_card(&self, id: &str) -> Result<Json, Refused> {
        let loaded = self.loaded()?;
        card(&*self.store()?, &loaded, id, &self.tasks())
    }
}
