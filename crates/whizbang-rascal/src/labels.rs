//! `labels`: an image and a list of classes in, the pixels a fixed-class segmenter labels
//! with each class, out (plan mE, task T2, item 2; ADR 0052).
//!
//! **A fixed-class segmenter** declares the feature `labels` beside `phrase`: it labels
//! every pixel of a picture with exactly one class of a closed list, and answers a phrase
//! that is one of its classes with at most one instance, the pixels so labelled, scored by
//! the class's confidence there (the adapter `eomt-ade20k`, ADE20K's 150 classes). So asking
//! every class of its list is the picture's labels: a class for each pixel, and each
//! class's confidence. This action asks them: the classes the call names, in batches of at
//! most [`QUERY_LIMIT`] phrases at one instance each, of one backend that declares both
//! features, chosen by `backend` or the default; one that declares no `labels` is refused
//! when the call is prepared, and nothing runs (ADR 0006). The model computes the picture
//! once for the batches (the adapter keeps the last picture's labels).
//!
//! **What it commits:** one mask at the port `mask` for each class some pixel has, in the
//! order the call lists the classes, its core the pixels of that class, on the image's own
//! binding, its origin naming this action, its version, the class and the image, and its
//! source the class, by the digest of `labels class <name>` ([`class_digest`]): so a
//! class's mask is not the mask `segment` makes of the same pixels when asked the class
//! as a phrase, which the store would refuse a second action's committing. A mask
//! holds no score, so the run hands back, as its measurements, the classes in the same
//! order, each with its confidence and its count of pixels:
//!
//! ```json
//! {"classes": [{"name": "sky", "score": 0.8, "pixels": 1200}, {"name": "floor", ...}]}
//! ```
//!
//! and writes the same, a line a class, in its diagnostic [`DIAGNOSTIC`], with what the
//! backend said beside each answer. Two classes that label one pixel, or more than one
//! instance of a class, is an answer no fixed-class segmenter gives: the run fails, and
//! nothing is committed. A pixel no class asked labels is not one: a call may ask some
//! classes alone.

use whizbang_core::artifact::{
    self, Artifact, ArtifactType, ImageRole, Mask, Origin, Source, Step, UseCase,
};
use whizbang_core::capability::{
    PHRASE_LIMIT, Picture, QUERY_LIMIT, Query, SegmentRequest, feature,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Capability, Context, Count, Failure, InputPort, Kind,
    OutputPort, PortRef, PreparedCall, Produced, Requirement, ResultKind, Value, ValueType,
};
use whizbang_core::identity::{DecodeLimits, Digest};

use crate::grow::text_of;

/// The action's name.
pub const NAME: &str = "labels";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// The most classes one call names: what one byte of a picture's labels can tell apart,
/// beside none.
pub const CLASSES_MOST: usize = 255;

/// The most requests one call sends: its classes in batches of [`QUERY_LIMIT`].
pub const REQUESTS_MOST: u64 = CLASSES_MOST.div_ceil(QUERY_LIMIT) as u64;

/// The diagnostic every run that was answered writes.
pub const DIAGNOSTIC: &str = "labels.txt";

/// The classes a call names, from its text: one a line, each kept as it is but for the
/// line's end, 1 to [`CLASSES_MOST`] of them, none empty, none over [`PHRASE_LIMIT`]
/// bytes and none twice. Why not, as a sentence.
pub fn classes_of(text: &str) -> Result<Vec<String>, String> {
    let named: Vec<String> = text.lines().map(str::to_owned).collect();
    if named.is_empty() || named.len() > CLASSES_MOST {
        return Err(format!("classes names 1 to {CLASSES_MOST}, one a line"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for name in &named {
        if name.trim().is_empty() || name.len() > PHRASE_LIMIT {
            return Err(format!(
                "a class is a phrase of 1 to {PHRASE_LIMIT} bytes, one a line"
            ));
        }
        if !seen.insert(name.as_str()) {
            return Err("a class is named twice".to_owned());
        }
    }
    Ok(named)
}

/// The digest a mask of a class names as its source: of `labels class ` and the name.
pub fn class_digest(name: &str) -> Digest {
    Digest::of_bytes(format!("labels class {name}").as_bytes())
}

/// The action `labels`.
pub struct Labels {
    descriptor: ActionDescriptor,
}

impl Labels {
    pub fn new() -> Labels {
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Asks a fixed-class segmenter which pixels of an image it labels with each class named, as one mask for each.",
        )
        .argument(Argument::required(
            "classes",
            "The classes to ask, one a line: each one of the segmenter's own.",
            ValueType::Text,
        ))
        .argument(Argument::optional(
            "backend",
            "The segmenter, by its name in the config; the default when not given.",
            ValueType::Text,
        ))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The picture to label.",
        ))
        .output(
            OutputPort::counted(
                "mask",
                ArtifactType::Mask,
                "One mask for each class some pixel has, in the order the classes are named.",
                Count::between(0, CLASSES_MOST as u32),
            )
            .on(PortRef::input("image")),
        )
        .needs(
            Requirement::new(Capability::Segment)
                .feature(feature::PHRASE)
                .feature(feature::LABELS)
                .requests(REQUESTS_MOST)
                .chosen_by("backend"),
        )
        .results(ResultKind::Measurements)
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Labels { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Labels {
    fn default() -> Labels {
        Labels::new()
    }
}

impl Action for Labels {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let given = call
            .arguments()
            .iter()
            .find_map(|(name, value)| (name == "classes").then_some(value));
        let Some(Value::Text(text)) = given else {
            return Err(Failure::error("argument", "the call gives no classes"));
        };
        let classes = classes_of(text).map_err(|why| Failure::invalid(&why))?;
        let input = (call.inputs().iter())
            .find(|input| input.port() == "image")
            .ok_or_else(|| Failure::error("input", "the call gives no image"))?;
        let segment = (context.capabilities().segment())
            .ok_or_else(|| Failure::error("segment", "the run was handed no segmenter"))?;
        let limits = DecodeLimits::default();
        let read = artifact::get(context.store(), ArtifactType::Image, input.id(), &limits)
            .map_err(|error| Failure::error("input", &error.without_paths()))?;
        let Artifact::Image(image) = read else {
            return Err(Failure::error("input", "what was read is not an image"));
        };
        if image.role() != ImageRole::Picture {
            return Err(Failure::invalid(
                "the image is a rendering of another artifact, and a rendering is not labelled",
            ));
        }
        let pixels = image
            .pixels(&limits)
            .map_err(|error| Failure::error("input", &error.without_paths()))?;
        let binding = image.binding();
        let picture = Picture::new(pixels).map_err(|unfit| Failure::invalid(&unfit.to_string()))?;
        let unlike = || {
            Failure::error(
                "segment",
                "the segmenter did not label each pixel with one class at most, as a fixed-class segmenter does",
            )
        };

        // Each class some pixel has, in the order named: its plane, score and pixels.
        let mut found: Vec<(usize, Vec<u8>, f64, u64)> = Vec::new();
        let mut covered: Vec<bool> = Vec::new();
        let mut said = String::new();
        for (batch, names) in classes.chunks(QUERY_LIMIT).enumerate() {
            let queries = (names.iter())
                .map(|name| Query::phrase(name))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|unfit| Failure::invalid(&unfit.to_string()))?;
            let request = SegmentRequest::new(picture.clone(), queries, 1)
                .and_then(|request| request.under(&binding))
                .map_err(|unfit| Failure::invalid(&unfit.to_string()))?;
            let response = match segment.segment(&request) {
                Ok(response) => response,
                Err(fault) => {
                    let said = format!("failed {}: {}\n", fault.kind().name(), fault.sentence());
                    let _ = context.diagnostics().write(DIAGNOSTIC, said.as_bytes());
                    return Err(Failure::error("segment", fault.sentence()));
                }
            };
            for (at, answered) in response.answers().iter().enumerate() {
                let instance = match answered.as_slice() {
                    [] => continue,
                    [instance] => instance,
                    _ => return Err(unlike()),
                };
                let plane = instance.plane();
                if covered.is_empty() {
                    covered = vec![false; plane.len()];
                }
                let mut count = 0u64;
                for (place, sample) in plane.iter().enumerate() {
                    if *sample != 0 {
                        if covered[place] {
                            return Err(unlike());
                        }
                        covered[place] = true;
                        count += 1;
                    }
                }
                found.push((
                    batch * QUERY_LIMIT + at,
                    plane.to_vec(),
                    instance.score(),
                    count,
                ));
            }
            for line in response.diagnostics().lines() {
                said.push_str(&format!("backend {line}\n"));
            }
        }

        let mut produced = Produced::nothing();
        let mut listed = Vec::new();
        let mut lines = format!("classes {} present {}\n", classes.len(), found.len());
        for (index, plane, score, count) in found {
            let name = &classes[index];
            let step = Step::new(NAME, VERSION)
                .argument("class", name)
                .input("image", image.id().0);
            let made = Origin::new(step).and_then(|origin| {
                Mask::from_core(
                    binding.clone(),
                    UseCase::Custom,
                    plane,
                    Source {
                        locations: Vec::new(),
                        instances: vec![class_digest(name)],
                    },
                    origin,
                )
            });
            let mask = made.map_err(|error| Failure::error("mask", &error.without_paths()))?;
            lines.push_str(&format!(
                "class {} mask {} score {} pixels {count}\n",
                text_of_name(name),
                mask.id().0,
                text_of(score)
            ));
            listed.push(serde_json::json!({"name": name, "score": score, "pixels": count}));
            produced = produced.with("mask", mask);
        }
        lines.push_str(&said);
        context
            .diagnostics()
            .write(DIAGNOSTIC, lines.as_bytes())
            .map_err(|why| Failure::error("diagnostic", &why))?;
        context.events().progress(&format!(
            "labels: {} of {} classes in the picture",
            listed.len(),
            classes.len()
        ));
        Ok(produced.with_measurements(serde_json::json!({ "classes": listed })))
    }
}

/// A class's name as the diagnostic writes it: between quotes, a quote or a backslash
/// in it escaped, so a name with spaces is one field.
fn text_of_name(name: &str) -> String {
    serde_json::Value::String(name.to_owned()).to_string()
}

/// Registers the action.
pub fn register(
    registry: &mut whizbang_core::contract::Registry,
) -> Result<(), whizbang_core::contract::RegistrationError> {
    registry.register(std::sync::Arc::new(Labels::new()))
}
