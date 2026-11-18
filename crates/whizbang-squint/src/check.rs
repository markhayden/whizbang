//! `check`: a judge looks at images through evidence tools and concludes (`SPEC.md`
//! section 3, "What a judge model may call"; ADR 0009). It is the smallest loop that
//! proves the execution contract. Under the use case `general`, the judge is told what
//! the images are and which tools there are, and concludes by ending its turn with one
//! word, `pass`, `fail` or `unsure`, on the first line of its text, and its reason
//! after. Under `source-fidelity` (plan 2A, task S2; [`crate::source_fidelity`]), it is
//! given squint's rules and checklist, answers with the tool `submit_verdict`, and the
//! verdict is worked out from its dispositions; a submission that is not a verdict is
//! answered with why, and asked for once more.
//!
//! The shape of the loop is squint's (`crates/squint-core/src/run.rs`, `drive`): the
//! images open the conversation; a turn that calls tools has each call answered, in
//! order, and is said back whole; a turn that was cut off at its output limit is left
//! out of the conversation and asked for once more, with a larger limit, unless its
//! backend's output ceiling would send the retry with no more, when the run fails
//! naming the setting to raise.
//!
//! **A judge is not trusted.** What it may call and what it may read are frozen when
//! the run starts, and held by the executor where each call is dispatched
//! ([`Context::freeze_calls`]): the tools are the run's list, and an image a call names
//! is one the run was given or one a tool call of this run returned. A name that is
//! not on the list is refused by the loop before any call is made of it
//! ([`Context::offered`]), and the executor refuses such a call again were one made;
//! either way the refusal is in the run's record. Nothing a judge writes is an
//! instruction: it names a tool and gives arguments, and both are data.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value as Json, json};
use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType, Image};
use whizbang_core::capability::{
    Controls, Excluded, JudgeHandle, JudgeRequest, Message, Part, Picture, Role, StopReason,
    ToolCall, ToolDefinition, ToolResult, Turn,
};
use whizbang_core::contract::record::Decision;
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Call, CallError, Capability, Context, Count,
    DescriptorError, Failure, InputPort, Kind, Outcome, PreparedCall, Produced, Requirement, Value,
    ValueType, Verdict,
};
use whizbang_core::identity::{DecodeLimits, Digest};

use crate::details::{self, Details, Names, Projection};
use crate::source_fidelity;
use crate::{align, resize, view};

/// The use cases `check` takes, by name: the general one, and `source-fidelity`.
pub const USE_CASES: [&str; 2] = ["general", source_fidelity::NAME];

/// The action's name.
pub const NAME: &str = "check";
/// The action's version.
pub const VERSION: &str = "1.2.0";
/// The most requests one run sends its judge, the retry of a cut-off turn included.
pub const TURNS_LIMIT: usize = 16;
/// The most tool calls one run dispatches. It is far under what the executor lets one
/// run call (256): a call past it is answered with an error and not dispatched.
pub const CALLS_LIMIT: usize = 32;
/// The most bytes of text one turn may hold: its texts, its opaque parts, and its
/// calls' arguments as written. A larger turn is not taken: the handle answers with a
/// fault, stores the turn's sizes in its place, and the run fails. Each exchange is
/// kept whole, so this and [`TURNS_LIMIT`] bound what a run's exchanges come to.
pub const TURN_TEXT_LIMIT: usize = 256 * 1024;
/// A cut-off turn is asked for again with this many times the output tokens.
pub const RETRY_FACTOR: u32 = 4;
/// The most images a run is given.
pub const IMAGES_LIMIT: u32 = 8;

const SYSTEM: &str = "You check images. Look closer with the tools you are given when a \
    closer look would change your answer. Then end your turn with one word on the first \
    line, pass, fail or unsure, and your reason after it.";

/// The evidence tools on offer to a run that names them. A run's list is frozen from
/// this when the run starts; what is offered or withdrawn afterwards is seen by the
/// next run and by no run that has started.
///
/// The tools there are, are the ones this build links: `view`, `align` and `resize`.
/// Offering is saying that one of them may be named by a run.
pub struct Toolbox {
    offered: Mutex<BTreeSet<&'static str>>,
}

/// The tools this build links, by name.
const LINKED: [&str; 3] = [view::NAME, align::NAME, resize::NAME];

impl Toolbox {
    /// A toolbox that offers every tool this build links.
    pub fn standard() -> Arc<Toolbox> {
        Arc::new(Toolbox {
            offered: Mutex::new(LINKED.into_iter().collect()),
        })
    }

    fn names(&self) -> std::sync::MutexGuard<'_, BTreeSet<&'static str>> {
        self.offered.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Offers the tool of this name from now on. False when this build links no such
    /// tool: nothing is offered.
    pub fn offer(&self, name: &str) -> bool {
        let linked = LINKED.into_iter().find(|linked| *linked == name);
        linked.map(|linked| self.names().insert(linked)).is_some()
    }

    /// Offers the tool of this name no longer.
    pub fn withdraw(&self, name: &str) {
        self.names().remove(name);
    }

    /// The tools on offer now.
    pub fn offered(&self) -> Vec<&'static str> {
        self.names().iter().copied().collect()
    }
}

/// The action.
pub struct Check {
    descriptor: ActionDescriptor,
    toolbox: Arc<Toolbox>,
}

impl Check {
    /// `check`, over the tools this toolbox offers.
    pub fn new(toolbox: Arc<Toolbox>) -> Result<Check, DescriptorError> {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Recipe,
            "Has a judge look at images through evidence tools and conclude: pass, fail or unsure.",
        )
        .input(InputPort::variadic(
            "images",
            ArtifactType::Image,
            "The images the judge is shown, and may look into.",
            Count::between(1, IMAGES_LIMIT),
        ))
        .argument(Argument::with_default(
            "tools",
            "The evidence tools the judge may call, by name, separated by commas.",
            ValueType::Text,
            Value::text(view::NAME),
        ))
        .argument(Argument::optional(
            "judge",
            "The judge to ask; without it, the configured default.",
            ValueType::Text,
        ))
        .argument(Argument::with_default(
            "use_case",
            "general, or source-fidelity: the source first among the images, then the edit of it.",
            ValueType::Text,
            Value::text("general"),
        ))
        .argument(Argument::optional(
            "brief",
            "Under source-fidelity, what the edit was to do and keep, as data.",
            ValueType::Text,
        ))
        .argument(Argument::with_default(
            "details",
            "none, or first: before the first turn, the tools named by detail are run on each pair of images the use case relates, and what they measured opens the conversation.",
            ValueType::choice(&Details::NAMES),
            Value::text("none"),
        ))
        .argument(Argument::with_default(
            "detail",
            "The tools whose details are measured first, by name, separated by commas: align.",
            ValueType::Text,
            Value::text(align::NAME),
        ))
        .argument(Argument::with_default(
            "projection",
            "How a call of align by the judge is answered: full, or visual only, its pictures and a fixed sentence (squint's projection of arm C).",
            ValueType::choice(&Projection::NAMES),
            Value::text("full"),
        ))
        .argument(Argument::with_default(
            "max_output_tokens",
            "The most output tokens of one turn of the judge.",
            ValueType::integer(Some(1), Some(i64::from(u32::MAX / RETRY_FACTOR))),
            Value::Integer(1024),
        ))
        .needs(
            Requirement::new(Capability::Judge)
                .feature("image")
                .feature("tool")
                .chosen_by("judge")
                // What a call reserves covers every turn of the loop.
                .requests(TURNS_LIMIT as u64),
        );
        Ok(Check {
            descriptor: draft.build()?,
            toolbox,
        })
    }
}

fn failed(kind: &str, why: &dyn std::fmt::Display) -> Failure {
    Failure::error(kind, &why.to_string())
}

/// One image the judge has been shown: an input of the run, or a crop a tool call of
/// this run returned.
struct Shown {
    id: ArtifactId,
    picture: Picture,
}

fn shown(context: &Context, id: ArtifactId) -> Result<(Image, Shown), Failure> {
    let limits = DecodeLimits::default();
    let unread = |why: &dyn std::fmt::Display| failed("image", why);
    let image = match artifact::get(context.store(), ArtifactType::Image, id, &limits) {
        Ok(Artifact::Image(image)) => image,
        Ok(_) => return Err(unread(&"not an image")),
        Err(error) => return Err(unread(&error)),
    };
    let pixels = image.pixels(&limits).map_err(|error| unread(&error))?;
    let picture = Picture::new(pixels).map_err(|error| unread(&error))?;
    Ok((image, Shown { id, picture }))
}

/// What a judge is told of an image, beside being shown it: the name a tool call
/// gives to look into it.
fn label(shown: &Shown) -> String {
    let (width, height) = (shown.picture.width(), shown.picture.height());
    format!("image {} ({width} by {height} pixels)", shown.id)
}

/// The tool `view` as a judge is told of it.
fn view_tool() -> Result<ToolDefinition, Failure> {
    let schema = json!({
        "type": "object",
        "required": ["image", "bbox"],
        "properties": {
            "image": { "type": "string" },
            "also": { "type": "string" },
            "bbox": { "type": "array", "items": { "type": "number" } },
        },
    });
    let description = "Shows a region of an image closer. image is the name of an image you \
        were shown; bbox is left, top, right and bottom as fractions of the image; also is a \
        second image to show the same region of.";
    ToolDefinition::new(view::NAME, description, schema).map_err(|why| failed("tools", &why))
}

/// The call of the action `view` that a judge's arguments ask for, or why they ask for
/// none. An image is named by its identity, as the judge was told it; whether the run
/// may read it is not looked at here: the executor holds the call to that.
fn view_call(arguments: &Json) -> Result<Call, String> {
    let Some(arguments) = arguments.as_object() else {
        return Err("the arguments are not an object".to_owned());
    };
    let known = ["image", "also", "bbox"];
    if arguments.keys().any(|key| !known.contains(&key.as_str())) {
        return Err("the arguments are image, bbox and also, and no other".to_owned());
    }
    let named = |key: &str| match arguments.get(key) {
        None => Ok(None),
        Some(Json::String(text)) => match text.parse::<Digest>() {
            Ok(digest) => Ok(Some(ArtifactId(digest))),
            Err(_) => Err(format!("{key} is not the name of an image you were shown")),
        },
        Some(_) => Err(format!("{key} is not a text")),
    };
    let Some(image) = named("image")? else {
        return Err("image is required".to_owned());
    };
    // Four entries, each a number: an entry that is none is not skipped, or five
    // entries of which four are numbers would be a box.
    let sides = arguments.get("bbox").and_then(Json::as_array);
    let sides: Option<Vec<f64>> = sides.and_then(|sides| sides.iter().map(Json::as_f64).collect());
    let Some(sides) = sides.filter(|sides| sides.len() == 4) else {
        return Err("bbox is four numbers: left, top, right and bottom".to_owned());
    };
    let mut call = Call::new(view::NAME).input("image", image);
    if let Some(also) = named("also")? {
        call = call.input("also", also);
    }
    for (name, side) in view::SIDES.into_iter().zip(sides) {
        call = call.argument(name, Value::Number(side));
    }
    Ok(call)
}

/// The verdict a judge's last turn states: the first line of its text is the word,
/// and the rest is the reason.
fn verdict_of(turn: &Turn) -> Option<Verdict> {
    let text = turn.texts().collect::<Vec<_>>().join("\n");
    let (word, why) = text.trim().split_once('\n').unwrap_or((text.trim(), ""));
    let decision = match word.trim().to_ascii_lowercase().as_str() {
        "pass" => Decision::Pass,
        "fail" => Decision::Fail,
        "unsure" => Decision::Unsure,
        _ => return None,
    };
    Some(Verdict::new(decision, why.trim()))
}

/// Adds the tool the judge answers with under `source-fidelity`.
fn run_tools_submit(tools: &mut Vec<ToolDefinition>) -> Result<(), Failure> {
    let submit = source_fidelity::submit_tool();
    tools.push(submit.ok_or_else(|| failed("tools", &"the submission's schema is not read"))?);
    Ok(())
}

/// One run of the loop: what it froze, and what it has dispatched and kept so far.
struct Run<'a> {
    context: &'a Context,
    judge: &'a JudgeHandle,
    /// The tools of the run, frozen: nothing reads the toolbox after this is made.
    tools: Vec<ToolDefinition>,
    /// The ids of the tool calls answered so far: an id answers one call.
    answered: BTreeSet<String>,
    dispatched: usize,
    crops: usize,
    /// The names the judge may give an image beside its identity.
    names: Names,
    /// How a call of `align` is answered.
    projection: Projection,
}

impl Run<'_> {
    /// Answers one call of the judge. A name that is not on the run's list is
    /// refused here, with no call made; a call of a listed tool goes to the executor,
    /// which holds it to the list again and to what the run may read. What comes
    /// back, a crop, a refusal or the tool's own rejection, is the call's result.
    fn dispatch(&mut self, call: &ToolCall) -> Result<ToolResult, Failure> {
        let answer = |text: &str, error: bool, images: Vec<Picture>| {
            let result = ToolResult::new(call.id(), text, error, images);
            result.map_err(|why| failed("tool_result", &why))
        };
        if self.dispatched >= CALLS_LIMIT {
            let said = format!("not called: a run calls at most {CALLS_LIMIT} tools");
            return answer(&said, true, Vec::new());
        }
        self.dispatched += 1;
        // The first line: a name that is not on the run's frozen list is refused here,
        // before anything is handed to the executor. Nothing is prepared, admitted or
        // run for it, and the refusal is in the run's record.
        if let Err(refusal) = self.context.offered(call.name()) {
            return answer(&format!("refused: {refusal}"), true, Vec::new());
        }
        let listed = self.tools.iter().any(|tool| tool.name() == call.name());
        let id = format!("call:{}", self.dispatched - 1);
        let mut said_as: Option<details::AlignAsked> = None;
        let mut resized: Option<String> = None;
        let asked = match (listed, call.name()) {
            (true, view::NAME) => view_call(call.arguments()),
            (true, align::NAME) => {
                details::align_call(call.arguments(), &self.names).map(|asked| {
                    let call = asked.call.clone();
                    said_as = Some(asked);
                    call
                })
            }
            (true, resize::NAME) => {
                details::resize_call(call.arguments(), &self.names).map(|(call, frame)| {
                    resized = Some(frame);
                    call
                })
            }
            // The second line, which the check above leaves nothing for: a name that
            // is no tool of the run would go to the executor as it was named, with
            // nothing in it, and the executor refuses a call the run did not freeze
            // before it admits it.
            _ => Ok(Call::new(call.name())),
        };
        let asked = match asked {
            Ok(asked) => asked,
            Err(why) => return answer(&format!("not called: {why}"), true, Vec::new()),
        };
        self.context.events().progress("a tool call was dispatched");
        let finished = match self.context.executor().submit(&asked) {
            Ok(finished) => finished,
            Err(CallError::Refused(refusal)) => {
                return answer(&format!("refused: {refusal}"), true, Vec::new());
            }
            Err(other) => return Err(failed("dispatch", &other)),
        };
        if !listed {
            // The executor ran what the run did not list: the contract is broken.
            return Err(failed(
                "dispatch",
                &"a tool that is not on the run's list ran",
            ));
        }
        if let Some(asked) = &said_as
            && let Outcome::Completed { outputs } = finished.outcome()
        {
            let committed: Vec<ArtifactId> = outputs.iter().map(|output| output.id()).collect();
            let report =
                details::report_of(self.context, asked.image_id, asked.against_id, &committed)?;
            let mut pictures = Vec::new();
            for output in outputs {
                let picture = self.evidence(output.id())?;
                self.names
                    .frame(&align::aligned_frame(&asked.image), output.id());
                pictures.push(picture.picture);
            }
            let said = details::align_answer(&id, &report, asked, self.projection);
            return answer(&said, false, pictures);
        }
        if let Some(frame) = &resized
            && let Outcome::Completed { outputs } = finished.outcome()
        {
            let mut pictures = Vec::new();
            let mut said = String::new();
            for output in outputs {
                let picture = self.evidence(output.id())?;
                self.names.frame(&format!("crop:{id}:0"), output.id());
                let (width, height) = (picture.picture.width(), picture.picture.height());
                said = details::resize_answer(&id, frame, width, height);
                pictures.push(picture.picture);
            }
            return answer(&said, false, pictures);
        }
        match finished.outcome() {
            Outcome::Completed { outputs } => {
                let mut said = Vec::new();
                let mut pictures = Vec::new();
                for output in outputs {
                    let crop = self.evidence(output.id())?;
                    said.push(label(&crop));
                    pictures.push(crop.picture);
                }
                answer(&said.join("\n"), false, pictures)
            }
            Outcome::Invalid { why } => answer(&format!("rejected: {why}"), true, Vec::new()),
            Outcome::Error { kind, .. } => {
                answer(&format!("the tool failed: {kind}"), true, Vec::new())
            }
            Outcome::Cancelled => Err(failed("cancelled", &"a tool call was cancelled")),
        }
    }

    /// An image a tool call of this run returned, kept as the run's evidence.
    fn evidence(&mut self, id: ArtifactId) -> Result<Shown, Failure> {
        let (image, shown) = shown(self.context, id)?;
        // The evidence is the run's own: kept with it as a diagnostic, whatever the run
        // comes to, and never one of its outputs. It is an image's file, so it is kept
        // exactly or not at all: a file with a secret taken out of it would be no image
        // (ADR 0009).
        self.crops += 1;
        let name = format!("evidence-{:03}", self.crops);
        let file = Artifact::Image(image).to_bytes();
        let kept = self.context.diagnostics().write_exact(&name, &file);
        kept.map_err(|why| failed("evidence", &why))?;
        Ok(shown)
    }

    /// The details measured first (ADR 0028): for each pair, `image` registered onto
    /// `against`, each named as the judge is told of it, the tools of `detail` are called
    /// through the executor, and what they found is said, with the pictures they returned.
    /// A call that could not measure is said too, with why.
    fn measure_first(
        &mut self,
        pairs: &[(&str, ArtifactId, &str, ArtifactId)],
        detail: &[&str],
    ) -> Result<Vec<Part>, Failure> {
        let mut parts = Vec::new();
        for (image, image_id, against, against_id) in pairs {
            for tool in detail {
                if *tool != align::NAME {
                    continue;
                }
                let call = Call::new(align::NAME)
                    .input("image", *image_id)
                    .input("against", *against_id);
                self.context.events().progress("a detail was measured");
                let finished = match self.context.executor().submit(&call) {
                    Ok(finished) => finished,
                    Err(CallError::Refused(refusal)) => {
                        return Err(failed("details", &refusal));
                    }
                    Err(other) => return Err(failed("details", &other)),
                };
                let outputs = match finished.outcome() {
                    Outcome::Completed { outputs } => outputs,
                    Outcome::Invalid { why } | Outcome::Error { message: why, .. } => {
                        let mark = details::mark(tool);
                        parts.push(Part::Text(format!(
                            "{tool} ({mark}), {image} onto {against}: not measured: {why}"
                        )));
                        continue;
                    }
                    Outcome::Cancelled => {
                        return Err(failed("cancelled", &"a detail's call was cancelled"));
                    }
                };
                let committed: Vec<ArtifactId> = outputs.iter().map(|output| output.id()).collect();
                let report = details::report_of(self.context, *image_id, *against_id, &committed)?;
                if parts.is_empty() {
                    parts.push(Part::Text(details::DETAILS_INTRO.to_owned()));
                }
                parts.push(Part::Text(details::align_detail(&report, image, against)));
                for output in outputs {
                    let frame = align::aligned_frame(image);
                    let shown = self.evidence(output.id())?;
                    self.names.frame(&frame, output.id());
                    let (width, height) = (shown.picture.width(), shown.picture.height());
                    parts.push(Part::Text(format!(
                        "Image `{frame}` ({image} warped into {against}'s grid, \
                         {width}x{height}, Flat); for the tool view it is named {}.",
                        shown.id
                    )));
                    parts.push(Part::Image(shown.picture));
                }
            }
        }
        Ok(parts)
    }

    /// Answers every call of a turn, in the order the judge made them.
    fn answer(&mut self, turn: &Turn) -> Result<Message, Failure> {
        let mut results = Vec::new();
        for call in turn.tool_calls() {
            if !self.answered.insert(call.id().to_owned()) {
                let why = "a tool call has the id of a call that was answered before";
                return Err(failed("judge_output", &why));
            }
            results.push(Part::ToolResult(self.dispatch(call)?));
        }
        Message::new(Role::User, results).map_err(|why| failed("judge_request", &why))
    }
}

impl Action for Check {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let argument = |name: &str| {
            let mut arguments = call.arguments().iter();
            arguments
                .find(|(argument, _)| argument == name)
                .map(|(_, value)| value)
        };
        let Some(judge) = context.capabilities().judge() else {
            return Err(failed("no_judge", &"the run was handed no judge"));
        };
        // A turn with more text than the loop reads is not taken, and not stored.
        let judge = &judge.clone().turns_within(TURN_TEXT_LIMIT as u64);
        // The run's tools are frozen here, once: the names its call gave, each a tool
        // that is on offer now. The executor is told the same list, and holds every
        // call to it. Nothing below reads the toolbox again.
        let Some(Value::Text(named)) = argument("tools") else {
            return Err(Failure::invalid("the call names no tools"));
        };
        // In the order the call names them, each once: the order the judge is told of them.
        let mut listed: Vec<&str> = Vec::new();
        for name in named.split(',').map(str::trim) {
            if !name.is_empty() && !listed.contains(&name) {
                listed.push(name);
            }
        }
        let named = listed;
        let offered = self.toolbox.offered();
        if let Some(name) = named.iter().find(|name| !offered.contains(name)) {
            let shown: String = name.chars().take(64).collect();
            return Err(Failure::invalid(&format!(
                "{shown:?} is no evidence tool on offer"
            )));
        }
        let mut tools = Vec::new();
        for name in &named {
            match *name {
                view::NAME => tools.push(view_tool()?),
                align::NAME => {
                    tools.push(details::align_tool().map_err(|why| failed("tools", &why))?)
                }
                resize::NAME => {
                    tools.push(details::resize_tool().map_err(|why| failed("tools", &why))?);
                }
                _ => {}
            }
        }
        let chosen = |name: &str, names: &[&str]| match argument(name) {
            Some(Value::Text(text)) if names.contains(&text.as_str()) => Ok(text.clone()),
            _ => Err(Failure::invalid(&format!(
                "{name} is one of {}",
                names.join(", ")
            ))),
        };
        let details = Details::named(&chosen("details", &Details::NAMES)?).unwrap_or(Details::None);
        let projection = Projection::named(&chosen("projection", &Projection::NAMES)?)
            .unwrap_or(Projection::Full);
        let mut detail: Vec<&str> = Vec::new();
        if details == Details::First {
            let Some(Value::Text(asked)) = argument("detail") else {
                return Err(Failure::invalid("the call names no detail"));
            };
            for name in asked
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
            {
                if !details::DETAILED.contains(&name) || !offered.contains(&name) {
                    let shown: String = name.chars().take(64).collect();
                    return Err(Failure::invalid(&format!(
                        "{shown:?} is no tool whose details are measured first: {}",
                        details::DETAILED.join(", ")
                    )));
                }
                if !detail.contains(&name) {
                    detail.push(name);
                }
            }
        }
        let frozen = named.iter().chain(detail.iter()).copied();
        if !context.freeze_calls(frozen) {
            return Err(failed(
                "tools",
                &"the run's tools were frozen before it started",
            ));
        }
        let Some(Value::Integer(tokens)) = argument("max_output_tokens") else {
            return Err(Failure::invalid("the call gives no output limit"));
        };
        let tokens = u32::try_from(*tokens).unwrap_or(1);

        let use_case = match argument("use_case") {
            Some(Value::Text(name)) if USE_CASES.contains(&name.as_str()) => name.as_str(),
            _ => {
                return Err(Failure::invalid(
                    "the use case is general or source-fidelity",
                ));
            }
        };
        let fidelity = use_case == source_fidelity::NAME;
        let mut run = Run {
            context,
            judge,
            tools,
            answered: BTreeSet::new(),
            dispatched: 0,
            crops: 0,
            names: Names::default(),
            projection,
        };
        // The images open the conversation, each with the name a tool call gives.
        let mut opening = Vec::new();
        let system = if fidelity {
            let inputs = call.inputs();
            let [source, candidate] = inputs else {
                return Err(Failure::invalid(
                    "source-fidelity is given two images: the source, then the edit of it",
                ));
            };
            for (role, input) in [("candidate", candidate), ("source", source)] {
                run.names.role(role, input.id());
                let (_, image) = shown(context, input.id())?;
                let (width, height) = (image.picture.width(), image.picture.height());
                let title = format!("{}{}", role[..1].to_ascii_uppercase(), &role[1..]);
                opening.push(Part::Text(format!(
                    "Image `{role}` (role {title}, frame raw:{role}, {width}x{height}, Flat); \
                     for the tool view it is named {}.",
                    image.id
                )));
                opening.push(Part::Image(image.picture));
            }
            // The pair the use case relates: the candidate registered onto the source.
            let pairs = [("candidate", candidate.id(), "source", source.id())];
            opening.extend(run.measure_first(&pairs, &detail)?);
            opening.push(Part::Text(source_fidelity::checklist()));
            opening.push(Part::Text(source_fidelity::INSTRUCTION.to_owned()));
            let brief = match argument("brief") {
                Some(Value::Text(brief)) if !brief.trim().is_empty() => brief.as_str(),
                _ => source_fidelity::NO_BRIEF,
            };
            run_tools_submit(&mut run.tools)?;
            source_fidelity::system(brief, &details::tools_section(&named))
        } else {
            for input in call.inputs() {
                let (_, image) = shown(context, input.id())?;
                opening.push(Part::Text(label(&image)));
                opening.push(Part::Image(image.picture));
            }
            SYSTEM.to_owned()
        };
        let unfit = |why: whizbang_core::capability::Unfit| failed("judge_request", &why);
        let mut messages = vec![Message::new(Role::User, opening).map_err(unfit)?];
        let mut allowed = tokens;
        let mut retried = false;
        // A submission that is not a verdict, or a turn that ends without one, is
        // answered with why and asked for once more: one repair, as squint allowed.
        let mut repaired = false;
        for _ in 0..TURNS_LIMIT {
            let sent = allowed;
            let controls = Controls::new(allowed).map_err(unfit)?;
            let request = JudgeRequest::new(&system, messages.clone(), run.tools.clone(), controls);
            let turn = run.judge.send(&request.map_err(unfit)?);
            let turn = turn.map_err(|fault| failed("judge_fault", &fault))?;
            allowed = tokens;
            match turn.stop() {
                // Cut off: the turn is left out of the conversation, whatever it
                // holds, and asked for once more with a larger limit.
                // A turn whose backend sends no more than its output ceiling, and was
                // sent with it already, is not asked for again: the retry would be
                // sent with the same limit. The fault names the setting to raise.
                StopReason::MaxTokens => {
                    run.judge.exclude_last(Excluded::Truncated);
                    if retried {
                        let why = "the judge was cut off again after its limit was raised";
                        return Err(failed("judge_output", &why));
                    }
                    let raised = tokens.saturating_mul(RETRY_FACTOR);
                    if let Some(ceiling) = turn.output_ceiling()
                        && raised.min(ceiling) <= sent.min(ceiling)
                    {
                        let why = format!(
                            "the judge was cut off at its backend's output ceiling of \
                             {ceiling} tokens, and a retry would be sent with no more: raise \
                             max_output_tokens in the judge's table of the config"
                        );
                        return Err(failed("judge_output_ceiling", &why));
                    }
                    retried = true;
                    allowed = raised;
                }
                StopReason::Refusal => {
                    run.judge.exclude_last(Excluded::Refusal);
                    return Err(failed("judge_refused", &"the judge declined"));
                }
                StopReason::ToolUse => {
                    let said = turn.message();
                    let Some(said) = said else {
                        return Err(failed("judge_output", &"a turn that calls tools is empty"));
                    };
                    let submitted: Vec<&ToolCall> = turn
                        .tool_calls()
                        .filter(|call| call.name() == source_fidelity::SUBMIT)
                        .collect();
                    if fidelity && !submitted.is_empty() {
                        let calls = turn.tool_calls().count();
                        let read = match submitted.as_slice() {
                            [call] if calls == 1 => source_fidelity::decided_of(call.arguments()),
                            _ => Err("submit_verdict is called once, alone".to_owned()),
                        };
                        let why = match read {
                            Ok(decided) => {
                                // Each item of the reason whole, one to a line, beside
                                // the verdict's summary: the writer takes the secrets
                                // out of the whole first.
                                let mut lines = String::new();
                                for item in &decided.items {
                                    lines.push_str(&Json::from(item.as_str()).to_string());
                                    lines.push('\n');
                                }
                                let kept = context
                                    .diagnostics()
                                    .write(source_fidelity::REASONS, lines.as_bytes());
                                kept.map_err(|why| failed("reasons", &why))?;
                                return Ok(Produced::nothing().with_verdict(decided.verdict));
                            }
                            Err(why) => why,
                        };
                        if repaired {
                            return Err(failed(
                                "invalid_submission",
                                &format!("the judge's submission is not a verdict: {why}"),
                            ));
                        }
                        repaired = true;
                        let mut results = Vec::new();
                        for call in turn.tool_calls() {
                            let said = format!("Submission rejected: {why}");
                            let result = ToolResult::new(call.id(), &said, true, Vec::new());
                            results.push(Part::ToolResult(
                                result.map_err(|why| failed("tool_result", &why))?,
                            ));
                        }
                        results.push(Part::Text(source_fidelity::REPAIR.to_owned()));
                        messages.push(said);
                        messages.push(Message::new(Role::User, results).map_err(unfit)?);
                        continue;
                    }
                    let results = run.answer(&turn)?;
                    messages.push(said);
                    messages.push(results);
                }
                StopReason::EndTurn if fidelity && turn.tool_calls().next().is_none() => {
                    if repaired {
                        let why = "the judge ended its turn without calling submit_verdict";
                        return Err(failed("judge_output", &why));
                    }
                    repaired = true;
                    if let Some(said) = turn.message() {
                        messages.push(said);
                    }
                    let repair = Part::Text(source_fidelity::REPAIR.to_owned());
                    messages.push(Message::new(Role::User, vec![repair]).map_err(unfit)?);
                }
                StopReason::EndTurn if turn.tool_calls().next().is_none() => {
                    let Some(verdict) = verdict_of(&turn) else {
                        run.judge.exclude_last(Excluded::NoCall);
                        let why = "the judge ended its turn without pass, fail or unsure";
                        return Err(failed("judge_output", &why));
                    };
                    return Ok(Produced::nothing().with_verdict(verdict));
                }
                StopReason::EndTurn => {
                    let why = "the judge ended its turn with tool calls unanswered";
                    return Err(failed("judge_output", &why));
                }
                _ => {
                    let why = "the judge stopped for a reason this loop does not know";
                    return Err(failed("judge_output", &why));
                }
            }
        }
        let why = format!("the judge did not conclude within {TURNS_LIMIT} requests");
        Err(failed("judge_loop", &why))
    }
}
