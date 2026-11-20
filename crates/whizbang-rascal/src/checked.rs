//! `remove_checked`: the loop "remove, check, retry" (`SPEC.md` section 3, "Loop across
//! tools"; section 7, "Bounded check and retry"; plan m4, task L1; ADR 0039). It removes
//! what a phrase names from a picture, has squint check the result, and tries again a
//! bounded number of times, every call a nested call on the one reservation of its run.
//!
//! **An attempt** is one removal with the check of its result, made of calls of two
//! actions and no other: the shipped recipe `remove_prompt`, run in stretches (ADR 0033,
//! "A run from a step"), and squint's checking recipe `source-fidelity`.
//!
//! - *Find* (the first attempt only): `remove_prompt` until its step `select`. The loop
//!   is handed one mask, the found mask.
//! - *Shape*: `remove_prompt` from `grow` until `feather`, given the found mask. The loop
//!   is handed the shaped mask, the one a fill would paint.
//! - *Compare*: a retry whose shaped mask has the identity of the attempt's before it
//!   would repeat that attempt. It is not filled, and the attempts end.
//! - *Fill*: `remove_prompt` from `fill`, given the shaped mask.
//! - *Check*: `source-fidelity`, with the filled picture as the candidate and the
//!   picture the loop was given as the source, under the job [`job`] writes.
//!
//! So no fill is paid for before its mask is in hand, and the source of every attempt is
//! the picture the loop was given, never an earlier attempt's result.
//!
//! **The plan** of a call is settled when the call is prepared ([`Action::plan`]; ADR
//! 0004, "A call's plan"): how many attempts it may make, the first included, and
//! whether it checks, with the reason. The run reads it and never works it out again.
//!
//! **What may enlarge a mask** is one table, [`table`]. **A retry**, here, is the wider
//! mask: the shape stretch run again from the same found mask with twice the `px` of its
//! step `grow`, once in a run. The retry aimed at the place a finding named is the next
//! task's (plan m4, task L2); until then a finding's place is not used.
//!
//! **What is handed back.** The loop completes whenever a fill completed, and commits one
//! attempt's picture and mask as its own ([`kept`]), with that attempt's verdict and
//! findings when it has them and a short result ([`RESULT`]). It ends as an error only
//! when no fill completed, with the type and the message of the call that failed.

use std::sync::Arc;

use serde_json::{Value as Json, json};
use whizbang_core::artifact::{self, ArtifactId, ArtifactType};
use whizbang_core::contract::record::{ATTEMPT_TEXT_LIMIT, Attempt, Changed, Decision};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Call, CallError, Capability, Context, Failure, Findings,
    Finished, InputPort, Kind, Outcome, OutputPort, PLAN_WHY_LIMIT, Plan, Planning, PreparedCall,
    Produced, RefusalKind, Registry, Requirement, ResultKind, RunId, Value, ValueType, Verdict,
};
use whizbang_core::identity::{DecodeLimits, Digest};
use whizbang_core::recipe::{FROM, Recipe, SET, UNTIL, Written, from_port, until_port};

use crate::operation;

/// The action's name.
pub const NAME: &str = "remove_checked";

/// The action's version.
pub const VERSION: &str = "1.0.0";

/// The recipe an attempt's mask and fill are made with, in stretches.
pub const REMOVE: &str = "remove_prompt";

/// Squint's checking recipe, which the loop calls and looks no further into.
pub const CHECK: &str = "source-fidelity";

/// The most attempts a call may ask, the first included, and how many it asks when it
/// says nothing: rascal's limit, kept.
pub const ATTEMPTS_MAX: u32 = 3;

/// The steps of [`REMOVE`] the stretches are named by.
const SELECT: &str = "select";
const GROW: &str = "grow";
const FEATHER: &str = "feather";
const FILL: &str = "fill";

/// The setting a run notes: the digest of [`JOB_TEMPLATE`].
pub const JOB_SETTING: &str = "job.template";

/// The kind of the short result a run hands back as its measurements.
pub const RESULT: &str = "remove_checked/1";

/// The template of the job the check is given (`job/1`, ADR 0028), version 1: JSON in
/// which `{phrase}` stands for the call's phrase. The phrase is put in as the text it
/// is, inside JSON strings, and reaches the judge as the data of a job, never as an
/// instruction of the loop's. The requirements' ids are fixed: [`table`] reads them.
pub const JOB_TEMPLATE: &str = r#"{
  "contract": "job/1",
  "use_case": "source-fidelity",
  "brief": "The candidate is the source with one thing removed and the place where it was filled in. What was to be removed, in the words of the person who asked: {phrase}",
  "requirements": [
    {"id": "RQ1", "kind": "must_hold", "text": "What was to be removed is no longer visible in the candidate: {phrase}"},
    {"id": "RQ2", "kind": "must_hold", "text": "Nothing else in the picture changed."},
    {"id": "RQ3", "kind": "must_hold", "text": "The filled area looks like its surroundings."}
  ],
  "images": [
    {"id": "candidate", "role": "candidate", "label": null},
    {"id": "source", "role": "source", "label": null}
  ]
}"#;

/// The digest of [`JOB_TEMPLATE`], which every run notes ([`JOB_SETTING`]).
pub fn job_template_digest() -> Digest {
    Digest::of_bytes(JOB_TEMPLATE.as_bytes())
}

/// The job the check of a removal of `phrase` is given, as the argument `job` holds it.
pub fn job(phrase: &str) -> String {
    fn filled(value: &mut Json, phrase: &str) {
        match value {
            Json::String(text) => *text = text.replace("{phrase}", phrase),
            Json::Array(items) => items.iter_mut().for_each(|item| filled(item, phrase)),
            Json::Object(fields) => fields.values_mut().for_each(|item| filled(item, phrase)),
            _ => {}
        }
    }
    // The template is written above, in full, and a test of this crate reads it.
    let mut job: Json = serde_json::from_str(JOB_TEMPLATE).unwrap_or(Json::Null);
    filled(&mut job, phrase);
    job.to_string()
}

/// What may enlarge a mask: the one table (plan m4, task L1, item 4; ADR 0039, "The
/// table"). A finding is read by three things and nothing else: every obligation it
/// cites, its disposition and its confidence; never by a judge's words, and the same
/// for both forms of answer.
pub mod table {
    use whizbang_core::contract::{Confidence, Disposition, Finding, Findings, FoundAt};

    /// What must be preserved: a protected subject, the scene's geometry, and the
    /// job's requirement that nothing else changed.
    pub const PRESERVED: [&str; 3] = ["SF1", "SF2", "RQ2"];

    /// The job's requirement that the named thing is no longer visible.
    pub const NAMED: &str = "RQ1";

    /// The row of the table one finding is of. Row 0 is not a finding's: it is the
    /// findings saying they are not complete ([`Reading::CutShort`]).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Row {
        /// Row 1: a violation that cites anything to be preserved, at either
        /// confidence, whatever else it cites. Ends the attempts, and beats every row.
        Damaged,
        /// Row 2: a confident violation of the named requirement that cites nothing
        /// to be preserved. The one row that allows a retry.
        Remains,
        /// Row 3: a tentative violation of the named requirement. Enlarges nothing.
        NotSure,
        /// Row 4: held, an allowed change, hidden, cannot tell, or observed. Enlarges
        /// nothing.
        NotAViolation,
        /// Row 5: a violation of anything else alone. Enlarges nothing, and forbids
        /// nothing.
        Other,
    }

    /// The row a finding is of.
    pub fn row(finding: &Finding) -> Row {
        if finding.disposition != Disposition::Violation {
            return Row::NotAViolation;
        }
        let cites = |id: &str| finding.obligations.iter().any(|cited| cited == id);
        if PRESERVED.iter().any(|id| cites(id)) {
            return Row::Damaged;
        }
        match (cites(NAMED), finding.confidence) {
            (true, Some(Confidence::Confident)) => Row::Remains,
            (true, _) => Row::NotSure,
            (false, _) => Row::Other,
        }
    }

    /// What a verdict's findings allow, read whole.
    #[derive(Debug, Clone, PartialEq)]
    pub enum Reading {
        /// Row 0: the findings were cut short. Read before any other row: what was
        /// dropped may be a finding of row 1.
        CutShort,
        /// A finding of row 1 is among them.
        Damaged,
        /// No finding of row 1, and at least one of row 2: a retry is allowed, from
        /// the places of the row 2 findings alone, each in the findings' order, none
        /// for a finding that cites no place.
        Remains(Vec<Option<FoundAt>>),
        /// Nothing in them says, with confidence, that the named thing is still
        /// there.
        Nothing,
    }

    /// Reads findings by the table.
    pub fn read(findings: &Findings) -> Reading {
        if !findings.is_complete() {
            return Reading::CutShort;
        }
        let rows: Vec<Row> = findings.items().iter().map(row).collect();
        if rows.contains(&Row::Damaged) {
            return Reading::Damaged;
        }
        let places: Vec<Option<FoundAt>> = (findings.items().iter().zip(&rows))
            .filter(|(_, row)| **row == Row::Remains)
            .map(|(finding, _)| finding.place.clone())
            .collect();
        match places.is_empty() {
            true => Reading::Nothing,
            false => Reading::Remains(places),
        }
    }
}

/// One line of at most `limit` characters, with no control character and no space at
/// either end: what a plan's reason and an attempt's reason may be.
fn line(text: &str, limit: usize) -> String {
    let plain: String = (text.chars())
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let cut: String = plain.trim().chars().take(limit).collect();
    let cut = cut.trim().to_owned();
    match cut.is_empty() {
        true => "no reason was given".to_owned(),
        false => cut,
    }
}

/// The action `remove_checked`.
pub struct RemoveChecked {
    descriptor: ActionDescriptor,
    /// What the step `grow` of [`REMOVE`] is given as its `px`: a wider mask is twice it.
    grow_px: f64,
}

/// Why the loop was not built over a registry, or not registered, in a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unbuilt(String);

impl std::fmt::Display for Unbuilt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the loop {NAME} cannot be built: {}", self.0)
    }
}

impl std::error::Error for Unbuilt {}

fn unbuilt(why: &str) -> Unbuilt {
    Unbuilt(why.to_owned())
}

impl RemoveChecked {
    /// The loop over the actions of `registry`, which must hold the recipes it calls:
    /// [`REMOVE`], with the steps the stretches are named by, and [`CHECK`]. What the
    /// loop needs is what one attempt of theirs needs; its plan states the rest.
    pub fn new(registry: &Registry) -> Result<RemoveChecked, Unbuilt> {
        let (Some(remove), Some(check)) = (registry.get(REMOVE), registry.get(CHECK)) else {
            return Err(unbuilt(
                "the recipes remove_prompt and source-fidelity are not both actions of this build",
            ));
        };
        // The stretches are named by steps of the recipe as it is shipped, and the
        // wider mask is twice what its step `grow` is given.
        let recipe = Recipe::read(crate::recipes::REMOVE_PROMPT)
            .map_err(|error| unbuilt(&error.to_string()))?;
        let steps: Vec<&str> = recipe.steps.iter().map(|step| step.id.as_str()).collect();
        if [SELECT, GROW, FEATHER, FILL]
            .iter()
            .any(|step| !steps.contains(step))
        {
            return Err(unbuilt(
                "remove_prompt has not the steps select, grow, feather and fill",
            ));
        }
        let grow = recipe.steps.iter().find(|step| step.id == GROW);
        let px = grow.and_then(|step| match step.params.get("px") {
            Some(Written::Float(px)) => Some(*px),
            Some(Written::Integer(px)) => Some(*px as f64),
            _ => None,
        });
        let Some(grow_px) = px else {
            return Err(unbuilt("the step grow of remove_prompt states no px"));
        };

        let mut draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Loop,
            "Removes what a phrase names, has the result checked against its source, and tries again a bounded number of times.",
        )
        .use_when(
            "To remove a thing you can name in words and know whether the result was checked; it hands back its best attempt with the verdict, and says when it was not checked.",
        )
        .argument(Argument::required(
            "prompt",
            "What to remove, in words.",
            ValueType::Text,
        ))
        .argument(Argument::optional(
            "backend",
            "The segmenter, by its name in the config; the default when not given.",
            ValueType::Text,
        ))
        .argument(Argument::optional(
            "filler",
            "The filler, by its name in the config; the default when not given.",
            ValueType::Text,
        ))
        .argument(Argument::optional(
            "judge",
            "The judge, by its name in the config; the default when not given.",
            ValueType::Text,
        ))
        .argument(Argument::with_default(
            "attempts",
            "The most attempts, the first included; fewer are made when the cap of one call covers fewer.",
            ValueType::integer(Some(1), Some(i64::from(ATTEMPTS_MAX))),
            Value::Integer(i64::from(ATTEMPTS_MAX)),
        ))
        .argument(Argument::with_default(
            "check",
            "auto checks when a judge is ready and the cap covers a check, and says when it did not; required refuses instead; off makes one unchecked pass.",
            ValueType::choice(&["auto", "required", "off"]),
            Value::text("auto"),
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The picture to remove it from.",
        ))
        .input(
            InputPort::optional(
                "protect",
                ArtifactType::Mask,
                "A mask on the picture whose pixels no attempt paints.",
            )
            .on("image"),
        )
        .output(OutputPort::one(
            "image",
            ArtifactType::Image,
            "The kept attempt's picture.",
        ))
        .output(OutputPort::one(
            "mask",
            ArtifactType::Mask,
            "The kept attempt's mask, rebound onto its picture.",
        ))
        .results(ResultKind::Verdict)
        .results(ResultKind::Findings)
        .results(ResultKind::Measurements)
        .plans()
        .concurrent(true);
        // What one attempt needs: the needs of the two recipes, each chosen by the
        // loop's argument of the same name. A need of `remove_prompt` that applies only
        // to a whole run of it (its filler) is a need of every checked removal.
        let mut seen = Vec::new();
        for descriptor in [remove.descriptor(), check.descriptor()] {
            for need in descriptor.needs() {
                if seen.contains(&need.capability()) {
                    return Err(unbuilt(
                        "remove_prompt and source-fidelity need one capability twice",
                    ));
                }
                seen.push(need.capability());
                let mut stated = Requirement::new(need.capability()).requests(need.sends());
                for feature in need.features() {
                    stated = stated.feature(feature);
                }
                if let Some(chooser) = need.chooser() {
                    stated = stated.chosen_by(chooser);
                }
                draft = draft.needs(stated);
            }
        }
        let descriptor = draft.build().map_err(|error| unbuilt(&error.why))?;
        for argument in ["backend", "filler", "judge"] {
            let chosen = (descriptor.needs().iter()).any(|need| need.chooser() == Some(argument));
            if !chosen {
                return Err(unbuilt(
                    "the segmenter, the filler and the judge are not each chosen by an argument",
                ));
            }
        }
        Ok(RemoveChecked {
            descriptor,
            grow_px,
        })
    }
}

/// Registers `remove_checked` over the actions `registry` holds already.
pub fn register(registry: &mut Registry) -> Result<(), Unbuilt> {
    let made = RemoveChecked::new(registry)?;
    let registered = registry.register(Arc::new(made));
    registered.map_err(|error| unbuilt(&error.to_string()))
}

/// The whole number of an argument of a planned call.
fn asked_attempts(planning: &Planning<'_>) -> u32 {
    match planning.argument("attempts") {
        Some(Value::Integer(asked)) => u32::try_from(*asked)
            .unwrap_or(ATTEMPTS_MAX)
            .clamp(1, ATTEMPTS_MAX),
        _ => ATTEMPTS_MAX,
    }
}

impl Action for RemoveChecked {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    /// The plan: the most attempts, up to the number asked, whose whole bound is within
    /// the cap of one call, each one request of the filler and one run of the check,
    /// with the segmenter asked once; or one attempt, unchecked, with the reason, when
    /// no judge is ready or the cap covers no check, unless the check is required.
    fn plan(&self, planning: &Planning<'_>) -> Option<Plan> {
        let asked = asked_attempts(planning);
        let mode = match planning.argument("check") {
            Some(Value::Text(mode)) => mode.as_str(),
            _ => "auto",
        };
        let judge = planning.capability(Capability::Judge);
        // The bound of everything one capability is sent in one attempt.
        let bound = |capability: Capability| -> u64 {
            let Some(able) = planning.capability(capability) else {
                return 0;
            };
            let one = able.bound().map_or(0, |bound| bound.micros());
            one.saturating_mul(able.requests())
        };
        let (segment, fill, check) = (
            bound(Capability::Segment),
            bound(Capability::Fill),
            bound(Capability::Judge),
        );
        let checks = judge.map_or(0, |able| able.requests());
        let unchecked = |why: &str| {
            Plan::new()
                .sending(Capability::Fill, 1)
                .sending(Capability::Judge, 0)
                .attempts(asked, 1, false)
                .because(&line(why, PLAN_WHY_LIMIT))
        };
        if mode == "off" {
            return Some(unchecked("the check is off"));
        }
        if mode == "auto"
            && let Some(why) = judge.and_then(|able| able.why_not())
        {
            return Some(unchecked(&format!("no judge is ready: {why}")));
        }
        // How many checked attempts the cap of one call covers.
        let whole = |attempts: u32| {
            let each = fill.saturating_add(check);
            segment.saturating_add(each.saturating_mul(u64::from(attempts)))
        };
        let covered = match planning.cap() {
            None => asked,
            Some(cap) => (1..=asked)
                .rev()
                .find(|attempts| whole(*attempts) <= cap.micros())
                .unwrap_or(0),
        };
        if covered == 0 && mode == "auto" {
            return Some(unchecked("the call cap does not cover a check"));
        }
        // A required check the cap does not cover is planned as one checked attempt,
        // and refused as any call over its cap is.
        let allowed = covered.max(1);
        let plan = Plan::new()
            .sending(Capability::Fill, u64::from(allowed))
            .sending(Capability::Judge, checks.saturating_mul(u64::from(allowed)))
            .attempts(asked, allowed, true);
        Some(match allowed < asked {
            true => plan.because(&format!(
                "the call cap covers {allowed} of the {asked} attempts asked"
            )),
            false => plan,
        })
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        Looping::new(self, call, context)?.run()
    }
}

/// One attempt that filled: what it made, and what its check came to.
struct Filled {
    number: u32,
    image: ArtifactId,
    mask: ArtifactId,
    verdict: Option<Verdict>,
    findings: Option<Findings>,
    /// Why it has no verdict, when it has none.
    unchecked: Option<String>,
}

impl Filled {
    /// Its place among the attempts when one is kept: a pass first, then unsure, then
    /// fail, then an attempt whose check could not run; then fewer findings; then the
    /// earlier.
    fn rank(&self) -> (u8, u64, u32) {
        let by_verdict = match self.verdict.as_ref().map(Verdict::decision) {
            Some(Decision::Pass) => 0,
            Some(Decision::Unsure) => 1,
            Some(Decision::Fail) => 2,
            None => 3,
        };
        let findings = self.findings.as_ref().map_or(0, |findings| {
            (findings.items().len() as u64).saturating_add(findings.left_out().findings)
        });
        (by_verdict, findings, self.number)
    }
}

/// The attempt that is kept, of those that filled: the first that passed; failing
/// that, an unsure before a fail, and either before an attempt whose check could not
/// run; then the attempt with fewer findings; then the earlier one.
fn kept(filled: &[Filled]) -> Option<&Filled> {
    filled.iter().min_by_key(|attempt| attempt.rank())
}

/// How a nested call of the loop's came back, when it did not complete.
enum Stopped {
    /// The loop's run was cancelled, or its deadline passed.
    Loop(Failure),
    /// The call was refused for want of allowance: the refusal's own sentence.
    Budget(String),
    /// The call ended otherwise: the type and the message of its own end.
    Call { kind: String, message: String },
}

impl Stopped {
    /// The failure of a loop that ends on this before any fill completed: the cause is
    /// the call's own.
    fn failure(self) -> Failure {
        match self {
            Stopped::Loop(failure) => failure,
            Stopped::Budget(sentence) => Failure::error("budget", &sentence),
            Stopped::Call { kind, message } => Failure::error(&kind, &message),
        }
    }

    /// In one line, for an attempt's reason.
    fn said(&self) -> String {
        match self {
            Stopped::Loop(_) => "the run was stopped".to_owned(),
            Stopped::Budget(sentence) => sentence.clone(),
            Stopped::Call { kind, message } => {
                format!("an error of the type {kind}: {message}")
            }
        }
    }
}

/// One run of the loop.
struct Looping<'a> {
    action: &'a RemoveChecked,
    call: &'a PreparedCall,
    context: &'a Context,
    image: ArtifactId,
    protect: Option<ArtifactId>,
    prompt: &'a str,
    allowed: u32,
    checks: bool,
    /// Why the plan checks nothing, when it checks nothing.
    why: Option<String>,
}

/// What the allowance has spent so far, as a signed amount that grows as it is spent:
/// what is owed, less what is left.
fn spent(context: &Context) -> i128 {
    let allowance = context.allowance();
    let owes = i128::from(allowance.owes_micros().unwrap_or(0));
    owes - i128::from(allowance.left_micros().unwrap_or(0))
}

impl<'a> Looping<'a> {
    fn new(
        action: &'a RemoveChecked,
        call: &'a PreparedCall,
        context: &'a Context,
    ) -> Result<Looping<'a>, Failure> {
        // The plan the call was prepared and admitted with: never worked out again.
        let stated = call
            .plan()
            .and_then(|plan| plan.stated().map(|s| (plan, s)));
        let Some((plan, attempts)) = stated else {
            return Err(Failure::error(
                "plan",
                "the loop was handed no plan of its attempts",
            ));
        };
        let prompt = operation::text(call, "prompt")
            .ok_or_else(|| Failure::invalid("the call gives no prompt"))?;
        if prompt.trim().is_empty() {
            return Err(Failure::invalid("the prompt names nothing to remove"));
        }
        Ok(Looping {
            action,
            call,
            context,
            image: operation::one(call, "image")?,
            protect: operation::given(call, "protect").first().copied(),
            prompt,
            allowed: attempts.allowed,
            checks: attempts.checks,
            why: plan.why().map(str::to_owned),
        })
    }

    /// A call of [`REMOVE`], with what every stretch is given.
    fn remove(&self) -> Call {
        let mut call = Call::new(REMOVE)
            .argument("prompt", Value::text(self.prompt))
            .input("image", self.image);
        for name in ["backend", "filler"] {
            if let Some(chosen) = operation::text(self.call, name) {
                call = call.argument(name, Value::text(chosen));
            }
        }
        if let Some(protect) = self.protect {
            call = call.input("protect", protect);
        }
        call
    }

    /// Makes one nested call, having looked at the run's signal first, and labels it
    /// in the run's record. What comes back is the call's run when it completed.
    fn made(&self, label: &str, call: &Call, runs: &mut Vec<RunId>) -> Result<Finished, Stopped> {
        let context = self.context;
        if context.cancellation().is_cancelled() || context.deadline_passed() {
            return Err(Stopped::Loop(self.stopped()));
        }
        let finished = context.executor().submit(call).map_err(|error| {
            let budget = match &error {
                CallError::Refused(refusal) => refusal.kind() == RefusalKind::OverCaller,
                _ => false,
            };
            match budget {
                true => Stopped::Budget(error.to_string()),
                false => Stopped::Call {
                    kind: "call".to_owned(),
                    message: error.to_string(),
                },
            }
        })?;
        runs.push(finished.run().clone());
        context.label_step(finished.run(), label, &[], None);
        match finished.outcome() {
            Outcome::Completed { .. } => Ok(finished),
            Outcome::Cancelled => Err(Stopped::Loop(self.stopped())),
            Outcome::Error { kind, message } => Err(Stopped::Call {
                kind: kind.clone(),
                message: message.clone(),
            }),
            Outcome::Invalid { why } => Err(Stopped::Call {
                kind: "invalid".to_owned(),
                message: why.clone(),
            }),
        }
    }

    fn stopped(&self) -> Failure {
        match self.context.cancellation().is_cancelled() {
            true => Failure::error("cancelled", "the loop stopped: its run was cancelled"),
            false => Failure::error("deadline", "the loop stopped: its deadline passed"),
        }
    }

    fn say(&self, message: &str) {
        self.context
            .events()
            .progress(&format!("{NAME}: {message}"));
    }

    fn run(&self) -> Result<Produced, Failure> {
        let context = self.context;
        context.freeze_steps([REMOVE, CHECK]);
        context
            .facts()
            .setting(JOB_SETTING, &Value::Text(job_template_digest().to_string()));
        let one = |finished: &Finished, port: &str| -> Option<ArtifactId> {
            let mut outputs = finished.outcome().outputs().iter();
            outputs.find_map(|output| (output.port() == port).then(|| output.id()))
        };
        let missing = |port: &str| Stopped::Call {
            kind: "output".to_owned(),
            message: format!("a call of {REMOVE} handed back no {port}"),
        };

        let mut filled: Vec<Filled> = Vec::new();
        let mut found: Option<ArtifactId> = None;
        let mut shaped_before: Option<ArtifactId> = None;
        // What the next attempt's retry changes: a wider mask, used once in a run.
        let mut wider: Option<f64> = None;
        let mut widened = false;
        let mut ended = String::new();
        let mut begun = 0;

        for number in 1..=self.allowed {
            begun = number;
            let changed = wider;
            let before = spent(context);
            let mut runs: Vec<RunId> = Vec::new();
            let label = |stretch: &str| format!("a{number}_{stretch}");
            self.say(&format!("attempt {number} of {} starts", self.allowed));
            // Notes an attempt that has no verdict, and says so.
            let note = |attempt: Attempt, runs: &[RunId]| {
                let settled = (spent(context) - before).clamp(0, i128::from(u64::MAX)) as u64;
                let mut attempt = attempt.settled(settled);
                if let Some(px) = changed {
                    attempt = attempt.changed(Changed::Wider { px });
                }
                let attempt = runs.iter().fold(attempt, |attempt, run| attempt.ran(run));
                // Said as it ends, for a door that tells its caller while the loop runs.
                let word = match (&attempt.verdict, &attempt.unchecked) {
                    (Some(Decision::Pass), _) => "pass".to_owned(),
                    (Some(Decision::Fail), _) => "fail".to_owned(),
                    (Some(Decision::Unsure), _) => "unsure".to_owned(),
                    (None, why) => format!("not checked: {}", why.as_deref().unwrap_or("-")),
                };
                self.say(&format!("attempt {number} ended: {word}"));
                context.facts().attempt(attempt);
            };
            // An attempt that made no fill: the first is the loop's error, with the
            // call's own cause; a retry leaves the attempt before it kept.
            let no_fill = |stopped: Stopped, what: &str, runs: &[RunId]| {
                if let Stopped::Loop(failure) = stopped {
                    return Err(failure);
                }
                if number == 1 {
                    return Err(stopped.failure());
                }
                let said = line(&format!("{what}: {}", stopped.said()), ATTEMPT_TEXT_LIMIT);
                note(Attempt::unchecked(number, &said), runs);
                Ok(match stopped {
                    Stopped::Budget(_) => {
                        format!("the budget ran out after attempt {}", number - 1)
                    }
                    _ => format!("the retry {what}, and the attempts before it stand"),
                })
            };

            // Find, in the first attempt alone.
            let mask = match found {
                Some(mask) => mask,
                None => {
                    let call = self.remove().argument(UNTIL, Value::text(SELECT));
                    let finished =
                        self.made(&label("find"), &call, &mut runs)
                            .and_then(|finished| {
                                one(&finished, &until_port(SELECT, "mask"))
                                    .ok_or_else(|| missing("mask"))
                            });
                    match finished {
                        Ok(mask) => mask,
                        // No fill has completed: the loop's error, with the call's
                        // own cause.
                        Err(Stopped::Loop(failure)) => return Err(failure),
                        Err(stopped) => return Err(stopped.failure()),
                    }
                }
            };
            found = Some(mask);

            // Shape: the mask a fill would paint.
            let mut call = self
                .remove()
                .argument(FROM, Value::text(GROW))
                .argument(UNTIL, Value::text(FEATHER))
                .input(&from_port(SELECT, "mask"), mask);
            if let Some(px) = changed {
                call = call.argument(SET, Value::text(&format!("{GROW}.px={px}")));
            }
            let shaped = self
                .made(&label("shape"), &call, &mut runs)
                .and_then(|finished| {
                    one(&finished, &until_port(FEATHER, "mask")).ok_or_else(|| missing("mask"))
                });
            let shaped = match shaped {
                Ok(shaped) => shaped,
                Err(stopped) => {
                    ended = no_fill(stopped, "could not make its mask", &runs)?;
                    break;
                }
            };

            // Compare: a retry that would paint what the attempt before it painted.
            if shaped_before == Some(shaped) {
                let said = "its shaped mask is the one before it, and nothing was filled";
                note(Attempt::unchecked(number, said), &runs);
                ended = format!(
                    "the mask of attempt {number} is the one before it, and an attempt is not made twice"
                );
                break;
            }
            shaped_before = Some(shaped);

            // Fill: paid for only now, with the mask in hand.
            let call = self
                .remove()
                .argument(FROM, Value::text(FILL))
                .input(&from_port(FEATHER, "mask"), shaped);
            let made = self
                .made(&label("fill"), &call, &mut runs)
                .and_then(|finished| {
                    let image = one(&finished, "image").ok_or_else(|| missing("image"))?;
                    let mask = one(&finished, "mask").ok_or_else(|| missing("mask"))?;
                    Ok((image, mask))
                });
            let (image, mask) = match made {
                Ok(made) => made,
                Err(stopped) => {
                    ended = no_fill(stopped, "could not be filled", &runs)?;
                    break;
                }
            };
            let mut attempt = Filled {
                number,
                image,
                mask,
                verdict: None,
                findings: None,
                unchecked: None,
            };

            if !self.checks {
                let why = self
                    .why
                    .clone()
                    .unwrap_or_else(|| "the plan checks nothing".to_owned());
                let said = line(&why, ATTEMPT_TEXT_LIMIT);
                note(Attempt::unchecked(number, &said), &runs);
                attempt.unchecked = Some(said);
                filled.push(attempt);
                ended = line(&format!("one unchecked pass: {why}"), ATTEMPT_TEXT_LIMIT);
                break;
            }

            // Check: squint's recipe, and nothing under it.
            let mut call = Call::new(CHECK)
                .argument("job", Value::text(&job(self.prompt)))
                .input("images", image)
                .input("images", self.image);
            if let Some(judge) = operation::text(self.call, "judge") {
                call = call.argument("judge", Value::text(judge));
            }
            let checked = self.made(&label("check"), &call, &mut runs);
            let finished = match checked {
                Ok(finished) => finished,
                Err(Stopped::Loop(failure)) => {
                    // The fill completed, and its attempt is in the record.
                    note(
                        Attempt::unchecked(number, "the run was stopped before its check ended"),
                        &runs,
                    );
                    return Err(failure);
                }
                Err(stopped) => {
                    let said = match &stopped {
                        Stopped::Budget(sentence) => format!("the budget ran out: {sentence}"),
                        other => format!("the check ended in {}", other.said()),
                    };
                    let said = line(&said, ATTEMPT_TEXT_LIMIT);
                    note(Attempt::unchecked(number, &said), &runs);
                    attempt.unchecked = Some(said);
                    filled.push(attempt);
                    ended = match stopped {
                        Stopped::Budget(_) => format!("the budget ran out after attempt {number}"),
                        _ => format!("the check of attempt {number} could not run"),
                    };
                    break;
                }
            };
            // The verdict is the decision the findings carry, never one worked out
            // from the list; a check that hands back no findings allows nothing.
            let findings = finished.findings().cloned();
            let decision = (findings.as_ref().map(Findings::decision))
                .or_else(|| finished.verdict().map(Verdict::decision));
            let Some(decision) = decision else {
                let said = "the check handed back no verdict";
                note(Attempt::unchecked(number, said), &runs);
                attempt.unchecked = Some(said.to_owned());
                filled.push(attempt);
                ended = format!("the check of attempt {number} could not run");
                break;
            };
            note(Attempt::judged(number, decision), &runs);
            let why = finished.verdict().map_or("", Verdict::why).to_owned();
            attempt.verdict = Some(Verdict::new(decision, &why));
            attempt.findings = findings.clone();
            filled.push(attempt);
            if decision == Decision::Pass {
                ended = format!("attempt {number} passed");
                break;
            }
            let reading = match &findings {
                Some(findings) => table::read(findings),
                None => table::Reading::CutShort,
            };
            ended = match reading {
                table::Reading::CutShort => format!(
                    "the findings of attempt {number} were cut short, and findings cut short never enlarge a mask"
                ),
                table::Reading::Damaged => format!(
                    "attempt {number} changed something that must be preserved, which painting more cannot mend"
                ),
                table::Reading::Nothing => format!(
                    "no finding of attempt {number} says with confidence that the named thing is still there"
                ),
                // Until the located retry lands, a place is not used: the mask is
                // widened whether the findings cite one or not.
                table::Reading::Remains(_) if number == self.allowed => {
                    "the last attempt the plan allows".to_owned()
                }
                table::Reading::Remains(_) if widened => {
                    "a wider mask was already used once".to_owned()
                }
                table::Reading::Remains(_) => {
                    widened = true;
                    wider = Some(self.action.grow_px * 2.0);
                    String::new()
                }
            };
            if !ended.is_empty() {
                break;
            }
        }
        if ended.is_empty() {
            ended = "the last attempt the plan allows".to_owned();
        }

        let Some(best) = kept(&filled) else {
            // Every way out of the first attempt without a fill has returned already.
            return Err(Failure::error("loop", "no attempt made a fill"));
        };
        context
            .facts()
            .attempts_ended(Some(best.number), &line(&ended, ATTEMPT_TEXT_LIMIT));
        let limits = DecodeLimits::default();
        let read = |artifact_type: ArtifactType, id: ArtifactId| {
            artifact::get(context.store(), artifact_type, id, &limits)
                .map_err(|error| Failure::error("output", &error.without_paths()))
        };
        let mut produced = Produced::nothing()
            .with("image", read(ArtifactType::Image, best.image)?)
            .with("mask", read(ArtifactType::Mask, best.mask)?)
            .with_measurements(json!({
                "result": RESULT,
                "checked": best.verdict.is_some(),
                "unchecked": best.unchecked,
                "attempts": begun,
                "kept": best.number,
                "ended": line(&ended, ATTEMPT_TEXT_LIMIT),
            }));
        if let Some(verdict) = &best.verdict {
            produced = produced.with_verdict(verdict.clone());
        }
        if let Some(findings) = &best.findings {
            produced = produced.with_findings(findings.to_json());
        }
        Ok(produced)
    }
}
