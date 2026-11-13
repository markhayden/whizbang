//! `check`: a judge looks at images through evidence tools and concludes (`SPEC.md`
//! section 3, "What a judge model may call"; ADR 0009). It is the smallest loop that
//! proves the execution contract. Squint's rules, its checklist and its submission
//! format are not here: the judge is told what the images are and which tools there
//! are, and concludes by ending its turn with one word, `pass`, `fail` or `unsure`, on
//! the first line of its text, and its reason after.
//!
//! The shape of the loop is squint's (`crates/squint-core/src/run.rs`, `drive`): the
//! images open the conversation; a turn that calls tools has each call answered, in
//! order, and is said back whole; a turn that was cut off at its output limit is left
//! out of the conversation and asked for once more, with a larger limit.
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

use crate::view;

/// The action's name.
pub const NAME: &str = "check";
/// The action's version.
pub const VERSION: &str = "1.0.0";
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
/// The tools there are, are the ones this build links: `view`. Offering is saying that
/// one of them may be named by a run.
pub struct Toolbox {
    offered: Mutex<BTreeSet<&'static str>>,
}

/// The tools this build links, by name.
const LINKED: [&str; 1] = [view::NAME];

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
            "max_output_tokens",
            "The most output tokens of one turn of the judge.",
            ValueType::integer(Some(1), Some(i64::from(u32::MAX / RETRY_FACTOR))),
            Value::Integer(1024),
        ))
        .needs(
            Requirement::new(Capability::Judge)
                .feature("image")
                .feature("tool")
                .chosen_by("judge"),
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
    let sides = arguments.get("bbox").and_then(Json::as_array);
    let sides: Option<Vec<f64>> =
        sides.map(|sides| sides.iter().filter_map(Json::as_f64).collect());
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
        let asked = match (listed, call.name()) {
            (true, view::NAME) => view_call(call.arguments()),
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
        match finished.outcome() {
            Outcome::Completed { outputs } => {
                let mut said = Vec::new();
                let mut pictures = Vec::new();
                for output in outputs {
                    let (image, crop) = shown(self.context, output.id())?;
                    // The evidence is the run's own: kept with it as a diagnostic,
                    // whatever the run comes to, and never one of its outputs.
                    self.crops += 1;
                    let name = format!("evidence-{:03}", self.crops);
                    let file = Artifact::Image(image).to_bytes();
                    let kept = self.context.diagnostics().write(&name, &file);
                    kept.map_err(|why| failed("evidence", &why))?;
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
        let named: BTreeSet<&str> = named.split(',').map(str::trim).collect();
        let named: Vec<&str> = named.into_iter().filter(|name| !name.is_empty()).collect();
        let offered = self.toolbox.offered();
        if let Some(name) = named.iter().find(|name| !offered.contains(name)) {
            let shown: String = name.chars().take(64).collect();
            return Err(Failure::invalid(&format!(
                "{shown:?} is no evidence tool on offer"
            )));
        }
        let mut tools = Vec::new();
        for name in &named {
            if *name == view::NAME {
                tools.push(view_tool()?);
            }
        }
        if !context.freeze_calls(named.iter().copied()) {
            return Err(failed(
                "tools",
                &"the run's tools were frozen before it started",
            ));
        }
        let Some(Value::Integer(tokens)) = argument("max_output_tokens") else {
            return Err(Failure::invalid("the call gives no output limit"));
        };
        let tokens = u32::try_from(*tokens).unwrap_or(1);

        // The images open the conversation, each with the name a tool call gives.
        let mut opening = Vec::new();
        for input in call.inputs() {
            let (_, image) = shown(context, input.id())?;
            opening.push(Part::Text(label(&image)));
            opening.push(Part::Image(image.picture));
        }
        let unfit = |why: whizbang_core::capability::Unfit| failed("judge_request", &why);
        let mut messages = vec![Message::new(Role::User, opening).map_err(unfit)?];
        let mut run = Run {
            context,
            judge,
            tools,
            answered: BTreeSet::new(),
            dispatched: 0,
            crops: 0,
        };
        let mut allowed = tokens;
        let mut retried = false;
        for _ in 0..TURNS_LIMIT {
            let controls = Controls::new(allowed).map_err(unfit)?;
            let request = JudgeRequest::new(SYSTEM, messages.clone(), run.tools.clone(), controls);
            let turn = run.judge.send(&request.map_err(unfit)?);
            let turn = turn.map_err(|fault| failed("judge_fault", &fault))?;
            allowed = tokens;
            match turn.stop() {
                // Cut off: the turn is left out of the conversation, whatever it
                // holds, and asked for once more with a larger limit.
                StopReason::MaxTokens => {
                    run.judge.exclude_last(Excluded::Truncated);
                    if retried {
                        let why = "the judge was cut off again after its limit was raised";
                        return Err(failed("judge_output", &why));
                    }
                    retried = true;
                    allowed = tokens.saturating_mul(RETRY_FACTOR);
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
                    let results = run.answer(&turn)?;
                    messages.push(said);
                    messages.push(results);
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
