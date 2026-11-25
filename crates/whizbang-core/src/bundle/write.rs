//! Writing a bundle from a store (ADR 0048, "Bundles"; plan m7, task B1, item 2).

use std::collections::{BTreeMap, BTreeSet};

use crate::artifact::{self, Artifact, ArtifactId, ArtifactType, Contents};
use crate::capability::written;
use crate::contract::record::{
    self, ArgumentValue, DIAGNOSTICS_PREFIX, INTERRUPTED, JOURNAL_NAME, RECORD_NAME, RunRecord,
    Status, StoredRecordError,
};
use crate::contract::request::with_descendants;
use crate::contract::{EXCHANGE_PREFIX, RunId, is_diagnostic_name};
use crate::identity::{DecodeLimits, Digest, PixelDigest};
use crate::store::{Entry, Generation, HOLDS_PREFIX, Kind};

use super::{
    ANSWERED_BY, CACHE_HIT, CACHE_SETTING, FILL_ACTION, FILL_ANSWER, FORMAT, File, Link, MANIFEST,
    MANIFEST_MAX_BYTES, MAX_BYTES, MEMBERS_LIMIT, Manifest, Role, STEPS_LIMIT, Step, StepInput,
    Use, Withheld, file_member, run_member, tar,
};

/// A bundle that was written: its bytes, their SHA-256, which names it, and its manifest.
#[derive(Debug, Clone)]
pub struct Made {
    pub bytes: Vec<u8>,
    pub digest: Digest,
    pub manifest: Manifest,
}

/// The one input or output of one run: its port, its place among that port's, its
/// artifact's type and its identity.
pub(super) struct Port {
    pub(super) port: String,
    pub(super) place: u32,
    pub(super) artifact_type: ArtifactType,
    pub(super) artifact: Digest,
}

/// Each input or output of a record, with its place among those of its port, in order.
fn placed<'a>(ports: impl Iterator<Item = (&'a str, ArtifactType, Digest)>) -> Vec<Port> {
    let mut places: BTreeMap<String, u32> = BTreeMap::new();
    ports
        .map(|(port, artifact_type, artifact)| {
            let place = places.entry(port.to_owned()).or_insert(0);
            let found = Port {
                port: port.to_owned(),
                place: *place,
                artifact_type,
                artifact,
            };
            *place += 1;
            found
        })
        .collect()
}

/// A record's inputs, placed.
pub(super) fn inputs(record: &RunRecord) -> Vec<Port> {
    let ports = record.inputs().iter();
    placed(ports.map(|input| (input.port.as_str(), input.artifact_type, input.artifact)))
}

/// A record's outputs, placed.
pub(super) fn outputs(record: &RunRecord) -> Vec<Port> {
    let ports = record.outputs().iter();
    placed(ports.map(|output| (output.port.as_str(), output.artifact_type, output.artifact)))
}

/// Whether a record is of a run a dead process left.
pub(super) fn interrupted(record: &RunRecord) -> bool {
    let outcome = record.outcome();
    outcome.status == Status::Error
        && (outcome.error.as_ref()).is_some_and(|error| error.kind == INTERRUPTED)
}

/// A text setting of a record, by its name.
fn setting<'a>(record: &'a RunRecord, name: &str) -> Option<&'a str> {
    let settings = record.asked().settings.iter();
    settings
        .filter(|setting| setting.name() == name)
        .find_map(|setting| match setting.value() {
            ArgumentValue::Text(text) => Some(text.as_str()),
            _ => None,
        })
}

/// What is being gathered: the members by name, the file members' manifest entries by
/// file digest, what is withheld, and the pixels of each image carried.
#[derive(Default)]
struct Gathered {
    members: BTreeMap<String, Vec<u8>>,
    files: BTreeMap<Digest, File>,
    withheld: Vec<Withheld>,
    pixels: BTreeSet<PixelDigest>,
    /// Each picture a stored exchange names: its run, the exchange and the pixels.
    pictures: Vec<(String, String, PixelDigest)>,
}

impl Gathered {
    /// Adds the file of one input or output of a run, checked to be the artifact the
    /// record names, once for each file digest.
    fn file(&mut self, bytes: Vec<u8>, port: &Port, run: &str, role: Role) -> Result<(), String> {
        let digest = Digest::of_bytes(&bytes);
        let used = Use {
            run: run.to_owned(),
            role,
            port: port.port.clone(),
            place: port.place,
        };
        if let Some(file) = self.files.get_mut(&digest) {
            if file.artifact != port.artifact {
                return Err(format!(
                    "the store holds one file for two artifacts, {} and {}",
                    file.artifact, port.artifact
                ));
            }
            file.of.push(used);
            return Ok(());
        }
        let limits = DecodeLimits::default();
        let read = Artifact::from_bytes_as(port.artifact_type, &bytes, &limits)
            .map_err(|why| format!("the file of {} cannot be read: {why}", port.artifact))?;
        if read.id().0 != port.artifact {
            return Err(format!(
                "the store's file for {} is the file of {}",
                port.artifact,
                read.id()
            ));
        }
        if let Artifact::Image(image) = &read {
            self.pixels.insert(image.identity().pixels);
        }
        self.members.insert(file_member(&digest), bytes);
        self.files.insert(
            digest,
            File {
                file: digest,
                artifact: port.artifact,
                artifact_type: port.artifact_type,
                of: vec![used],
            },
        );
        Ok(())
    }
}

/// The record of a run, and its bytes as the store holds them, or why not.
fn record_of(
    store: &(impl Contents + ?Sized),
    generation: &Generation,
    run: &str,
) -> Result<(RunRecord, Vec<u8>), String> {
    let shown: String = run.chars().take(64).collect();
    let Some(held) = generation.run(run) else {
        return Err(format!("the store holds no run {shown:?}"));
    };
    let Some(entry) = held.get(RECORD_NAME) else {
        return Err(format!(
            "the run {shown:?} has not ended: it has no record, and a bundle carries runs \
             that ended"
        ));
    };
    let bytes = store
        .read_record(entry)
        .map_err(|why| format!("the record of {shown:?} cannot be read: {why}"))?;
    let record = RunRecord::from_bytes(&bytes)
        .map_err(|why| format!("the record of {shown:?} is not one this build reads: {why}"))?;
    if record.identity().run != run {
        return Err(format!("the record of {shown:?} names another run"));
    }
    Ok((record, bytes))
}

/// The root of a run that another run called: its parents followed up, at most as many
/// as the store has runs.
fn root_of(
    store: &(impl Contents + ?Sized),
    generation: &Generation,
    record: &RunRecord,
) -> String {
    let mut at = record.clone();
    for _ in 0..generation.runs().len() {
        let Some(parent) = at.identity().parent.clone() else {
            break;
        };
        match record::stored(generation, &parent, |entry| store.read_record(entry)) {
            Some(Ok(record)) => at = record,
            Some(Err(StoredRecordError::NoRecord)) | Some(Err(_)) | None => return parent,
        }
    }
    at.identity().run.clone()
}

/// The entry of an input of a run, as the run read it: its hold, else the name the first
/// run of its tree keeps for the runs under it, else the name outside runs.
fn input_entry<'a>(
    generation: &'a Generation,
    run: &str,
    root: &str,
    name: &str,
) -> Option<&'a Entry> {
    let output = |entry: &&Entry| entry.kind == Kind::Output;
    let held = generation.run(run)?;
    (held.get(&format!("{HOLDS_PREFIX}{name}")).filter(output))
        .or_else(|| (generation.run(root)?.get(name)).filter(output))
        .or_else(|| generation.get(name).filter(output))
}

/// Writes a bundle of these root runs, a chain in the order given, from the store,
/// through one snapshot of it. `shipped` names each action this build ships, with its
/// version: a step of any other is carried as evidence, not replayable. `whizbang` is
/// the version that writes it.
pub fn make(
    store: &(impl Contents + ?Sized),
    steps: &[&str],
    shipped: &BTreeMap<String, String>,
    whizbang: &str,
) -> Result<Made, String> {
    if steps.is_empty() {
        return Err("a bundle holds one run at least".to_owned());
    }
    if steps.len() > STEPS_LIMIT {
        return Err(format!(
            "a bundle holds {STEPS_LIMIT} runs at most, and {} were given",
            steps.len()
        ));
    }
    let generation = store
        .snapshot()
        .map_err(|why| format!("the store cannot be read: {why}"))?;
    let mut gathered = Gathered::default();
    let mut manifest_steps = Vec::new();
    // Every output of every earlier step: the step, the port, the place and the identity.
    let mut earlier: Vec<(u32, String, u32, Digest)> = Vec::new();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for (index, given) in steps.iter().enumerate() {
        let (root, _) = record_of(store, &generation, given)?;
        if root.identity().parent.is_some() {
            let top = root_of(store, &generation, &root);
            return Err(format!(
                "the run {given} is a call made inside the run {}: a step is a run that \
                 nothing called, and carries the runs under it; give {} instead",
                top.chars().take(64).collect::<String>(),
                top.chars().take(64).collect::<String>()
            ));
        }
        if !taken.insert((*given).to_owned()) {
            return Err(format!("the run {given} is given twice"));
        }
        let family = with_descendants(&generation, &[given], |entry| store.read_record(entry));
        for run in &family {
            RunId::parse(run).map_err(|why| format!("a run cannot travel in a bundle: {why}"))?;
            if run != given && !taken.insert(run.clone()) {
                return Err(format!("the run {run} is under two of the runs given"));
            }
            gather(store, &generation, run, given, &mut gathered)?;
        }
        // The step's inputs: each linked to the earlier output it took, by identity.
        let mut step_inputs = Vec::new();
        for input in inputs(&root) {
            let could: Vec<&(u32, String, u32, Digest)> = (earlier.iter())
                .filter(|(_, _, _, identity)| *identity == input.artifact)
                .collect();
            let (artifact, from) = match could.as_slice() {
                [] => (Some(input.artifact), None),
                [(step, port, place, _)] => (
                    None,
                    Some(Link {
                        step: *step,
                        port: port.clone(),
                        place: *place,
                    }),
                ),
                [(a, at, _, _), (b, bt, _, _), ..] => {
                    return Err(format!(
                        "the input {} of step {index} ({given}) could be the output {at} of \
                         step {a} or the output {bt} of step {b}: the chain cannot say which \
                         it took; bundle the runs apart",
                        input.port
                    ));
                }
            };
            step_inputs.push(StepInput {
                port: input.port,
                place: input.place,
                artifact,
                from,
            });
        }
        for output in outputs(&root) {
            earlier.push((index as u32, output.port, output.place, output.artifact));
        }
        let asked = root.asked();
        let (replayable, why) = match (&asked.action, &asked.version) {
            _ if interrupted(&root) => (
                false,
                Some("it was interrupted: the process that ran it ended first".to_owned()),
            ),
            (Some(action), Some(version)) => match shipped.get(action) {
                Some(shipped) if shipped == version => (true, None),
                Some(shipped) => (
                    false,
                    Some(format!(
                        "this build ships {action} at version {shipped}, not {version}"
                    )),
                ),
                None => (
                    false,
                    Some(format!(
                        "{action} is not an action this build ships: a recipe file's or a \
                         tool file's action is carried as evidence, and a bundle's never runs"
                    )),
                ),
            },
            _ => (false, Some("its record lost its admission".to_owned())),
        };
        manifest_steps.push(Step {
            run: (*given).to_owned(),
            action: asked.action.clone(),
            version: asked.version.clone(),
            config: asked.config,
            replayable,
            why,
            runs: family,
            inputs: step_inputs,
        });
    }
    // A picture an exchange names that no image the bundle carries has.
    let mut seen = BTreeSet::new();
    for (run, diagnostic, pixels) in std::mem::take(&mut gathered.pictures) {
        if gathered.pixels.contains(&pixels)
            || !seen.insert((run.clone(), diagnostic.clone(), pixels))
        {
            continue;
        }
        gathered.withheld.push(Withheld::Picture {
            run,
            diagnostic,
            pixels,
            why: "the judge was shown a picture whose pixels no image the bundle carries has"
                .to_owned(),
        });
    }
    let manifest = Manifest {
        bundle: FORMAT,
        whizbang: whizbang.to_owned(),
        steps: manifest_steps,
        artifacts: gathered.files.into_values().collect(),
        withheld: gathered.withheld,
    };
    let said = manifest.to_bytes();
    if said.len() as u64 > MANIFEST_MAX_BYTES {
        return Err(format!(
            "the manifest would be {} bytes, over its bound of {MANIFEST_MAX_BYTES}",
            said.len()
        ));
    }
    if gathered.members.len() + 1 > MEMBERS_LIMIT {
        return Err(format!(
            "the bundle would hold {} members, over its bound of {MEMBERS_LIMIT}",
            gathered.members.len() + 1
        ));
    }
    let mut writer = tar::Writer::new(0);
    writer.add(MANIFEST, &said).map_err(|why| why.to_string())?;
    for (name, bytes) in &gathered.members {
        writer.add(name, bytes).map_err(|why| why.to_string())?;
        if writer.len() > MAX_BYTES {
            return Err(format!(
                "the bundle would be over its bound of {MAX_BYTES} bytes"
            ));
        }
    }
    let bytes = writer.finish();
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!(
            "the bundle would be over its bound of {MAX_BYTES} bytes"
        ));
    }
    Ok(Made {
        digest: Digest::of_bytes(&bytes),
        bytes,
        manifest,
    })
}

/// Gathers one run of a step's family: its record, its diagnostics, the files of its
/// inputs and outputs, and what of them is withheld.
fn gather(
    store: &(impl Contents + ?Sized),
    generation: &Generation,
    run: &str,
    root: &str,
    gathered: &mut Gathered,
) -> Result<(), String> {
    let (record, bytes) = record_of(store, generation, run)?;
    gathered.members.insert(run_member(run, RECORD_NAME), bytes);
    let held = generation.run(run);
    for diagnostic in record.diagnostics() {
        let name = diagnostic.name();
        let plain = name == JOURNAL_NAME
            || name
                .strip_prefix(DIAGNOSTICS_PREFIX)
                .is_some_and(is_diagnostic_name);
        if !plain {
            return Err(format!("the run {run} names a diagnostic {name:?}"));
        }
        let Some(entry) = held.and_then(|held| held.get(name)) else {
            gathered.withheld.push(Withheld::Diagnostic {
                run: run.to_owned(),
                name: name.to_owned(),
                why: "its record lists it, and the store no longer holds it".to_owned(),
            });
            continue;
        };
        let bytes = store
            .read_record(entry)
            .map_err(|why| format!("the diagnostic {name} of {run} cannot be read: {why}"))?;
        if Digest::of_bytes(&bytes) != diagnostic.digest()
            || bytes.len() as u64 != diagnostic.bytes()
        {
            return Err(format!(
                "the store's {name} of the run {run} is not what its record says"
            ));
        }
        let exchange = format!("{DIAGNOSTICS_PREFIX}{EXCHANGE_PREFIX}");
        if name.starts_with(&exchange)
            && let Ok((parts, pictures)) = written::named_outside(&bytes)
        {
            for part in parts {
                gathered.withheld.push(Withheld::OpaquePart {
                    run: run.to_owned(),
                    diagnostic: name.to_owned(),
                    adapter: part.adapter,
                    digest: part.digest,
                    bytes: part.bytes,
                    why: "a provider's opaque part is kept as its digest and length, never its \
                          body, which cannot have its secrets taken out"
                        .to_owned(),
                });
            }
            for pixels in pictures {
                gathered
                    .pictures
                    .push((run.to_owned(), name.to_owned(), pixels));
            }
        }
        gathered.members.insert(run_member(run, name), bytes);
    }
    let asked = record.asked();
    let answer = format!("{DIAGNOSTICS_PREFIX}{FILL_ANSWER}");
    if asked.action.as_deref() == Some(FILL_ACTION)
        && record.outcome().status == Status::Completed
        && !record.diagnostics().iter().any(|d| d.name() == answer)
    {
        gathered.withheld.push(Withheld::FillAnswer {
            run: run.to_owned(),
            why: "the run was made before a fill kept its painted answer, so it cannot be \
                  answered offline"
                .to_owned(),
        });
    }
    if setting(&record, CACHE_SETTING) == Some(CACHE_HIT) {
        gathered.withheld.push(Withheld::CachedCheck {
            run: run.to_owned(),
            answered_by: setting(&record, ANSWERED_BY).map(str::to_owned),
            why: "it was answered from the decision cache, and the turns that answered it \
                  are another run's"
                .to_owned(),
        });
    }
    for input in inputs(&record) {
        let name = artifact::name(input.artifact_type, ArtifactId(input.artifact));
        match input_entry(generation, run, root, &name) {
            Some(entry) => {
                let bytes = store.read(entry).map_err(|why| {
                    format!("the input {} of {run} cannot be read: {why}", input.port)
                })?;
                gathered.file(bytes, &input, run, Role::Input)?;
            }
            None => gathered
                .withheld
                .push(withheld_artifact(run, Role::Input, &input)),
        }
    }
    for output in outputs(&record) {
        let name = format!("{}{}/{}", record::OUTPUTS_PREFIX, output.port, output.place);
        let entry = held.and_then(|held| held.get(&name));
        match entry.filter(|entry| entry.kind == Kind::Output) {
            Some(entry) => {
                let bytes = store.read(entry).map_err(|why| {
                    format!("the output {} of {run} cannot be read: {why}", output.port)
                })?;
                gathered.file(bytes, &output, run, Role::Output)?;
            }
            None => gathered
                .withheld
                .push(withheld_artifact(run, Role::Output, &output)),
        }
    }
    Ok(())
}

fn withheld_artifact(run: &str, role: Role, port: &Port) -> Withheld {
    Withheld::Artifact {
        run: run.to_owned(),
        role,
        port: port.port.clone(),
        place: port.place,
        artifact: port.artifact,
        why: "its record names it, and the store no longer holds its file".to_owned(),
    }
}
