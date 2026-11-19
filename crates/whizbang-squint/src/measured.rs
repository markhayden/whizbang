//! What is measured before the judge's first turn, as a step of its own (plan m3, task Q1,
//! item 1; ADR 0035): the action `details`, the first step of each use case's recipe.
//!
//! The owner's goal stands (ADR 0029): squint hands the judge every detail it can measure,
//! and the judge makes the final conclusion. `details` calls `align` for each pair the use
//! case relates, each comparison the run names on each pair that registered, and `view` for
//! the tiles of a job with one image, each as a nested call of its own run; and it hands back
//! in order the images they made (each aligned frame, each comparison's map, each tile) at
//! its output `images`, and what each found, or why it found nothing, as its measurements:
//! what each tool's own run handed back (`align`'s homography, residual and quality, a
//! comparison's numbers), never worked out again. The recipe binds both to the judge's step,
//! which says them to the judge as `check` said its own details before.
//!
//! `check` called directly with `details = first` measures through [`measure`] too, as
//! nested calls of its own run, so the two say the same.

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use whizbang_core::artifact::{ArtifactId, ArtifactType};
use whizbang_core::contract::record::{ArgumentValue, DIAGNOSTICS_PREFIX, RunRecord, Status};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Call, CallError, Context, Count, DescriptorError, Failure,
    Finished, InputPort, Kind, Outcome, OutputPort, PreparedCall, Produced, ResultKind, RunId,
    Value, ValueType,
};

use crate::details::{self, Details};
use crate::job::{Job, Role};
use crate::{align, compare, use_case, view};

/// The image an identity names, as a text.
fn artifact_of(text: &str) -> Option<ArtifactId> {
    text.parse::<whizbang_core::identity::Digest>()
        .ok()
        .map(ArtifactId)
}

/// The action's name: the first step of a use case's recipe.
pub const NAME: &str = "details";
/// The action's version.
pub const VERSION: &str = "1.0.0";
/// The diagnostic in which a run of `details` keeps the measurements it handed back whole.
pub const KEPT: &str = "measured.json";

/// The most images `details` hands back: an aligned frame and a map for each of nine
/// pairs, and four tiles, with room.
pub const IMAGES_LIMIT: u32 = 32;

/// One thing measured first: a tool's call on a pair, or one tile, with what its run
/// handed back or why it measured nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// `align`, `change_map`, `edge_similarity` or `tiles`.
    pub tool: String,
    /// The image measured, by the name of its role in the job (`candidate`).
    pub image: String,
    /// What it was measured against, for a pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub against: Option<String>,
    /// A tile's place and its box in fractions of the image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<[f64; 4]>,
    /// What the tool's run handed back as its measurements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measured: Option<Json>,
    /// Why nothing was measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
    /// The images the tool's run committed, by identity, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// The run of the tool that measured it, or that failed to: what a step handed it is
    /// held to (ADR 0035). None only for a comparison not made, because `align` gave its
    /// pair no frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
}

impl Entry {
    fn of(tool: &str, image: &str, against: Option<&str>) -> Entry {
        Entry {
            tool: tool.to_owned(),
            image: image.to_owned(),
            against: against.map(str::to_owned),
            place: None,
            bbox: None,
            measured: None,
            failed: None,
            images: Vec::new(),
            run: None,
        }
    }

    fn by(mut self, run: &RunId) -> Entry {
        self.run = Some(run.as_str().to_owned());
        self
    }

    fn failing(mut self, why: &str) -> Entry {
        self.failed = Some(why.to_owned());
        self
    }

    /// The images the entry names, as identities; an identity that does not parse is
    /// no image.
    pub fn image_ids(&self) -> Result<Vec<ArtifactId>, String> {
        (self.images.iter())
            .map(|id| artifact_of(id).ok_or_else(|| format!("{id:?} is no image's identity")))
            .collect()
    }
}

/// Everything measured first, in the order it is said: the measurements of `details`.
/// `run` is the run of `details` that made them, which a judge's step holds the whole
/// document to (ADR 0035); a document `check` makes for itself names none.
pub fn document(entries: &[Entry], run: Option<&str>) -> Json {
    match run {
        Some(run) => json!({ "run": run, "entries": entries }),
        None => json!({ "entries": entries }),
    }
}

/// A document of measurements read: the run that made it, and its entries; or why it is
/// not one.
pub fn read(text: &str) -> Result<(Option<String>, Vec<Entry>), String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Document {
        #[serde(default)]
        run: Option<String>,
        entries: Vec<Entry>,
    }
    let read: Document = serde_json::from_str(text)
        .map_err(|_| "the measurements handed on are not read".to_owned())?;
    Ok((read.run, read.entries))
}

/// The entries of a document of measurements, or why it is not one.
pub fn entries_of(text: &str) -> Result<Vec<Entry>, String> {
    read(text).map(|(_, entries)| entries)
}

/// One pair the use case relates: the image registered, onto the image it is held to, each
/// with the name of its role and its identity.
pub struct Pair {
    pub image: String,
    pub image_id: ArtifactId,
    pub against: String,
    pub against_id: ArtifactId,
}

/// The pairs a job relates, in its order: each candidate registered onto the source and
/// onto each reference; and its candidates, for tiles.
pub fn pairs_of(job: &Job, inputs: &[ArtifactId]) -> (Vec<Pair>, Vec<(String, ArtifactId)>) {
    let images: Vec<_> = job.images.iter().zip(inputs.iter().copied()).collect();
    let candidates: Vec<(String, ArtifactId)> = (images.iter())
        .filter(|(image, _)| image.role == Role::Candidate)
        .map(|(image, id)| (image.id.clone(), *id))
        .collect();
    let mut pairs = Vec::new();
    for (candidate, c) in &candidates {
        for (other, o) in &images {
            if other.role != Role::Candidate {
                pairs.push(Pair {
                    image: candidate.clone(),
                    image_id: *c,
                    against: other.id.clone(),
                    against_id: *o,
                });
            }
        }
    }
    (pairs, candidates)
}

/// One call made first, through the executor of `context`: its run, or why it measured
/// nothing. A refusal or a cancellation ends the run that made it.
fn first(context: &Context, call: &Call) -> Result<(RunId, Result<Finished, String>), Failure> {
    context.events().progress("a detail was measured");
    let finished = match context.executor().submit(call) {
        Ok(finished) => finished,
        Err(CallError::Refused(refusal)) => {
            return Err(Failure::error("details", &refusal.to_string()));
        }
        Err(other) => return Err(Failure::error("details", &other.to_string())),
    };
    let run = finished.run().clone();
    match finished.outcome() {
        Outcome::Completed { .. } => Ok((run, Ok(finished))),
        Outcome::Invalid { why } | Outcome::Error { message: why, .. } => {
            Ok((run, Err(why.clone())))
        }
        Outcome::Cancelled => Err(Failure::error("cancelled", "a detail's call was cancelled")),
    }
}

/// What a completed call handed back: its measurements, and the images it committed.
fn handed(finished: &Finished, tool: &str) -> Result<(Json, Vec<ArtifactId>), Failure> {
    let measured = finished
        .measurements()
        .cloned()
        .ok_or_else(|| Failure::error("details", &format!("{tool} handed back no measurements")))?;
    let ids = finished
        .outcome()
        .outputs()
        .iter()
        .map(|output| output.id());
    Ok((measured, ids.collect()))
}

/// Measures each pair with the tools of `detail`, in order, and cuts each of `tiles` into
/// the four tiles of [`details::TILE_BOXES`] when `detail` names them; each a nested call
/// through the executor of `context`. `align` comes first on a pair; a comparison then
/// compares the frame it returned with the image it was registered onto. A pair whose
/// call of `align` failed is said with why, and nothing more is measured on it; a pair that
/// did not register is compared with nothing, and each comparison says why.
pub fn measure(
    context: &Context,
    pairs: &[Pair],
    detail: &[&str],
    tiles: &[(String, ArtifactId)],
) -> Result<Vec<Entry>, Failure> {
    let mut entries = Vec::new();
    for pair in pairs {
        let (image, against) = (pair.image.as_str(), pair.against.as_str());
        let mut aligned: Result<ArtifactId, String> =
            Err(format!("{image} was not registered onto {against}"));
        if detail.contains(&align::NAME) {
            let call = Call::new(align::NAME)
                .input("image", pair.image_id)
                .input("against", pair.against_id);
            let (run, done) = first(context, &call)?;
            let entry = Entry::of(align::NAME, image, Some(against)).by(&run);
            let finished = match done {
                Ok(finished) => finished,
                Err(why) => {
                    entries.push(entry.failing(&why));
                    continue;
                }
            };
            let (measured, made) = handed(&finished, align::NAME)?;
            aligned = made.last().copied().ok_or_else(|| {
                format!("{image} did not register onto {against}, so the two are not in one grid")
            });
            entries.push(Entry {
                measured: Some(measured),
                images: made.iter().map(ToString::to_string).collect(),
                ..entry
            });
        }
        for tool in (detail.iter()).filter(|tool| details::AFTER_ALIGN.contains(tool)) {
            let entry = Entry::of(tool, image, Some(against));
            let aligned_id = match &aligned {
                Ok(id) => *id,
                Err(why) => {
                    entries.push(entry.failing(why));
                    continue;
                }
            };
            let call = details::compare_of(tool, pair.against_id, aligned_id);
            let (run, done) = first(context, &call)?;
            let entry = entry.by(&run);
            match done {
                Ok(finished) => {
                    let (measured, made) = handed(&finished, tool)?;
                    entries.push(Entry {
                        measured: Some(measured),
                        images: made.iter().map(ToString::to_string).collect(),
                        ..entry
                    });
                }
                Err(why) => entries.push(entry.failing(&why)),
            }
        }
    }
    if detail.contains(&details::TILES) {
        for (image, id) in tiles {
            for (place, bbox) in details::TILE_BOXES {
                let mut call = Call::new(view::NAME).input("image", *id);
                for (name, side) in view::SIDES.into_iter().zip(bbox) {
                    call = call.argument(name, Value::Number(side));
                }
                let entry = Entry {
                    place: Some(place.to_owned()),
                    bbox: Some(bbox),
                    ..Entry::of(details::TILES, image, None)
                };
                let (run, done) = first(context, &call)?;
                let entry = entry.by(&run);
                match done {
                    // The first output is the crop of the image; a second would be of `also`.
                    Ok(finished) => {
                        let made = finished.outcome().outputs().iter().take(1);
                        let images = made.map(|output| output.id().to_string()).collect();
                        entries.push(Entry { images, ..entry });
                    }
                    Err(why) => entries.push(entry.failing(&why)),
                }
            }
        }
    }
    Ok(entries)
}

/// The details a run names (`detail`), each once, in order, held to what can be measured
/// first and to what is on offer (`offered`): a comparison runs `align` first, named or
/// not; `tiles` is cut by `view`.
pub fn named_details<'a>(asked: &'a str, offered: &[String]) -> Result<Vec<&'a str>, Failure> {
    let mut detail: Vec<&str> = Vec::new();
    for name in asked.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        let tool = if name == details::TILES {
            view::NAME
        } else {
            name
        };
        let known = details::DETAILED.contains(&name) || name == details::TILES;
        if !known || !offered.iter().any(|o| o == tool) {
            let shown: String = name.chars().take(64).collect();
            return Err(Failure::invalid(&format!(
                "{shown:?} is no tool whose details are measured first: {}, {}",
                details::DETAILED.join(", "),
                details::TILES
            )));
        }
        if !detail.contains(&name) {
            detail.push(name);
        }
    }
    let compares = (detail.iter()).any(|name| details::AFTER_ALIGN.contains(name));
    if compares && !detail.contains(&align::NAME) {
        detail.insert(0, align::NAME);
    }
    Ok(detail)
}

/// The action `details`.
pub struct DetailsStep(ActionDescriptor);

impl DetailsStep {
    pub fn new() -> Result<DetailsStep, DescriptorError> {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Measures the details of a use case's images before its judge looks, and hands on what each tool found.",
        )
        .input(InputPort::variadic(
            "images",
            ArtifactType::Image,
            "The job's images, in its order.",
            Count::between(1, crate::check::IMAGES_LIMIT),
        ))
        .argument(Argument::required(
            "use_case",
            "The use case of the table whose pairs are measured.",
            ValueType::choice(&use_case::NAMES),
        ))
        .argument(Argument::required(
            "job",
            "The job in the contract job/1: its images in the order of the inputs, with their roles.",
            ValueType::Text,
        ))
        .argument(Argument::with_default(
            "details",
            "none, which measures nothing, or first.",
            ValueType::choice(&Details::NAMES),
            Value::text("first"),
        ))
        .argument(Argument::with_default(
            "detail",
            "The details measured, separated by commas: align and the two comparisons that run align first, for each pair; or tiles, for a job with one image.",
            ValueType::Text,
            Value::text(align::NAME),
        ))
        .output(OutputPort::counted(
            "images",
            ArtifactType::Image,
            "The images the details made, in the order the measurements name them: aligned frames, maps and tiles.",
            Count::between(0, IMAGES_LIMIT),
        ))
        .results(ResultKind::Measurements)
        .concurrent(true);
        Ok(DetailsStep(draft.build()?))
    }
}

impl Action for DetailsStep {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let text = |name: &str| {
            call.arguments().iter().find_map(|(n, value)| match value {
                Value::Text(text) if n == name => Some(text.as_str()),
                _ => None,
            })
        };
        let use_case = text("use_case").unwrap_or_default();
        let job =
            Job::from_json(text("job").unwrap_or_default()).map_err(|w| Failure::invalid(&w))?;
        if job.use_case != use_case {
            return Err(Failure::invalid(
                "the job is of another use case than the call",
            ));
        }
        let inputs: Vec<ArtifactId> = call.inputs().iter().map(|input| input.id()).collect();
        if job.images.len() != inputs.len() {
            return Err(Failure::invalid(&format!(
                "the job names {} images, and the call gives {}",
                job.images.len(),
                inputs.len()
            )));
        }
        let detail = match text("details") {
            Some("none") => Vec::new(),
            _ => {
                let offered: Vec<String> = [view::NAME, align::NAME]
                    .into_iter()
                    .chain(details::AFTER_ALIGN)
                    .map(str::to_owned)
                    .collect();
                named_details(text("detail").unwrap_or_default(), &offered)?
            }
        };
        let (pairs, candidates) = pairs_of(&job, &inputs);
        if detail.contains(&details::TILES) && job.images.iter().any(|i| i.role != Role::Candidate)
        {
            return Err(Failure::invalid(&details::tiles_refused(use_case)));
        }
        let calls = detail
            .iter()
            .map(|d| if *d == details::TILES { view::NAME } else { *d });
        context.freeze_calls(calls);
        let entries = measure(context, &pairs, &detail, &candidates)?;
        // The document names this run, whose calls it lists whole (ADR 0035).
        let made_by = context.lineage().last().map(|run| run.as_str().to_owned());
        let handed = document(&entries, made_by.as_deref());
        // And keeps it whole, for the judge's step to hold what it is handed to.
        let kept = context
            .diagnostics()
            .write(KEPT, handed.to_string().as_bytes());
        kept.map_err(|why| Failure::error("details", &why.to_string()))?;
        let mut produced = Produced::nothing().with_measurements(handed);
        for entry in &entries {
            for id in entry
                .image_ids()
                .map_err(|why| Failure::error("details", &why))?
            {
                let read = whizbang_core::artifact::get(
                    context.store(),
                    ArtifactType::Image,
                    id,
                    &whizbang_core::identity::DecodeLimits::default(),
                )
                .map_err(|error| Failure::error("details", &error.without_paths()))?;
                produced = produced.with("images", read);
            }
        }
        Ok(produced)
    }
}

/// What a run of a tool that measured kept of its measurements: its report, the same
/// document it handed back (`align`'s and the comparisons'; ADR 0035).
fn report_name(tool: &str) -> Option<&'static str> {
    match tool {
        align::NAME => Some(align::REPORT),
        compare::CHANGE_MAP => Some(compare::CHANGE_MAP_REPORT),
        compare::EDGE_SIMILARITY => Some(compare::EDGE_SIMILARITY_REPORT),
        _ => None,
    }
}

/// A pair as an entry names it: the image, and what it is held against.
type PairNames = (String, Option<String>);

/// Holds measurements an earlier step handed on to the runs that made them (ADR 0035), or
/// says why they are not taken. They are taken only by a run that is a step of a recipe
/// (the run nothing called, an MCP client or a command line, hands none), and each entry
/// only as the run it names left it, in this store: a completed run of its tool (or one
/// that failed, for an entry that says why), under a step of the same recipe as this run,
/// on the images this run was given (`images`) and handed (`handed`), that committed the
/// images the entry names and kept the measurements it says. An entry that names no run
/// is only a comparison of a pair whose run of `align`, named before it, gave no frame.
pub fn held_to_their_runs(
    context: &Context,
    made_by: Option<&str>,
    entries: &[Entry],
    job_text: &str,
    job: &Job,
    images: &[ArtifactId],
    handed: &[ArtifactId],
) -> Result<(), String> {
    let [.., recipe, _] = context.lineage() else {
        return Err(
            "measurements are handed on only by an earlier step of a recipe, from the \
                    runs that made them"
                .to_owned(),
        );
    };
    let store = context.store();
    let generation = store.snapshot().map_err(|error| error.to_string())?;
    let record_of = |run: &str| -> Result<RunRecord, String> {
        let read = whizbang_core::contract::record::stored(&generation, run, |entry| {
            store.read_record(entry)
        });
        match read {
            Some(Ok(record)) => Ok(record),
            _ => Err(format!(
                "the run {run:?} the measurements name is not in this store"
            )),
        }
    };
    // Every image an entry names was handed on with it.
    for entry in entries {
        if entry.image_ids()?.iter().any(|id| !handed.contains(id)) {
            return Err(
                "the measurements handed on name an image that was not handed on with \
                        them"
                    .to_owned(),
            );
        }
    }
    // The whole document: made by a run of `details` that is a step of this run's recipe,
    // whose every call, in order, is the run of one entry, and no other.
    let made_by =
        made_by.ok_or("the measurements handed on name no run of details that made them")?;
    let step = record_of(made_by)?;
    let completed = step.outcome().status == Status::Completed;
    if step.asked().action.as_deref() != Some(NAME)
        || step.identity().parent.as_deref() != Some(recipe.as_str())
        || !completed
    {
        return Err(
            "the measurements handed on were not made by a step of details of this \
                    run's recipe"
                .to_owned(),
        );
    }
    let called: Vec<&str> = step.steps().iter().map(|call| call.run()).collect();
    let named: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.run.as_deref())
        .collect();
    if called != named {
        return Err(
            "the measurements handed on are not every call the step of details made, \
                    in its order"
                .to_owned(),
        );
    }
    // Of this run's own job and images: the very job, and the same images in the same
    // order, as the step's record holds them.
    let asked_job = step.asked().arguments.iter().find(|a| a.name == "job");
    let same_job = matches!(asked_job.map(|a| &a.value),
        Some(ArgumentValue::Text(text)) if text == job_text);
    let measured_images: Vec<ArtifactId> = (step.inputs().iter())
        .filter(|input| input.port == "images")
        .map(|input| ArtifactId(input.artifact))
        .collect();
    if !same_job || measured_images != images {
        return Err(
            "the step of details measured another job or other images than this run's".to_owned(),
        );
    }
    // Tiles are of the candidate of a job with one image, as `details` cuts them.
    let paired = job.images.iter().any(|image| image.role != Role::Candidate);
    let tiled = entries.iter().filter(|entry| entry.tool == details::TILES);
    for entry in tiled {
        let candidate = (job.images.iter())
            .any(|image| image.id == entry.image && image.role == Role::Candidate);
        if paired || !candidate {
            return Err(
                "the measurements handed on hold tiles of no candidate of a job with \
                        one image"
                    .to_owned(),
            );
        }
    }
    // The job's images, by the names of their roles.
    let id_of = |name: &str| -> Result<ArtifactId, String> {
        (job.images.iter().zip(images))
            .find(|(image, _)| image.id == name)
            .map(|(_, id)| *id)
            .ok_or_else(|| format!("an entry names {name:?}, no image of the job"))
    };
    // The frame each verified run of align made, by its pair.
    let mut frames: Vec<(PairNames, Option<ArtifactId>)> = Vec::new();
    for entry in entries {
        let tool = entry.tool.as_str();
        let ids = entry.image_ids()?;
        let pair = (entry.image.clone(), entry.against.clone());
        let image_id = id_of(&entry.image)?;
        let Some(run) = &entry.run else {
            let image = &entry.image;
            let against = entry.against.as_deref().unwrap_or_default();
            let said =
                format!("{image} did not register onto {against}, so the two are not in one grid");
            let unaligned = frames
                .iter()
                .any(|(of, frame)| *of == pair && frame.is_none());
            let derived = details::AFTER_ALIGN.contains(&tool)
                && unaligned
                && ids.is_empty()
                && entry.measured.is_none()
                && entry.failed.as_deref() == Some(said.as_str());
            if !derived {
                return Err(format!("an entry of {tool} names no run that measured it"));
            }
            continue;
        };
        let wrong = |why: &str| format!("the run {run} an entry of {tool} names {why}");
        let record = record_of(run)?;
        let action = if tool == details::TILES {
            view::NAME
        } else {
            tool
        };
        if record.asked().action.as_deref() != Some(action) {
            return Err(wrong("is no run of it"));
        }
        // On the entry's own pair, port by port.
        let at = |port: &str| -> Vec<ArtifactId> {
            (record.inputs().iter())
                .filter(|input| input.port == port)
                .map(|input| ArtifactId(input.artifact))
                .collect()
        };
        let inputs = record.inputs().len();
        let on_its_pair = match tool {
            align::NAME => {
                let against = id_of(entry.against.as_deref().unwrap_or_default())?;
                inputs == 2 && at("image") == [image_id] && at("against") == [against]
            }
            details::TILES => {
                let place = entry.place.as_deref().unwrap_or_default();
                let boxed = details::TILE_BOXES.iter().find(|(p, _)| *p == place);
                let bbox = entry.bbox.unwrap_or_default();
                let sides: Vec<Option<f64>> = (view::SIDES.iter())
                    .map(|side| {
                        let given = record.asked().arguments.iter().find(|a| a.name == *side);
                        match given.map(|a| &a.value) {
                            Some(ArgumentValue::Number(number)) => Some(*number),
                            _ => None,
                        }
                    })
                    .collect();
                inputs == 1
                    && at("image") == [image_id]
                    && boxed.is_some_and(|(_, b)| *b == bbox)
                    && sides == bbox.map(Some)
            }
            _ => {
                // A comparison of the frame the verified align of this pair made, in the
                // grid of the image it was registered onto.
                let against = id_of(entry.against.as_deref().unwrap_or_default())?;
                let frame = frames
                    .iter()
                    .find(|(of, _)| *of == pair)
                    .and_then(|(_, f)| *f);
                inputs == 2
                    && at("frame") == [against]
                    && frame.is_some_and(|frame| at("against") == [frame])
            }
        };
        if !on_its_pair {
            return Err(wrong("was not run on the pair the entry names"));
        }
        let status = record.outcome().status;
        if let Some(why) = &entry.failed {
            let said = (record.outcome().invalid.clone())
                .or_else(|| record.outcome().error.as_ref().map(|e| e.message.clone()));
            if status == Status::Completed || said.as_deref() != Some(why.as_str()) {
                return Err(wrong("did not fail as the entry says"));
            }
            if !ids.is_empty() || entry.measured.is_some() {
                return Err(wrong("measured nothing, and the entry says it did"));
            }
            continue;
        }
        if status != Status::Completed {
            return Err(wrong("did not complete"));
        }
        let made: Vec<ArtifactId> = (record.outputs().iter())
            .map(|output| ArtifactId(output.artifact))
            .collect();
        let committed: Vec<ArtifactId> = if tool == details::TILES {
            made.into_iter().take(1).collect()
        } else {
            made
        };
        if committed != ids {
            return Err(wrong("committed other images than the entry names"));
        }
        let kept = match report_name(tool) {
            None => None,
            Some(name) => {
                let named = format!("{DIAGNOSTICS_PREFIX}{name}");
                let entry = (generation.run(run)).and_then(|kept| kept.get(&named));
                let bytes = entry
                    .map(|entry| store.read_record(entry))
                    .and_then(Result::ok)
                    .ok_or_else(|| wrong("kept no measurements"))?;
                Some(
                    serde_json::from_slice::<Json>(&bytes)
                        .map_err(|_| wrong("kept measurements that are not read"))?,
                )
            }
        };
        if kept != entry.measured {
            return Err(wrong("kept other measurements than the entry says"));
        }
        if tool == align::NAME {
            frames.push((pair, committed.last().copied()));
        }
    }
    // Every entry it made, and no other, in its order, as it kept them.
    let kept = (generation.run(made_by))
        .and_then(|run| run.get(&format!("{DIAGNOSTICS_PREFIX}{KEPT}")))
        .map(|entry| store.read_record(entry))
        .and_then(Result::ok)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|text| read(&text));
    match kept {
        Some(Ok((_, made))) if made.as_slice() == entries => {}
        _ => {
            return Err(
                "the measurements handed on are not what the step of details made, \
                        whole and in order"
                    .to_owned(),
            );
        }
    }
    Ok(())
}
