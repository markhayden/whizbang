//! What the page is told of each task and tool, and how what it sends becomes a call (plan
//! m5, task W1, items 2 and 4): the kits of the description with their tasks, each task's
//! form built from its descriptor alone, so a kit added to the registry shows in the page
//! with no change to the interface (`SPEC.md` section 1).
//!
//! **A form from a descriptor.** Each argument by its type (text, number or integer with
//! its bounds, boolean, a choice), and an argument that chooses the backend of a
//! capability (`Requirement::chosen_by`) as a choice of the config's backends of that
//! capability; each input port by its type and count: a picture, an ordered list of
//! pictures held to the port's count, a location drawn on its picture, or any other
//! artifact by its type. What a recipe takes to run from a step and until one (`set`,
//! `from`, `until`, `from_*`) is the runner's own and is left out. `segment`'s query is
//! filled from the canvas: a click is a point, a drag a box.
//!
//! **Squint's use cases** take their pictures and job as `whizbang squint check` builds
//! them (ADR 0028): the candidate, the source where the use case takes one, references in
//! order where it takes them, and the brief; the host makes the `job/1` from them as the
//! command line's shorthand does (`door::shorthand`, `door::check_call`). No job file is
//! taken from the page.

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json, json};
use whizbang_core::artifact::{ArtifactId, ArtifactType};
use whizbang_core::contract::{ActionDescriptor, Call, Kind, ResultKind, ValueType};
use whizbang_core::description::Kit;
use whizbang_core::identity::Digest;
use whizbang_core::recipe::{FROM, SET, UNTIL};
use whizbang_squint::job::Role;

use crate::door;

/// The arguments a run of a recipe takes from the runner, never from a form.
const RUNNER_ARGUMENTS: [&str; 6] = [SET, FROM, UNTIL, "recipe_digest", "cell", "measured"];

/// The kit a recipe that names none belongs to: rascal's runner runs a user's recipe
/// (`whizbang rascal run`).
pub const DEFAULT_KIT: &str = "rascal";

/// Whether the action is one of squint's use cases, whose form is the shorthand's.
pub fn is_use_case(name: &str) -> bool {
    whizbang_squint::use_case::NAMES.contains(&name)
}

/// The kit a task belongs to: the one it names, or [`DEFAULT_KIT`] for a recipe or a
/// loop that names none. A tool (a primitive) names none and is in no kit.
pub fn kit_of(descriptor: &ActionDescriptor) -> Option<&str> {
    match (descriptor.kit(), descriptor.kind()) {
        (Some(kit), _) => Some(kit),
        (None, Kind::Primitive) => None,
        (None, _) => Some(DEFAULT_KIT),
    }
}

/// A field's words for a person: the name with its underscores as spaces, its first
/// letter upper case; and a few names said as the design says them.
fn label(name: &str) -> String {
    let said = match name {
        "image" => "Picture",
        "prompt" => "What to remove",
        "backend" => "Model",
        "filler" => "Filler",
        "protect" => "Keep out",
        "location" => "Where",
        "stroke" => "Stroke",
        "candidate" => "Edited picture",
        "source" => "Source picture",
        "references" => "References",
        "brief" => "What should be true",
        "query" => "What to find",
        _ => "",
    };
    if !said.is_empty() {
        return said.to_owned();
    }
    let spaced = name.replace('_', " ");
    let mut letters = spaced.chars();
    match letters.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + letters.as_str(),
        None => String::new(),
    }
}

fn field(name: &str, kind: &str, required: bool, purpose: &str) -> Map<String, Json> {
    let mut field = Map::new();
    field.insert("name".to_owned(), json!(name));
    field.insert("label".to_owned(), json!(label(name)));
    field.insert("purpose".to_owned(), json!(purpose));
    field.insert("kind".to_owned(), json!(kind));
    field.insert("required".to_owned(), json!(required));
    for empty in [
        "default",
        "min",
        "max",
        "choices",
        "capability",
        "artifact",
        "count",
        "draw",
        "on",
        "canvas",
    ] {
        field.insert(empty.to_owned(), Json::Null);
    }
    field
}

/// The form of a squint use case: the shorthand's fields.
fn use_case_fields(name: &str) -> Vec<Json> {
    let roles = whizbang_squint::use_case::rules(name)
        .map(|rules| rules.rules.requires_roles)
        .unwrap_or_default();
    let mut fields = Vec::new();
    let mut candidate = field("candidate", "picture", true, "The picture that is checked.");
    candidate.insert("artifact".to_owned(), json!("image"));
    fields.push(Json::Object(candidate));
    if roles.contains(&Role::Source) {
        let mut source = field(
            "source",
            "picture",
            true,
            "The picture the checked one was made from.",
        );
        source.insert("artifact".to_owned(), json!("image"));
        fields.push(Json::Object(source));
    }
    if roles.contains(&Role::Reference) {
        let mut references = field(
            "references",
            "pictures",
            true,
            "Pictures of the subject, in order, that the checked one is held to.",
        );
        references.insert("artifact".to_owned(), json!("image"));
        references.insert(
            "count".to_owned(),
            json!({"min": 1, "max": door::REFERENCES_LIMIT}),
        );
        fields.push(Json::Object(references));
    }
    let briefed = name == "brief-compliance";
    fields.push(Json::Object(field(
        "brief",
        "text",
        briefed,
        "What the picture should hold, in words.",
    )));
    let mut judge = field(
        "judge",
        "backend",
        false,
        "The judge, by its name in the config.",
    );
    judge.insert("capability".to_owned(), json!("judge"));
    fields.push(Json::Object(judge));
    fields
}

/// The form of any other action, from its descriptor.
fn descriptor_fields(descriptor: &ActionDescriptor) -> Vec<Json> {
    let mut fields = Vec::new();
    let chooses: BTreeMap<&str, &str> = descriptor
        .needs()
        .iter()
        .filter_map(|need| Some((need.chooser()?, need.capability().name())))
        .collect();
    let runs_from = descriptor.argument(FROM).is_some();
    let first_image = descriptor
        .inputs()
        .iter()
        .find(|port| port.artifact() == ArtifactType::Image)
        .map(|port| port.name().to_owned());
    // `segment`'s query: a phrase, a point or a box, said by its argument `kind`.
    let canvas_query = descriptor
        .argument("kind")
        .is_some_and(|kind| match kind.value_type() {
            ValueType::Choice { choices } => {
                choices.iter().any(|c| c == "point") && choices.iter().any(|c| c == "box")
            }
            _ => false,
        })
        && descriptor.argument("query").is_some();
    for port in descriptor.inputs() {
        if runs_from && port.name().starts_with("from_") {
            continue;
        }
        let count = port.count();
        let required = count.min() > 0;
        let kind = match (port.artifact(), count.max()) {
            (ArtifactType::Image, Some(1)) => "picture",
            (ArtifactType::Image, _) => "pictures",
            _ => "artifact",
        };
        let mut made = field(port.name(), kind, required, port.purpose());
        made.insert("artifact".to_owned(), json!(port.artifact().name()));
        if count.max() != Some(1) {
            made.insert(
                "count".to_owned(),
                json!({"min": count.min(), "max": count.max()}),
            );
        }
        if port.artifact() == ArtifactType::Location {
            let draw = match port.name() {
                "stroke" => json!(["stroke"]),
                "line" => json!(["line"]),
                _ => json!(["box", "polygon"]),
            };
            made.insert("draw".to_owned(), draw);
            let on = port
                .belongs_to()
                .map(str::to_owned)
                .or_else(|| first_image.clone());
            made.insert("on".to_owned(), json!(on));
        }
        fields.push(Json::Object(made));
    }
    for argument in descriptor.arguments() {
        let name = argument.name();
        if RUNNER_ARGUMENTS.contains(&name) {
            continue;
        }
        let required = argument.is_required();
        let mut made = match (chooses.get(name), argument.value_type()) {
            (Some(capability), _) => {
                let mut made = field(name, "backend", required, argument.purpose());
                made.insert("capability".to_owned(), json!(capability));
                made
            }
            (None, ValueType::Number { min, max }) => {
                let mut made = field(name, "number", required, argument.purpose());
                made.insert("min".to_owned(), json!(min));
                made.insert("max".to_owned(), json!(max));
                made
            }
            (None, ValueType::Integer { min, max }) => {
                let mut made = field(name, "integer", required, argument.purpose());
                made.insert("min".to_owned(), json!(min));
                made.insert("max".to_owned(), json!(max));
                made
            }
            (None, ValueType::Boolean) => field(name, "boolean", required, argument.purpose()),
            (None, ValueType::Text) => field(name, "text", required, argument.purpose()),
            (None, ValueType::Choice { choices }) => {
                let mut made = field(name, "choice", required, argument.purpose());
                made.insert("choices".to_owned(), json!(choices));
                made
            }
        };
        if let Some(default) = argument.default() {
            made.insert("default".to_owned(), value_json(default));
        }
        if canvas_query && name == "query" {
            made.insert("canvas".to_owned(), json!("point_or_box"));
            made.insert("on".to_owned(), json!(first_image));
        }
        fields.push(Json::Object(made));
    }
    fields
}

fn value_json(value: &whizbang_core::contract::Value) -> Json {
    use whizbang_core::contract::Value;
    match value {
        Value::Number(number) => json!(number),
        Value::Integer(number) => json!(number),
        Value::Boolean(boolean) => json!(boolean),
        Value::Text(text) => json!(text),
    }
}

/// The form of an action: a use case's shorthand, or its descriptor's.
pub fn fields_of(descriptor: &ActionDescriptor) -> Vec<Json> {
    match is_use_case(descriptor.name()) {
        true => use_case_fields(descriptor.name()),
        false => descriptor_fields(descriptor),
    }
}

/// Each field of the form that chooses a backend, with its capability's name.
pub fn choosers_of(descriptor: &ActionDescriptor) -> Vec<(String, String)> {
    fields_of(descriptor)
        .iter()
        .filter(|field| field["kind"] == "backend")
        .filter_map(|field| {
            Some((
                field["name"].as_str()?.to_owned(),
                field["capability"].as_str()?.to_owned(),
            ))
        })
        .collect()
}

/// What a task takes and gives, in words: "image + words → image".
fn shape_of(descriptor: &ActionDescriptor, fields: &[Json]) -> String {
    let takes: Vec<&str> = fields
        .iter()
        .filter(|field| field["required"] == true)
        .map(
            |field| match (field["kind"].as_str(), field["artifact"].as_str()) {
                (Some("picture"), _) => "image",
                (Some("pictures"), _) => "images",
                (_, Some("location")) => "place",
                (_, Some("mask")) => "mask",
                (Some("artifact"), _) => "artifact",
                _ => "words",
            },
        )
        .collect();
    let mut gives: Vec<&str> = descriptor
        .outputs()
        .iter()
        .map(|port| port.artifact().name())
        .take(1)
        .collect();
    if descriptor.answers_with(ResultKind::Verdict) {
        gives = vec!["verdict"];
    }
    let mut deduped: Vec<&str> = Vec::new();
    for word in takes {
        if !deduped.contains(&word) {
            deduped.push(word);
        }
    }
    format!("{} → {}", deduped.join(" + "), gives.join(" + "))
}

/// One task or tool as the page is told of it.
pub fn task_json(descriptor: &ActionDescriptor, kit: Option<&str>, tools: &[String]) -> Json {
    let fields = fields_of(descriptor);
    let kind = match descriptor.kind() {
        Kind::Primitive => "primitive",
        Kind::Recipe => "recipe",
        Kind::Loop => "loop",
    };
    let outputs: Vec<Json> = descriptor
        .outputs()
        .iter()
        .map(|port| json!({"name": port.name(), "type": port.artifact().name()}))
        .collect();
    json!({
        "name": descriptor.name(),
        "kit": kit,
        "title": descriptor.title().unwrap_or(descriptor.name()),
        "kind": kind,
        "purpose": descriptor.purpose(),
        "use": descriptor.use_when(),
        "uses": descriptor.uses(),
        "shape": shape_of(descriptor, &fields),
        "tools": tools,
        "fields": fields,
        "outputs": outputs,
        "plans": descriptor.plans(),
    })
}

/// The kit a task is listed in: its own, when the build lists that kit; else
/// [`DEFAULT_KIT`], so that a user's recipe naming a kit the build does not have is never
/// left out. A tool is listed in none.
pub fn listed_in<'a>(descriptor: &'a ActionDescriptor, kits: &[Kit]) -> Option<&'a str> {
    let kit = kit_of(descriptor)?;
    match kits.iter().any(|listed| listed.name() == kit) {
        true => Some(kit),
        false => Some(DEFAULT_KIT),
    }
}

/// The kits of the description, each with its tasks; then every tool, each in the kit of
/// the first task whose steps call it, or in none.
pub fn kits_json(
    kits: &[Kit],
    descriptors: &[ActionDescriptor],
    steps: &BTreeMap<String, Vec<String>>,
) -> (Json, Json) {
    let tools_of = |name: &str| steps.get(name).cloned().unwrap_or_default();
    let mut listed = Vec::new();
    let mut tool_kit: BTreeMap<String, String> = BTreeMap::new();
    let mut ordered: Vec<&ActionDescriptor> = descriptors
        .iter()
        .filter(|d| d.kind() != Kind::Primitive)
        .collect();
    ordered.sort_by_key(|d| match d.kind() {
        Kind::Recipe => 0,
        _ => 1,
    });
    for kit in kits {
        let tasks: Vec<Json> = ordered
            .iter()
            .filter(|d| listed_in(d, kits) == Some(kit.name()))
            .map(|d| {
                for tool in tools_of(d.name()) {
                    tool_kit
                        .entry(tool)
                        .or_insert_with(|| kit.name().to_owned());
                }
                task_json(d, Some(kit.name()), &tools_of(d.name()))
            })
            .collect();
        listed.push((kit, tasks));
    }
    let tools: Vec<Json> = descriptors
        .iter()
        .filter(|d| d.kind() == Kind::Primitive)
        .map(|d| task_json(d, tool_kit.get(d.name()).map(String::as_str), &[]))
        .collect();
    let kits: Vec<Json> = listed
        .into_iter()
        .map(|(kit, tasks)| {
            let count = tool_kit.values().filter(|k| *k == kit.name()).count();
            json!({"id": kit.name(), "name": kit.title(), "line": kit.line(),
                   "tools": count, "tasks": tasks})
        })
        .collect();
    (Json::Array(kits), Json::Array(tools))
}

/// An artifact named by its identity in what the page sent.
fn identity_of(value: &Json, what: &str) -> Result<ArtifactId, String> {
    value
        .as_str()
        .and_then(|text| text.parse::<Digest>().ok())
        .map(ArtifactId)
        .ok_or_else(|| {
            format!(
                "{what} is no picture of the store: give it a picture from the library or a drop"
            )
        })
}

/// The call of a use case from its form, as `whizbang squint check` makes it from its
/// shorthand: the candidate, the source, the references in order, the brief and the
/// judge. A field the form does not have is refused.
///
/// With `job`, the job the call `job_of` names gives (plan m5, task W2, item 3: the job the
/// loop writes from a removal's phrase, or a check's own job made again from its run, its
/// example or its bundle's step; `play_result::JobOf`), the pictures fill its images by
/// their roles, in its order, and no brief is taken beside it.
pub fn use_case_call(
    use_case: &str,
    arguments: &Map<String, Json>,
    job: Option<&whizbang_squint::job::Job>,
) -> Result<Call, String> {
    const FIELDS: [&str; 6] = [
        "candidate",
        "source",
        "references",
        "brief",
        "judge",
        "job_of",
    ];
    if let Some(other) = arguments
        .keys()
        .find(|name| !FIELDS.contains(&name.as_str()))
    {
        let _ = other;
        return Err("a field was given that this task does not take".to_owned());
    }
    let Some(candidate) = arguments.get("candidate") else {
        return Err("give the picture to check".to_owned());
    };
    let candidate = identity_of(candidate, "the picture to check")?;
    let source = match arguments.get("source") {
        None | Some(Json::Null) => None,
        Some(given) => Some(identity_of(given, "the source picture")?),
    };
    let references: Vec<ArtifactId> = match arguments.get("references") {
        None | Some(Json::Null) => Vec::new(),
        Some(Json::Array(given)) => given
            .iter()
            .map(|one| identity_of(one, "a reference"))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err("references are a list of pictures".to_owned()),
    };
    let text = |name: &str| match arguments.get(name) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::String(text)) if text.trim().is_empty() => Ok(None),
        Some(Json::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(format!("{name} is words")),
    };
    let brief = text("brief")?;
    let judge = text("judge")?;
    if let Some(job) = job {
        if brief.is_some() {
            return Err("a check given a run's job takes no brief beside it".to_owned());
        }
        let mut references = references.iter();
        let mut images = Vec::new();
        for image in &job.images {
            let given = match image.role {
                Role::Candidate => Some(candidate),
                Role::Source => source,
                Role::Reference => references.next().copied(),
            };
            let Some(given) = given else {
                return Err(format!(
                    "the job names a {} picture, and none was given for it",
                    image.role.word()
                ));
            };
            images.push(given);
        }
        if references.next().is_some()
            || (source.is_some() && !job.images.iter().any(|i| i.role == Role::Source))
        {
            return Err("more pictures were given than the job names".to_owned());
        }
        let measuring = door::Measuring::stated(None, None)?;
        return Ok(door::check_call(job, &images, judge.as_deref(), &measuring));
    }
    let candidate = candidate.to_string();
    let source = source.map(|id| id.to_string());
    let references: Vec<String> = references.iter().map(ToString::to_string).collect();
    let references: Vec<&str> = references.iter().map(String::as_str).collect();
    let measuring = door::Measuring::stated(None, None)?;
    let (job, images) = door::shorthand(&door::CheckAsked {
        use_case,
        candidate: &candidate,
        source: source.as_deref(),
        references: &references,
        judge: judge.as_deref(),
        brief: brief.as_deref(),
        request_id: None,
        details: measuring.details,
        detail: None,
        projection: measuring.projection,
    })?;
    Ok(door::check_call(
        &job,
        &images,
        judge.as_deref(),
        &measuring,
    ))
}
