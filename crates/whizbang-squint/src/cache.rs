//! The decision cache (`SPEC.md` section 9; plan 2C, task M1, item 4; ADR 0031, "The
//! decision cache"; after squint's `crates/squint-core/src/cache.rs`, at the commit
//! `docs/sources.toml` pins). A run of `check` under a use case of the table, in a
//! registry that turns the cache on, works out its key before it sends anything: the
//! complete job, every semantic argument, the bindings of its images in order (so the
//! pictures by their hashes), the resolved rules, checklist and prompt, the revisions of
//! the tools it may call, the judge's identity, the digest of the config, the limits and
//! the harness's version ([`Material`]). A run that finds a run of the same key that
//! completed answers with that run's verdict, sends nothing, and records that it did,
//! naming the run ([`ANSWERED_BY`]). A run that concludes keeps its key among its
//! diagnostics, so that a later run can find it, only when every turn of its judge
//! said which model answered (a backend that cannot report its identity is not
//! cached).
//!
//! The key is the SHA-256 of the material, and the material itself is kept beside it
//! and compared whole on a hit: two runs share an answer only when everything above is
//! the same, byte for byte. A hit is a run like any other: it is admitted, and its
//! reservation is taken and settled at nothing. Evaluation never reads the cache: its
//! registry does not turn it on, and a run that names its cell of a matrix never looks.

use serde_json::{Value as Json, json};
use whizbang_core::artifact::{ArtifactId, Contents};
use whizbang_core::contract::record::{self, DIAGNOSTICS_PREFIX, Status};
use whizbang_core::contract::{ModelIdentity, Value};
use whizbang_core::identity::{BindingId, Digest};

use crate::check::{self, TURN_TEXT_LIMIT, TURNS_LIMIT};
use crate::job::Job;
use crate::use_case::{self, Checklist, REASONS};
use crate::{align, compare, details, resize, view};

/// The name and version of the material's form.
pub const CONTRACT: &str = "decision-cache/1";
/// The start of the name of the diagnostic that holds a run's key: the rest is the
/// first hex digits of the key, as many as a name holds.
pub const PREFIX: &str = "cache-";
/// The setting a run notes of the cache: `hit`, `kept`, or why it was not kept.
pub const SETTING: &str = "decision_cache";
/// The setting a hit notes: the run whose verdict answered it.
pub const ANSWERED_BY: &str = "answered_by_run";
/// The arguments that are not part of the key: where a run sits in a matrix, which
/// `check` does not read, and the job, which is in the key in a form of its own.
const NOT_SEMANTIC: [&str; 2] = ["cell", "job"];

/// What the key of one run is made of.
pub struct Material<'a> {
    pub job: &'a Job,
    /// The call's arguments, as prepared.
    pub arguments: &'a [(String, Value)],
    /// The call's images, in order: each identity and its binding.
    pub images: &'a [(ArtifactId, BindingId)],
    /// The system text, the checklist as the judge is shown it, and the rules' version.
    pub system: &'a str,
    pub checklist: &'a str,
    pub rules_version: u32,
    /// The tools the run may call or measures first, by name.
    pub tools: &'a [&'a str],
    pub judge: &'a ModelIdentity,
    /// The digest of the config the call was prepared under.
    pub config: Digest,
}

/// A run's key: the material, written once, and its digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    material: String,
    digest: Digest,
}

impl Key {
    /// The material, as it is kept.
    pub fn material(&self) -> &str {
        &self.material
    }

    pub fn digest(&self) -> Digest {
        self.digest
    }

    /// The name of the diagnostic that keeps it: [`PREFIX`] and as many of the key's
    /// hex digits as a name of 64 characters holds.
    pub fn name(&self) -> String {
        let hex = self.digest.hex();
        format!("{PREFIX}{}", &hex[..64 - PREFIX.len()])
    }
}

fn value_json(value: &Value) -> Json {
    match value {
        Value::Number(number) => json!({"number": number}),
        Value::Integer(number) => json!({"integer": number}),
        Value::Boolean(boolean) => json!({"boolean": boolean}),
        Value::Text(text) => json!({"text": text}),
    }
}

/// The revision of a tool this build links, by its name: the action's version, and
/// for `align`, `change_map` and `edge_similarity` the version of squint's code beside it.
pub fn tool_version(name: &str) -> Option<String> {
    match name {
        view::NAME => Some(view::VERSION.to_owned()),
        align::NAME => Some(format!("{}+{}", align::ACTION_VERSION, align::VERSION)),
        resize::NAME => Some(resize::VERSION.to_owned()),
        compare::CHANGE_MAP => Some(format!(
            "{}+{}",
            compare::ACTION_VERSION,
            compare::CHANGE_MAP_VERSION
        )),
        compare::EDGE_SIMILARITY => Some(format!(
            "{}+{}",
            compare::ACTION_VERSION,
            compare::EDGE_SIMILARITY_VERSION
        )),
        _ => None,
    }
}

/// The key of this material. The material is canonical JSON: the object's keys in
/// order, the lists in the order they were given.
pub fn key(material: &Material<'_>) -> Key {
    let mut job = material.job.clone();
    // Where a picture came from is not what is judged: its binding is.
    for image in &mut job.images {
        image.source = None;
    }
    let arguments: serde_json::Map<String, Json> = (material.arguments.iter())
        .filter(|(name, _)| !NOT_SEMANTIC.contains(&name.as_str()))
        .map(|(name, value)| (name.clone(), value_json(value)))
        .collect();
    let images: Vec<Json> = (material.images.iter())
        .map(|(id, binding)| json!({"artifact": id.to_string(), "binding": binding.to_string()}))
        .collect();
    let tools: Vec<Json> = (material.tools.iter())
        .map(|name| json!({"name": name, "version": tool_version(name)}))
        .collect();
    let judge = material.judge;
    let written = json!({
        "contract": CONTRACT,
        "harness": {"check": check::VERSION, "squint": env!("CARGO_PKG_VERSION")},
        "job": serde_json::to_value(&job).unwrap_or(Json::Null),
        "arguments": arguments,
        "images": images,
        "rules_version": material.rules_version,
        "prompt": {
            "system": material.system,
            "checklist": material.checklist,
            "instruction": use_case::INSTRUCTION,
            "repair": use_case::REPAIR,
            "details": details::DETAILS_INTRO,
        },
        "tools": tools,
        "judge": {"backend": judge.backend(), "model": judge.model(), "version": judge.version()},
        "config": material.config.to_string(),
        "limits": {"turns": TURNS_LIMIT, "calls": check::CALLS_LIMIT,
                   "turn_text": TURN_TEXT_LIMIT, "retries": check::RETRIES_LIMIT,
                   "retry_factor": check::RETRY_FACTOR, "images": check::IMAGES_LIMIT},
    })
    .to_string();
    let digest = Digest::of_bytes(written.as_bytes());
    Key {
        material: written,
        digest,
    }
}

/// The prompt a run of this job is told, as `check` makes it: the system text and
/// the checklist, rendered; and the rules' version. Or why the job cannot be judged.
pub fn prompt_of(job: &Job, tools: &[&str]) -> Result<(String, String, u32), String> {
    let resolved = use_case::resolve(job)?;
    let checklist = Checklist::build(&resolved, job);
    let system = use_case::system_with(&resolved, job, &details::tools_section(tools));
    Ok((system, checklist.render(&resolved), resolved.version))
}

/// What a hit answers with: the run, its verdict, and what it kept of its reasons and
/// of what it decided, when it kept them.
#[derive(Debug, Clone)]
pub struct Hit {
    pub run: String,
    pub decision: record::Decision,
    pub why: String,
    pub reasons: Option<Vec<u8>>,
    pub judged: Option<Vec<u8>>,
}

/// The newest run in the store whose key is this one, whole, and that completed with a
/// verdict of `check`. A run whose kept material differs in any byte, whose record
/// cannot be read, or that did not complete, is passed over.
pub fn find(store: &dyn Contents, key: &Key) -> Option<Hit> {
    let generation = store.snapshot().ok()?;
    let name = format!("{DIAGNOSTICS_PREFIX}{}", key.name());
    for (id, run) in generation.runs().iter().rev() {
        let Some(entry) = run.get(&name) else {
            continue;
        };
        if store.read_record(entry).ok().as_deref() != Some(key.material().as_bytes()) {
            continue;
        }
        let Some(Ok(earlier)) = record::stored(&generation, id, |e| store.read_record(e)) else {
            continue;
        };
        let completed = earlier.outcome().status == Status::Completed;
        let of_check = earlier.asked().action.as_deref() == Some(check::NAME);
        let Some((decision, why)) = earlier.verdict() else {
            continue;
        };
        if !completed || !of_check {
            continue;
        }
        let kept = |name: &str| {
            let entry = run.get(&format!("{DIAGNOSTICS_PREFIX}{name}"))?;
            store.read_record(entry).ok()
        };
        return Some(Hit {
            run: id.clone(),
            decision,
            why: why.to_owned(),
            reasons: kept(REASONS),
            judged: kept(check::JUDGED),
        });
    }
    None
}
