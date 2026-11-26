//! `whizbang try <tool>` and `whizbang eval tool <tool> --set <set>` (plan
//! `docs/plans/m2c-milestone-2.md`, task X1, item 5): a tool's read of one pair, with no
//! judge; and its read of each item of a set, scored against the set's labels on rung 2
//! ([`whizbang_eval::tool`]).
//!
//! The tools that can be tried are the two that compare a pair in one grid, `change_map`
//! and `edge_similarity`, each declared here with its primary measurement and the way that
//! means "changed" ([`declared`]); and each tool file the config lists whose images are
//! `source` and `candidate`, declared by its file (ADR 0032; [`declared_in`]). A read is two
//! runs in the store: `align`, the candidate registered onto the source; then the tool, on
//! the source and the frame `align` returned. A pair that does not register gives no read:
//! in a set, that item is an error, never a pass or a fail. A tool file that takes the
//! argument `job` is given each item's job, in the contract `job/1`, by `eval tool`: its
//! requirements, and never its label.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value as Json;
use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::{Call, Executor, RunId, Value};
use whizbang_core::identity::Digest;
use whizbang_eval::sets::{self, Expected};
use whizbang_eval::tool::{Accuracy, Changed, Read, Scored, score};
use whizbang_squint::compare;
use whizbang_squint::job::{Job, Role};

use crate::door::{self, printable};
use crate::import;
use crate::tool_file::{self, Files as ToolFiles};

/// A tool that can be tried: its name, its primary measurement, the way of it that means
/// changed, the diagnostic its run states its measurements in, whether it is a tool file
/// that takes the item's job, and the arguments of text it declares, which the rules'
/// `[hints]` may fill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    pub tool: String,
    pub primary: String,
    pub changed: Changed,
    pub report: String,
    pub file: bool,
    pub takes_job: bool,
    pub texts: Vec<String>,
}

/// The tools this build links that can be tried: each its name, primary measurement, the
/// way that is changed, and its report.
const LINKED: [(&str, &str, Changed, &str); 2] = [
    (
        compare::CHANGE_MAP,
        "changed_fraction",
        Changed::Larger,
        compare::CHANGE_MAP_REPORT,
    ),
    (
        compare::EDGE_SIMILARITY,
        "edge_similarity",
        Changed::Smaller,
        compare::EDGE_SIMILARITY_REPORT,
    ),
];

/// The declaration of the tool of this name among those this build links.
pub fn declared(tool: &str) -> Result<Declared, String> {
    declared_in(tool, None)
}

/// The same, among those this build links and the tool files the config lists: a tool
/// file whose images are `source` and `candidate`.
pub fn declared_in(tool: &str, files: Option<&ToolFiles>) -> Result<Declared, String> {
    if let Some((tool, primary, changed, report)) = LINKED.into_iter().find(|d| d.0 == tool) {
        return Ok(Declared {
            tool: tool.to_owned(),
            primary: primary.to_owned(),
            changed,
            report: report.to_owned(),
            file: false,
            takes_job: false,
            texts: Vec::new(),
        });
    }
    let file = files.and_then(|files| files.declared(tool));
    if let Some(file) = file.filter(|d| d.bindings == ["source", "candidate"]) {
        return Ok(Declared {
            tool: file.name.clone(),
            primary: file.primary.clone(),
            changed: file.changed(),
            report: tool_file::REPORT.to_owned(),
            file: true,
            takes_job: file
                .arguments
                .iter()
                .any(|a| a.name == "job" && a.kind == "text"),
            texts: (file.arguments.iter())
                .filter(|a| a.kind == "text")
                .map(|a| a.name.clone())
                .collect(),
        });
    }
    let mut names: Vec<String> = LINKED.iter().map(|d| d.0.to_owned()).collect();
    names.push("a tool file of the config whose images are source and candidate".to_owned());
    Err(format!(
        "{} is no tool that can be tried: {}",
        printable(tool),
        names.join(", ")
    ))
}

/// The arguments the rules' hints give a tool (plan mE, task T1, item 4): each that the
/// hints name for it and that it declares as an argument of text.
pub fn hinted_arguments(
    declared: &Declared,
    hints: &whizbang_squint::use_case::Hints,
) -> Vec<(String, Value)> {
    (hints.arguments(&declared.tool).into_iter())
        .filter(|(name, _)| declared.texts.contains(name))
        .map(|(name, value)| (name, Value::text(&value)))
        .collect()
}

/// The arguments `try` gives a tool file, each `NAME=VALUE`: a name it declares, given
/// once, and a number where it declares one. A tool this build links takes none.
pub fn arguments_of(
    declared: &Declared,
    files: Option<&ToolFiles>,
    given: &[String],
) -> Result<Vec<(String, Value)>, String> {
    let file = files.and_then(|files| files.declared(&declared.tool));
    let mut arguments: Vec<(String, Value)> = Vec::new();
    for text in given {
        let Some((name, value)) = text.split_once('=') else {
            return Err(format!("--arg {}: give it as NAME=VALUE", printable(text)));
        };
        let Some(argument) = file.and_then(|f| f.arguments.iter().find(|a| a.name == name)) else {
            return Err(format!(
                "{} takes no argument {}",
                declared.tool,
                printable(name)
            ));
        };
        if arguments.iter().any(|(given, _)| given == name) {
            return Err(format!("--arg {} is given twice", printable(name)));
        }
        let value = if argument.kind == "number" {
            let number: f64 = value
                .parse()
                .map_err(|_| format!("--arg {}: a number", printable(name)))?;
            Value::Number(number)
        } else {
            Value::text(value)
        };
        arguments.push((name.to_owned(), value));
    }
    Ok(arguments)
}

/// One read of a pair: the runs it made, and the measurements, or why there are none.
#[derive(Debug, Clone)]
pub struct Tried {
    pub align: RunId,
    pub tool: Option<RunId>,
    /// Each measurement's name, value and unit, as the tool's run states them.
    pub measured: Result<Vec<(String, f64, String)>, String>,
    /// What the tool's run committed: the change map, for `change_map`.
    pub outputs: Vec<ArtifactId>,
}

impl Tried {
    /// The primary measurement, or why there is none.
    pub fn primary(&self, declared: &Declared) -> Result<f64, String> {
        let measured = self.measured.as_ref().map_err(Clone::clone)?;
        measured
            .iter()
            .find(|(name, _, _)| *name == declared.primary)
            .map(|(_, value, _)| *value)
            .ok_or_else(|| format!("the run states no {}", declared.primary))
    }
}

/// Runs a call and waits: its run, and its outputs or why it did not complete.
fn ran(
    executor: &dyn Executor,
    call: Call,
) -> Result<(RunId, Result<Vec<ArtifactId>, String>), String> {
    use whizbang_core::contract::{Answer, Outcome, Request, RunState};
    let answer = executor
        .request(&Request::new(call))
        .map_err(|e| e.to_string())?;
    let (run, state) = match answer {
        Answer::Ran(finished) => (
            finished.run().clone(),
            RunState::Ended(finished.outcome().clone()),
        ),
        Answer::Repeat(earlier) => (earlier.run().clone(), earlier.state().clone()),
        Answer::Started(run) => (run, RunState::NotEnded { here: true }),
    };
    let ended = match state {
        RunState::Ended(Outcome::Completed { outputs }) => {
            Ok(outputs.iter().map(|output| output.id()).collect())
        }
        RunState::Ended(Outcome::Invalid { why }) => Err(format!("refused: {why}")),
        RunState::Ended(Outcome::Error { kind, message }) => Err(format!("{kind}: {message}")),
        RunState::Ended(Outcome::Cancelled) => Err("cancelled".to_owned()),
        _ => Err("did not end".to_owned()),
    };
    Ok((run, ended))
}

/// The tool's read of a pair: `candidate` registered onto `source` by `align`, then the
/// tool on `source` and the aligned frame. A pair that does not register, or a tool that
/// does not complete, gives no measurements, and says why.
pub fn read_pair(
    executor: &dyn Executor,
    root: &Path,
    declared: &Declared,
    source: ArtifactId,
    candidate: ArtifactId,
) -> Result<Tried, String> {
    read_pair_with(executor, root, declared, source, candidate, &[])
}

/// The same, a tool file given these arguments.
pub fn read_pair_with(
    executor: &dyn Executor,
    root: &Path,
    declared: &Declared,
    source: ArtifactId,
    candidate: ArtifactId,
    arguments: &[(String, Value)],
) -> Result<Tried, String> {
    let align = Call::new(whizbang_squint::align::NAME)
        .input("image", candidate)
        .input("against", source);
    let (align_run, aligned) = ran(executor, align)?;
    let aligned = match aligned {
        Ok(outputs) => outputs.first().copied().ok_or_else(|| {
            "the candidate did not register onto the source, so the two are not in one grid"
                .to_owned()
        }),
        Err(why) => Err(format!("align: {why}")),
    };
    let aligned = match aligned {
        Ok(aligned) => aligned,
        Err(why) => {
            return Ok(Tried {
                align: align_run,
                tool: None,
                measured: Err(why),
                outputs: Vec::new(),
            });
        }
    };
    let call = if declared.file {
        let mut call = Call::new(&declared.tool)
            .input("source", source)
            .input("candidate", aligned);
        for (name, value) in arguments {
            call = call.argument(name, value.clone());
        }
        call
    } else {
        whizbang_squint::details::compare_of(&declared.tool, source, aligned)
    };
    let (run, outputs) = ran(executor, call)?;
    let outputs = match outputs {
        Ok(outputs) => outputs,
        Err(why) => {
            return Ok(Tried {
                align: align_run,
                tool: Some(run),
                measured: Err(format!("{}: {why}", declared.tool)),
                outputs: Vec::new(),
            });
        }
    };
    let measured =
        door::diagnostic(root, &run, &declared.report).and_then(|bytes| measurements(&bytes));
    Ok(Tried {
        align: align_run,
        tool: Some(run),
        measured,
        outputs,
    })
}

/// The measurements a run's diagnostic states: each its name, value and unit.
fn measurements(bytes: &[u8]) -> Result<Vec<(String, f64, String)>, String> {
    let read: Json =
        serde_json::from_slice(bytes).map_err(|_| "the diagnostic is not JSON".to_owned())?;
    let listed = read["measurements"]
        .as_array()
        .ok_or("the diagnostic lists no measurements")?;
    listed
        .iter()
        .map(|m| {
            let name = m["name"].as_str().ok_or("a measurement has no name")?;
            let value = m["value"].as_f64().ok_or("a measurement has no value")?;
            let unit = m["unit"].as_str().unwrap_or_default();
            Ok((name.to_owned(), value, unit.to_owned()))
        })
        .collect()
}

/// `whizbang try`: what to print of one read.
pub fn tried_text(declared: &Declared, tried: &Tried) -> Result<String, String> {
    let mut text = format!("run {} align\n", printable(tried.align.as_str()));
    if let Some(run) = &tried.tool {
        text.push_str(&format!(
            "run {} {}\n",
            printable(run.as_str()),
            declared.tool
        ));
    }
    match &tried.measured {
        Ok(measured) => {
            for (name, value, unit) in measured {
                text.push_str(&format!(
                    "{} {value} {}\n",
                    printable(name),
                    printable(unit)
                ));
            }
            for output in &tried.outputs {
                text.push_str(&format!("map {output}\n"));
            }
            Ok(text)
        }
        Err(why) => Err(format!("{text}no read: {}", printable(why))),
    }
}

/// The source and the candidate of an item's job, as the store holds them.
fn pair_of(
    set: &str,
    item: &str,
    job: &Job,
    held: &BTreeMap<Digest, ArtifactId>,
) -> Result<(ArtifactId, ArtifactId), String> {
    let find = |role: Role| -> Result<ArtifactId, String> {
        let image = job
            .images
            .iter()
            .find(|image| image.role == role)
            .ok_or_else(|| format!("{set}: item {item} has no image of the role {role:?}"))?;
        let pin = image
            .source
            .as_ref()
            .and_then(|source| source.sha256.as_deref());
        pin.and_then(|pin| Digest::from_hex(&pin.to_ascii_lowercase()))
            .and_then(|digest| held.get(&digest).copied())
            .ok_or_else(|| {
                format!(
                    "{set}: the picture of item {item} image {} is not in the store: fetch the \
                     set first (`whizbang eval sets fetch --set {set}`)",
                    image.id
                )
            })
    };
    Ok((find(Role::Source)?, find(Role::Candidate)?))
}

/// One item to read: its set, its id, its family, its label, its pair, and the arguments
/// a tool file is given for it.
type Planned = (
    String,
    String,
    String,
    Expected,
    (ArtifactId, ArtifactId),
    Vec<(String, Value)>,
);

/// The seed of the halves and of the bootstrap, and its count of resamples.
pub const SEED: u64 = 20_261_118;
pub const RESAMPLES: usize = 2_000;

/// `whizbang eval tool`: the tool's read of each item of each set, each a run, scored
/// against the sets' labels. A set without labels (a blind set) is refused; a picture the
/// store does not hold is refused before anything runs, naming the set to fetch.
pub fn eval_tool(
    executor: &dyn Executor,
    root: &Path,
    sets_root: &Path,
    declared: &Declared,
    ids: &[String],
    resamples: usize,
) -> Result<(Scored, String), String> {
    let held = import::pictures_by_file(root)?;
    let mut planned: Vec<Planned> = Vec::new();
    for id in ids {
        let set = sets::find(sets_root, id)?;
        let Some(labels) = &set.labels else {
            return Err(format!("{}: has no labels to score against", printable(id)));
        };
        let expected: BTreeMap<&str, Expected> = labels
            .label
            .iter()
            .map(|label| (label.item.as_str(), label.expected))
            .collect();
        let family: BTreeMap<&str, &str> = set
            .manifest
            .item
            .iter()
            .map(|item| (item.id.as_str(), item.family.as_str()))
            .collect();
        for (item, text) in &set.jobs {
            let job = Job::from_toml(text).map_err(|why| format!("{id}: item {item}: {why}"))?;
            let pair = pair_of(id, item, &job, &held)?;
            let (Some(expected), Some(family)) =
                (expected.get(item.as_str()), family.get(item.as_str()))
            else {
                return Err(format!("{id}: item {item} has no label or no family"));
            };
            // A tool file that takes the job is given it whole: its requirements, never
            // the item's label, which is the set's and not the job's.
            let mut arguments = if declared.takes_job {
                vec![("job".to_owned(), Value::text(&job.to_json()))]
            } else {
                Vec::new()
            };
            // And what the rules of its use case hint to the tool (plan mE, task T1, item
            // 4), as the arguments of text it declares under those names.
            if let Some(rules) = whizbang_squint::use_case::rules(&job.use_case) {
                arguments.extend(hinted_arguments(declared, &rules.hints));
            }
            planned.push((
                id.clone(),
                item.clone(),
                (*family).to_owned(),
                *expected,
                pair,
                arguments,
            ));
        }
    }
    let mut reads = Vec::new();
    let mut lines = String::new();
    for (set, item, family, expected, (source, candidate), arguments) in planned {
        let tried = read_pair_with(executor, root, declared, source, candidate, &arguments)?;
        let value = tried.primary(declared);
        let shown = match &value {
            Ok(value) => format!("{value}"),
            Err(_) => "error".to_owned(),
        };
        let run = tried.tool.as_ref().unwrap_or(&tried.align);
        lines.push_str(&format!(
            "item {}/{} {:?} {} {shown} run {}\n",
            printable(&set),
            printable(&item),
            expected,
            declared.primary,
            printable(run.as_str())
        ));
        reads.push(Read {
            set,
            item,
            family,
            expected,
            value,
        });
    }
    let scored = score(&reads, declared.changed, SEED, resamples)?;
    let mut text = lines;
    text.push_str(&format!(
        "{} {} ({} is changed): {} items ({} fail, {} pass) in {} families\n",
        declared.tool,
        declared.primary,
        match declared.changed {
            Changed::Larger => "larger",
            Changed::Smaller => "smaller",
        },
        scored.items,
        scored.fail_items,
        scored.pass_items,
        scored.families
    ));
    text.push_str(&format!(
        "auc {:.4} (95% {:.4} to {:.4}, {} resamples by family, seed {})\n",
        scored.auc, scored.auc_lower, scored.auc_upper, scored.resamples, scored.seed
    ));
    match &scored.accuracy {
        Accuracy::Inconclusive { why } => {
            text.push_str(&format!("balanced accuracy inconclusive: {why}\n"));
        }
        Accuracy::Read {
            threshold,
            balanced_accuracy,
            chosen_accuracy,
            chosen_on,
            read_on,
        } => text.push_str(&format!(
            "balanced accuracy {balanced_accuracy:.4} at {threshold} (chosen on {} families at \
             {chosen_accuracy:.4}, read on {})\n",
            chosen_on.len(),
            read_on.len()
        )),
    }
    for error in &scored.errors {
        text.push_str(&format!(
            "error {}/{}: {}\n",
            printable(&error.set),
            printable(&error.item),
            printable(&error.why)
        ));
    }
    Ok((scored, text))
}
