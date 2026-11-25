//! Reading a bundle, trusting nothing (ADR 0048, "Reading one trusts nothing"; plan m7,
//! task B1, item 3). A bundle is read whole, one member at a time, and every rule below
//! passes before anything acts on it; a refusal names the first fault.
//!
//! - *Bounds*: [`MAX_BYTES`] in all, [`MEMBERS_LIMIT`] members, a manifest of
//!   [`MANIFEST_MAX_BYTES`]; a member's header is read and its size checked before its
//!   bytes; one member is held at a time, and artifacts are decoded one at a time under
//!   the core's limits.
//! - *Names*: each member a plain file, its name relative, with no `..` or `.`, no empty
//!   part, of one of the shapes a bundle has; no name twice; every member the manifest
//!   names there and no other.
//! - *Artifacts*: each file's SHA-256 is its name's and the manifest's; it decodes as
//!   the type the manifest says, to the identity the manifest says.
//! - *Records and journals*: each read by its own strict reader; each diagnostic's bytes
//!   match the digest and length its record says.
//! - *The whole*: each step's record agrees with the manifest; the families are trees,
//!   each with the step's run as its one root; the two directions of a family agree;
//!   every input and output a record names is a member or withheld; every link names an
//!   earlier step's committed output with the identity the later step took.
//! - *Version*: an unknown manifest version is refused, naming it.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use sha2::{Digest as _, Sha256};

use crate::artifact::Artifact;
use crate::contract::record::{DIAGNOSTICS_PREFIX, JOURNAL_NAME, Lost, RECORD_NAME, RunRecord};
use crate::contract::{Journal, RunId, is_diagnostic_name};
use crate::identity::{DecodeLimits, Digest};

use super::write::{inputs, interrupted, outputs};
use super::{
    FILES_PREFIX, FORMAT, MANIFEST, MANIFEST_MAX_BYTES, MAX_BYTES, MEMBERS_LIMIT, Manifest,
    RUNS_PREFIX, Refused, Role, STEPS_LIMIT, Use, Withheld, file_member, run_member, tar,
};

/// A bundle that was read and passed every rule.
#[derive(Debug, Clone)]
pub struct Checked {
    /// The SHA-256 of the whole file, which names it.
    pub digest: Digest,
    /// How many bytes it is.
    pub bytes: u64,
    pub manifest: Manifest,
    /// Each member's name, the digest of its bytes and their length, in the archive's
    /// order, the manifest first.
    pub members: Vec<(String, Digest, u64)>,
}

/// A reader that hashes what it reads.
pub(super) struct Hashing<R> {
    pub(super) from: R,
    pub(super) hash: Sha256,
}

impl<R: Read> Read for Hashing<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.from.read(buffer)?;
        self.hash.update(&buffer[..read]);
        Ok(read)
    }
}

/// A member's name, shown in a refusal: escaped, its first 160 characters.
fn shown(name: &str) -> String {
    let cut: String = name.chars().take(160).collect();
    format!("{cut:?}")
}

/// Whether a name is plain: relative, with no empty part and none that is `.` or `..`.
fn plain(name: &str) -> bool {
    !name.starts_with('/')
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && !name.chars().any(char::is_control)
}

/// What a member's name says it is.
enum Shape<'a> {
    File(Digest),
    Record(&'a str),
    /// A run's journal or diagnostic: the run, and the name its record lists it by.
    Diagnostic(&'a str, &'a str),
}

fn shape(name: &str) -> Option<Shape<'_>> {
    if let Some(hex) = name.strip_prefix(FILES_PREFIX) {
        return Digest::from_hex(hex).map(Shape::File);
    }
    let rest = name.strip_prefix(RUNS_PREFIX)?;
    let (run, what) = rest.split_once('/')?;
    if RunId::parse(run).is_err() {
        return None;
    }
    if what == RECORD_NAME {
        return Some(Shape::Record(run));
    }
    if what == JOURNAL_NAME {
        return Some(Shape::Diagnostic(run, what));
    }
    let diagnostic = what.strip_prefix(DIAGNOSTICS_PREFIX)?;
    is_diagnostic_name(diagnostic).then_some(Shape::Diagnostic(run, what))
}

/// Reads a bundle whole from `from` and holds it to every rule. Nothing is changed
/// anywhere: what comes back is what an import or a replay may act on.
pub fn check(from: impl Read) -> Result<Checked, Refused> {
    let mut hashing = Hashing {
        from,
        hash: Sha256::new(),
    };
    let mut tar = tar::Reader::new(&mut hashing, MAX_BYTES);
    let unread = |why: tar::Unread| Refused::new(why.to_string());
    // The manifest: first, and bounded before it is read.
    let Some(head) = tar.next_head().map_err(unread)? else {
        return Err(Refused::new("the archive holds nothing"));
    };
    if head.name != MANIFEST {
        return Err(Refused::new(format!(
            "its first member is {}, not the manifest, {MANIFEST}",
            shown(&head.name)
        )));
    }
    if head.bytes > MANIFEST_MAX_BYTES {
        return Err(Refused::new(format!(
            "its manifest is {} bytes, over its bound of {MANIFEST_MAX_BYTES}",
            head.bytes
        )));
    }
    let said = tar.bytes(&head).map_err(unread)?;
    let manifest = manifest(&said)?;
    let mut members = vec![(head.name, Digest::of_bytes(&said), head.bytes)];
    drop(said);

    // What the manifest names: each file, and each run's record.
    let mut files: BTreeMap<Digest, usize> = BTreeMap::new();
    for (at, file) in manifest.artifacts.iter().enumerate() {
        if files.insert(file.file, at).is_some() {
            return Err(Refused::new(format!(
                "the manifest names the file {} twice",
                file.file
            )));
        }
    }
    let mut runs: BTreeMap<String, usize> = BTreeMap::new();
    for (at, step) in manifest.steps.iter().enumerate() {
        for run in &step.runs {
            RunId::parse(run).map_err(|why| Refused::new(format!("the manifest names {why}")))?;
            if runs.insert(run.clone(), at).is_some() {
                return Err(Refused::new(format!(
                    "the manifest names the run {run} twice"
                )));
            }
        }
    }

    let limits = DecodeLimits::default();
    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut held_files: BTreeSet<Digest> = BTreeSet::new();
    let mut records: BTreeMap<String, RunRecord> = BTreeMap::new();
    let mut diagnostics: BTreeMap<(String, String), (Digest, u64)> = BTreeMap::new();
    while let Some(head) = tar.next_head().map_err(unread)? {
        if members.len() >= MEMBERS_LIMIT {
            return Err(Refused::new(format!(
                "it holds more than {MEMBERS_LIMIT} members"
            )));
        }
        let name = head.name.clone();
        if !plain(&name) {
            return Err(Refused::new(format!(
                "the member {} is not named plainly: a name is relative, with no empty part, \
                 no . and no ..",
                shown(&name)
            )));
        }
        if name == MANIFEST || !names.insert(name.clone()) {
            return Err(Refused::new(format!(
                "the member {} is there twice",
                shown(&name)
            )));
        }
        let not_named = || {
            Refused::new(format!(
                "the member {} is not one the manifest names",
                shown(&name)
            ))
        };
        let Some(what) = shape(&name) else {
            return Err(not_named());
        };
        // The member is named before its bytes are read.
        match &what {
            Shape::File(digest) if !files.contains_key(digest) => return Err(not_named()),
            Shape::Record(run) | Shape::Diagnostic(run, _) if !runs.contains_key(*run) => {
                return Err(not_named());
            }
            _ => {}
        }
        let bytes = tar.bytes(&head).map_err(unread)?;
        let digest = Digest::of_bytes(&bytes);
        members.push((name.clone(), digest, head.bytes));
        match what {
            Shape::File(named) => {
                let file = &manifest.artifacts[files[&named]];
                if digest != named {
                    return Err(Refused::new(format!(
                        "the file {} is not the bytes its name says: its SHA-256 is {digest}",
                        shown(&name)
                    )));
                }
                let read = Artifact::from_bytes_as(file.artifact_type, &bytes, &limits).map_err(
                    |why| {
                        Refused::new(format!(
                            "the file {} is not {} that this build reads: {why}",
                            shown(&name),
                            file.artifact_type
                        ))
                    },
                )?;
                if read.id().0 != file.artifact {
                    return Err(Refused::new(format!(
                        "the file {} decodes to the artifact {}, not {}, which the manifest \
                         says it holds",
                        shown(&name),
                        read.id(),
                        file.artifact
                    )));
                }
                held_files.insert(named);
            }
            Shape::Record(run) => {
                let record = RunRecord::from_bytes(&bytes).map_err(|why| {
                    Refused::new(format!(
                        "the record of {run} is not one this build reads: {why}"
                    ))
                })?;
                if record.identity().run != run {
                    return Err(Refused::new(format!(
                        "the record held as {run}'s names another run"
                    )));
                }
                records.insert(run.to_owned(), record);
            }
            Shape::Diagnostic(run, listed) => {
                if listed == JOURNAL_NAME {
                    Journal::read(&bytes).map_err(|why| {
                        Refused::new(format!(
                            "the journal of {run} is not one this build reads: {why}"
                        ))
                    })?;
                }
                diagnostics.insert((run.to_owned(), listed.to_owned()), (digest, head.bytes));
            }
        }
    }
    let bytes = tar.read_so_far();
    let digest = Digest::from_bytes(hashing.hash.finalize().into());

    // Every member the manifest names is there.
    for file in &manifest.artifacts {
        if !held_files.contains(&file.file) {
            return Err(Refused::new(format!(
                "the manifest names the file {}, which the bundle does not hold",
                file_member(&file.file)
            )));
        }
    }
    for run in runs.keys() {
        if !records.contains_key(run) {
            return Err(Refused::new(format!(
                "the manifest names the run {run}, whose record the bundle does not hold"
            )));
        }
    }
    whole(&manifest, &records, &diagnostics)?;
    Ok(Checked {
        digest,
        bytes,
        manifest,
        members,
    })
}

/// The manifest, read: its version first, alone.
fn manifest(said: &[u8]) -> Result<Manifest, Refused> {
    let value: serde_json::Value = serde_json::from_slice(said)
        .map_err(|why| Refused::new(format!("its manifest is not JSON: {why}")))?;
    let version = value.get("bundle");
    if version.and_then(serde_json::Value::as_u64) != Some(u64::from(FORMAT)) {
        let named: String = match version {
            Some(version) => version.to_string().chars().take(64).collect(),
            None => "none".to_owned(),
        };
        return Err(Refused::new(format!(
            "its manifest's version is {named}, and this build reads version {FORMAT} alone"
        )));
    }
    let manifest: Manifest = serde_json::from_value(value)
        .map_err(|why| Refused::new(format!("its manifest is not one this build reads: {why}")))?;
    if manifest.steps.is_empty() || manifest.steps.len() > STEPS_LIMIT {
        return Err(Refused::new(format!(
            "its manifest has {} steps: a bundle has 1 to {STEPS_LIMIT}",
            manifest.steps.len()
        )));
    }
    Ok(manifest)
}

/// Holds what was read to the rules of the whole.
fn whole(
    manifest: &Manifest,
    records: &BTreeMap<String, RunRecord>,
    diagnostics: &BTreeMap<(String, String), (Digest, u64)>,
) -> Result<(), Refused> {
    // Each run's family, by the step it is of.
    let mut step_of: BTreeMap<&str, usize> = BTreeMap::new();
    for (at, step) in manifest.steps.iter().enumerate() {
        for run in &step.runs {
            step_of.insert(run, at);
        }
    }

    // Each step's record agrees with the manifest; its links name earlier outputs.
    for (at, step) in manifest.steps.iter().enumerate() {
        if step.runs.first() != Some(&step.run) {
            return Err(Refused::new(format!(
                "step {at}'s family does not start with its run, {}",
                step.run
            )));
        }
        let record = &records[&step.run];
        let asked = record.asked();
        if record.identity().parent.is_some() {
            return Err(Refused::new(format!(
                "step {at}'s run, {}, was called by another run: a step is a root",
                step.run
            )));
        }
        if asked.action != step.action
            || asked.version != step.version
            || asked.config != step.config
        {
            return Err(Refused::new(format!(
                "the manifest's step {at} contradicts the record of {}: its action, version \
                 or config differs",
                step.run
            )));
        }
        let taken = inputs(record);
        if taken.len() != step.inputs.len() {
            return Err(Refused::new(format!(
                "the manifest's step {at} lists {} inputs, and the record of {} {}",
                step.inputs.len(),
                step.run,
                taken.len()
            )));
        }
        for (input, port) in step.inputs.iter().zip(&taken) {
            if input.port != port.port || input.place != port.place {
                return Err(Refused::new(format!(
                    "the manifest's step {at} contradicts the record of {}: its input {} is \
                     not at the record's place",
                    step.run, input.port
                )));
            }
            match (&input.artifact, &input.from) {
                (Some(identity), None) if *identity == port.artifact => {}
                (None, Some(link)) => {
                    let earlier = (link.step as usize) < at;
                    let committed = earlier
                        && outputs(&records[&manifest.steps[link.step as usize].run])
                            .iter()
                            .any(|output| {
                                output.port == link.port
                                    && output.place == link.place
                                    && output.artifact == port.artifact
                            });
                    if !committed {
                        return Err(Refused::new(format!(
                            "the link of step {at}'s input {} names the output {} of step {}, \
                             which no earlier step's record committed with the identity step \
                             {at}'s record took",
                            input.port, link.port, link.step
                        )));
                    }
                }
                _ => {
                    return Err(Refused::new(format!(
                        "the manifest's step {at} contradicts the record of {}: its input {} \
                         names another artifact, or both or neither of an identity and a link",
                        step.run, input.port
                    )));
                }
            }
        }
    }

    // Each family is a tree whose one root is its step's run.
    for step in &manifest.steps {
        let count = step.runs.len();
        for run in step.runs.iter().skip(1) {
            let mut at = run.as_str();
            let mut reached = false;
            for _ in 0..=count {
                let Some(parent) = records[at].identity().parent.as_deref() else {
                    return Err(Refused::new(format!(
                        "the run {at} names no parent and is not its family's root, {}: a \
                         family has two roots",
                        step.run
                    )));
                };
                if !records.contains_key(parent) {
                    return Err(Refused::new(format!(
                        "the run {at} names the parent {}, which the bundle does not hold",
                        parent.chars().take(64).collect::<String>()
                    )));
                }
                if step_of[parent] != step_of[run.as_str()] {
                    return Err(Refused::new(format!(
                        "the run {at} names a parent of another step's family"
                    )));
                }
                if parent == step.run {
                    reached = true;
                    break;
                }
                at = parent;
            }
            if !reached {
                return Err(Refused::new(format!(
                    "the run {run}'s parents make a cycle that never reaches its family's \
                     root, {}",
                    step.run
                )));
            }
        }
    }

    // The two directions agree: what a record lists as its calls, and what each call
    // names as its parent.
    let mut listed_by: BTreeMap<&str, &str> = BTreeMap::new();
    for (id, record) in records {
        for call in record.steps() {
            let child = call.run();
            if listed_by.insert(child, id).is_some() {
                return Err(Refused::new(format!(
                    "the run {} is listed as a call by two runs, or twice",
                    child.chars().take(64).collect::<String>()
                )));
            }
            let Some(called) = records.get(child) else {
                return Err(Refused::new(format!(
                    "the run {id} lists a call, {}, whose record the bundle does not hold",
                    child.chars().take(64).collect::<String>()
                )));
            };
            if called.identity().parent.as_deref() != Some(id) {
                return Err(Refused::new(format!(
                    "the run {child} is listed as a call by {id} and names another parent"
                )));
            }
            if called.asked().action.as_deref() != Some(call.action()) {
                return Err(Refused::new(format!(
                    "the run {id} lists its call {child} as another action than its record"
                )));
            }
            let agrees = match call.status() {
                Some(status) => status == called.outcome().status,
                None => interrupted(called),
            };
            if !agrees {
                return Err(Refused::new(format!(
                    "the run {id} noted another ending for its call {child} than its record's"
                )));
            }
        }
    }
    for (id, record) in records {
        let Some(parent) = record.identity().parent.as_deref() else {
            continue;
        };
        let lost = records[parent].outcome().lost.contains(&Lost::Steps);
        if listed_by.get(id.as_str()) != Some(&parent) && !lost {
            return Err(Refused::new(format!(
                "the run {id} names {parent} as its parent, which does not list it"
            )));
        }
    }

    // Every input and output of every record is a file member or withheld, and each file
    // member is what its records took or made.
    let mut uses: BTreeSet<&Use> = BTreeSet::new();
    for file in &manifest.artifacts {
        for used in &file.of {
            if !uses.insert(used) {
                return Err(Refused::new(format!(
                    "the manifest names two files for the {} {} of {}",
                    used.role.name(),
                    used.port,
                    used.run
                )));
            }
            let Some(record) = records.get(&used.run) else {
                return Err(Refused::new(format!(
                    "the manifest names a file of the run {}, which the bundle does not hold",
                    used.run
                )));
            };
            let ports = match used.role {
                Role::Input => inputs(record),
                Role::Output => outputs(record),
            };
            let agrees = ports.iter().any(|port| {
                port.port == used.port
                    && port.place == used.place
                    && port.artifact == file.artifact
                    && port.artifact_type == file.artifact_type
            });
            if !agrees {
                return Err(Refused::new(format!(
                    "the manifest contradicts the record of {}: it holds no {} {} at place {} \
                     of the artifact {}",
                    used.run,
                    used.role.name(),
                    used.port,
                    used.place,
                    file.artifact
                )));
            }
        }
    }
    let mut withheld_ports = BTreeSet::new();
    let mut withheld_diagnostics = BTreeSet::new();
    for withheld in &manifest.withheld {
        if !records.contains_key(withheld.run()) {
            return Err(Refused::new(format!(
                "the manifest withholds something of the run {}, which the bundle does not \
                 hold",
                withheld.run().chars().take(64).collect::<String>()
            )));
        }
        match withheld {
            Withheld::Artifact {
                run,
                role,
                port,
                place,
                ..
            } => {
                withheld_ports.insert((run.as_str(), *role, port.as_str(), *place));
            }
            Withheld::Diagnostic { run, name, .. } => {
                withheld_diagnostics.insert((run.as_str(), name.as_str()));
            }
            _ => {}
        }
    }
    for (id, record) in records {
        for (role, ports) in [
            (Role::Input, inputs(record)),
            (Role::Output, outputs(record)),
        ] {
            for port in ports {
                let used = Use {
                    run: id.clone(),
                    role,
                    port: port.port.clone(),
                    place: port.place,
                };
                let withheld =
                    withheld_ports.contains(&(id.as_str(), role, port.port.as_str(), port.place));
                if !uses.contains(&used) && !withheld {
                    return Err(Refused::new(format!(
                        "the {} {} of the run {id} is neither a member nor withheld",
                        role.name(),
                        port.port
                    )));
                }
            }
        }
        // Each diagnostic it lists is a member with the bytes it says, or withheld.
        for listed in record.diagnostics() {
            let key = (id.clone(), listed.name().to_owned());
            match diagnostics.get(&key) {
                Some((digest, bytes)) => {
                    if *digest != listed.digest() || *bytes != listed.bytes() {
                        return Err(Refused::new(format!(
                            "the diagnostic {} of the run {id} is not the bytes its record \
                             says",
                            shown(&run_member(id, listed.name()))
                        )));
                    }
                }
                None if withheld_diagnostics.contains(&(id.as_str(), listed.name())) => {}
                None => {
                    return Err(Refused::new(format!(
                        "the record of {id} lists the diagnostic {}, which the bundle neither \
                         holds nor withholds",
                        listed.name()
                    )));
                }
            }
        }
    }
    for (run, name) in diagnostics.keys() {
        let listed = records[run]
            .diagnostics()
            .iter()
            .any(|listed| listed.name() == name);
        if !listed {
            return Err(Refused::new(format!(
                "the member {} is not one its run's record lists",
                shown(&run_member(run, name))
            )));
        }
    }
    Ok(())
}
