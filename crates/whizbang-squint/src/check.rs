//! `check`: a judge looks at images through evidence tools and concludes (`SPEC.md`
//! section 3, "What a judge model may call"; ADR 0009). It is the smallest loop that
//! proves the execution contract. Under the use case `general`, the judge is told what
//! the images are and which tools there are, and concludes by ending its turn with one
//! word, `pass`, `fail` or `unsure`, on the first line of its text, and its reason
//! after. Under each of the five use cases of the table ([`crate::use_case`]; plan 2A,
//! task S2, and plan 2C, task A1), it is given a job (`job/1`, [`crate::job`]) whose
//! images are the call's, in its order, with their roles and labels; it is told squint's
//! rules and the job's requirements, and the checklist made of them; it answers with the
//! tool `submit_verdict`, and the verdict is worked out from its dispositions. A
//! submission that is not a verdict is answered with why, and asked for once more. What
//! the run decided is kept beside its record ([`JUDGED`]), and from it the run hands
//! its caller its findings with the verdict: every answer that counts against a pass,
//! as data ([`crate::findings`]; plan m4, task K2).
//!
//! **Retrying** (squint's `retry.rs`, at the commit `docs/sources.toml` pins). A request
//! the judge's backend could not answer now (a fault `unavailable`, as a 429 or a 5xx
//! is, or `timed_out`) is sent again, at most [`RETRIES_LIMIT`] times in a run, after the
//! wait the backend said, or else after 1, 2, 4, … seconds up to 30, never longer than
//! 30. Any other fault ends the run. Squint drew each wait at random under that ceiling;
//! here it is the ceiling, so that a run waits the same on every machine. A 400 whose body
//! named no cause, which squint retried once, is a fault `rejected` from whizbang's
//! adapter, and is not retried.
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
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};
use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType, Image};
use whizbang_core::capability::{
    Controls, Excluded, FaultKind, JudgeHandle, JudgeRequest, Message, Part, Picture, Role,
    StopReason, ToolCall, ToolDefinition, ToolResult, Turn,
};
use whizbang_core::contract::record::Decision;
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Call, CallError, Capability, Committed, Context, Count,
    DescriptorError, Failure, InputPort, Kind, Outcome, PreparedCall, Produced, Requirement,
    ResultKind, Value, ValueType, Verdict,
};
use whizbang_core::identity::DecodeLimits;

use crate::cache;
use crate::details::{self, Details, Names, Projection};
use crate::files::{self, FileTool, FileTools};
use crate::job::{Job, Role as ImageRole};
use crate::measured::{self, Entry};
use crate::use_case::{self, Checklist, Resolved};
use crate::{align, compare, resize, view};

/// The use cases `check` takes, by name: the general one, then the five of the table.
pub const USE_CASES: [&str; 6] = [
    "general",
    use_case::NAMES[0],
    use_case::NAMES[1],
    use_case::NAMES[2],
    use_case::NAMES[3],
    use_case::NAMES[4],
];

/// The action's name.
pub const NAME: &str = "check";
/// The action a tool file's capability is asked through (`whizbang_rascal::segment`).
const SEGMENT: &str = "segment";
/// The action's version.
pub const VERSION: &str = "1.7.0";
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
/// The most images a run is given: a candidate, a source and eight references.
pub const IMAGES_LIMIT: u32 = 10;
/// The most times one run sends a request again after its judge could not answer it.
pub const RETRIES_LIMIT: u32 = 3;
/// The longest a run waits before it sends a request again, in milliseconds.
pub const RETRY_WAIT_LIMIT_MS: u64 = 30_000;
/// The name of the run's diagnostic that holds what a run under a use case decided: the
/// job's images with their roles, labels and identities; the checklist; each repair and
/// each retry; and each answer's obligations, disposition, confidence, row and location.
/// The findings the run hands back are made from it ([`crate::findings::of_judged`]).
pub const JUDGED: &str = "judged.json";

/// The name of the run's diagnostic that holds, for a run under a use case of the table,
/// each rejection of a submission in order, by the request it answered and its class, and
/// why the run ended (plan e1, task T1). It is written on every way the loop ends,
/// completed or failed, and holds classes and causes only: no word of the judge.
pub const REJECTIONS: &str = "rejections.json";

/// The contract of [`REJECTIONS`].
pub const REJECTIONS_CONTRACT: &str = "rejections/1";

/// Why a run of the loop ended, as [`REJECTIONS`] records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// It reached a verdict.
    Concluded,
    /// Its submission was rejected after the one repair.
    InvalidSubmission,
    /// A turn ended without a submission after the one repair.
    NoSubmissionAfterRepair,
    /// A turn was cut off again after its limit was raised.
    CutOff,
    /// A turn was cut off at its backend's output ceiling.
    Ceiling,
    /// The judge declined.
    Refused,
    /// The requests of the run were spent without a verdict.
    Turns,
    /// The judge's backend failed, and the request was not, or no longer, sent again.
    Fault,
    /// Anything else: a turn the loop cannot read, a tool or the store failing, a
    /// cancellation.
    Other,
}

impl Cause {
    /// Every cause, in the order of the plan.
    pub const ALL: [Cause; 9] = [
        Cause::Concluded,
        Cause::InvalidSubmission,
        Cause::NoSubmissionAfterRepair,
        Cause::CutOff,
        Cause::Ceiling,
        Cause::Refused,
        Cause::Turns,
        Cause::Fault,
        Cause::Other,
    ];

    /// The cause as it is recorded.
    pub fn name(self) -> &'static str {
        match self {
            Cause::Concluded => "concluded",
            Cause::InvalidSubmission => "invalid_submission",
            Cause::NoSubmissionAfterRepair => "no_submission_after_repair",
            Cause::CutOff => "cut_off",
            Cause::Ceiling => "ceiling",
            Cause::Refused => "refused",
            Cause::Turns => "turns",
            Cause::Fault => "fault",
            Cause::Other => "other",
        }
    }

    /// The cause of a run's end that named none itself, from what it ended as.
    fn of(ended: &Result<Produced, Failure>) -> Cause {
        match ended {
            Ok(_) => Cause::Concluded,
            Err(Failure::Error { kind, .. }) => match kind.as_str() {
                "invalid_submission" => Cause::InvalidSubmission,
                "judge_output_ceiling" => Cause::Ceiling,
                "judge_refused" => Cause::Refused,
                "judge_loop" => Cause::Turns,
                "judge_fault" => Cause::Fault,
                _ => Cause::Other,
            },
            Err(_) => Cause::Other,
        }
    }
}

/// What a run under a use case keeps of its rejections as the loop goes: each one, by the
/// number of the request it answered (from 1) and its class; and the cause of its end
/// where a failure's kind does not tell it (`judge_output` is more than one cause).
#[derive(Default)]
struct Tally {
    rejections: Vec<(usize, use_case::Class)>,
    cause: Option<Cause>,
}

impl Tally {
    /// The diagnostic, for a run that ended so.
    fn document(&self, ended: &Result<Produced, Failure>) -> Json {
        let rejections: Vec<Json> = (self.rejections.iter())
            .map(|(turn, class)| json!({"turn": turn, "class": class.name()}))
            .collect();
        let cause = self.cause.unwrap_or_else(|| Cause::of(ended));
        json!({"contract": REJECTIONS_CONTRACT, "rejections": rejections, "cause": cause.name()})
    }
}

/// The input that takes the images an earlier step measured first and handed on.
pub const MEASURED_IMAGES: &str = "measured_images";

/// The values of the argument `second_look`.
pub const SECOND_LOOK: [&str; 3] = ["off", "on", "strict"];

const SYSTEM: &str = "You check images. Look closer with the tools you are given when a \
    closer look would change your answer. Then end your turn with one word on the first \
    line, pass, fail or unsure, and your reason after it.";

/// The evidence tools on offer to a run that names them. A run's list is frozen from
/// this when the run starts; what is offered or withdrawn afterwards is seen by the
/// next run and by no run that has started.
///
/// The tools there are, are the ones this build links: `view`, `align`, `resize`,
/// `change_map` and `edge_similarity`. Offering is saying that one of them may be named
/// by a run. The last two are experimental (`SPEC.md` section 8): on offer, but run only
/// when a run names them. Beside them, the tool files the config lists, when the binary
/// hands them ([`Toolbox::with_files`]; plan 2C, task X1; ADR 0032): each on offer, and
/// each run only when a run names it.
pub struct Toolbox {
    offered: Mutex<BTreeSet<&'static str>>,
    files: Option<Arc<dyn FileTools>>,
}

/// The tools this build links, by name.
const LINKED: [&str; 5] = [
    view::NAME,
    align::NAME,
    resize::NAME,
    compare::CHANGE_MAP,
    compare::EDGE_SIMILARITY,
];

impl Toolbox {
    /// A toolbox that offers every tool this build links.
    pub fn standard() -> Arc<Toolbox> {
        Arc::new(Toolbox {
            offered: Mutex::new(LINKED.into_iter().collect()),
            files: None,
        })
    }

    /// The same, with these tool files on offer beside the tools this build links.
    pub fn with_files(files: Arc<dyn FileTools>) -> Arc<Toolbox> {
        Arc::new(Toolbox {
            offered: Mutex::new(LINKED.into_iter().collect()),
            files: Some(files),
        })
    }

    /// The tool file of this name, when one is on offer.
    fn file(&self, name: &str) -> Option<FileTool> {
        let files = self.files.as_ref()?;
        files.listed().into_iter().find(|tool| tool.name == name)
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

    /// The tools on offer now: the linked ones, then the tool files.
    pub fn offered(&self) -> Vec<String> {
        let mut offered: Vec<String> = self.names().iter().map(|n| (*n).to_owned()).collect();
        if let Some(files) = &self.files {
            offered.extend(files.listed().into_iter().map(|tool| tool.name));
        }
        offered
    }
}

/// The action.
pub struct Check {
    descriptor: ActionDescriptor,
    toolbox: Arc<Toolbox>,
    /// Whether a run under a use case of the table reads and keeps the decision cache
    /// ([`crate::cache`]): off unless the registry's maker turns it on.
    cache: bool,
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
            "general, or a use case of the table: source-fidelity, reference-consistency, reality-checks, brief-compliance or product-fidelity.",
            ValueType::Text,
            Value::text("general"),
        ))
        .argument(Argument::optional(
            "job",
            "Under a use case of the table, the job in the contract job/1: its brief, its requirements, and its images in the order of the inputs, with their roles.",
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
            "The details measured first, separated by commas: align, and the two comparisons that run align first, for a pair; or tiles, for one image: its candidate in four tiles by view.",
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
            "second_look",
            "off, on, or strict: one more turn, in the run's budget, to settle items a submission left tentative; strict settles toward pass only with a cited location.",
            ValueType::choice(&SECOND_LOOK),
            Value::text("off"),
        ))
        .input(InputPort::variadic(
            MEASURED_IMAGES,
            ArtifactType::Image,
            "The images an earlier step measured first, as it handed them on: aligned frames, maps and tiles.",
            Count::between(0, measured::IMAGES_LIMIT),
        ))
        .argument(Argument::measurements(
            "measured",
            "What an earlier step of a recipe measured first, bound to its measurements alone: said to the judge before its first turn once held to the runs that made it.",
        ))
        .argument(Argument::optional(
            "recipe_digest",
            "The digest of the recipe this run is a step of, which a recipe gives: kept in the decision cache's key.",
            ValueType::Text,
        ))
        .argument(Argument::optional(
            "cell",
            "The cell of an evaluation matrix this run is, kept in its record: the matrix, set, item, judge, arm, repetition and attempt. check does not read it.",
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
                .chosen_by("judge")
                // What a call reserves covers every turn of the loop.
                .requests(TURNS_LIMIT as u64),
        )
        .results(ResultKind::Verdict)
        // Under a use case of the table it hands back its findings too (plan m4, task
        // K2; `crate::findings`).
        .results(ResultKind::Findings);
        Ok(Check {
            descriptor: draft.build()?,
            toolbox,
            cache: false,
        })
    }

    /// The same, with the decision cache on (`[cache]` in the config): a run under a
    /// use case of the table that names no cell of a matrix answers from an earlier
    /// run of the same key, and keeps its own key when its judge reported its identity.
    pub fn caching(mut self) -> Check {
        self.cache = true;
        self
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

/// Every tool this build links, by name: what a run may offer a judge beside tool files
/// and the submission (plan m3, task G1, item 4: each is sent to every judge in a test).
pub fn linked() -> &'static [&'static str] {
    &LINKED
}

/// What a judge is told of a tool this build links.
pub fn definition(name: &str) -> Result<ToolDefinition, String> {
    match name {
        view::NAME => view_tool().map_err(|_| "the tool view cannot be described".to_owned()),
        align::NAME => details::align_tool(),
        resize::NAME => details::resize_tool(),
        compare::CHANGE_MAP | compare::EDGE_SIMILARITY => details::compare_tool(name),
        other => Err(format!("{other:?} is no tool this build links")),
    }
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
/// none. An image is named by its identity, as the judge was told it, or, under a use
/// case, as the opening calls it, by its id or its raw frame (`candidate`,
/// `raw:candidate`), as `align` takes it: a judge of the Google kind named images so in
/// 230 of its 998 calls of `view` in the gate run of the details first, each a turn
/// spent on a refusal. Whether the run may read it is not looked at here: the executor
/// holds the call to that.
fn view_call(arguments: &Json, names: &details::Names) -> Result<Call, String> {
    let Some(arguments) = arguments.as_object() else {
        return Err("the arguments are not an object".to_owned());
    };
    let known = ["image", "also", "bbox"];
    if arguments.keys().any(|key| !known.contains(&key.as_str())) {
        return Err("the arguments are image, bbox and also, and no other".to_owned());
    }
    let named = |key: &str| match arguments.get(key) {
        None => Ok(None),
        Some(Json::String(text)) => match names.resolve(text) {
            Some((id, _)) => Ok(Some(id)),
            None => Err(format!("{key} is not the name of an image you were shown")),
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

/// Adds the tool the judge answers with under a use case of the table.
fn run_tools_submit(tools: &mut Vec<ToolDefinition>) -> Result<(), Failure> {
    let submit = use_case::submit_tool();
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
    /// The provider's own ids of the tool calls that completed, and the ids of the runs
    /// that measured the details said first: what a submission may cite beside `shown`,
    /// though no judge is shown them as text.
    completed: Vec<String>,
    /// The ids the judge was shown of what completed, in the order it was: the id each
    /// tool's result gives (`call:0`), and the id of the run that measured each detail
    /// handed to it first. What a submission may cite, and what a refusal lists (the gate
    /// of the details first on 2026-11-19: 66 of 67 runs refused for their citations
    /// cited ids they were shown, or calls whose results showed none).
    shown: Vec<String>,
    /// How many details measured first were given an id to cite them by.
    details_cited: usize,
    dispatched: usize,
    crops: usize,
    /// The names the judge may give an image beside its identity.
    names: Names,
    /// How a call of `align` is answered.
    projection: Projection,
    /// The tool files the run named, and what runs them.
    files: Vec<FileTool>,
    file_tools: Option<Arc<dyn FileTools>>,
    /// The revision of each tool file the decision cache's key holds, and whether one
    /// ran as another: then the key is not kept.
    revisions: Vec<(String, String)>,
    edited: bool,
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
        if let Some(tool) = self.files.iter().find(|tool| tool.name == call.name()) {
            return self.dispatch_file(&id, tool.clone(), call);
        }
        let mut said_as: Option<details::AlignAsked> = None;
        let mut resized: Option<String> = None;
        let mut compared: Option<details::CompareAsked> = None;
        let asked = match (listed, call.name()) {
            (true, view::NAME) => view_call(call.arguments(), &self.names),
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
            (true, compare::CHANGE_MAP | compare::EDGE_SIMILARITY) => {
                details::compare_call(call.name(), call.arguments(), &self.names).map(|asked| {
                    let call = asked.call.clone();
                    compared = Some(asked);
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
            // What align's own run measured, handed back with its outcome.
            let report = finished.measurements().cloned().unwrap_or(Json::Null);
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
        if let Some(asked) = &compared
            && let Outcome::Completed { outputs } = finished.outcome()
        {
            let measured = finished.measurements().cloned().unwrap_or(Json::Null);
            let measured = details::measures_of(&measured);
            let mut pictures = Vec::new();
            for output in outputs {
                let picture = self.evidence(output.id())?;
                self.names.frame(&format!("crop:{id}:0"), output.id());
                pictures.push(picture.picture);
            }
            let said = details::compare_answer(&id, asked, &measured);
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
                // The id to cite it by, as every other tool's result gives it.
                answer(&format!("{id}: {}", said.join("\n")), false, pictures)
            }
            Outcome::Invalid { why } => answer(&format!("rejected: {why}"), true, Vec::new()),
            Outcome::Error { kind, .. } => {
                answer(&format!("the tool failed: {kind}"), true, Vec::new())
            }
            Outcome::Cancelled => Err(failed("cancelled", &"a tool call was cancelled")),
        }
    }

    /// Answers a call of a tool file the run named: run under this run's context, so that
    /// what it asks for is a nested call of this run, and its output, held to its
    /// declaration, is the answer. The digest it ran is noted among the run's settings.
    fn dispatch_file(
        &mut self,
        id: &str,
        tool: FileTool,
        call: &ToolCall,
    ) -> Result<ToolResult, Failure> {
        let answer = |text: &str, error: bool| {
            let result = ToolResult::new(call.id(), text, error, Vec::new());
            result.map_err(|why| failed("tool_result", &why))
        };
        let (images, arguments) = match files::call_of(&tool, call.arguments(), &self.names) {
            Ok(asked) => asked,
            Err(why) => return answer(&format!("not called: {why}"), true),
        };
        let Some(file_tools) = &self.file_tools else {
            return answer("not called: no tool file is on offer", true);
        };
        self.context.events().progress("a tool file was called");
        match file_tools.run(&tool.name, &images, &arguments, self.context) {
            Ok(ran) => {
                let setting = format!("tool_file:{}", tool.name);
                self.context
                    .facts()
                    .setting(&setting, &Value::text(&ran.digest));
                // The key holds the file as it was when the run began: a run of another
                // digest is no answer that key may give again.
                let keyed = self.revisions.iter().find(|(name, _)| *name == tool.name);
                if keyed.is_some_and(|(_, revision)| *revision != ran.digest) {
                    self.edited = true;
                }
                answer(&files::answer(id, &tool.name, &ran), false)
            }
            Err(why) => answer(&format!("the tool failed: {why}"), true),
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

    /// The details measured first (ADR 0029), said as `check` has said them: for each pair,
    /// what `align` found and the frame it returned, then each comparison of that frame and
    /// its map; then each candidate's tiles. Each entry is what a tool's own run handed back
    /// (plan m3, task Q1, item 2), whether this run called the tool or an earlier step of its
    /// recipe did and handed it on; each image is kept as the run's evidence and named as
    /// the judge is told of it. An entry that measured nothing is said with why.
    fn said_details(&mut self, entries: &[Entry]) -> Result<Vec<Part>, Failure> {
        let mut parts = Vec::new();
        let mut tiles: Vec<Part> = Vec::new();
        let mut tiled: Option<&str> = None;
        for entry in entries {
            let image = entry.image.as_str();
            let against = entry.against.as_deref().unwrap_or_default();
            let frame = align::aligned_frame(image);
            let ids = entry.image_ids().map_err(|why| failed("details", &why))?;
            let tool = entry.tool.as_str();
            let mark = details::mark(tool);
            if tool == details::TILES {
                if tiled != Some(image) {
                    tiles.push(Part::Text(details::TILES_INTRO.to_owned()));
                    tiled = Some(image);
                }
                let place = entry.place.as_deref().unwrap_or_default();
                if let Some(why) = &entry.failed {
                    tiles.push(Part::Text(format!(
                        "Tile `tile:{image}:{place}` ({}): not measured: {why}",
                        details::TILES
                    )));
                    continue;
                }
                let bbox = entry.bbox.unwrap_or([0.0, 0.0, 1.0, 1.0]);
                let cite = self.cited_as(entry);
                for id in ids {
                    let shown = self.evidence(id)?;
                    self.names.frame(&format!("tile:{image}:{place}"), shown.id);
                    let size = (shown.picture.width(), shown.picture.height());
                    let id = shown.id.to_string();
                    let said = details::tile_said(image, place, bbox, size, &id);
                    tiles.push(Part::Text(format!("{said}{cite}")));
                    tiles.push(Part::Image(shown.picture));
                }
                continue;
            }
            if let Some(why) = &entry.failed {
                parts.push(Part::Text(if tool == align::NAME {
                    format!("{tool} ({mark}), {image} onto {against}: not measured: {why}")
                } else {
                    format!("{tool} ({mark}), {frame} against {against}: not measured: {why}")
                }));
                continue;
            }
            let measured = entry.measured.clone().unwrap_or(Json::Null);
            if parts.is_empty() {
                parts.push(Part::Text(details::DETAILS_INTRO.to_owned()));
            }
            let cite = self.cited_as(entry);
            if tool == align::NAME {
                let said = details::align_detail(&measured, image, against);
                parts.push(Part::Text(format!("{said}{cite}")));
                for id in ids {
                    let shown = self.evidence(id)?;
                    self.names.frame(&frame, id);
                    let (width, height) = (shown.picture.width(), shown.picture.height());
                    parts.push(Part::Text(format!(
                        "Image `{frame}` ({image} warped into {against}'s grid, \
                         {width}x{height}, Flat); for the tool view it is named {}.",
                        shown.id
                    )));
                    parts.push(Part::Image(shown.picture));
                }
                continue;
            }
            let map = format!("map:{image}:{against}");
            let measurements = details::measures_of(&measured);
            let said = details::compare_detail(tool, &measurements, against, &frame, &map);
            parts.push(Part::Text(format!("{said}{cite}")));
            for id in ids {
                let shown = self.evidence(id)?;
                self.names.frame(&map, id);
                let (width, height) = (shown.picture.width(), shown.picture.height());
                parts.push(Part::Text(format!(
                    "Image `{map}` (the change map of {frame} against {against}, in \
                     {against}'s grid, {width}x{height}; bright is different, dark the \
                     same); for the tool view it is named {}.",
                    shown.id
                )));
                parts.push(Part::Image(shown.picture));
            }
        }
        parts.extend(tiles);
        Ok(parts)
    }

    /// What a detail measured first is cited by, said after it: `detail:0`, `detail:1`, in
    /// the order the details are said, which a submission may then cite as a call of this
    /// run; and the id of the run that measured it, the entry's own, which it may cite
    /// too (the gate of the details first on 2026-11-19: its judges were shown the
    /// details as evidence and given no id to cite them by). The id said is the run's
    /// order, not the measuring run's id, so that the same details make the same request
    /// whichever run measured them. Nothing for an entry no run is known to have made.
    fn cited_as(&mut self, entry: &Entry) -> String {
        let Some(run) = &entry.run else {
            return String::new();
        };
        let id = format!("detail:{}", self.details_cited);
        self.details_cited += 1;
        self.shown.push(id.clone());
        self.completed.push(run.clone());
        format!(" Cite it as {id}.")
    }

    /// The crop `view` cuts of this image for this box, as a call of the run, kept as its
    /// evidence; or why it cut none.
    fn crops_of(
        &mut self,
        id: ArtifactId,
        bbox: [f64; 4],
    ) -> Result<Result<Vec<Shown>, String>, Failure> {
        let mut call = Call::new(view::NAME).input("image", id);
        for (name, side) in view::SIDES.into_iter().zip(bbox) {
            call = call.argument(name, Value::Number(side));
        }
        let outputs = match self.first(&call)? {
            Ok(outputs) => outputs,
            Err(why) => return Ok(Err(why)),
        };
        let mut shown = Vec::new();
        // The first output is the crop of the image; a second would be of `also`.
        for output in outputs.iter().take(1) {
            shown.push(self.evidence(output.id())?);
        }
        Ok(Ok(shown))
    }

    /// What the second look shows and asks (plan m3, task U1, item 4): the submission
    /// received; crops by `view` at full resolution of the regions the judge cited for the
    /// items it left tentative, at most [`SECOND_LOOK_CROPS`], each with a margin of a
    /// twentieth of the picture; where it cited none, the four tiles of the candidate; and
    /// the question to settle those items alone. The parts, and how many crops they hold.
    fn second_look(
        &mut self,
        submit: &str,
        items: &[String],
        decided: &use_case::Decided,
        job: &Job,
        strict: bool,
    ) -> Result<(Vec<Part>, usize), Failure> {
        let listed = items.join(", ");
        let received = format!(
            "Submission received. It leaves {listed} tentative, so the verdict would be \
             unsure. A second look follows."
        );
        let result = ToolResult::new(submit, &received, false, Vec::new());
        let mut parts = vec![Part::ToolResult(
            result.map_err(|why| failed("tool_result", &why))?,
        )];
        parts.push(Part::Text(format!(
            "Second look at {listed}. Below, at full resolution, are crops of what you cited \
             for them (or, where you cited no region, the candidate in four tiles)."
        )));
        // The regions cited, each once, in the order of the items.
        let mut regions: Vec<(String, ArtifactId, [f64; 4])> = Vec::new();
        for answer in &decided.answers {
            let id = answer["id"].as_str().unwrap_or_default();
            if !items.iter().any(|item| item == id) {
                continue;
            }
            let location = &answer["location"];
            let Some(frame) = location["frame"].as_str() else {
                continue;
            };
            let Some((artifact, image)) = self.names.resolve(frame) else {
                continue;
            };
            let Some(bbox) = cited_box(location) else {
                continue;
            };
            let margin = 0.05;
            let bbox = [
                (bbox[0] - margin).max(0.0),
                (bbox[1] - margin).max(0.0),
                (bbox[2] + margin).min(1.0),
                (bbox[3] + margin).min(1.0),
            ];
            if regions.len() < SECOND_LOOK_CROPS
                && !regions.iter().any(|(_, a, b)| *a == artifact && *b == bbox)
            {
                regions.push((image, artifact, bbox));
            }
        }
        if regions.is_empty() {
            let candidate = job.images.iter().find(|i| i.role == ImageRole::Candidate);
            if let Some(candidate) = candidate
                && let Some((artifact, image)) = self.names.resolve(&candidate.id)
            {
                for (_, bbox) in details::TILE_BOXES {
                    regions.push((image.clone(), artifact, bbox));
                }
            }
        }
        let mut crops = 0;
        for (image, artifact, bbox) in regions {
            let [l, t, r, b] = bbox;
            match self.crops_of(artifact, bbox)? {
                Ok(shown) => {
                    for shown in shown {
                        crops += 1;
                        let name = format!("look:{crops}");
                        self.names.frame(&name, shown.id);
                        let (width, height) = (shown.picture.width(), shown.picture.height());
                        parts.push(Part::Text(format!(
                            "Crop `{name}` of {image}: left {l:.2}, top {t:.2}, right {r:.2}, \
                             bottom {b:.2} of it, {width}x{height}; for the tool view it is \
                             named {}.",
                            shown.id
                        )));
                        parts.push(Part::Image(shown.picture));
                    }
                }
                Err(why) => parts.push(Part::Text(format!(
                    "A crop of {image} at left {l:.2}, top {t:.2}, right {r:.2}, bottom {b:.2} \
                     was not cut: {why}"
                ))),
            }
        }
        parts.push(Part::Text(format!(
            "Settle {listed} alone. Call submit_verdict once more, its items holding only \
             {listed}, each with its disposition: confident if what you now see settles it, \
             either way, or tentative if it still does not. Give no findings: those of your \
             submission are kept. Locations stay on the raw frames of the job's images."
        )));
        if strict {
            parts.push(Part::Text(STRICT_LOOK.to_owned()));
        }
        Ok((parts, crops))
    }

    /// One call made first, through the executor: what it committed, or why it measured
    /// nothing. A refusal or a cancellation ends the run.
    fn first(&mut self, call: &Call) -> Result<Result<Vec<Committed>, String>, Failure> {
        self.context.events().progress("a detail was measured");
        let finished = match self.context.executor().submit(call) {
            Ok(finished) => finished,
            Err(CallError::Refused(refusal)) => return Err(failed("details", &refusal)),
            Err(other) => return Err(failed("details", &other)),
        };
        match finished.outcome() {
            Outcome::Completed { outputs } => Ok(Ok(outputs.clone())),
            Outcome::Invalid { why } | Outcome::Error { message: why, .. } => Ok(Err(why.clone())),
            Outcome::Cancelled => Err(failed("cancelled", &"a detail's call was cancelled")),
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
            let result = self.dispatch(call)?;
            // A result that is no error is of a call that was counted, under the id its
            // result gave the judge.
            if !result.is_error() {
                self.completed.push(call.id().to_owned());
                self.shown.push(format!("call:{}", self.dispatched - 1));
            }
            results.push(Part::ToolResult(result));
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
        // The images the judge is shown, apart from those an earlier step handed on.
        let images: Vec<&whizbang_core::contract::ResolvedInput> = (call.inputs().iter())
            .filter(|input| input.port() == "images")
            .collect();
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
        if let Some(name) = named
            .iter()
            .find(|name| !offered.iter().any(|o| o == *name))
        {
            let shown: String = name.chars().take(64).collect();
            return Err(Failure::invalid(&format!(
                "{shown:?} is no evidence tool on offer"
            )));
        }
        let mut tools = Vec::new();
        let mut named_files: Vec<FileTool> = Vec::new();
        for name in &named {
            match *name {
                linked if LINKED.contains(&linked) => {
                    tools.push(definition(linked).map_err(|why| failed("tools", &why))?);
                }
                other => {
                    if let Some(tool) = self.toolbox.file(other) {
                        tools.push(files::definition(&tool).map_err(|why| failed("tools", &why))?);
                        named_files.push(tool);
                    }
                }
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
            detail = measured::named_details(asked, &offered)?;
        }
        // What an earlier step measured and handed on (plan m3, task Q1): said to the
        // judge, and never measured again here.
        let handed_on = match argument("measured") {
            Some(Value::Text(text)) => {
                if details == Details::First {
                    return Err(Failure::invalid(
                        "a run handed what was measured first measures nothing itself: details is none",
                    ));
                }
                Some(measured::read(text).map_err(|why| Failure::invalid(&why))?)
            }
            _ => None,
        };
        // The images handed on with it, and only those, are what its entries name; and
        // each entry is held to the run that made it (ADR 0035).
        let handed_images: Vec<ArtifactId> = (call.inputs().iter())
            .filter(|input| input.port() == MEASURED_IMAGES)
            .map(|input| input.id())
            .collect();
        // A tool file's capabilities are nested calls of this run: `segment` is frozen
        // with the tools when the run names one, and is never offered to the judge.
        let asks: &[&str] = if named_files.is_empty() {
            &[]
        } else {
            &[SEGMENT]
        };
        let frozen = named
            .iter()
            .chain(detail.iter())
            .chain(asks.iter())
            .map(|name| {
                if *name == details::TILES {
                    view::NAME
                } else {
                    *name
                }
            });
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
                    "the use case is general, source-fidelity, reference-consistency, \
                     reality-checks, brief-compliance or product-fidelity",
                ));
            }
        };
        // Under a use case of the table, the job is read and held to the images before
        // anything is sent: a call it does not fit is invalid, and nothing leaves.
        let mut judging = match use_case {
            "general" => None,
            _ => Some(judging(call, use_case, argument("job"))?),
        };
        // Each entry handed on is held to the run that made it, on its own pair of the
        // job's images, and the document to the step of details that made it whole (ADR
        // 0035).
        if let Some((made_by, entries)) = &handed_on {
            let Some(judging) = &judging else {
                return Err(Failure::invalid(
                    "measurements are handed on only under a use case of the table",
                ));
            };
            let ids: Vec<ArtifactId> = images.iter().map(|input| input.id()).collect();
            let job_text = match argument("job") {
                Some(Value::Text(text)) => text.as_str(),
                _ => "",
            };
            measured::held_to_their_runs(
                context,
                made_by.as_deref(),
                entries,
                job_text,
                &judging.job,
                &ids,
                &handed_images,
            )
            .map_err(|why| Failure::invalid(&why))?;
        }
        // Tiles are of a job with one image: one that relates a pair is refused, naming
        // its use case, before anything is measured or sent.
        if detail.contains(&details::TILES) {
            let paired = judging.as_ref().map(|judging| {
                (judging.job.images.iter()).any(|image| image.role != ImageRole::Candidate)
            });
            match paired {
                None => {
                    return Err(Failure::invalid(
                        "the detail tiles is measured under a use case of the table",
                    ));
                }
                Some(true) => return Err(Failure::invalid(&details::tiles_refused(use_case))),
                Some(false) => {}
            }
        }
        // `on` or `strict`; `off`, or a run with no checklist, gives none.
        let look_mode = chosen("second_look", &SECOND_LOOK)?;
        let second_look = look_mode != "off" && judging.is_some();
        let strict = look_mode == "strict";
        // The decision cache's key is known here, before anything is measured or sent;
        // a run of the same key that completed answers this one (crate::cache).
        let mut revisions: Vec<(String, String)> = Vec::new();
        // What was measured first is in the key as it was measured, not by the runs that
        // happened to measure it: each run is a new id, and the same measurements of the
        // same images are the same details.
        let keyed_arguments: Vec<(String, Value)> = (call.arguments().iter())
            .map(|(name, value)| match (name.as_str(), &handed_on) {
                ("measured", Some((_, entries))) => {
                    let unrun: Vec<Entry> = (entries.iter())
                        .map(|entry| Entry {
                            run: None,
                            ..entry.clone()
                        })
                        .collect();
                    (
                        name.clone(),
                        Value::text(&measured::document(&unrun, None).to_string()),
                    )
                }
                _ => (name.clone(), value.clone()),
            })
            .collect();
        let key = match &judging {
            Some(judging) if self.cache && argument("cell").is_none() => {
                let images: Vec<_> = (images.iter())
                    .map(|input| (input.id(), input.binding()))
                    .collect();
                let tools: Vec<&str> = named.iter().chain(detail.iter()).copied().collect();
                revisions = named_files
                    .iter()
                    .map(|tool| {
                        let files = self.toolbox.files.as_ref();
                        let revision = files.and_then(|files| files.revision(&tool.name));
                        (tool.name.clone(), revision.unwrap_or_default())
                    })
                    .collect();
                let system = use_case::system_with(
                    &judging.resolved,
                    &judging.job,
                    &details::tools_section(&named),
                );
                Some(cache::key(&cache::Material {
                    job: &judging.job,
                    arguments: &keyed_arguments,
                    images: &images,
                    system: &system,
                    checklist: &judging.checklist.render(&judging.resolved),
                    rules_version: judging.resolved.version,
                    tools: &tools,
                    files: &revisions,
                    judge: judge.declaration().model(),
                    config: call.config(),
                }))
            }
            _ => None,
        };
        if let Some(key) = &key
            && let Some(hit) = cache::find(context.store(), key)
        {
            return answered_from(context, &hit);
        }
        // Every rejection, and why the run ended, kept on every way the loop below ends
        // under a use case of the table (plan e1, task T1).
        let under_use_case = judging.is_some();
        let mut tally = Tally::default();
        let ended = (|| -> Result<Produced, Failure> {
            // Whether every turn said which model answered it: only then is the run's
            // verdict kept for a later run to answer from.
            let mut reported = true;
            let mut run = Run {
                context,
                judge,
                tools,
                answered: BTreeSet::new(),
                completed: Vec::new(),
                shown: Vec::new(),
                details_cited: 0,
                dispatched: 0,
                crops: 0,
                names: Names::default(),
                projection,
                files: named_files,
                file_tools: self.toolbox.files.clone(),
                revisions,
                edited: false,
            };
            // The images open the conversation, each with the name a tool call gives.
            let mut opening = Vec::new();
            let system = if let Some(judging) = &judging {
                for (image, input) in judging.job.images.iter().zip(images.iter()) {
                    run.names.role(&image.id, input.id());
                    let (_, shown) = shown(context, input.id())?;
                    let (width, height) = (shown.picture.width(), shown.picture.height());
                    let view_name = shown.id.to_string();
                    let said = use_case::opening(image, width, height, &view_name);
                    opening.push(Part::Text(said));
                    opening.push(Part::Image(shown.picture));
                }
                // The pairs the use case relates: the candidate registered onto the source,
                // and onto each reference, in the job's order; measured here, unless an
                // earlier step measured them and handed them on.
                let entries = match &handed_on {
                    Some((_, entries)) => entries.clone(),
                    None => {
                        let ids: Vec<ArtifactId> = images.iter().map(|input| input.id()).collect();
                        let (pairs, candidates) = measured::pairs_of(&judging.job, &ids);
                        measured::measure(context, &pairs, &detail, &candidates)?
                    }
                };
                opening.extend(run.said_details(&entries)?);
                opening.push(Part::Text(judging.checklist.render(&judging.resolved)));
                opening.push(Part::Text(use_case::INSTRUCTION.to_owned()));
                run_tools_submit(&mut run.tools)?;
                let tools = details::tools_section(&named);
                use_case::system_with(&judging.resolved, &judging.job, &tools)
            } else {
                for input in &images {
                    let (_, image) = shown(context, input.id())?;
                    opening.push(Part::Text(label(&image)));
                    opening.push(Part::Image(image.picture));
                }
                SYSTEM.to_owned()
            };
            let unfit = |why: whizbang_core::capability::Unfit| failed("judge_request", &why);
            let mut messages = vec![Message::new(Role::User, opening).map_err(unfit)?];
            // The limit each turn is sent with: the call's, until a turn is cut off; then
            // the raised limit, for that turn's retry and every turn after it, since a judge
            // that wrote past the limit once is likely to again (plan 2C, task F1: E1's run
            // ended 20 of 220 decisions at a cut-off turn whose retry was refused).
            let mut limit = tokens;
            let mut allowed = tokens;
            let mut retried = false;
            // Each request sent again after the judge could not answer it: its fault's kind
            // and the wait.
            let mut retries: Vec<Json> = Vec::new();
            // A submission that is not a verdict, or a turn that ends without one, is
            // answered with why and asked for once more: one repair, as squint allowed.
            let mut repaired = false;
            // The second look, once asked: what the first submission decided, and what was
            // asked of it (plan m3, task U1, item 4).
            let mut looking: Option<Looking> = None;
            for sent_turns in 0..TURNS_LIMIT {
                let sent = allowed;
                let controls = Controls::new(allowed).map_err(unfit)?;
                let request =
                    JudgeRequest::new(&system, messages.clone(), run.tools.clone(), controls);
                let answered = run.judge.send(&request.map_err(unfit)?);
                // The turn of the second look settles its items or leaves the first verdict
                // standing; nothing it does fails the run.
                if let Some(looking) = looking.take() {
                    let judging = judging
                        .as_ref()
                        .ok_or_else(|| failed("judged", &"no job"))?;
                    let (decided, look) = second_settled(answered, &looking, judging, run.judge);
                    let concluded = Concluded {
                        retries: &retries,
                        second: Some(look),
                        key: key.as_ref(),
                        reported,
                        edited: run.edited,
                    };
                    return conclude(context, call, judging, &decided, &concluded);
                }
                let turn = match answered {
                    Ok(turn) => turn,
                    Err(fault) => {
                        let again =
                            matches!(fault.kind(), FaultKind::Unavailable | FaultKind::TimedOut);
                        let count = u32::try_from(retries.len()).unwrap_or(u32::MAX);
                        if !again || count >= RETRIES_LIMIT {
                            return Err(failed("judge_fault", &fault));
                        }
                        let backoff = 1000u64 << count.min(5);
                        let wait = fault.wait_ms().unwrap_or(backoff).min(RETRY_WAIT_LIMIT_MS);
                        retries.push(json!({"kind": fault.kind().name(), "wait_ms": wait}));
                        context
                            .events()
                            .progress("the judge could not answer now: the request is sent again");
                        wait_for(context, wait)?;
                        continue;
                    }
                };
                allowed = limit;
                reported &= turn.reported_model().is_some();
                match turn.stop() {
                    // Cut off: the turn is left out of the conversation, whatever it
                    // holds, and asked for once more with a larger limit, which the turns
                    // after it keep.
                    // A turn whose backend sends no more than its output ceiling, and was
                    // sent with it already, is not asked for again: the retry would be
                    // sent with the same limit. The fault names the setting to raise.
                    StopReason::MaxTokens => {
                        run.judge.exclude_last(Excluded::Truncated);
                        if retried {
                            let why = "the judge was cut off again after its limit was raised";
                            tally.cause = Some(Cause::CutOff);
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
                        limit = raised;
                        allowed = raised;
                    }
                    StopReason::Refusal => {
                        run.judge.exclude_last(Excluded::Refusal);
                        return Err(failed("judge_refused", &"the judge declined"));
                    }
                    StopReason::ToolUse => {
                        let said = turn.message();
                        let Some(said) = said else {
                            return Err(failed(
                                "judge_output",
                                &"a turn that calls tools is empty",
                            ));
                        };
                        let submitted: Vec<&ToolCall> = turn
                            .tool_calls()
                            .filter(|call| call.name() == use_case::SUBMIT)
                            .collect();
                        if let Some(judging) = &mut judging
                            && !submitted.is_empty()
                        {
                            let calls = turn.tool_calls().count();
                            let arguments =
                                submitted.first().map(|submit| submit.arguments().clone());
                            let read = match submitted.as_slice() {
                                [submit] if calls == 1 => use_case::cited_calls_held(
                                    submit.arguments(),
                                    &run.shown,
                                    &run.completed,
                                )
                                .and_then(|()| {
                                    use_case::decided_of(
                                        submit.arguments(),
                                        &judging.job,
                                        &judging.resolved,
                                        &judging.checklist,
                                    )
                                }),
                                _ => Err(use_case::Rejection::new(
                                    use_case::Class::NotAlone,
                                    "submit_verdict is called once, alone",
                                )),
                            };
                            let why = match read {
                                Ok(decided) => {
                                    let tentative = tentative_items(&decided);
                                    // A turn is left for the second look, or none is asked.
                                    let room = sent_turns + 1 < TURNS_LIMIT;
                                    if second_look
                                        && room
                                        && decided.verdict.decision() == Decision::Unsure
                                        && !tentative.is_empty()
                                    {
                                        let submit_id = submitted
                                            .first()
                                            .map(|submit| submit.id().to_owned())
                                            .unwrap_or_default();
                                        let (asked, crops) = run.second_look(
                                            &submit_id,
                                            &tentative,
                                            &decided,
                                            &judging.job,
                                            strict,
                                        )?;
                                        messages.push(said);
                                        messages
                                            .push(Message::new(Role::User, asked).map_err(unfit)?);
                                        looking = Some(Looking {
                                            first: decided,
                                            arguments: arguments.unwrap_or(Json::Null),
                                            items: tentative,
                                            crops,
                                            strict,
                                        });
                                        continue;
                                    }
                                    let concluded = Concluded {
                                        retries: &retries,
                                        second: None,
                                        key: key.as_ref(),
                                        reported,
                                        edited: run.edited,
                                    };
                                    return conclude(context, call, judging, &decided, &concluded);
                                }
                                Err(rejection) => {
                                    tally.rejections.push((sent_turns + 1, rejection.class));
                                    rejection.message
                                }
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
                            judging.repairs.push(why);
                            results.push(Part::Text(use_case::REPAIR.to_owned()));
                            messages.push(said);
                            messages.push(Message::new(Role::User, results).map_err(unfit)?);
                            continue;
                        }
                        let results = run.answer(&turn)?;
                        messages.push(said);
                        messages.push(results);
                    }
                    StopReason::EndTurn
                        if judging.is_some() && turn.tool_calls().next().is_none() =>
                    {
                        let why = "the judge ended its turn without calling submit_verdict";
                        let class = use_case::Class::NoSubmission;
                        tally.rejections.push((sent_turns + 1, class));
                        if repaired {
                            tally.cause = Some(Cause::NoSubmissionAfterRepair);
                            return Err(failed("judge_output", &why));
                        }
                        repaired = true;
                        if let Some(judging) = &mut judging {
                            judging.repairs.push(why.to_owned());
                        }
                        if let Some(said) = turn.message() {
                            messages.push(said);
                        }
                        let repair = Part::Text(use_case::REPAIR.to_owned());
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
        })();
        if under_use_case {
            let kept = tally.document(&ended).to_string();
            let written = context.diagnostics().write(REJECTIONS, kept.as_bytes());
            if let (Err(why), Ok(_)) = (written, &ended) {
                return Err(failed("rejections", &why));
            }
        }
        ended
    }
}

/// A second look asked (plan m3, task U1, item 4): what the first submission decided, its
/// arguments, the items asked to be settled, and how many crops were shown.
struct Looking {
    first: use_case::Decided,
    arguments: Json,
    items: Vec<String>,
    crops: usize,
    /// Whether an item may be settled toward pass only with a cited location.
    strict: bool,
}

/// The box a location cites, in fractions of its frame: a box as it is, a polygon's
/// bounds; none for the whole image or an absence.
fn cited_box(location: &Json) -> Option<[f64; 4]> {
    let number = |v: &Json| v.as_f64();
    match location["kind"].as_str()? {
        "bbox" => {
            let b: Vec<f64> = location["bbox"]
                .as_array()?
                .iter()
                .filter_map(number)
                .collect();
            let [l, t, r, b] = b.as_slice() else {
                return None;
            };
            Some([*l, *t, *r, *b])
        }
        "polygon" => {
            let points = location["polygon"].as_array()?;
            let (mut l, mut t, mut r, mut b) = (1.0f64, 1.0f64, 0.0f64, 0.0f64);
            for point in points {
                let x = number(point.get(0)?)?;
                let y = number(point.get(1)?)?;
                (l, t, r, b) = (l.min(x), t.min(y), r.max(x), b.max(y));
            }
            (l < r && t < b).then_some([l, t, r, b])
        }
        _ => None,
    }
}

/// What a strict second look adds to its question.
pub const STRICT_LOOK: &str = "An item you settle as held or allowed_change counts as \
    confident only with a location: a bbox or polygon on the raw frame of the part of the \
    picture that shows it holds. Without one it stays tentative. A violation is settled as \
    you see it.";

/// The most crops a second look shows.
pub const SECOND_LOOK_CROPS: usize = 4;

/// The items of the checklist a submission left tentative so that its verdict is unsure:
/// each answered with confidence `tentative` at the row `unsure`.
fn tentative_items(decided: &use_case::Decided) -> Vec<String> {
    (decided.answers.iter())
        .filter(|answer| answer["row"] == "unsure" && answer["confidence"] == "tentative")
        .filter_map(|answer| answer["id"].as_str().map(str::to_owned))
        .collect()
}

/// What the turn of the second look comes to: the verdict worked out again by squint's
/// table from the first submission with the items it settled put in their place, or the
/// first verdict when the turn settles nothing; and what the run keeps of the look. Only
/// the items asked about may be answered, each once; findings are the first submission's.
fn second_settled(
    answered: Result<Turn, whizbang_core::capability::Fault>,
    looking: &Looking,
    judging: &Judging,
    judge: &JudgeHandle,
) -> (use_case::Decided, Json) {
    let kept = |why: &str| -> Result<use_case::Decided, String> { Err(why.to_owned()) };
    let settled = match &answered {
        Err(_) => kept("the judge could not answer"),
        Ok(turn) if matches!(turn.stop(), StopReason::MaxTokens) => {
            judge.exclude_last(Excluded::Truncated);
            kept("the turn was cut off")
        }
        Ok(turn) if matches!(turn.stop(), StopReason::Refusal) => {
            judge.exclude_last(Excluded::Refusal);
            kept("the judge declined")
        }
        Ok(turn) => {
            let calls: Vec<&ToolCall> = turn.tool_calls().collect();
            match calls.as_slice() {
                [submit] if submit.name() == use_case::SUBMIT => merged(
                    &looking.arguments,
                    submit.arguments(),
                    &looking.items,
                    looking.strict,
                )
                .and_then(|arguments| {
                    use_case::decided_of(
                        &arguments,
                        &judging.job,
                        &judging.resolved,
                        &judging.checklist,
                    )
                    .map_err(|rejection| rejection.message)
                }),
                _ => kept("the turn did not call submit_verdict once, alone"),
            }
        }
    };
    let rows = |decided: &use_case::Decided| -> Vec<Json> {
        (decided.answers.iter())
            .filter(|answer| {
                (answer["id"].as_str()).is_some_and(|id| looking.items.iter().any(|i| i == id))
            })
            .map(|answer| {
                json!({"id": answer["id"], "disposition": answer["disposition"],
                       "confidence": answer["confidence"], "row": answer["row"]})
            })
            .collect()
    };
    let word = |decided: &use_case::Decided| {
        serde_json::to_value(decided.verdict.decision()).unwrap_or(Json::Null)
    };
    let mut look = json!({
        "items": looking.items,
        "strict": looking.strict,
        "crops": looking.crops,
        "before": rows(&looking.first),
        "verdict_before": word(&looking.first),
    });
    match settled {
        Ok(decided) => {
            look["outcome"] = json!("settled");
            look["after"] = json!(rows(&decided));
            look["verdict_after"] = word(&decided);
            (decided, look)
        }
        Err(why) => {
            look["outcome"] = json!("kept");
            look["why"] = json!(why);
            look["verdict_after"] = word(&looking.first);
            (looking.first.clone(), look)
        }
    }
}

/// The first submission's arguments with the items of the second put in their place:
/// refused when the second answers an item it was not asked about, or one twice. Under a
/// strict look, an item settled as held or allowed_change with no box or polygon is kept
/// tentative: unsure may turn to fail freely, and to pass only with a cited location.
fn merged(first: &Json, second: &Json, asked: &[String], strict: bool) -> Result<Json, String> {
    let Some(given) = second.get("items").and_then(Json::as_array) else {
        return Err("the second submission holds no list of items".to_owned());
    };
    let mut merged = first.clone();
    let Some(items) = merged.get_mut("items").and_then(Json::as_array_mut) else {
        return Err("the first submission holds no list of items".to_owned());
    };
    let mut seen: Vec<&str> = Vec::new();
    for item in given {
        let id = item.get("id").and_then(Json::as_str).unwrap_or_default();
        if !asked.iter().any(|a| a == id) || seen.contains(&id) {
            return Err(
                "the second submission answers an item it was not asked, or one twice".to_owned(),
            );
        }
        seen.push(id);
        let mut item = item.clone();
        let toward_pass = matches!(
            item.get("disposition").and_then(Json::as_str),
            Some("held" | "allowed_change")
        );
        let located = matches!(
            item.pointer("/location/kind").and_then(Json::as_str),
            Some("bbox" | "polygon")
        );
        if strict && toward_pass && !located {
            item["confidence"] = json!("tentative");
        }
        if let Some(slot) = items
            .iter_mut()
            .find(|i| i.get("id").and_then(Json::as_str) == Some(id))
        {
            *slot = item;
        }
    }
    Ok(merged)
}

/// What a run that reached a verdict keeps beside it.
struct Concluded<'a> {
    retries: &'a [Json],
    second: Option<Json>,
    key: Option<&'a cache::Key>,
    reported: bool,
    edited: bool,
}

/// A run under a use case that reached its verdict: each item of its reason whole, one to
/// a line, beside the verdict's summary (the writer takes the secrets out of the whole
/// first); what it decided ([`JUDGED`]); and its key for the decision cache.
fn conclude(
    context: &Context,
    call: &PreparedCall,
    judging: &Judging,
    decided: &use_case::Decided,
    concluded: &Concluded<'_>,
) -> Result<Produced, Failure> {
    let mut lines = String::new();
    for item in &decided.items {
        lines.push_str(&Json::from(item.as_str()).to_string());
        lines.push('\n');
    }
    let kept = context
        .diagnostics()
        .write(use_case::REASONS, lines.as_bytes());
    kept.map_err(|why| failed("reasons", &why))?;
    let (kept, exact) = judging.keep(
        context,
        call,
        concluded.retries,
        decided,
        concluded.second.as_ref(),
    )?;
    // The findings are made from the document just kept, by the code a later run
    // answered from the cache makes them with (plan m4, task K2, items 1 and 3). The
    // decision they carry is worked out from every answer the document holds, and is
    // the verdict's.
    let findings = crate::findings::of_judged(&kept).map_err(|why| failed("findings", &why))?;
    if findings.decision() != decided.verdict.decision() {
        let why = "the decision of the answers that were kept is not the verdict's";
        return Err(failed("findings", &why));
    }
    if let Some(key) = concluded.key {
        let not_kept = if !concluded.reported {
            Some("not kept: the judge did not report its identity")
        } else if concluded.edited {
            Some("not kept: a tool file it ran was edited after its key was made")
        } else if !exact {
            // A later run would read the document as it is stored, and a secret taken
            // out of it as bytes may leave one that gives other findings or none.
            Some("not kept: what the run decided holds a secret, and is kept without it")
        } else {
            None
        };
        keep_key(context, key, not_kept);
    }
    let produced = Produced::nothing().with_verdict(decided.verdict.clone());
    Ok(produced.with_findings(findings.to_json()))
}

/// A run under a use case of the table: its job, what resolution made of it, its
/// checklist, and the repairs it has asked for.
struct Judging {
    job: Job,
    resolved: Resolved,
    checklist: Checklist,
    repairs: Vec<String>,
}

/// The job of a call under a use case, read and held to the call: in the contract
/// `job/1`, of the call's use case, with one image for each image the call gives, a
/// brief under `brief-compliance`, and resolved under the use case's rules, which refuse
/// a job that lacks a role the use case requires or names one it does not take.
fn judging(call: &PreparedCall, use_case: &str, job: Option<&Value>) -> Result<Judging, Failure> {
    let Some(Value::Text(text)) = job else {
        return Err(Failure::invalid(&format!(
            "{use_case} is given a job, in the contract job/1"
        )));
    };
    let job = Job::from_json(text).map_err(|why| Failure::invalid(&why))?;
    if job.use_case != use_case {
        return Err(Failure::invalid(
            "the job is of another use case than the call",
        ));
    }
    let given = (call.inputs().iter())
        .filter(|input| input.port() == "images")
        .count();
    let named = job.images.len();
    if named != given {
        return Err(Failure::invalid(&format!(
            "the job names {named} images, and the call gives {given}"
        )));
    }
    let briefed = job
        .brief
        .as_deref()
        .is_some_and(|brief| !brief.trim().is_empty());
    if use_case == "brief-compliance" && !briefed {
        return Err(Failure::invalid("brief-compliance is given a brief"));
    }
    let resolved = use_case::resolve(&job).map_err(|why| Failure::invalid(&why))?;
    let checklist = Checklist::build(&resolved, &job);
    Ok(Judging {
        job,
        resolved,
        checklist,
        repairs: Vec::new(),
    })
}

impl Judging {
    /// Keeps what the run decided among its diagnostics ([`JUDGED`]): the document, and
    /// whether it is stored exactly as it is here. It is not when it holds a secret of
    /// the run's: the document is then kept with the secret taken out, as any text is.
    fn keep(
        &self,
        context: &Context,
        call: &PreparedCall,
        retries: &[Json],
        decided: &use_case::Decided,
        second: Option<&Json>,
    ) -> Result<(Vec<u8>, bool), Failure> {
        let given = (call.inputs().iter()).filter(|input| input.port() == "images");
        let images: Vec<Json> = (self.job.images.iter().zip(given))
            .map(|(image, input)| {
                json!({"id": image.id, "role": image.role, "label": image.label,
                       "artifact": input.id().to_string()})
            })
            .collect();
        let mut kept = json!({
            "use_case": self.resolved.use_case,
            "rules_version": self.resolved.version,
            "images": images,
            "checklist": self.checklist.ids().collect::<Vec<_>>(),
            "repairs": self.repairs,
            "retries": retries,
            "answers": decided.answers,
        });
        if let Some(second) = second {
            kept["second_look"] = second.clone();
        }
        let kept = kept.to_string().into_bytes();
        let exact = context.diagnostics().write_exact(JUDGED, &kept).is_ok();
        if !exact {
            let written = context.diagnostics().write(JUDGED, &kept);
            written.map_err(|why| failed("judged", &why))?;
        }
        Ok((kept, exact))
    }
}

/// A run answered from the decision cache: the earlier run's verdict and its findings,
/// with what it kept of its reasons and of what it decided, and the earlier run named
/// in this run's record. Nothing is sent.
fn answered_from(context: &Context, hit: &cache::Hit) -> Result<Produced, Failure> {
    let facts = context.facts();
    facts.setting(cache::SETTING, &Value::text("hit"));
    facts.setting(cache::ANSWERED_BY, &Value::text(&hit.run));
    let judged = Some(hit.judged.clone());
    for (name, bytes) in [(use_case::REASONS, &hit.reasons), (JUDGED, &judged)] {
        if let Some(bytes) = bytes {
            let written = context.diagnostics().write(name, bytes);
            written.map_err(|why| failed("cache", &why))?;
        }
    }
    let said = format!(
        "answered from the decision cache: the verdict of run {}",
        hit.run
    );
    context.events().progress(&said);
    let produced = Produced::nothing().with_verdict(Verdict::new(hit.decision, &hit.why));
    Ok(produced.with_findings(hit.findings.to_json()))
}

/// Keeps a run's key for a later run to answer from, unless there is a reason not to
/// (`not_kept`: a judge that did not say which model answered, a tool file edited, a
/// document of what was decided that is not stored as it was made); and notes in the
/// record whether it was kept. A key that cannot be written is not kept, and the run's
/// verdict stands.
fn keep_key(context: &Context, key: &cache::Key, not_kept: Option<&str>) {
    let said = if let Some(why) = not_kept {
        why
    } else if context
        .diagnostics()
        .write(&key.name(), key.material().as_bytes())
        .is_ok()
    {
        "kept"
    } else {
        "not kept: the key could not be written"
    };
    context.facts().setting(cache::SETTING, &Value::text(said));
}

/// Waits `ms` milliseconds before a request is sent again, looking every 50 at whether
/// the run was cancelled or its deadline passed, either of which ends the run.
fn wait_for(context: &Context, ms: u64) -> Result<(), Failure> {
    let until = Instant::now() + Duration::from_millis(ms);
    // The wait is at most RETRY_WAIT_LIMIT_MS, so this many looks are enough.
    for _ in 0..=(RETRY_WAIT_LIMIT_MS / 50) {
        if context.cancellation().is_cancelled() {
            return Err(failed(
                "cancelled",
                &"the run was cancelled while it waited",
            ));
        }
        if context.deadline_passed() {
            return Err(failed(
                "deadline",
                &"the run's deadline passed while it waited",
            ));
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        std::thread::sleep(left.min(Duration::from_millis(50)));
    }
    Ok(())
}
