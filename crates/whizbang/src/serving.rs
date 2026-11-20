//! What `whizbang serve` offers over the MCP server (`whizbang_serve`; plan 2C, task
//! M1; ADR 0031): each action of the registry as a tool, `check` in the shape a caller
//! of a judge asks in, and what is said of a run that ended, as the command line says
//! it ([`crate::door`]). Every call is a call of an action, which the server hands the
//! executor's entry point; nothing here runs anything.
//!
//! `check` takes the use case, its images by their identities in the store with their
//! roles (and a reference's label), the brief, and the judge by its name; the job is
//! made from them as the command line's shorthand makes one, references named
//! `reference-1`, `reference-2`, … in the order given. A picture given inline is not
//! taken: a picture comes into the store by `whizbang store import`, and is named here
//! by the identity that prints.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Map, Value as Json, json};
use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::record::JOURNAL_NAME;
use whizbang_core::contract::{
    ActionDescriptor, Call, JournalLine, Kind, Outcome, RunId, read_journal,
};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_serve::tools::{self, REQUEST_ID, Tool};
use whizbang_serve::{Door, Said};
use whizbang_squint::job::{Job, JobImage, Role};

use crate::door::{self, Details, Measuring, REFERENCES_LIMIT, printable};
use crate::pack::OneWorker;

/// The actions of a registry, offered as tools, over the store at `root`.
pub struct Actions {
    descriptors: Vec<ActionDescriptor>,
    root: PathBuf,
    worker: Option<Arc<OneWorker>>,
}

impl Actions {
    /// The actions these descriptors describe, in their order, run against the store at
    /// `root`; `worker` is the pack's one worker of the process, stopped at the end.
    pub fn new(
        descriptors: Vec<ActionDescriptor>,
        root: PathBuf,
        worker: Option<Arc<OneWorker>>,
    ) -> Actions {
        Actions {
            descriptors,
            root,
            worker,
        }
    }
}

const CHECK: &str = whizbang_squint::check::NAME;

/// `check` as a tool: the shape a caller of a judge asks in.
pub fn check_tool() -> Tool {
    let image = json!({
        "type": "object",
        "properties": {
            "id": tools::identity_schema("The picture, by its identity in the store, as whizbang store import prints it."),
            "role": {"type": "string", "enum": ["candidate", "source", "reference"],
                     "description": "What the picture is for: exactly one candidate, at most one source, any number of references."},
            "label": {"type": "string", "maxLength": 200,
                      "description": "A reference's words, such as front of the box; reference N by default."},
        },
        "required": ["id", "role"],
        "additionalProperties": false,
    });
    Tool {
        name: CHECK.to_owned(),
        description: "Have the configured judge check a picture under a use case: pass, fail or unsure, with each item of its reason.".to_owned(),
        schema: json!({
            "type": "object",
            "properties": {
                "use_case": {"type": "string", "enum": whizbang_squint::use_case::NAMES,
                             "description": "The use case the picture is judged under."},
                "images": {"type": "array", "items": image, "minItems": 1,
                           "maxItems": REFERENCES_LIMIT + 2,
                           "description": "The pictures, in order, each with its role."},
                "brief": {"type": "string", "description": "The job's requirements in words. brief-compliance needs one."},
                "judge": {"type": "string", "description": "The judge, by its name in the config; without it the default."},
                "details": {"type": "string", "enum": Details::NAMES, "default": "none",
                            "description": "none: nothing is measured first, and the judge may call view and align; first: align registers the candidate onto the source and each reference before the judge looks, and the judge may call resize too."},
                REQUEST_ID: tools::request_id_schema(),
            },
            "required": ["use_case", "images"],
            "additionalProperties": false,
        }),
    }
}

fn text<'a>(arguments: &'a Map<String, Json>, name: &str) -> Result<Option<&'a str>, String> {
    match arguments.get(name) {
        None => Ok(None),
        Some(Json::String(text)) => Ok(Some(text)),
        Some(_) => Err(format!("{name} is a string")),
    }
}

/// A call of `check` from the tool's arguments, and the request id; or why not, in
/// words that say nothing that was sent.
pub fn check_call(arguments: &Map<String, Json>) -> Result<(Call, Option<String>), String> {
    const TAKEN: [&str; 6] = [
        "use_case", "images", "brief", "judge", "details", REQUEST_ID,
    ];
    if arguments.keys().any(|key| !TAKEN.contains(&key.as_str())) {
        return Err(
            "an argument was given that check does not take; its schema lists those it takes"
                .to_owned(),
        );
    }
    let use_case = text(arguments, "use_case")?
        .filter(|name| whizbang_squint::use_case::NAMES.contains(name))
        .ok_or_else(|| {
            format!(
                "use_case is one of {}",
                whizbang_squint::use_case::NAMES.join(", ")
            )
        })?;
    let Some(Json::Array(given)) = arguments.get("images") else {
        return Err("images is an array of pictures, each with its id and role".to_owned());
    };
    let most = REFERENCES_LIMIT + 2;
    if given.is_empty() || given.len() > most {
        return Err(format!("images holds 1 to {most} pictures"));
    }
    let (mut images, mut ids, mut references) = (Vec::new(), Vec::new(), 0);
    for (at, picture) in given.iter().enumerate() {
        let not = || {
            format!(
                "images {}: a picture is its id, its role and perhaps a label",
                at + 1
            )
        };
        let Some(picture) = picture.as_object() else {
            return Err(not());
        };
        if picture
            .keys()
            .any(|key| !["id", "role", "label"].contains(&key.as_str()))
        {
            return Err(not());
        }
        let id: ArtifactId = picture.get("id").and_then(tools::identity).ok_or_else(|| {
            format!(
                "images {}: id is no artifact identity: an identity is sha256: and 64 lower-case hex digits",
                at + 1
            )
        })?;
        let label = match picture.get("label") {
            None => None,
            Some(Json::String(label)) if label.chars().count() <= 200 => Some(label.clone()),
            Some(_) => return Err(not()),
        };
        let (name, role) = match picture.get("role").and_then(Json::as_str) {
            Some("candidate") => ("candidate".to_owned(), Role::Candidate),
            Some("source") => ("source".to_owned(), Role::Source),
            Some("reference") => {
                references += 1;
                (format!("reference-{references}"), Role::Reference)
            }
            _ => return Err(not()),
        };
        let label = match role {
            Role::Reference => label.or_else(|| Some(format!("reference {references}"))),
            _ => label,
        };
        images.push(JobImage {
            id: name,
            role,
            label,
            source: None,
            projection: None,
        });
        ids.push(id);
    }
    let brief = text(arguments, "brief")?;
    let job = Job {
        use_case: use_case.to_owned(),
        brief: brief.map(str::to_owned),
        requirements: Vec::new(),
        images,
    };
    job.validate().map_err(|why| printable(&why))?;
    let briefed = brief.is_some_and(|brief| !brief.trim().is_empty());
    if use_case == "brief-compliance" && !briefed {
        return Err("brief-compliance holds the picture to a brief: give it as brief".to_owned());
    }
    // With nothing stated the call is the control arm, whole, as every door's is: no
    // details, `view` and `align` offered, a call of `align` answered with its pictures
    // alone. `first` is arm F, whole.
    let measuring = Measuring::stated(text(arguments, "details")?, None)?;
    let call = door::check_call(&job, &ids, text(arguments, "judge")?, &measuring);
    let request_id = text(arguments, REQUEST_ID)?.map(str::to_owned);
    Ok((call, request_id))
}

impl Door for Actions {
    fn tools(&self) -> Vec<Tool> {
        // The recipes first, then the loops, then the primitives, each kind in the order
        // the registry lists them: what a description lists (ADR 0033).
        let mut descriptors: Vec<&ActionDescriptor> = self.descriptors.iter().collect();
        descriptors.sort_by_key(|descriptor| match descriptor.kind() {
            Kind::Recipe => 0,
            Kind::Loop => 1,
            Kind::Primitive => 2,
        });
        descriptors
            .into_iter()
            .map(|descriptor| match descriptor.name() {
                CHECK => check_tool(),
                _ => tools::tool_of(descriptor),
            })
            .collect()
    }

    fn call(
        &self,
        name: &str,
        arguments: &Map<String, Json>,
    ) -> Result<(Call, Option<String>), String> {
        if name == CHECK {
            return check_call(arguments);
        }
        let descriptor = self.descriptors.iter().find(|d| d.name() == name);
        let descriptor = descriptor.ok_or("the server offers no tool of that name")?;
        tools::call_of(descriptor, arguments)
    }

    fn said(&self, name: &str, run: &RunId, outcome: &Outcome) -> Said {
        let line = format!("run {}", printable(run.as_str()));
        // The plan the run was admitted under, for a call its action planned (ADR
        // 0004, "A call's plan"): said with every answer, whatever the outcome.
        let plan = door::plan_of(&self.root, run).map(|plan| printable(&plan));
        let planned = |text: String| match &plan {
            Some(plan) => format!("{plan}\n{text}"),
            None => text,
        };
        match door::ended(&line, outcome.clone()) {
            Ok((text, outputs)) => {
                let mut text = planned(text);
                if name == CHECK {
                    door::said_verdict(&mut text, door::verdict(&self.root, run));
                }
                for output in outputs {
                    let port = printable(output.port());
                    text.push_str(&format!("{port} {}\n", output.id().0));
                }
                Said { text, error: false }
            }
            Err(text) => Said {
                text: planned(text),
                error: true,
            },
        }
    }

    fn journal(&self, run: &RunId) -> Vec<JournalLine> {
        let read = || {
            let reader = Reader::open(&self.root, Arc::new(RealFs)).ok()?;
            let generation = reader.snapshot().ok()?;
            let entry = generation.run(run.as_str())?.get(JOURNAL_NAME)?;
            read_journal(&reader.read(entry).ok()?).ok()
        };
        read().unwrap_or_default()
    }

    fn closing(&self) {
        if let Some(worker) = &self.worker {
            worker.kill();
        }
    }
}
