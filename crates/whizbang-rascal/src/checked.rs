//! `remove_checked`: the loop "remove, check, retry" (`SPEC.md` section 3, "Loop across
//! tools"; section 7, "Bounded check and retry"; plan m4, task L1; ADR 0039). It removes
//! what a phrase names from a picture, has squint check the result, and tries again a
//! bounded number of times, every call a nested call on the one reservation of its run.
//!
//! **An attempt** is one removal with the check of its result, made of calls of the
//! shipped recipe `remove_prompt`, run in stretches (ADR 0033, "A run from a step"), and
//! of squint's checking recipe `source-fidelity`; a located retry also calls `view`,
//! `segment`, `carry` and the recipe `mask_rect`, to make its found mask.
//!
//! - *Find* (the first attempt only): `remove_prompt` until its step `select`. The loop
//!   is handed one mask, the found mask. A located retry makes its own ([`located`]).
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
//! **What may enlarge a mask** is one table, [`table`]. **A retry** is one of two kinds
//! (plan m4, task L2). *Located*, when a finding the table allows a retry on names a
//! place on the attempt's picture: the picture the loop was given is cut to each place
//! with `view`, `segment` is asked the same phrase inside the crop, each mask it gives is
//! carried onto the picture with `carry`, a place where nothing is found joins as itself
//! (`mask_rect`), and `remove_prompt` from `select` until `select` joins them to the
//! found mask of the attempt before: the found mask of this attempt ([`located`]).
//! *Wider*, when no such finding names a usable place: the shape stretch run again from
//! the same found mask with twice the `px` of its step `grow`, once in a run.
//!
//! **What is handed back.** The loop completes whenever a fill completed, and commits one
//! attempt's picture and mask as its own ([`kept`]), with that attempt's verdict and
//! findings when it has them and a short result ([`RESULT`]). It ends as an error only
//! when no fill completed, with the type and the message of the call that failed.

use serde_json::{Value as Json, json};
use whizbang_core::artifact::{self, ArtifactId, ArtifactType};
use whizbang_core::capability::PHRASE_LIMIT;
use whizbang_core::contract::record::{ATTEMPT_TEXT_LIMIT, Attempt, Changed, Crop, Decision};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Call, CallError, Capability, Context, Failure, Findings,
    Finished, InputPort, Kind, Outcome, OutputPort, PLAN_WHY_LIMIT, Plan, Planning, PreparedCall,
    Produced, RefusalKind, Registry, Requirement, ResultKind, RunId, Value, ValueType, Verdict,
};
use whizbang_core::identity::{DecodeLimits, Digest};
use whizbang_core::recipe::{FROM, Recipe, SET, UNTIL, Written, from_port, until_port};

use crate::{carry, operation, segment};

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

/// What a located retry looks into a place with, and what joins a place as itself.
pub const VIEW: &str = "view";
pub const ITSELF: &str = "mask_rect";

/// The steps of [`REMOVE`] the stretches are named by.
const SEGMENT: &str = "segment";
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
    // The template is written above, in full, and a test of the binary's reads what
    // this makes of it with squint's own reader (`checked::the_job_the_loop_writes_…`).
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

/// From the places findings named to what a located retry cuts (plan m4, task L2, items
/// 2 and 3; ADR 0039, "The located retry"): whole numbers of pixels of the picture the
/// loop was given, which is the size of the picture a place is on.
///
/// The rule of a place is rascal's (`crates/rascal-mask/src/retry.rs`, `padded` and the
/// crop of a place, at the commit `docs/sources.toml` pins): the place's own pixels, and
/// a cut around them widened by [`PAD`] of its size on each side. Two things are added,
/// because the cut is made by `view`: a cut is at least [`CUT_MIN`] pixels each way, and
/// a place whose cut would be over [`CUT_MAX`] on a side is not used, since `view` would
/// scale it down and a mask on a scaled crop has no exact place on the picture.
pub mod located {
    use whizbang_core::contract::FoundAt;
    use whizbang_core::identity::Digest;

    /// The most places one retry looks into: rascal's bound, and the most a findings
    /// document holds.
    pub const PLACES_MAX: usize = 8;

    /// The most instances `segment` is asked for inside one place.
    pub const INSTANCES_MAX: u32 = 4;

    /// How far a cut reaches past its place on each side, as a share of the place's
    /// size that way: rascal's crop pad.
    pub const PAD: f64 = 0.05;

    /// The least `view` cuts on a side, in pixels.
    pub const CUT_MIN: u32 = 8;

    /// The most `view` shows pixel for pixel on a side: a longer cut is scaled down.
    pub const CUT_MAX: u32 = 1024;

    /// One place a located retry uses: left, top, right and bottom, in whole pixels,
    /// the right and the bottom one past the last.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Place {
        /// The place's own pixels: those whose centres are inside what was cited.
        pub own: [u32; 4],
        /// What is cut from the picture to look for the named thing in.
        pub cut: [u32; 4],
    }

    impl Place {
        /// The cut as `view` is asked for it, in fractions of the picture: each edge
        /// half a pixel inside the cut, so that `view`, which snaps outward to whole
        /// pixels, cuts these pixels on every machine.
        pub fn fractions(&self, width: u32, height: u32) -> [f64; 4] {
            let [left, top, right, bottom] = self.cut.map(f64::from);
            let (width, height) = (f64::from(width), f64::from(height));
            [
                (left + 0.5) / width,
                (top + 0.5) / height,
                (right - 0.5) / width,
                (bottom - 0.5) / height,
            ]
        }
    }

    /// The pixels, from `low` to one past `high`, whose centres are at or past the
    /// fraction `from` of `size` and before the fraction `to`.
    fn centres(from: f64, to: f64, size: u32) -> (u32, u32) {
        let edge = |fraction: f64| {
            let at = (fraction * f64::from(size) - 0.5).ceil();
            at.clamp(0.0, f64::from(size)) as u32
        };
        (edge(from), edge(to))
    }

    /// One side of a cut: the place's side widened by [`PAD`] of it each way, snapped
    /// outward, held inside the picture, and then to at least [`CUT_MIN`] pixels.
    fn side(low: u32, high: u32, size: u32) -> Option<(u32, u32)> {
        let reach = PAD * f64::from(high - low);
        let near = (f64::from(low) - reach).floor().max(0.0) as u32;
        let far = ((f64::from(high) + reach).ceil() as u32).min(size);
        if far - near >= CUT_MIN {
            return Some((near, far));
        }
        if size < CUT_MIN {
            return None;
        }
        let near = near.saturating_sub((CUT_MIN - (far - near)) / 2);
        let far = (near + CUT_MIN).min(size);
        Some((far - CUT_MIN, far))
    }

    /// The box a place cites, in fractions: its own, or the box around its polygon.
    fn boxed(place: &FoundAt) -> Option<[f64; 4]> {
        if let Some(sides) = place.boxed {
            return Some(sides);
        }
        let points = place.polygon.as_ref()?;
        let over = |axis: usize, least: bool| {
            let each = points.iter().map(|point| point[axis]);
            match least {
                true => each.fold(f64::INFINITY, f64::min),
                false => each.fold(f64::NEG_INFINITY, f64::max),
            }
        };
        Some([over(0, true), over(1, true), over(0, false), over(1, false)])
    }

    /// The places a located retry uses, of those the findings the table allows a retry
    /// on cited, in their order: each on `candidate`, the picture the check was shown;
    /// each once; each covering a pixel of a picture `width` by `height`, with a cut
    /// `view` shows pixel for pixel; and the first [`PLACES_MAX`] of them. A polygon is
    /// used by the box around it. A place that is none of this is no place: a finding
    /// that cites only such is one that cites none.
    pub fn usable(
        places: &[Option<FoundAt>],
        candidate: Digest,
        width: u32,
        height: u32,
    ) -> Vec<Place> {
        let mut used: Vec<Place> = Vec::new();
        for place in places.iter().flatten() {
            if place.image != candidate {
                continue;
            }
            let Some([left, top, right, bottom]) = boxed(place) else {
                continue;
            };
            if ![left, top, right, bottom]
                .iter()
                .all(|side| side.is_finite())
            {
                continue;
            }
            let ((x0, x1), (y0, y1)) = (centres(left, right, width), centres(top, bottom, height));
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            let (Some(across), Some(down)) = (side(x0, x1, width), side(y0, y1, height)) else {
                continue;
            };
            if across.1 - across.0 > CUT_MAX || down.1 - down.0 > CUT_MAX {
                continue;
            }
            let place = Place {
                own: [x0, y0, x1, y1],
                cut: [across.0, down.0, across.1, down.1],
            };
            if !used.contains(&place) && used.len() < PLACES_MAX {
                used.push(place);
            }
        }
        used
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
    /// [`REMOVE`], with the steps the stretches are named by, [`CHECK`], and [`ITSELF`];
    /// and over `primitives`, which must hold what a located retry calls beside them:
    /// `view`, `segment` and `carry`. What the loop needs is what one attempt of the
    /// two recipes needs; its plan states the rest.
    pub fn new(registry: &Registry, primitives: &Registry) -> Result<RemoveChecked, Unbuilt> {
        RemoveChecked::over(registry, primitives, crate::recipes::REMOVE_PROMPT)
    }

    /// The same, over this text of the recipe `remove_prompt`: what [`RemoveChecked::new`]
    /// is given the shipped text for, and a test an edited one.
    pub fn over(
        registry: &Registry,
        primitives: &Registry,
        text: &str,
    ) -> Result<RemoveChecked, Unbuilt> {
        let (Some(remove), Some(check)) = (registry.get(REMOVE), registry.get(CHECK)) else {
            return Err(unbuilt(
                "the recipes remove_prompt and source-fidelity are not both actions of this build",
            ));
        };
        // What a located retry calls beside them.
        if [VIEW, segment::NAME, carry::NAME]
            .iter()
            .any(|action| primitives.get(action).is_none())
            || registry.get(ITSELF).is_none()
        {
            return Err(unbuilt(
                "view, segment, carry and mask_rect are not all actions of this build",
            ));
        }
        // The stretches are named by steps of the recipe as it is shipped, and the
        // wider mask is twice what its step `grow` is given.
        let recipe = Recipe::read(text).map_err(|error| unbuilt(&error.to_string()))?;
        let steps: Vec<&str> = recipe.steps.iter().map(|step| step.id.as_str()).collect();
        if [SEGMENT, SELECT, GROW, FEATHER, FILL]
            .iter()
            .any(|step| !steps.contains(step))
        {
            return Err(unbuilt(
                "remove_prompt has not the steps segment, select, grow, feather and fill",
            ));
        }
        // The stretches are run as neighbours: a step between two of them would be run
        // by neither, or by both.
        let at = |id: &str| steps.iter().position(|step| *step == id);
        // A located retry joins its masks by running `select` alone, given them as
        // what `segment` found.
        if at(SEGMENT) != Some(0) || at(SELECT) != Some(1) {
            return Err(unbuilt(
                "the steps segment and select of remove_prompt are not its first two",
            ));
        }
        if at(GROW) != at(SELECT).map(|select| select + 1) {
            return Err(unbuilt(
                "the step grow of remove_prompt does not come directly after select",
            ));
        }
        if at(FILL) != at(FEATHER).map(|feather| feather + 1) || at(FILL) != Some(steps.len() - 1) {
            return Err(unbuilt(
                "the step fill of remove_prompt does not come directly after feather, and last",
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
    /// with the segmenter asked once in the first attempt and once for each place a
    /// retry may look into ([`located::PLACES_MAX`]); or one attempt, unchecked, with
    /// the reason, when no judge is ready or the cap covers no check, unless the check
    /// is required.
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
        // What one attempt sends the filler: what its bound above was counted from.
        let fills = (planning.capability(Capability::Fill)).map_or(1, |able| able.requests());
        let unchecked = |why: &str| {
            Plan::new()
                .sending(Capability::Fill, fills)
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
        // What the attempts send the segmenter together: what the first sends to find,
        // and each retry once for each place it may look into.
        let finds = (planning.capability(Capability::Segment)).map_or(1, |able| able.requests());
        let segments = |attempts: u32| {
            let retries = u64::from(attempts.saturating_sub(1));
            finds.saturating_add(retries.saturating_mul(located::PLACES_MAX as u64))
        };
        // How many checked attempts the cap of one call covers. `segment` is the bound
        // of what the first attempt sends, so one request's is that over `finds`.
        let whole = |attempts: u32| {
            let each = fill.saturating_add(check);
            let one = segment / finds.max(1);
            (one.saturating_mul(segments(attempts)))
                .saturating_add(each.saturating_mul(u64::from(attempts)))
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
            .sending(Capability::Segment, segments(allowed))
            .sending(Capability::Fill, fills.saturating_mul(u64::from(allowed)))
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
    /// The loop's run was cancelled.
    Loop(Failure),
    /// The loop's deadline passed, before the call or while it ran.
    Deadline,
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
            Stopped::Deadline => {
                Failure::error("deadline", "the loop stopped: its deadline passed")
            }
            Stopped::Budget(sentence) => Failure::error("budget", &sentence),
            Stopped::Call { kind, message } => Failure::error(&kind, &message),
        }
    }

    /// In one line, for an attempt's reason.
    fn said(&self) -> String {
        match self {
            Stopped::Loop(_) => "the run was stopped".to_owned(),
            Stopped::Deadline => "the deadline passed".to_owned(),
            Stopped::Budget(sentence) => sentence.clone(),
            Stopped::Call { kind, message } => {
                format!("an error of the type {kind}: {message}")
            }
        }
    }
}

/// What the next attempt's retry changes.
enum Retry {
    /// The shape stretch's `grow` is given this `px`.
    Wider(f64),
    /// The found mask gains what is found in these places.
    Located(Vec<located::Place>),
}

/// The identity committed at a port of a call that completed, the first when there
/// are several.
fn one(finished: &Finished, port: &str) -> Option<ArtifactId> {
    at(finished, port).first().copied()
}

/// Every identity committed at a port of a call that completed, in order.
fn at(finished: &Finished, port: &str) -> Vec<ArtifactId> {
    let outputs = finished.outcome().outputs().iter();
    (outputs.filter(|output| output.port() == port))
        .map(|output| output.id())
        .collect()
}

/// A call that completed and handed back nothing at a port it was to.
fn missing(action: &str, port: &str) -> Stopped {
    Stopped::Call {
        kind: "output".to_owned(),
        message: format!("a call of {action} handed back no {port}"),
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
        // Held here, before any call is made, and not by the find after it was
        // admitted: a descriptor states no length of a text, so preparation cannot.
        if prompt.len() > PHRASE_LIMIT {
            return Err(Failure::invalid(&format!(
                "the prompt holds {} bytes, and a segmenter takes a phrase of {PHRASE_LIMIT} at most",
                prompt.len()
            )));
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
    /// in the run's record when it is a stretch of an attempt. A call that looks into
    /// a place has no label: a record holds at most 32, and two located retries of
    /// eight places make 48 such calls. What comes back is the call's run when it
    /// completed.
    fn made(
        &self,
        label: Option<&str>,
        call: &Call,
        runs: &mut Vec<RunId>,
    ) -> Result<Finished, Stopped> {
        let context = self.context;
        if context.cancellation().is_cancelled() {
            return Err(Stopped::Loop(self.cancelled()));
        }
        if context.deadline_passed() {
            return Err(Stopped::Deadline);
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
        if let Some(label) = label {
            context.label_step(finished.run(), label, &[], None);
        }
        // A call that did not complete under a loop that was cancelled, or whose
        // deadline has passed, stopped for that, whatever its own end says.
        let completed = matches!(finished.outcome(), Outcome::Completed { .. });
        if !completed && context.cancellation().is_cancelled() {
            return Err(Stopped::Loop(self.cancelled()));
        }
        if !completed && context.deadline_passed() {
            return Err(Stopped::Deadline);
        }
        match finished.outcome() {
            Outcome::Completed { .. } => Ok(finished),
            // Cancelled, and not by the loop's own signal or deadline: the call's own.
            Outcome::Cancelled => Err(Stopped::Call {
                kind: "cancelled".to_owned(),
                message: format!("the call of {} was cancelled", call.action()),
            }),
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

    fn cancelled(&self) -> Failure {
        Failure::error("cancelled", "the loop stopped: its run was cancelled")
    }

    fn say(&self, message: &str) {
        self.context
            .events()
            .progress(&format!("{NAME}: {message}"));
    }

    /// The found mask of a located retry (plan m4, task L2, items 3 and 4): for each
    /// place, the picture the loop was given cut to it, the phrase asked inside the
    /// crop, and each mask found carried onto the picture, or the place itself when
    /// nothing is found; then all of them joined to the found mask of the attempt
    /// before by the recipe's own `select`. With it, what the record lists of each crop.
    fn located(
        &self,
        number: u32,
        before: ArtifactId,
        places: &[located::Place],
        size: (u32, u32),
        runs: &mut Vec<RunId>,
    ) -> Result<(ArtifactId, Vec<Crop>), Stopped> {
        let mut joined = vec![before];
        let mut crops = Vec::new();
        for place in places {
            // The loop's own picture, never the fill the place was seen on.
            let mut call = Call::new(VIEW).input("image", self.image);
            let sides = place.fractions(size.0, size.1);
            for (name, side) in ["left", "top", "right", "bottom"].iter().zip(sides) {
                call = call.argument(name, Value::Number(side));
            }
            let finished = self.made(None, &call, runs)?;
            let crop = one(&finished, "crops").ok_or_else(|| missing(VIEW, "crop"))?;

            let mut call = Call::new(segment::NAME)
                .argument("query", Value::text(self.prompt))
                .argument(
                    "max_instances",
                    Value::Integer(i64::from(located::INSTANCES_MAX)),
                )
                .input("image", crop);
            if let Some(backend) = operation::text(self.call, "backend") {
                call = call.argument("backend", Value::text(backend));
            }
            let finished = self.made(None, &call, runs)?;
            // An instance has a pixel (`capability::Instance`), so a mask is something
            // found, and no mask is nothing found.
            let found = at(&finished, "mask");
            if found.is_empty() {
                // The judge said with confidence that the named thing is there, and the
                // table has held that it said so of nothing that must be preserved.
                let [left, top, right, bottom] = place.own;
                let call = Call::new(ITSELF)
                    .argument(
                        "box",
                        Value::text(&format!("{left} {top} {right} {bottom}")),
                    )
                    .input("image", self.image);
                let finished = self.made(None, &call, runs)?;
                joined.push(one(&finished, "mask").ok_or_else(|| missing(ITSELF, "mask"))?);
                crops.push(Crop {
                    crop: crop.0,
                    mask: None,
                });
                continue;
            }
            let mut call = Call::new(carry::NAME)
                .argument("left", Value::Integer(i64::from(place.cut[0])))
                .argument("top", Value::Integer(i64::from(place.cut[1])))
                .input("crop", crop)
                .input("image", self.image);
            for id in found {
                call = call.input("mask", id);
            }
            let finished = self.made(None, &call, runs)?;
            let carried = at(&finished, "mask");
            let Some(first) = carried.first() else {
                return Err(missing(carry::NAME, "mask"));
            };
            crops.push(Crop {
                crop: crop.0,
                mask: Some(first.0),
            });
            joined.extend(carried);
        }
        let mut call = self
            .remove()
            .argument(FROM, Value::text(SELECT))
            .argument(UNTIL, Value::text(SELECT));
        for id in joined {
            call = call.input(&from_port(SEGMENT, "mask"), id);
        }
        let finished = self.made(Some(&format!("a{number}_select")), &call, runs)?;
        let mask = one(&finished, &until_port(SELECT, "mask"));
        Ok((mask.ok_or_else(|| missing(REMOVE, "mask"))?, crops))
    }

    /// The size of the picture the loop was given, which is the size of every fill of
    /// it: read when a finding first names a place.
    fn size(&self) -> Option<(u32, u32)> {
        let space = operation::image(self.context, self.image).ok()?.space();
        Some((space.width(), space.height()))
    }

    fn run(&self) -> Result<Produced, Failure> {
        let context = self.context;
        context.freeze_steps([REMOVE, CHECK, VIEW, segment::NAME, carry::NAME, ITSELF]);
        context
            .facts()
            .setting(JOB_SETTING, &Value::Text(job_template_digest().to_string()));

        let mut filled: Vec<Filled> = Vec::new();
        let mut found: Option<ArtifactId> = None;
        let mut shaped_before: Option<ArtifactId> = None;
        // What the next attempt's retry changes; a wider mask is used once in a run.
        let mut next: Option<Retry> = None;
        let mut widened = false;
        let mut size: Option<(u32, u32)> = None;
        let mut ended = String::new();
        let mut begun = 0;

        for number in 1..=self.allowed {
            begun = number;
            let retry = next.take();
            // What the retry changed, as the record says it: a located retry's once
            // its crops are cut.
            let changed = std::cell::RefCell::new(match &retry {
                Some(Retry::Wider(px)) => Some(Changed::Wider { px: *px }),
                _ => None,
            });
            let before = spent(context);
            let mut runs: Vec<RunId> = Vec::new();
            let label = |stretch: &str| format!("a{number}_{stretch}");
            self.say(&format!("attempt {number} of {} starts", self.allowed));
            // Notes an attempt that has no verdict, and says so.
            let note = |attempt: Attempt, runs: &[RunId]| {
                let settled = (spent(context) - before).clamp(0, i128::from(u64::MAX)) as u64;
                let mut attempt = attempt.settled(settled);
                if let Some(changed) = changed.borrow().clone() {
                    attempt = attempt.changed(changed);
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
                    Stopped::Deadline => {
                        format!("the deadline passed after attempt {}", number - 1)
                    }
                    _ => format!("the retry {what}, and the attempts before it stand"),
                })
            };

            // Find, in the first attempt alone; a located retry makes its own found
            // mask from the one before, and a wider one keeps it.
            let mask = match (found, &retry) {
                (Some(before), Some(Retry::Located(places))) => {
                    let made =
                        self.located(number, before, places, size.unwrap_or_default(), &mut runs);
                    match made {
                        Ok((mask, crops)) => {
                            *changed.borrow_mut() = Some(Changed::Located { crops });
                            mask
                        }
                        Err(stopped) => {
                            ended = no_fill(stopped, "could not make its mask", &runs)?;
                            break;
                        }
                    }
                }
                (Some(mask), _) => mask,
                (None, _) => {
                    let call = self.remove().argument(UNTIL, Value::text(SELECT));
                    let finished =
                        self.made(Some(&label("find")), &call, &mut runs)
                            .and_then(|finished| {
                                one(&finished, &until_port(SELECT, "mask"))
                                    .ok_or_else(|| missing(REMOVE, "mask"))
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
            if let Some(Retry::Wider(px)) = &retry {
                call = call.argument(SET, Value::text(&format!("{GROW}.px={px}")));
            }
            let shaped = self
                .made(Some(&label("shape")), &call, &mut runs)
                .and_then(|finished| {
                    one(&finished, &until_port(FEATHER, "mask"))
                        .ok_or_else(|| missing(REMOVE, "mask"))
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
                .made(Some(&label("fill")), &call, &mut runs)
                .and_then(|finished| {
                    let image = one(&finished, "image").ok_or_else(|| missing(REMOVE, "image"))?;
                    let mask = one(&finished, "mask").ok_or_else(|| missing(REMOVE, "mask"))?;
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
            let checked = self.made(Some(&label("check")), &call, &mut runs);
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
                        Stopped::Deadline => {
                            "the deadline passed before its check ended".to_owned()
                        }
                        other => format!("the check ended in {}", other.said()),
                    };
                    let said = line(&said, ATTEMPT_TEXT_LIMIT);
                    note(Attempt::unchecked(number, &said), &runs);
                    attempt.unchecked = Some(said);
                    filled.push(attempt);
                    ended = match stopped {
                        Stopped::Budget(_) => format!("the budget ran out after attempt {number}"),
                        Stopped::Deadline => format!("the deadline passed after attempt {number}"),
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
                table::Reading::Remains(_) if number == self.allowed => {
                    "the last attempt the plan allows".to_owned()
                }
                // The places of the findings that allow the retry, and of no other:
                // on this attempt's picture, which is the size of the loop's own.
                table::Reading::Remains(places) => {
                    let named = places.iter().any(Option::is_some);
                    if named && size.is_none() {
                        size = self.size();
                    }
                    let usable = match (named, size) {
                        (true, Some((width, height))) => {
                            located::usable(&places, image.0, width, height)
                        }
                        _ => Vec::new(),
                    };
                    if !usable.is_empty() {
                        next = Some(Retry::Located(usable));
                        String::new()
                    } else if widened {
                        "a wider mask was already used once".to_owned()
                    } else {
                        widened = true;
                        next = Some(Retry::Wider(self.action.grow_px * 2.0));
                        String::new()
                    }
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
        // A run cancelled by now commits nothing, and its record then says that no
        // attempt was kept, whatever is said here: the record holds that of every run
        // that does not complete.
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
