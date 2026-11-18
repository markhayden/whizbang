//! `segment`: an image and one phrase in, one mask for each instance the configured
//! segmenter finds, out (`SPEC.md` section 7, "`segment`, as an action over the segment
//! capability"; plan 2A, task R1).
//!
//! The image is sent through the handle the executor hands the run, so the run's
//! cancellation, deadline, admission and record are the executor's; the action never
//! names a backend itself. The backend is the default for the capability, or the one the
//! call names in `backend`; one that does not declare the feature `phrase` is refused
//! when the call is prepared, and nothing runs.
//!
//! **Each mask** is one instance's plane as its core, on the image's own binding, with
//! nothing protected and no blend, and one step in its origin: this action, its version,
//! the phrase, `max_instances`, which instance it is, counted from 1 in the order the
//! segmenter gave them, and the image. A run's output holds an artifact's identity only,
//! and a mask holds no score and no box, so both are in the run's diagnostic
//! [`DIAGNOSTIC`], one line for each mask in the order of the outputs:
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

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// The diagnostic every run that was answered writes: each mask with its score and box.
pub const DIAGNOSTIC: &str = "segment.txt";

/// How many instances a call asks for when it does not say.
pub const MAX_INSTANCES_DEFAULT: i64 = 8;

/// One instance as the diagnostic says it: the mask made of it, its score and its box
/// (left, top, right, bottom, in pixels of the image).
fn line_of(mask: &Mask, score: f64, bounds: [f64; 4]) -> String {
    let edges: Vec<String> = bounds.iter().map(|edge| text_of(*edge)).collect();
    format!(
        "mask {} score {} box {}\n",
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
            "Finds each instance of a phrase in an image, as one mask for each.",
        )
        .argument(Argument::required(
            "phrase",
            "What to find, in words.",
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
                .feature(feature::PHRASE)
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
        // A call that was prepared has the phrase: the descriptor requires it.
        let Some(Value::Text(phrase)) = argument(call, "phrase") else {
            return Err(Failure::error("argument", "the call gives no phrase"));
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
        let query = Query::phrase(phrase).map_err(|unfit| Failure::invalid(&unfit.to_string()))?;
        let binding = image.binding();
        let request = Picture::new(pixels)
            .and_then(|picture| SegmentRequest::new(picture, vec![query], most))
            .and_then(|request| request.under(&binding))
            .map_err(|unfit| Failure::invalid(&unfit.to_string()))?;
        context.events().progress("segment: read the image");

        let response = segment
            .segment(&request)
            .map_err(|fault| Failure::error("segment", fault.sentence()))?;
        let found = response.answers().first().map_or(&[][..], Vec::as_slice);

        let mut produced = Produced::nothing();
        let mut said = format!("instances {}\n", found.len());
        for (at, instance) in found.iter().enumerate() {
            let step = Step::new(NAME, VERSION)
                .argument("phrase", phrase)
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
            said.push_str(&line_of(&mask, instance.score(), instance.bounds()));
            produced = produced.with("mask", mask);
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
