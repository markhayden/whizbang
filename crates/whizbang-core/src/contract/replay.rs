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
use crate::capability::recorded::{self, Held, Recorded};
use crate::identity::{Digest, Encoder, Fields};
use crate::store::{Generation, Kind};

use super::record::{
    ArgumentValue, DIAGNOSTICS_PREFIX, FromBundle, INTERRUPTED, OUTPUTS_PREFIX, RECORD_NAME,
    ReplayMode, Replayed, RunRecord,
};
use super::{Call, CallError, DESCENDANTS_LIMIT, Refusal, RequestId, RunId, StoreFailure, Value};

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

/// A replay, read from its source: the call made from the root's record, the family's
/// records, the root first, and, offline, the answers they recorded.
pub(super) struct Read {
    pub(super) call: Call,
    pub(super) root: String,
    pub(super) config: Option<Digest>,
    pub(super) recorded: Option<Arc<Recorded>>,
}

/// The family of a run of the store, read through one generation.
struct Store<'a> {
    contents: &'a (dyn Contents + Send + Sync),
    generation: &'a Generation,
}

impl Held for Store<'_> {
    fn diagnostic(&self, run: &str, name: &str) -> Result<Option<Vec<u8>>, String> {
        let Some(entry) = self.generation.run(run).and_then(|held| held.get(name)) else {
            return Ok(None);
        };
        (self.contents.read_record(entry))
            .map(Some)
            .map_err(|why| format!("the diagnostic {name} of the run {run} cannot be read: {why}"))
    }

    fn output(&self, run: &str, port: &str, place: u32) -> Result<Option<Vec<u8>>, String> {
        let name = format!("{OUTPUTS_PREFIX}{port}/{place}");
        let entry = self.generation.run(run).and_then(|held| held.get(&name));
        let Some(entry) = entry.filter(|entry| entry.kind == Kind::Output) else {
            return Ok(None);
        };
        (self.contents.read(entry))
            .map(Some)
            .map_err(|why| format!("the output {port} of the run {run} cannot be read: {why}"))
    }
}

impl Store<'_> {
    fn record(&self, run: &str) -> Result<Option<RunRecord>, CallError> {
        let Some(entry) = self
            .generation
            .run(run)
            .and_then(|held| held.get(RECORD_NAME))
        else {
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

fn not_replayed(run: &str, why: &str) -> CallError {
    CallError::Refused(Refusal::NotReplayed {
        run: run.to_owned(),
        why: why.to_owned(),
    })
}

/// Reads a replay of a run of the store: its family through this generation, every rule
/// of "What is replayed" held, and the call made from its record. `shipped` names each
/// action this build ships, with its version.
pub(super) fn read(
    replay: &Replay,
    contents: &(dyn Contents + Send + Sync),
    generation: &Generation,
    shipped: &BTreeMap<String, String>,
) -> Result<Read, CallError> {
    let root = match &replay.source {
        Source::Run(run) => run.as_str().to_owned(),
        Source::Bundle { .. } => {
            return Err(CallError::Refused(Refusal::NotReplayed {
                run: "-".to_owned(),
                why: "a replay from a bundle is not built yet".to_owned(),
            }));
        }
    };
    if replay.mode == ReplayMode::Live {
        return Err(not_replayed(&root, "a live replay is not built yet"));
    }
    let store = Store {
        contents,
        generation,
    };
    if generation.run(&root).is_none() {
        let run = RunId::parse(&root).map_err(|why| not_replayed(&root, &why))?;
        return Err(CallError::Refused(Refusal::NoSuchRun { run }));
    }
    let Some(record) = store.record(&root)? else {
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
    match shipped.get(action) {
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
            match store.record(&child)? {
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
    if let Some((run, names)) = recorded::tool_files(&runs).first_key_value() {
        return Err(not_replayed(
            &root,
            &format!(
                "the run {run} of its family called the tool file {}: a tool file is code, \
                 and an offline replay never runs one",
                names.join(", ")
            ),
        ));
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
    for input in root_record.inputs() {
        call = call.input(&input.port, ArtifactId(input.artifact));
    }
    let recorded = Arc::new(Recorded::of_family(&runs, &store));
    Ok(Read {
        call,
        root,
        config: root_record.asked().config,
        recorded: Some(recorded),
    })
}

/// The note a replay's root keeps of where it stopped, as [`recorded::NOTE`] names it
/// among its diagnostics.
pub fn note_name() -> String {
    format!("{DIAGNOSTICS_PREFIX}{}", recorded::NOTE)
}
