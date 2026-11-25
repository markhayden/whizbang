//! What a run that ended gives the page (plan m5, task W2): its record as the result
//! screen reads it, the tasks its outputs can be sent to, the draft "Run again" opens,
//! and what its command line is made of.
//!
//! **The record** (`GET /api/runs/{id}/record`): the outcome; the verdict, its reasons and
//! the places its findings cite; a loop's attempts, each with its outcome and the picture
//! its fill made, which is a diagnostic of the loop and not its output; the cost of the
//! run and of every run it called, split by capability and backend, a request no paid
//! entry names being free local work; the inputs and the outputs with their types. Every
//! text a model, a tool or a person gave is escaped before it is handed on to be shown;
//! the values of a draft (Send to, Run again) are the run's own, unescaped, since they
//! are sent again as they are, and the page puts them in as text.
//!
//! **Send to.** A port's descriptor says the type it takes and not its role, so a
//! result's output fills a field of another task only when exactly one field of that
//! task takes its type; where two or more do, the draft opens with none of them filled
//! and the person chooses. A rascal result that made a picture from a picture is sent to
//! "Check an edit" by its roles: its input as the source, its output as the candidate,
//! and, when its run had a phrase, the job the loop writes from it
//! (`whizbang_rascal::checked::job`), which the check reads from the store by the run's
//! id (`job_of`), never from the page.
//!
//! **Run again** is a new draft of the same task from the run's saved inputs and
//! arguments, with each backend it resolved written out; it is a copy, and changing it
//! changes nothing of the run.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Map, Value as Json, json};
use whizbang_core::artifact::ArtifactType;
use whizbang_core::contract::record::{self, ArgumentValue, Changed, Decision, RunRecord};
use whizbang_core::contract::{ActionDescriptor, Kind, RunId, Value};
use whizbang_core::description::Kit;
use whizbang_core::store::{Generation, Store};
use whizbang_serve::escaped;
use whizbang_serve::play::Refused;
use whizbang_squint::job::{Job, Role};

use crate::play_form;

/// The most runs one record's cost and backends are read from: the run and those it
/// called, and theirs, breadth first.
pub const TREE_LIMIT: usize = 256;

/// The argument a check of a use case names the run whose job it takes by.
pub const JOB_OF: &str = "job_of";

/// The actions whose argument `prompt` is the phrase the loop's job is written from.
const PHRASED: [&str; 2] = ["remove_prompt", whizbang_rascal::checked::NAME];

/// The use case a picture made from a picture is checked under.
const CHECK_AN_EDIT: &str = "source-fidelity";

fn busy() -> Refused {
    Refused::new(500, "the store could not answer")
}

/// The record of the run `run` in this generation, when the store holds one it reads.
pub fn record_in(store: &Store, generation: &Generation, run: &str) -> Option<RunRecord> {
    record::stored(generation, run, |entry| store.read(entry))?.ok()
}

/// The record of the run `run`: 404 when the store holds no such run, 409 when it holds
/// the run and no record yet (it has not ended).
pub fn record_of(store: &Store, run: &RunId) -> Result<RunRecord, Refused> {
    let generation = store.snapshot().map_err(|_| busy())?;
    match record::stored(&generation, run.as_str(), |entry| store.read(entry)) {
        None => Err(Refused::new(404, "no run has that id in this store")),
        Some(Ok(record)) => Ok(record),
        Some(Err(record::StoredRecordError::NoRecord)) => Err(Refused::new(
            409,
            "the run has not ended: it has no record yet",
        )),
        Some(Err(_)) => Err(Refused::new(500, "the run's record could not be read")),
    }
}

/// The run's record, then the records of the runs it called and theirs, breadth first, at
/// most [`TREE_LIMIT`]. A child whose record cannot be read is left out.
pub fn tree(store: &Store, top: RunRecord) -> Vec<RunRecord> {
    let Ok(generation) = store.snapshot() else {
        return vec![top];
    };
    let mut records = vec![top];
    let mut at = 0;
    while at < records.len() && records.len() < TREE_LIMIT {
        let children: Vec<String> = records[at]
            .steps()
            .iter()
            .map(|step| step.run().to_owned())
            .collect();
        for child in children {
            if records.len() >= TREE_LIMIT {
                break;
            }
            if let Some(record) = record_in(store, &generation, &child) {
                records.push(record);
            }
        }
        at += 1;
    }
    records
}

/// Millionths of a US dollar as dollars.
#[allow(clippy::cast_precision_loss)]
fn dollars(micros: u64) -> f64 {
    micros as f64 / 1_000_000.0
}

/// The cost of these runs, split by capability and backend: each line's capability,
/// backend, model, how many requests and what they settled; a line no paid request is in
/// is free (local work). The lines are in the order their first request was sent.
pub fn cost(records: &[RunRecord], resolve: &Resolve<'_>) -> Json {
    struct Line {
        capability: String,
        backend: String,
        model: String,
        requests: u64,
        micros: u64,
        paid: bool,
    }
    let mut lines: Vec<Line> = Vec::new();
    for record in records {
        for (at, model) in record.models().iter().enumerate() {
            let paid = record
                .spend()
                .requests
                .iter()
                .find(|paid| paid.request() == at as u64);
            let capability = model.capability.name().to_owned();
            let backend = resolve(&capability, &model.backend, &model.model)
                .unwrap_or_else(|| model.backend.clone());
            let line = match lines
                .iter_mut()
                .position(|line| line.capability == capability && line.backend == backend)
            {
                Some(found) => &mut lines[found],
                None => {
                    lines.push(Line {
                        capability,
                        backend,
                        model: model.model.clone(),
                        requests: 0,
                        micros: 0,
                        paid: false,
                    });
                    let last = lines.len() - 1;
                    &mut lines[last]
                }
            };
            line.requests += 1;
            if let Some(paid) = paid {
                line.micros += paid.settled_micros();
                line.paid = true;
            }
        }
    }
    let unresolved: u64 = records
        .iter()
        .flat_map(|record| record.spend().requests.iter())
        .map(|paid| paid.unresolved_micros())
        .sum();
    let total: u64 = lines.iter().map(|line| line.micros).sum();
    let shown: Vec<Json> = lines
        .iter()
        .map(|line| {
            json!({"capability": escaped(&line.capability), "backend": escaped(&line.backend),
                   "model": escaped(&line.model), "requests": line.requests,
                   "cost": dollars(line.micros), "free": !line.paid})
        })
        .collect();
    json!({"lines": shown, "total": dollars(total), "unresolved": dollars(unresolved)})
}

/// What names a backend by the config's name for it, from what a record says of a
/// request: its capability, and the kind and model the backend declared. None when the
/// config names none, or more than one, so.
pub type Resolve<'a> = dyn Fn(&str, &str, &str) -> Option<String> + 'a;

/// The config's name of the backend each capability was answered by in these runs: the
/// first request's, as `resolve` names it.
pub fn backends_used(records: &[RunRecord], resolve: &Resolve<'_>) -> BTreeMap<String, String> {
    let mut used = BTreeMap::new();
    for record in records {
        for model in record.models() {
            let capability = model.capability.name();
            if used.contains_key(capability) {
                continue;
            }
            if let Some(name) = resolve(capability, &model.backend, &model.model) {
                used.insert(capability.to_owned(), name);
            }
        }
    }
    used
}

/// An argument's value as JSON.
pub fn argument_json(value: &ArgumentValue) -> Json {
    match value {
        ArgumentValue::Number(number) => json!(number),
        ArgumentValue::Integer(number) => json!(number),
        ArgumentValue::Boolean(boolean) => json!(boolean),
        ArgumentValue::Text(text) => json!(text),
    }
}

/// An argument's value as a call's.
pub fn argument_value(value: &ArgumentValue) -> Value {
    match value {
        ArgumentValue::Number(number) => Value::Number(*number),
        ArgumentValue::Integer(number) => Value::Integer(*number),
        ArgumentValue::Boolean(boolean) => Value::Boolean(*boolean),
        ArgumentValue::Text(text) => Value::Text(text.clone()),
    }
}

fn argument<'a>(record: &'a RunRecord, name: &str) -> Option<&'a ArgumentValue> {
    record
        .asked()
        .arguments
        .iter()
        .find(|argument| argument.name == name)
        .map(|argument| &argument.value)
}

fn text_argument<'a>(record: &'a RunRecord, name: &str) -> Option<&'a str> {
    match argument(record, name)? {
        ArgumentValue::Text(text) => Some(text),
        _ => None,
    }
}

fn decision_word(decision: Decision) -> &'static str {
    match decision {
        Decision::Pass => "pass",
        Decision::Fail => "fail",
        Decision::Unsure => "unsure",
    }
}

/// Where a check takes its job from (the page's `job_of`), each a call the store holds:
/// a run of the store, by its id (Send to, Run again); a saved example (`{"example": id}`);
/// or a step of a bundle the store holds (`{"bundle": digest, "step": n}`). The host reads
/// the job there, never from the page (plan m7, N1's pass, finding 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobOf {
    Run(String),
    Example(String),
    Step { bundle: String, step: u32 },
}

impl JobOf {
    /// What the page sent as `job_of`.
    pub fn of(value: &Json) -> Result<JobOf, String> {
        const SAID: &str = "job_of names a run by its id, an example as {\"example\": id}, \
                            or a bundle's step as {\"bundle\": digest, \"step\": n}";
        match value {
            Json::String(run) => Ok(JobOf::Run(run.clone())),
            Json::Object(named) if named.len() == 1 => match named.get("example") {
                Some(Json::String(id)) => Ok(JobOf::Example(id.clone())),
                _ => Err(SAID.to_owned()),
            },
            Json::Object(named) if named.len() == 2 => {
                match (
                    named.get("bundle"),
                    named.get("step").and_then(Json::as_u64),
                ) {
                    (Some(Json::String(bundle)), Some(step)) => Ok(JobOf::Step {
                        bundle: bundle.clone(),
                        step: u32::try_from(step).map_err(|_| SAID.to_owned())?,
                    }),
                    _ => Err(SAID.to_owned()),
                }
            }
            _ => Err(SAID.to_owned()),
        }
    }

    /// As a draft gives it to the page.
    pub fn json(&self) -> Json {
        match self {
            JobOf::Run(run) => json!(run),
            JobOf::Example(id) => json!({"example": id}),
            JobOf::Step { bundle, step } => json!({"bundle": bundle, "step": step}),
        }
    }

    /// The recorded call it names: its action and its arguments.
    fn call(&self, store: &Store) -> Result<(String, Vec<record::Argument>), String> {
        match self {
            JobOf::Run(run) => {
                let run = RunId::parse(run).map_err(|_| "job_of names no run".to_owned())?;
                let record = record_of(store, &run).map_err(|refused| refused.why)?;
                let action = record.asked().action.clone().unwrap_or_default();
                Ok((action, record.asked().arguments.clone()))
            }
            JobOf::Example(id) => {
                if !whizbang_serve::play::saved::is_example_id(id) {
                    return Err("job_of names no example of this store".to_owned());
                }
                let generation = store.snapshot().map_err(|_| busy().why)?;
                let example = crate::play_saved::example_in(store, &generation, id)
                    .ok_or("job_of names no example of this store")?;
                Ok((example.action, example.arguments))
            }
            JobOf::Step { bundle, step } => {
                let record = crate::play_bundles::step_record(store, bundle, *step)?;
                let action = record.asked().action.clone().unwrap_or_default();
                Ok((action, record.asked().arguments.clone()))
            }
        }
    }
}

/// The job a check of the use case `use_case` takes from the call `of` names (the page's
/// `job_of`): the job a check of that use case was given, or the job the loop writes from
/// the phrase of a removal. Never a job from the page.
pub fn job_of(store: &Store, of: &JobOf, use_case: &str) -> Result<Job, String> {
    job_and_measuring_of(store, of, use_case).map(|(job, _)| job)
}

/// The same, and what a check of a use case measured first and offered its judge (its
/// argument `set`), which a check made again keeps; none for a removal's job.
pub fn job_and_measuring_of(
    store: &Store,
    of: &JobOf,
    use_case: &str,
) -> Result<(Job, Option<String>), String> {
    let (action, arguments) = of.call(store)?;
    let job = if let Some(phrase) = PHRASED
        .contains(&action.as_str())
        .then(|| text_of(&arguments, "prompt"))
        .flatten()
    {
        Job::from_json(&whizbang_rascal::checked::job(phrase))?
    } else if play_form::is_use_case(&action) {
        let text = text_of(&arguments, "job").ok_or("that run holds no job")?;
        Job::from_json(text)?
    } else {
        return Err("that run gives no job: it is no check and had no phrase".to_owned());
    };
    if job.use_case != use_case {
        return Err(format!(
            "that run's job is of another use case than {use_case}"
        ));
    }
    let measuring = play_form::is_use_case(&action)
        .then(|| text_of(&arguments, whizbang_core::recipe::SET).map(str::to_owned))
        .flatten();
    Ok((job, measuring))
}

/// A use case run's pictures by their role, from its job and its images in order: the
/// candidate, the source, and the references in order.
fn pictures_by_role(
    arguments: &[record::Argument],
    inputs: &[Given],
) -> Option<(Job, Map<String, Json>)> {
    let job = Job::from_json(text_of(arguments, "job")?).ok()?;
    let images: Vec<String> = inputs
        .iter()
        .filter(|input| input.port == "images")
        .map(|input| input.id.clone())
        .collect();
    if images.len() != job.images.len() {
        return None;
    }
    let mut values = Map::new();
    let mut references = Vec::new();
    for (image, id) in job.images.iter().zip(images) {
        match image.role {
            Role::Candidate => {
                values.insert("candidate".to_owned(), json!(id));
            }
            Role::Source => {
                values.insert("source".to_owned(), json!(id));
            }
            Role::Reference => references.push(json!(id)),
        }
    }
    if !references.is_empty() {
        values.insert("references".to_owned(), Json::Array(references));
    }
    Some((job, values))
}

/// An input a draft is made from: its port, its artifact's type and its identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Given {
    pub port: String,
    pub artifact_type: ArtifactType,
    pub id: String,
}

impl Given {
    /// A run's inputs, in order.
    pub fn of(record: &RunRecord) -> Vec<Given> {
        record
            .inputs()
            .iter()
            .map(|input| Given {
                port: input.port.clone(),
                artifact_type: input.artifact_type,
                id: input.artifact.to_string(),
            })
            .collect()
    }
}

fn text_of<'a>(arguments: &'a [record::Argument], name: &str) -> Option<&'a str> {
    match &arguments
        .iter()
        .find(|argument| argument.name == name)?
        .value
    {
        ArgumentValue::Text(text) => Some(text),
        _ => None,
    }
}

/// What the page is given of the descriptors and kits, to say where a result goes.
pub struct Tasks<'a> {
    pub descriptors: &'a [ActionDescriptor],
    pub kits: &'a [Kit],
}

impl Tasks<'_> {
    /// The task of this name, when this build has one.
    pub fn descriptor(&self, name: &str) -> Option<&ActionDescriptor> {
        self.descriptors.iter().find(|d| d.name() == name)
    }

    fn draft(&self, descriptor: &ActionDescriptor) -> Map<String, Json> {
        let mut draft = Map::new();
        draft.insert("task".to_owned(), json!(descriptor.name()));
        draft.insert(
            "kit".to_owned(),
            json!(play_form::listed_in(descriptor, self.kits)),
        );
        draft.insert(
            "title".to_owned(),
            json!(descriptor.title().unwrap_or(descriptor.name())),
        );
        draft
    }

    /// The tasks a result's outputs can be sent to, each as the draft it opens: by
    /// type, for any task; and "Check an edit" by its roles for a rascal result that
    /// made a picture from a picture.
    pub fn send_to(&self, record: &RunRecord) -> Vec<Json> {
        let action = record.asked().action.clone().unwrap_or_default();
        let outputs = record.outputs();
        let mut targets = Vec::new();
        if outputs.is_empty() {
            return targets;
        }
        let made = outputs
            .iter()
            .find(|output| output.artifact_type == ArtifactType::Image);
        let given = record
            .inputs()
            .iter()
            .find(|input| input.artifact_type == ArtifactType::Image && input.port == "image");
        let rascal = self
            .descriptor(&action)
            .and_then(|d| play_form::listed_in(d, self.kits))
            == Some("rascal");
        let by_roles = rascal && made.is_some() && given.is_some();
        for descriptor in self.descriptors {
            if descriptor.kind() == Kind::Primitive || descriptor.name() == action {
                continue;
            }
            let mut draft = self.draft(descriptor);
            if by_roles && descriptor.name() == CHECK_AN_EDIT {
                let (Some(made), Some(given)) = (made, given) else {
                    continue;
                };
                let mut values = Map::new();
                values.insert("source".to_owned(), json!(given.artifact.to_string()));
                values.insert("candidate".to_owned(), json!(made.artifact.to_string()));
                let mut from = vec![json!("source"), json!("candidate")];
                let phrase = PHRASED
                    .contains(&action.as_str())
                    .then(|| text_argument(record, "prompt"))
                    .flatten();
                if let Some(phrase) = phrase
                    && let Ok(job) = Job::from_json(&whizbang_rascal::checked::job(phrase))
                {
                    draft.insert(JOB_OF.to_owned(), json!(record.identity().run));
                    values.insert(
                        "brief".to_owned(),
                        json!(job.brief.as_deref().unwrap_or_default()),
                    );
                    from.push(json!("brief"));
                }
                draft.insert("values".to_owned(), Json::Object(values));
                draft.insert("from".to_owned(), Json::Array(from));
                draft.insert(
                    "line".to_owned(),
                    json!("the source, the edit and the words"),
                );
                targets.push(Json::Object(draft));
                continue;
            }
            let fields = play_form::fields_of(descriptor);
            let mut values = Map::new();
            let mut from = Vec::new();
            let mut takes = false;
            for output in outputs {
                let type_name = output.artifact_type.name();
                let taking: Vec<&Json> = fields
                    .iter()
                    .filter(|field| field["artifact"] == type_name)
                    .collect();
                if taking.is_empty() {
                    continue;
                }
                takes = true;
                // One field takes it: it is filled. Two or more: none is, and the person
                // chooses (no guess).
                if let [field] = taking.as_slice()
                    && let Some(name) = field["name"].as_str()
                    && !values.contains_key(name)
                {
                    let id = json!(output.artifact.to_string());
                    let value = match field["kind"].as_str() {
                        Some("pictures") => json!([id]),
                        _ => id,
                    };
                    values.insert(name.to_owned(), value);
                    from.push(json!(name));
                }
            }
            if !takes {
                continue;
            }
            let line = match from.len() {
                0 => "choose where the result goes".to_owned(),
                _ => {
                    let names: Vec<&str> = from.iter().filter_map(Json::as_str).collect();
                    format!("the result as its {}", names.join(" and "))
                }
            };
            draft.insert("values".to_owned(), Json::Object(values));
            draft.insert("from".to_owned(), Json::Array(from));
            draft.insert("line".to_owned(), json!(line));
            targets.push(Json::Object(draft));
        }
        targets
    }

    /// The draft "Run again" opens: the same task with the run's saved inputs and
    /// arguments, and each backend it resolved written out.
    pub fn again(&self, record: &RunRecord, used: &BTreeMap<String, String>) -> Option<Json> {
        let action = record.asked().action.clone()?;
        self.draft_of(
            &action,
            &record.asked().arguments,
            &Given::of(record),
            used,
            Some(&JobOf::Run(record.identity().run.clone())),
        )
    }

    /// A draft of the task `action` from these arguments and inputs, as a run's record
    /// holds them, with each backend in `used` written out where the arguments chose none.
    /// A draft left as it is makes the recorded call (plan m7, N1's pass, finding 4): for
    /// a check of a use case, the call whose job it takes (`job_of`: the run, the example
    /// or the bundle's step it was made from), whose words alone the form shows; for any
    /// other task, what the record holds that no field shows (a recipe's `set`, `from` or
    /// `until`, and the inputs of a run from a step), kept apart as `kept`. None when this
    /// build has no such task.
    pub fn draft_of(
        &self,
        action: &str,
        arguments: &[record::Argument],
        inputs: &[Given],
        used: &BTreeMap<String, String>,
        job_of: Option<&JobOf>,
    ) -> Option<Json> {
        let descriptor = self.descriptor(action)?;
        let mut draft = self.draft(descriptor);
        let mut values = Map::new();
        let named = |name: &str| {
            arguments
                .iter()
                .find(|argument| argument.name == name)
                .map(|argument| &argument.value)
        };
        if play_form::is_use_case(action) {
            let (job, pictures) = pictures_by_role(arguments, inputs)?;
            values.extend(pictures);
            if let Some(brief) = &job.brief {
                values.insert("brief".to_owned(), json!(brief));
            }
            if let Some(of) = job_of {
                draft.insert(JOB_OF.to_owned(), of.json());
            }
            let judge = text_of(arguments, "judge")
                .map(str::to_owned)
                .or_else(|| used.get("judge").cloned());
            if let Some(judge) = judge {
                values.insert("judge".to_owned(), json!(judge));
            }
        } else {
            let fields = play_form::fields_of(descriptor);
            let shown = |name: &str| fields.iter().any(|field| field["name"] == name);
            let mut kept = Map::new();
            for argument in arguments {
                if !shown(&argument.name) && descriptor.argument(&argument.name).is_some() {
                    kept.insert(argument.name.clone(), argument_json(&argument.value));
                }
            }
            for port in descriptor.inputs() {
                let ids: Vec<Json> = (inputs.iter())
                    .filter(|input| input.port == port.name())
                    .map(|input| json!(input.id))
                    .collect();
                if shown(port.name()) || ids.is_empty() {
                    continue;
                }
                let value = match port.count().max() {
                    Some(1) => ids.into_iter().next().unwrap_or_default(),
                    _ => Json::Array(ids),
                };
                kept.insert(port.name().to_owned(), value);
            }
            if !kept.is_empty() {
                draft.insert("kept".to_owned(), Json::Object(kept));
            }
            for field in fields.iter() {
                let Some(name) = field["name"].as_str() else {
                    continue;
                };
                let ids: Vec<Json> = inputs
                    .iter()
                    .filter(|input| input.port == name)
                    .map(|input| json!(input.id))
                    .collect();
                let value = match field["kind"].as_str() {
                    Some("pictures") if !ids.is_empty() => Some(Json::Array(ids)),
                    Some("picture" | "artifact") => ids.into_iter().next(),
                    Some("backend") => named(name).map(argument_json).or_else(|| {
                        used.get(field["capability"].as_str().unwrap_or_default())
                            .map(|backend| json!(backend))
                    }),
                    _ => named(name).map(argument_json),
                };
                if let Some(value) = value {
                    values.insert(name.to_owned(), value);
                }
            }
        }
        draft.insert("values".to_owned(), Json::Object(values));
        draft.insert("from".to_owned(), json!([]));
        Some(Json::Object(draft))
    }
}

/// A loop's attempts as the page shows them: each with its outcome, what its retry
/// changed, what it settled, and the picture its fill made (a diagnostic of the loop's,
/// marked so), with which was kept and why the attempts ended.
fn attempts_json(store: &Store, record: &RunRecord) -> Json {
    let Some(attempted) = record.attempts() else {
        return Json::Null;
    };
    let generation = store.snapshot().ok();
    let list: Vec<Json> = attempted
        .list
        .iter()
        .map(|attempt| {
            // The picture of the attempt: the last run of the removal in it that made one.
            let picture = generation.as_ref().and_then(|generation| {
                attempt.runs.iter().rev().find_map(|run| {
                    let child = record_in(store, generation, run)?;
                    if child.asked().action.as_deref() != Some(whizbang_rascal::checked::REMOVE) {
                        return None;
                    }
                    child
                        .outputs()
                        .iter()
                        .find(|output| output.artifact_type == ArtifactType::Image)
                        .map(|output| output.artifact.to_string())
                })
            });
            let changed = match &attempt.changed {
                None => Json::Null,
                Some(Changed::Wider { px }) => json!(format!("wider mask, {px} px")),
                Some(Changed::Located { crops }) => {
                    json!(format!("looked again in {} place(s)", crops.len()))
                }
            };
            json!({
                "attempt": attempt.attempt,
                "verdict": attempt.verdict.map(decision_word),
                "unchecked": attempt.unchecked.as_deref().map(escaped),
                "changed": changed,
                "cost": dollars(attempt.settled_micros),
                "picture": picture,
                "diagnostic": true,
            })
        })
        .collect();
    json!({"list": list, "kept": attempted.kept, "ended": escaped(&attempted.ended)})
}

/// What `GET /api/runs/{id}/record` answers: the run's record as the result screen reads
/// it (the module's words), every text escaped.
pub fn record_json(
    store: &Store,
    root: &std::path::Path,
    run: &RunId,
    tasks: &Tasks<'_>,
    resolve: &Resolve<'_>,
) -> Result<Json, Refused> {
    let top = record_of(store, run)?;
    let records = tree(store, top);
    let record = &records[0];
    let used = backends_used(&records, resolve);
    let outcome = record.outcome();
    let arguments: Vec<Json> = record
        .asked()
        .arguments
        .iter()
        .map(|argument| {
            let value = match argument_json(&argument.value) {
                Json::String(text) => json!(escaped(&text)),
                other => other,
            };
            json!({"name": argument.name, "value": value})
        })
        .collect();
    let inputs: Vec<Json> = record
        .inputs()
        .iter()
        .map(|input| {
            json!({"port": input.port, "type": input.artifact_type.name(),
                   "id": input.artifact.to_string()})
        })
        .collect();
    let outputs: Vec<Json> = record
        .outputs()
        .iter()
        .map(|output| {
            json!({"port": output.port, "type": output.artifact_type.name(),
                   "id": output.artifact.to_string()})
        })
        .collect();
    let verdict = crate::door::verdict(root, run).map(|stated| {
        let places: Vec<Json> = stated
            .findings
            .as_ref()
            .map(|found| {
                found
                    .places
                    .iter()
                    .map(|place| {
                        json!({"image": place.image.to_string(), "box": place.boxed,
                               "polygon": place.polygon})
                    })
                    .collect()
            })
            .unwrap_or_default();
        json!({
            "word": escaped(&stated.word),
            "why": escaped(&stated.why),
            "reasons": stated.items.iter().map(|item| escaped(item)).collect::<Vec<_>>(),
            "places": places,
        })
    });
    let error = outcome
        .error
        .as_ref()
        .map(|failed| json!({"kind": escaped(&failed.kind), "message": escaped(&failed.message)}));
    let action = record.asked().action.clone();
    let title = action
        .as_deref()
        .and_then(|action| tasks.descriptor(action))
        .map(|d| d.title().unwrap_or(d.name()).to_owned());
    Ok(json!({
        "run": record.identity().run,
        "parent": record.identity().parent,
        "action": action,
        "title": title,
        "outcome": outcome.status.name(),
        "error": error,
        "invalid": outcome.invalid.as_deref().map(escaped),
        "start_ms": record.time().start_ms,
        "end_ms": record.time().end_ms,
        "arguments": arguments,
        "inputs": inputs,
        "outputs": outputs,
        "verdict": verdict,
        "attempts": attempts_json(store, record),
        "cost": cost(&records, resolve),
        "reserved": record.spend().reserved_micros.map(dollars),
        "send_to": tasks.send_to(record),
        "again": tasks.again(record, &used),
        // A replay, or a chain's step: the run it makes again (plan m7, task U2).
        "replayed": crate::play_bundles::replayed_json(record),
    }))
}

/// What a run's command line is made of (plan m5, task W2, item 5): the call as its
/// record holds it, each choice it resolved from a default, and the files it ran under,
/// by their paths. The binary turns it into its command (`command_of.rs`).
#[derive(Debug, Clone, PartialEq)]
pub struct Commanded {
    /// The action, and its kind in this build.
    pub action: String,
    pub kind: Option<Kind>,
    /// Its arguments as the record holds them, those equal to the descriptor's default
    /// left out, in the descriptor's order.
    pub arguments: Vec<(String, Value)>,
    /// Its inputs, by port and identity, in order.
    pub inputs: Vec<(String, String)>,
    /// The descriptor's defaults that the record's arguments held, left out of
    /// `arguments`: what a command fills in by itself.
    pub defaults: Vec<(String, Value)>,
    /// Each argument that chooses a backend and was not given, with the backend the run
    /// resolved it to: written out, so a later change of default changes nothing.
    pub chosen: Vec<(String, String)>,
    /// The store, and the config, env file and models directory, as their paths.
    pub store: PathBuf,
    pub config: Option<PathBuf>,
    pub env_file: Option<PathBuf>,
    pub models_dir: Option<PathBuf>,
    /// The file of a user's recipe the config lists, for a recipe this build does not
    /// ship.
    pub recipe_file: Option<PathBuf>,
}

/// Values by name, in order.
type Named = Vec<(String, Value)>;

/// Whether two values are the same, of the same kind.
pub fn same_value(given: &Value, default: &Value) -> bool {
    match (given, default) {
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::Integer(a), Value::Integer(b)) => a == b,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::Text(a), Value::Text(b)) => a == b,
        _ => false,
    }
}

/// The parts of the command of the run whose records these are (the run first), as its
/// action's descriptor says them; `default_of` names the config's default backend of a
/// capability, for a choice no request of the run shows.
pub fn commanded(
    records: &[RunRecord],
    descriptor: Option<&ActionDescriptor>,
    resolve: &Resolve<'_>,
    default_of: &dyn Fn(&str) -> Option<String>,
) -> Result<Commanded, Refused> {
    let record = &records[0];
    let Some(action) = record.asked().action.clone() else {
        return Err(Refused::new(
            422,
            "the run's record does not say what was asked",
        ));
    };
    let default = |name: &str| {
        descriptor
            .and_then(|d| d.argument(name))
            .and_then(|argument| argument.default().cloned())
    };
    let (defaults, arguments): (Named, Named) = record
        .asked()
        .arguments
        .iter()
        .map(|argument| (argument.name.clone(), argument_value(&argument.value)))
        .partition(|(name, value)| {
            default(name).is_some_and(|default| same_value(value, &default))
        });
    let inputs = record
        .inputs()
        .iter()
        .map(|input| (input.port.clone(), input.artifact.to_string()))
        .collect();
    let used = backends_used(records, resolve);
    let mut chosen = Vec::new();
    let choosers: Vec<(String, String)> = match descriptor {
        Some(descriptor) if play_form::is_use_case(&action) => {
            let _ = descriptor;
            vec![("judge".to_owned(), "judge".to_owned())]
        }
        Some(descriptor) => descriptor
            .needs()
            .iter()
            .filter_map(|need| {
                Some((
                    need.chooser()?.to_owned(),
                    need.capability().name().to_owned(),
                ))
            })
            .collect(),
        None => Vec::new(),
    };
    for (argument, capability) in choosers {
        if arguments.iter().any(|(name, _)| *name == argument) {
            continue;
        }
        if let Some(backend) = used
            .get(&capability)
            .cloned()
            .or_else(|| default_of(&capability))
        {
            chosen.push((argument, backend));
        }
    }
    Ok(Commanded {
        action,
        kind: descriptor.map(ActionDescriptor::kind),
        arguments,
        defaults,
        inputs,
        chosen,
        store: PathBuf::new(),
        config: None,
        env_file: None,
        models_dir: None,
        recipe_file: None,
    })
}

/// A word quoted for a POSIX shell: as it is when it holds nothing a shell reads, else
/// in single quotes, a single quote in it written `'\''`. A new line stays a new line
/// inside the quotes, which a shell keeps. A word that begins with `=` is quoted too,
/// which zsh would otherwise expand as a command's path.
pub fn quoted(word: &str) -> String {
    let plain = !word.is_empty()
        && !word.starts_with('=')
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:=,@%+".contains(&b));
    if plain {
        return word.to_owned();
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// Whether a character is of Unicode's category Cf (format): invisible, and able to
/// make a pasted line read otherwise than it shows (bidirectional controls among them).
pub fn is_format(c: char) -> bool {
    matches!(c as u32,
        0x00AD | 0x0600..=0x0605 | 0x061C | 0x06DD | 0x070F | 0x0890..=0x0891 | 0x08E2
        | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F
        | 0xFEFF | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD | 0x13430..=0x1343F
        | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0001 | 0xE0020..=0xE007F)
}

/// Words a person can paste into a terminal safely: none holds a control character but a
/// new line or a tab (each kept as it is inside its quotes), nor a format character.
/// Otherwise 422: no command line says them safely.
pub fn said_safely(words: &[String]) -> Result<(), Refused> {
    let unsafe_char = |c: char| (c.is_control() && c != '\n' && c != '\t') || is_format(c);
    if words.iter().any(|word| word.chars().any(unsafe_char)) {
        return Err(Refused::new(
            422,
            "the run's words hold a control or format character that no command line says \
             safely",
        ));
    }
    Ok(())
}

/// A line a shell runs as nothing, saying `words`: `: '…'`, which an interactive zsh
/// takes as it takes any command, where it would refuse a `#` comment.
pub fn said_line(words: &str) -> String {
    format!(": {}\n", quoted(words))
}

/// A command line of these words, each quoted for a POSIX shell.
pub fn command_line(words: &[String]) -> String {
    words
        .iter()
        .map(|word| quoted(word))
        .collect::<Vec<_>>()
        .join(" ")
}
