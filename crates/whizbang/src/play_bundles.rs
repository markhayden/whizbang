//! Bundles, a chain's steps and a replay's report in the playground (plan m7, task U2;
//! ADR 0048; ADR 0049): what [`Playground`] answers the host's [`Bundles`] with.
//!
//! **A bundle** is written by the core's writer (`whizbang_core::bundle::make`) from the
//! store the playground holds, and taken in by the core's reader and import, one at a
//! time, read whole before anything is published. Whether a step replays is decided by
//! this build from the step's records (`replay::decide`), never from what the manifest
//! says of it.
//!
//! **A chain's step** opens as an ordinary draft: its inputs the bundle carries are given
//! the store's ordinary names as an import, so the draft prepares and runs as any; each
//! input linked to an earlier step's output is bound to the output of the new run of that
//! step at the link's port and place, and the identity it has now is shown beside the one
//! recorded. A link that finds no output, or two that could be it, stops the chain there.
//! Nothing is started here: a step runs when the person presses Run.
//!
//! Everything said that came from a store or a bundle is escaped here.

use std::collections::BTreeMap;
use std::io::Cursor;

use serde_json::{Value as Json, json};
use whizbang_core::artifact::{self, ArtifactId};
use whizbang_core::bundle::{self, Manifest, Role, Use, Withheld};
use whizbang_core::contract::RunId;
use whizbang_core::contract::record::{ReplayMode, RunRecord};
use whizbang_core::contract::replay::{self, Records, Stored};
use whizbang_core::identity::Digest;
use whizbang_core::store::{Generation, Store};
use whizbang_serve::escaped;
use whizbang_serve::play::Refused;
use whizbang_serve::play::bundles::Bundles;
use whizbang_serve::play::history::Document;

use crate::play_result::{Given, record_in};
use crate::playing::Playground;

fn busy() -> Refused {
    Refused::new(500, "the store could not answer")
}

fn no_bundle() -> Refused {
    Refused::new(404, "the store holds no bundle of that digest")
}

/// Every string in a value escaped, for text that came from a store or a bundle.
pub fn escaped_all(value: Json) -> Json {
    match value {
        Json::String(text) => json!(escaped(&text)),
        Json::Array(items) => Json::Array(items.into_iter().map(escaped_all).collect()),
        Json::Object(fields) => Json::Object(
            fields
                .into_iter()
                .map(|(name, value)| (name, escaped_all(value)))
                .collect(),
        ),
        other => other,
    }
}

/// What a manifest says of a withheld thing, in a line a person reads.
fn withheld_json(item: &Withheld) -> Json {
    let what = match item {
        Withheld::OpaquePart { diagnostic, .. } => format!("an opaque part of {diagnostic}"),
        Withheld::Picture { diagnostic, .. } => format!("a picture {diagnostic} names"),
        Withheld::Diagnostic { name, .. } => format!("the diagnostic {name}"),
        Withheld::FillAnswer { .. } => "the fill's painted answer".to_owned(),
        Withheld::CachedCheck { .. } => {
            "the turns of a check the decision cache answered".to_owned()
        }
        Withheld::Artifact {
            role, port, place, ..
        } => format!("the {} {port} {place}", role.name()),
    };
    json!({"run": escaped(item.run()), "what": escaped(&what), "why": escaped(item.why())})
}

/// The steps of a manifest as the page shows them: each step's run, action and version,
/// its family's size, and each input's link to an earlier step or its identity.
fn steps_json(manifest: &Manifest, decided: Option<&[(bool, Option<String>)]>) -> Vec<Json> {
    (manifest.steps.iter().enumerate())
        .map(|(at, step)| {
            let inputs: Vec<Json> = (step.inputs.iter())
                .map(|input| {
                    json!({
                        "port": escaped(&input.port),
                        "place": input.place,
                        "artifact": input.artifact.map(|digest| digest.to_string()),
                        "from": input.from.as_ref().map(|link| json!({
                            "step": link.step, "port": escaped(&link.port), "place": link.place,
                        })),
                    })
                })
                .collect();
            let (replayable, why) = match decided.and_then(|decided| decided.get(at)) {
                Some((replayable, why)) => (*replayable, why.clone()),
                None => (step.replayable, step.why.clone()),
            };
            json!({
                "step": at,
                "run": escaped(&step.run),
                "action": step.action.as_deref().map(escaped),
                "version": step.version.as_deref().map(escaped),
                "runs": step.runs.len(),
                "replayable": replayable,
                "why": why.as_deref().map(escaped),
                "inputs": inputs,
            })
        })
        .collect()
}

/// What a manifest holds, in counts: records, journals, files.
fn holds_json(manifest: &Manifest) -> Json {
    let runs: usize = manifest.steps.iter().map(|step| step.runs.len()).sum();
    json!({"records": runs, "files": manifest.artifacts.len()})
}

/// A bundle's digest as a path gives it.
fn digest_of(text: &str) -> Result<Digest, Refused> {
    text.parse::<Digest>().map_err(|_| no_bundle())
}

/// Whether each step of a bundle the store holds replays offline, decided from its
/// records by this build (`replay::decide`; ADR 0049), never from the manifest's claim.
fn decided(
    store: &Store,
    generation: &Generation,
    digest: &Digest,
    manifest: &Manifest,
) -> Result<Vec<(bool, Option<String>)>, Refused> {
    let stored = Stored::of_bundle(store, generation, digest).map_err(|_| no_bundle())?;
    let shipped = crate::bundling::shipped().map_err(|why| Refused::new(500, &escaped(&why)))?;
    let none = BTreeMap::new();
    Ok((manifest.steps.iter())
        .map(|step| {
            let mut runs = Vec::new();
            for run in &step.runs {
                match stored.record(run) {
                    Ok(Some(record)) => runs.push(record),
                    Ok(None) => return (false, Some(format!("the record of {run} is not in it"))),
                    Err(why) => {
                        return (
                            false,
                            Some(format!("the record of {run} does not read: {why}")),
                        );
                    }
                }
            }
            match replay::decide(&runs, &shipped, ReplayMode::Offline, &none) {
                Ok(()) => (true, None),
                Err(why) => (false, Some(why)),
            }
        })
        .collect())
}

/// The output of a new run a link binds to: the one at the link's place among the run's
/// outputs at its port. None found, or two that could be it (the step made one output at
/// that port when it was recorded, and its new run made more), stops the chain there,
/// with why.
pub fn bind(place: u32, recorded: usize, outputs: &[String]) -> Result<String, String> {
    if recorded == 1 && outputs.len() > 1 {
        return Err(format!(
            "made {} outputs there where the recorded run made one, so the chain cannot \
             tell which to take",
            outputs.len()
        ));
    }
    outputs
        .get(place as usize)
        .cloned()
        .ok_or_else(|| format!("made no output there at place {place}"))
}

impl Playground {
    fn root_path(&self) -> Result<std::path::PathBuf, Refused> {
        self.root()
            .map(std::path::Path::to_path_buf)
            .ok_or_else(|| Refused::new(503, "the playground keeps no store"))
    }

    /// The manifest of a bundle the store holds, with the generation it was read in.
    fn held_bundle(&self, text: &str) -> Result<(Digest, Generation, Manifest), Refused> {
        let digest = digest_of(text)?;
        let store = self.store()?;
        let generation = store.snapshot().map_err(|_| busy())?;
        let manifest = match bundle::manifest_of(store.as_ref(), &generation, &digest) {
            Some(Ok(manifest)) => manifest,
            Some(Err(_)) => return Err(Refused::new(500, "its manifest cannot be read")),
            None => return Err(no_bundle()),
        };
        Ok((digest, generation, manifest))
    }

    fn summary(
        &self,
        digest: &Digest,
        manifest: &Manifest,
        decided: Option<&[(bool, Option<String>)]>,
    ) -> Json {
        let titles = |action: &str| {
            (self.descriptors().iter())
                .find(|d| d.name() == action)
                .and_then(|d| d.title().map(escaped))
        };
        let mut steps = steps_json(manifest, decided);
        for step in &mut steps {
            if let Some(action) = step["action"].as_str() {
                step["title"] = json!(titles(action));
            }
        }
        json!({
            "digest": digest.to_string(),
            "whizbang": escaped(&manifest.whizbang),
            "steps": steps,
            "holds": holds_json(manifest),
            "withheld": manifest.withheld.iter().map(withheld_json).collect::<Vec<_>>(),
        })
    }
}

impl Bundles for Playground {
    fn bundle_run(&self, run: &RunId) -> Result<Document, Refused> {
        let root = self.root_path()?;
        let made = crate::bundling::make(&root, &[run.as_str()])
            .map_err(|why| Refused::new(422, &escaped(&why)))?;
        Ok(Document {
            name: format!("{}.whizbang-bundle", run.as_str()),
            content_type: "application/x-tar",
            bytes: made.bytes,
        })
    }

    fn bundle_about(&self, run: &RunId) -> Result<Json, Refused> {
        let store = self.store()?;
        let generation = store.snapshot().map_err(|_| busy())?;
        let top = record_in(&store, &generation, run.as_str())
            .ok_or_else(|| Refused::new(404, "the store holds no record of that run"))?;
        if let Some(parent) = &top.identity().parent {
            return Err(Refused::new(
                422,
                &format!(
                    "this run was called by the run {}: a bundle carries the run at the top \
                     of its tree",
                    escaped(parent)
                ),
            ));
        }
        let records = crate::play_result::tree(&store, top);
        // About the sum of what the store holds of the family: every name of each run, its
        // record, journal, diagnostics, inputs and outputs. Two runs that share a file count
        // it twice, and the bundle carries it once: the size is about, and said so.
        let mut bytes = 0u64;
        for record in &records {
            if let Some(held) = generation.run(&record.identity().run) {
                bytes += held
                    .entries()
                    .values()
                    .map(|entry| entry.bytes)
                    .sum::<u64>();
            }
        }
        let record = &records[0];
        let pictures = (record.inputs().iter())
            .filter(|input| input.artifact_type == whizbang_core::artifact::ArtifactType::Image)
            .count();
        let masks = (record.outputs().iter())
            .filter(|output| output.artifact_type == whizbang_core::artifact::ArtifactType::Mask)
            .count();
        Ok(json!({
            "run": run.as_str(),
            "action": record.asked().action.as_deref().map(escaped),
            "about_bytes": bytes,
            "runs": records.len(),
            "inputs": record.inputs().len(),
            "pictures": pictures,
            "outputs": record.outputs().len(),
            "masks": masks,
        }))
    }

    fn bundle_chain(&self, runs: &[RunId]) -> Result<(Document, Json), Refused> {
        let root = self.root_path()?;
        let ids: Vec<&str> = runs.iter().map(RunId::as_str).collect();
        let made =
            crate::bundling::make(&root, &ids).map_err(|why| Refused::new(422, &escaped(&why)))?;
        let mut said = self.summary(&made.digest, &made.manifest, None);
        said["bytes"] = json!(made.bytes.len());
        let first = ids.first().copied().unwrap_or_default();
        Ok((
            Document {
                name: format!("chain-{first}.whizbang-bundle"),
                content_type: "application/x-tar",
                bytes: made.bytes,
            },
            said,
        ))
    }

    fn check_chain(&self, runs: &[RunId]) -> Result<Json, Refused> {
        let store = self.store()?;
        let ids: Vec<&str> = runs.iter().map(RunId::as_str).collect();
        let linked =
            bundle::links(store.as_ref(), &ids).map_err(|why| Refused::new(422, &escaped(&why)))?;
        let titles = |action: &str| {
            (self.descriptors().iter())
                .find(|d| d.name() == action)
                .and_then(|d| d.title().map(escaped))
        };
        let steps: Vec<Json> = (linked.iter().enumerate())
            .map(|(at, (run, action, inputs))| {
                let inputs: Vec<Json> = (inputs.iter())
                    .map(|input| {
                        json!({
                            "port": escaped(&input.port), "place": input.place,
                            "artifact": input.artifact.map(|digest| digest.to_string()),
                            "from": input.from.as_ref().map(|link| json!({
                                "step": link.step, "port": escaped(&link.port), "place": link.place,
                            })),
                        })
                    })
                    .collect();
                json!({
                    "step": at, "run": escaped(run),
                    "action": action.as_deref().map(escaped),
                    "title": action.as_deref().and_then(titles),
                    "version": null, "runs": 1, "replayable": true, "why": null,
                    "inputs": inputs,
                })
            })
            .collect();
        Ok(
            json!({"digest": null, "steps": steps, "holds": {"records": 0, "files": 0}, "withheld": []}),
        )
    }

    fn import(&self, bytes: &[u8]) -> Result<Json, Refused> {
        let store = self.store()?;
        let imported =
            bundle::import(&store, || Ok(Cursor::new(bytes))).map_err(|why| match why {
                bundle::ImportError::Refused(refused) => {
                    Refused::new(422, &escaped(&refused.to_string()))
                }
                bundle::ImportError::Store(_) => Refused::new(
                    500,
                    "the store failed; nothing was imported and nothing ran",
                ),
            })?;
        let generation = store.snapshot().map_err(|_| busy())?;
        let decided = decided(&store, &generation, &imported.digest, &imported.manifest)?;
        let mut said = self.summary(&imported.digest, &imported.manifest, Some(&decided));
        said["imported"] = json!(imported.generation.is_some());
        said["bytes"] = json!(bytes.len());
        Ok(said)
    }

    fn list(&self) -> Result<Json, Refused> {
        let store = self.store()?;
        let generation = store.snapshot().map_err(|_| busy())?;
        let mut listed = Vec::new();
        for digest in bundle::held(&generation) {
            let Some(Ok(manifest)) = bundle::manifest_of(store.as_ref(), &generation, &digest)
            else {
                listed.push(json!({"digest": digest.to_string(), "steps": [], "unreadable": true}));
                continue;
            };
            // Whether each step replays is this build's decision, never the manifest's.
            let decided = decided(&store, &generation, &digest, &manifest)?;
            listed.push(self.summary(&digest, &manifest, Some(&decided)));
        }
        Ok(json!({"bundles": listed}))
    }

    fn show(&self, digest: &str) -> Result<Json, Refused> {
        let (digest, generation, manifest) = self.held_bundle(digest)?;
        let store = self.store()?;
        let decided = decided(&store, &generation, &digest, &manifest)?;
        Ok(self.summary(&digest, &manifest, Some(&decided)))
    }

    fn delete(&self, digest: &str) -> Result<Json, Refused> {
        let digest = digest_of(digest)?;
        let store = self.store()?;
        match bundle::delete(&store, &digest) {
            Ok(_) => {}
            Err(whizbang_core::store::Error::NoSuchEntry { .. }) => return Err(no_bundle()),
            Err(_) => return Err(busy()),
        }
        // What the bundle alone named goes; what a run, an example, an import or another
        // bundle names stays.
        let pruned = store.prune().map(|pruned| pruned.objects_removed).ok();
        Ok(json!({"digest": digest.to_string(), "deleted": true, "removed": pruned}))
    }

    fn open_step(&self, digest: &str, step: u32, runs: &[Option<String>]) -> Result<Json, Refused> {
        let (digest, generation, manifest) = self.held_bundle(digest)?;
        let store = self.store()?;
        let Some(at) = manifest.steps.get(step as usize) else {
            return Err(Refused::new(404, "the bundle has no step of that number"));
        };
        let stored =
            Stored::of_bundle(store.as_ref(), &generation, &digest).map_err(|_| no_bundle())?;
        let record: RunRecord = match stored.record(&at.run) {
            Ok(Some(record)) => record,
            _ => {
                return Err(Refused::new(
                    422,
                    "the bundle's record of the step does not read",
                ));
            }
        };
        let action = record.asked().action.clone().unwrap_or_default();
        let entry = bundle::entry(&digest);
        // The file a use names among the bundle's files.
        let file_of = |used: &Use| (manifest.artifacts.iter()).find(|file| file.of.contains(used));
        let mut given = Vec::new();
        let mut bindings = Vec::new();
        let mut stopped: Option<Json> = None;
        let mut publication = None;
        for input in &at.inputs {
            let recorded = file_of(&Use {
                run: at.run.clone(),
                role: Role::Input,
                port: input.port.clone(),
                place: input.place,
            });
            let Some(recorded) = recorded else {
                stopped = Some(json!({"why": escaped(&format!(
                    "the bundle withholds the input {} {} of step {}", input.port, input.place, step + 1
                ))}));
                break;
            };
            let kind = recorded.artifact_type;
            match &input.from {
                // An input the bundle carries: given the store's ordinary name, as an
                // import, unless the store holds it under one already.
                None => {
                    let id = ArtifactId(recorded.artifact);
                    if !artifact::holds(&generation, kind, id) {
                        let held = generation
                            .get(&format!("{entry}/files/{}", recorded.file.hex()))
                            .ok_or_else(busy)?;
                        let bytes = store.read_output(held).map_err(|_| busy())?;
                        if publication.is_none() {
                            publication = Some(store.begin().map_err(|_| busy())?);
                        }
                        if let Some(publication) = publication.as_mut() {
                            let named = artifact::name(kind, id);
                            publication.put(&named, &bytes).map_err(|_| busy())?;
                            publication.mark_imported(&named).map_err(|_| busy())?;
                        }
                    }
                    given.push(Given {
                        port: input.port.clone(),
                        artifact_type: kind,
                        id: recorded.artifact.to_string(),
                    });
                }
                // An input an earlier step made: the new run of that step's output at the
                // link's port and place.
                Some(link) => {
                    let earlier = manifest.steps.get(link.step as usize);
                    let new_run = runs.get(link.step as usize).cloned().flatten();
                    let (Some(earlier), Some(new_run)) = (earlier, new_run) else {
                        stopped = Some(json!({"why": escaped(&format!(
                            "step {} takes the output of step {}, which has not run in this chain",
                            step + 1, link.step + 1
                        ))}));
                        break;
                    };
                    let Some(made) = record_in(&store, &generation, &new_run) else {
                        stopped = Some(json!({"why": escaped(&format!(
                            "step {}'s run {new_run} has not ended", link.step + 1
                        )), "run": escaped(&new_run)}));
                        break;
                    };
                    // The run named for the earlier step is that step's run of this chain:
                    // its record names this bundle and that step.
                    let of_step = (made.replayed())
                        .and_then(|replayed| replayed.bundle.as_ref())
                        .is_some_and(|from| from.digest == digest && from.step == link.step);
                    if !of_step {
                        return Err(Refused::new(
                            422,
                            &format!(
                                "the run {} is not step {}'s run of this chain: its record \
                                 names no step of this bundle",
                                escaped(&new_run),
                                link.step + 1
                            ),
                        ));
                    }
                    let outputs: Vec<String> = (made.outputs().iter())
                        .filter(|output| output.port == link.port)
                        .map(|output| output.artifact.to_string())
                        .collect();
                    let recorded_count = (manifest.artifacts.iter())
                        .flat_map(|file| file.of.iter())
                        .filter(|used| {
                            used.run == earlier.run
                                && used.role == Role::Output
                                && used.port == link.port
                        })
                        .count();
                    match bind(link.place, recorded_count, &outputs) {
                        Ok(now) => {
                            let was = recorded.artifact.to_string();
                            bindings.push(json!({
                                "port": escaped(&input.port), "place": input.place,
                                "from": {"step": link.step, "port": escaped(&link.port), "place": link.place},
                                "run": escaped(&new_run),
                                "now": now, "recorded": was, "differs": now != was,
                            }));
                            given.push(Given {
                                port: input.port.clone(),
                                artifact_type: kind,
                                id: now,
                            });
                        }
                        Err(why) => {
                            stopped = Some(json!({
                                "why": escaped(&format!(
                                    "step {} takes step {}'s {} at place {}, and step {}'s new run {new_run} {why}",
                                    step + 1, link.step + 1, link.port, link.place, link.step + 1
                                )),
                                "run": escaped(&new_run),
                            }));
                            break;
                        }
                    }
                }
            }
        }
        let title = (self.descriptors().iter())
            .find(|d| d.name() == action)
            .map(|d| escaped(d.title().unwrap_or(d.name())));
        let step_json = json!({
            "index": step, "of": manifest.steps.len(), "run": escaped(&at.run),
            "action": escaped(&action), "title": title,
        });
        if let Some(stopped) = stopped {
            return Ok(json!({"step": step_json, "stopped": stopped, "bindings": bindings}));
        }
        // An ordinary draft, as Run again gives one: the record's arguments, the inputs as
        // bound, and the config's choices of backend (the bundle's store's names are not
        // this one's). Made before anything is published, so a step this build cannot
        // run leaves nothing behind.
        let draft = self
            .tasks()
            .draft_of(
                &action,
                &record.asked().arguments,
                &given,
                &BTreeMap::new(),
                None,
            )
            .ok_or_else(|| {
                Refused::new(
                    422,
                    &format!(
                        "this build has no task {}: the step can be read, not run",
                        escaped(&action)
                    ),
                )
            })?;
        if let Some(publication) = publication {
            publication
                .publish()
                .map_err(|_| Refused::new(500, "the store could not give the step its inputs"))?;
        }
        let mut said = draft;
        said["step"] = step_json;
        said["bindings"] = Json::Array(bindings);
        said["stopped"] = Json::Null;
        said["bundle"] = json!(digest.to_string());
        Ok(said)
    }

    fn replay_report(&self, run: &RunId) -> Result<Json, Refused> {
        let root = self.root_path()?;
        let store = self.store()?;
        let generation = store.snapshot().map_err(|_| busy())?;
        let Some(record) = record_in(&store, &generation, run.as_str()) else {
            return Err(Refused::new(409, "the replay has not ended"));
        };
        if record.replayed().is_none() {
            return Err(Refused::new(422, "this run is not a replay"));
        }
        let reported = crate::replaying::report_of(&root, run.as_str())
            .map_err(|why| Refused::new(422, &escaped(&why)))?;
        let said: Json = serde_json::from_str(&reported.json()).map_err(|_| busy())?;
        // What the replay's own family settled: nothing, offline.
        let spent: u64 = (crate::play_result::tree(&store, record).iter())
            .flat_map(|record| record.spend().requests.iter())
            .map(|paid| paid.settled_micros())
            .sum();
        let mut said = escaped_all(said);
        said["spent_micros"] = json!(spent);
        Ok(said)
    }
}

/// What a page is told of a replayed run in its record, beside the rest.
pub fn replayed_json(record: &RunRecord) -> Json {
    match record.replayed() {
        None => Json::Null,
        Some(replayed) => json!({
            "run": escaped(&replayed.run),
            "bundle": replayed.bundle.as_ref().map(|from| json!({
                "digest": from.digest.to_string(), "step": from.step,
            })),
            "mode": match replayed.mode { ReplayMode::Offline => "offline", ReplayMode::Live => "live" },
        }),
    }
}
