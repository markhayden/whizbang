//! `segment`: an image and one query in, one mask for each instance the configured
//! segmenter finds, out (`SPEC.md` section 7, "`segment`, as an action over the segment
//! capability"; plan 2A, task R1; plan 1D, task Q1).
//!
//! **The query** is one of three kinds, named by the argument `kind`: a `phrase`, what to
//! find in words; a `point`, `X Y`, the thing at that place; or a `box`, `L T R B`, the
//! thing in that box, each number in pixels of the image as `segment` prints a box, apart
//! by spaces or commas. The text is the argument `query`. A point is in a pixel of the
//! image and a box inside it, with area (`SegmentRequest::new`).
//!
//! The image is sent through the handle the executor hands the run, so the run's
//! cancellation, deadline, admission and record are the executor's; the action never
//! names a backend itself. The backend is the default for the capability, or the one the
//! call names in `backend`; one that does not declare the feature the call's `kind`
//! names is refused when the call is prepared, and again when it is admitted, and
//! nothing runs (ADR 0003, "A feature a call chooses").
//!
//! **Each mask** is one instance's plane as its core, on the image's own binding, with
//! nothing protected and no blend, and one step in its origin: this action, its version,
//! the kind and the query asked, `max_instances`, which instance it is, counted from 1 in
//! the fixed order ([`in_order`]), best first, and the image. A run's output holds an
//! artifact's identity only, and a mask holds no score and no box, so both are in the
//! run's diagnostic [`DIAGNOSTIC`], one line for each mask in the order of the outputs:
//!
//! ```text
//! instances 2
//! mask sha256:<64 hex digits> score 0.9 box 10 20 30 40
//! mask sha256:<64 hex digits> score 0.8 box 0 0 4 4
//! ```
//!
//! each number the shortest decimal that reads back as the same double. Finding no
//! instance is an outcome, not a failure: the run completes, with no mask and
//! `instances 0`.
//!
//! **What the backend said beside its answer** (plan 1D, task V1) is in the same
//! diagnostic, the same way whichever kind of backend answered: an instance's predicted
//! IoU, where its model gave one, ends its mask's line as `iou 0.97`; and each line of the
//! backend's own diagnostics follows the masks, after `backend `:
//!
//! ```text
//! instances 1
//! mask sha256:<64 hex digits> score 0.95 box 10 20 30 40 iou 0.95
//! backend 1 instances on mps
//! ```
//!
//! A request the backend failed leaves one line in the same diagnostic, `failed`, the
//! fault's kind and its sentence, which a remote backend leads with the request's
//! dispatch id (ADR 0046):
//!
//! ```text
//! failed timed_out: dispatch <32 hex digits>: ...
//! ```
//!
//! `whizbang runs show <run> --diagnostic diagnostics/segment.txt` prints it.

use whizbang_core::artifact::{
    self, Artifact, ArtifactType, ImageRole, Mask, Origin, Source, Step, UseCase,
};
use whizbang_core::capability::{INSTANCE_LIMIT, Picture, Query, SegmentRequest, feature};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Capability, Context, Count, Failure, InputPort, Kind,
    OutputPort, PortRef, PreparedCall, Produced, Requirement, Value, ValueType,
};
use whizbang_core::identity::DecodeLimits;

use crate::grow::text_of;

/// The action's name.
pub const NAME: &str = "segment";

/// The action's version, which a mask it made records in its origin. 2.0.0 took the query
/// as its kind and its text, where 1.0.0 took a phrase alone; 2.1.0 reads the instances in
/// the fixed order ([`in_order`]), where 2.0.0 took them in the backend's.
pub const VERSION: &str = "2.1.0";

/// The kinds of query, by the name the argument `kind` takes, which is the feature a
/// backend must declare to be asked one.
pub const KINDS: [&str; 3] = [feature::PHRASE, feature::POINT, feature::BOX];

/// The query a call asks, from its kind and its text: a phrase as it is; a point as two
/// numbers and a box as four, apart by spaces or commas. Why not, as a sentence.
pub fn query_of(kind: &str, text: &str) -> Result<Query, String> {
    let numbers = || -> Option<Vec<f64>> {
        text.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|word| !word.is_empty())
            .map(|word| word.parse::<f64>().ok().filter(|n| n.is_finite()))
            .collect()
    };
    let unfit = |unfit: whizbang_core::capability::Unfit| unfit.to_string();
    match kind {
        feature::PHRASE => Query::phrase(text).map_err(unfit),
        feature::POINT => match numbers().as_deref() {
            Some(&[x, y]) => Query::point(x, y).map_err(unfit),
            _ => Err("a point is two numbers, X and Y, in pixels of the image".to_owned()),
        },
        feature::BOX => match numbers().as_deref() {
            Some(&[left, top, right, bottom]) => {
                Query::boxed(left, top, right, bottom).map_err(unfit)
            }
            _ => Err(
                "a box is four numbers, left, top, right and bottom, in pixels of the image"
                    .to_owned(),
            ),
        },
        other => Err(format!("{other} is no kind of query: phrase, point or box")),
    }
}

/// The diagnostic every run that was answered writes: each mask with its score and box.
pub const DIAGNOSTIC: &str = "segment.txt";

/// How many instances a call asks for when it does not say.
pub const MAX_INSTANCES_DEFAULT: i64 = 8;

/// The order the instances of an answer are read in, whatever order the backend gave
/// them in (plan m3, task R1, item 5; the carry-over row on the order of a segmenter's
/// answer): best score first; then by the box's top, its left, its right and its bottom;
/// then by the predicted IoU, none first and then the higher; then by the plane's bytes.
/// It is the order of the pack's answers (ADR 0020, `whizbang_backends::pack::proto`), with
/// the query left out, since a call asks one, and the predicted IoU, best first, read
/// here and not from the wire: so a mask's place in a run's outputs, and the instance its
/// origin names, are the same whichever backend answered. Numbers are compared by IEEE
/// 754's total order. It is total: two instances it finds equal are the same instance.
pub fn in_order(
    one: (f64, [f64; 4], Option<f64>, &[u8]),
    other: (f64, [f64; 4], Option<f64>, &[u8]),
) -> std::cmp::Ordering {
    let number = |a: f64, b: f64| a.total_cmp(&b);
    let iou = match (one.2, other.2) {
        (Some(one), Some(other)) => number(other, one),
        (one, other) => one.is_some().cmp(&other.is_some()),
    };
    number(other.0, one.0)
        .then(number(one.1[1], other.1[1]))
        .then(number(one.1[0], other.1[0]))
        .then(number(one.1[2], other.1[2]))
        .then(number(one.1[3], other.1[3]))
        .then(iou)
        .then_with(|| one.3.cmp(other.3))
}

/// One instance as the diagnostic says it: the mask made of it, its score and its box
/// (left, top, right, bottom, in pixels of the image).
fn line_of(mask: &Mask, score: f64, bounds: [f64; 4], iou: Option<f64>) -> String {
    let edges: Vec<String> = bounds.iter().map(|edge| text_of(*edge)).collect();
    let iou = iou.map_or_else(String::new, |iou| format!(" iou {}", text_of(iou)));
    format!(
        "mask {} score {} box {}{iou}\n",
        mask.id().0,
        text_of(score),
        edges.join(" ")
    )
}

/// The action `segment`.
pub struct Segment {
    descriptor: ActionDescriptor,
}

impl Segment {
    pub fn new() -> Segment {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Finds each instance a phrase, a point or a box asks for in an image, as one mask for each.",
        )
        .argument(Argument::with_default(
            "kind",
            "What the query is: phrase, point or box.",
            ValueType::choice(&KINDS),
            Value::text(feature::PHRASE),
        ))
        .argument(Argument::required(
            "query",
            "A phrase in words; a point as X Y; a box as left top right bottom; in pixels.",
            ValueType::Text,
        ))
        .argument(Argument::with_default(
            "max_instances",
            "The most instances to find, from 1 to 64.",
            ValueType::integer(Some(1), Some(i64::from(INSTANCE_LIMIT))),
            Value::Integer(MAX_INSTANCES_DEFAULT),
        ))
        .argument(Argument::optional(
            "backend",
            "The segmenter, by its name in the config; the default when not given.",
            ValueType::Text,
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The picture to look in.",
        ))
        .output(
            OutputPort::counted(
                "mask",
                ArtifactType::Mask,
                "One mask for each instance found, its core the instance.",
                Count::between(0, INSTANCE_LIMIT),
            )
            .on(PortRef::input("image")),
        )
        .needs(
            Requirement::new(Capability::Segment)
                .feature_from("kind")
                .chosen_by("backend"),
        )
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Segment { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Segment {
    fn default() -> Segment {
        Segment::new()
    }
}

fn argument<'a>(call: &'a PreparedCall, name: &str) -> Option<&'a Value> {
    call.arguments()
        .iter()
        .find_map(|(given, value)| (given == name).then_some(value))
}

impl Action for Segment {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        // A call that was prepared has the query and its kind: the descriptor requires
        // the one and gives the other a default.
        let Some(Value::Text(text)) = argument(call, "query") else {
            return Err(Failure::error("argument", "the call gives no query"));
        };
        let kind = match argument(call, "kind") {
            Some(Value::Text(kind)) => kind.as_str(),
            _ => feature::PHRASE,
        };
        let most = match argument(call, "max_instances") {
            Some(Value::Integer(most)) => *most,
            _ => MAX_INSTANCES_DEFAULT,
        };
        let most = u32::try_from(most)
            .map_err(|_| Failure::error("argument", "max_instances is from 1 to 64"))?;
        let input = call
            .inputs()
            .iter()
            .find(|input| input.port() == "image")
            .ok_or_else(|| Failure::error("input", "the call gives no image"))?;
        let segment = context
            .capabilities()
            .segment()
            .ok_or_else(|| Failure::error("segment", "the run was handed no segmenter"))?;

        // The store's own words for a failure name its files by their paths, and a
        // message is kept in the run's record: so the words without paths.
        let limits = DecodeLimits::default();
        let read = artifact::get(context.store(), ArtifactType::Image, input.id(), &limits)
            .map_err(|error| Failure::error("input", &error.without_paths()))?;
        let Artifact::Image(image) = read else {
            return Err(Failure::error("input", "what was read is not an image"));
        };
        if image.role() != ImageRole::Picture {
            return Err(Failure::invalid(
                "the image is a rendering of another artifact, and a rendering is not segmented",
            ));
        }
        let pixels = image
            .pixels(&limits)
            .map_err(|error| Failure::error("input", &error.without_paths()))?;
        let query = query_of(kind, text).map_err(|why| Failure::invalid(&why))?;
        let binding = image.binding();
        let request = Picture::new(pixels)
            .and_then(|picture| SegmentRequest::new(picture, vec![query], most))
            .and_then(|request| request.under(&binding))
            .map_err(|unfit| Failure::invalid(&unfit.to_string()))?;
        context.events().progress("segment: read the image");

        let response = match segment.segment(&request) {
            Ok(response) => response,
            Err(fault) => {
                // What the backend's fault says is kept in the run's diagnostic too, so a
                // run that was cancelled or out of time, whose end keeps no reason, still
                // says it (plan m6, task R1: the dispatch id a remote request was charged
                // under). It is best kept: the run ends in the fault either way.
                let said = format!("failed {}: {}\n", fault.kind().name(), fault.sentence());
                let _ = context.diagnostics().write(DIAGNOSTIC, said.as_bytes());
                return Err(Failure::error("segment", fault.sentence()));
            }
        };
        let found = response.answers().first().map_or(&[][..], Vec::as_slice);
        // Each instance with its place in the answer, in the fixed order.
        let key = |at: usize| {
            let instance = &found[at];
            let iou = response.predicted_iou(0, at);
            (instance.score(), instance.bounds(), iou, instance.plane())
        };
        let mut order: Vec<usize> = (0..found.len()).collect();
        order.sort_by(|one, other| in_order(key(*one), key(*other)));

        let mut produced = Produced::nothing();
        let mut said = format!("instances {}\n", found.len());
        for (at, given) in order.into_iter().enumerate() {
            let instance = &found[given];
            let step = Step::new(NAME, VERSION)
                .argument("kind", kind)
                .argument("query", text)
                .argument("max_instances", &most.to_string())
                .argument("instance", &(at + 1).to_string())
                .input("image", image.id().0);
            let made = Origin::new(step).and_then(|origin| {
                Mask::from_core(
                    binding.clone(),
                    UseCase::Custom,
                    instance.plane().to_vec(),
                    Source::default(),
                    origin,
                )
            });
            let mask = made.map_err(|error| Failure::error("mask", &error.without_paths()))?;
            let iou = response.predicted_iou(0, given);
            said.push_str(&line_of(&mask, instance.score(), instance.bounds(), iou));
            produced = produced.with("mask", mask);
        }
        for line in response.diagnostics().lines() {
            said.push_str(&format!("backend {line}\n"));
        }
        context
            .diagnostics()
            .write(DIAGNOSTIC, said.as_bytes())
            .map_err(|why| Failure::error("diagnostic", &why))?;
        context.events().progress(&format!(
            "segment: {} instance{} found",
            found.len(),
            if found.len() == 1 { "" } else { "s" }
        ));
        Ok(produced)
    }
}

/// Registers the action.
pub fn register(
    registry: &mut whizbang_core::contract::Registry,
) -> Result<(), whizbang_core::contract::RegistrationError> {
    registry.register(std::sync::Arc::new(Segment::new()))
}
