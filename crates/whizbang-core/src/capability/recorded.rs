//! Recorded capability handles (plan m7, task R1, item 2; ADR 0049): what answers the
//! requests of a run replayed offline, from what the run it replays recorded, and from
//! nothing else.
//!
//! One set of answers is made of a root run's family, as the store or an imported bundle
//! holds it ([`Recorded::of_family`]): for each capability, every request the family
//! sent, in the order its runs are matched by place (the root first, then each step of a
//! run, at any depth, each run's own requests in the order its record lists them).
//!
//! | Capability | Answered from |
//! |---|---|
//! | `segment` | The original `segment` run's committed masks, with each one's score, box and predicted IoU from its `segment.txt` |
//! | `fill` | The painted picture the original `fill` run kept (`fill-answer.png`) |
//! | `judge` | Its stored exchanges, turn by turn: each request the replay makes is compared with the one recorded, and the recorded answer given |
//!
//! The handles ([`RecordedOffered`]) are offered under the choices the record's settings
//! name, so preparation finds them; they declare the model the record names, and no
//! recipient and no price, so egress has nothing to send and the ledger nothing to
//! reserve. Nothing of the config is read for them, no key is read and no worker is
//! started.
//!
//! **Divergence is the replay's.** A request the original did not make, or made
//! otherwise (another model, another request, one past the last recorded), is
//! `diverged`; an answer the store cannot give (an opaque part withheld, a diagnostic
//! gone, a fill with no kept answer, a check the decision cache answered) is
//! `unanswerable`. The first of either is noted on the replay ([`Course`]), and every
//! later request of the family is refused the same way, whatever the actions made of the
//! first: an action may handle a capability's fault and go on, and the replay's report
//! is still the first stop. Each is answered as a fault of the kind `unavailable`, whose
//! sentence says which.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use crate::artifact::{Artifact, ArtifactType};
use crate::contract::record::{DIAGNOSTICS_PREFIX, RequestEnded, RunRecord};
use crate::contract::{
    Backends, Capability, EXCHANGE_PREFIX, Estimate, Made, ModelIdentity, Offered, Unavailable,
};
use crate::identity::{BindingId, DecodeLimits, identify};

use super::written;
use super::{
    Bounds, Declaration, Fault, FaultKind, FillRequest, Filler, Instance, Judge, JudgeRequest,
    Painted, Query, SegmentRequest, SegmentResponse, Segmenter, Support, Turn, feature,
};

/// The action whose run's segment request is answered from its masks and its diagnostic
/// (`whizbang_rascal::segment::NAME`), and that diagnostic
/// (`whizbang_rascal::segment::DIAGNOSTIC`). A test of the binary holds both to rascal's.
pub const SEGMENT_ACTION: &str = "segment";
pub const SEGMENT_DIAGNOSTIC: &str = "segment.txt";

/// The action whose run's fill request is answered from the painted answer it kept, and
/// that diagnostic (ADR 0048; [`crate::bundle::FILL_ACTION`], [`crate::bundle::FILL_ANSWER`]).
pub use crate::bundle::{FILL_ACTION, FILL_ANSWER};

/// The diagnostic of a replay's root that notes where the replay stopped, when it did:
/// `diverged` or `unanswerable`, the capability, the request's number in the family from
/// 1, and why, on one line.
pub const NOTE: &str = "replay.txt";

/// The identity a capability's handle declares when the original family sent no request
/// of it: any request of it is then `diverged`.
const UNRECORDED: (&str, &str, &str) = ("replay", "unrecorded", "none");

/// Why a replay stopped, as a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopKind {
    /// A request the original did not make, or made otherwise.
    Diverged,
    /// An answer the store cannot give.
    Unanswerable,
}

impl StopKind {
    pub fn name(self) -> &'static str {
        match self {
            StopKind::Diverged => "diverged",
            StopKind::Unanswerable => "unanswerable",
        }
    }
}

/// Where a replay stopped: the first `diverged` or `unanswerable` answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stop {
    pub kind: StopKind,
    pub capability: Capability,
    /// The request's number among the family's requests of that capability, from 1.
    pub request: usize,
    /// Why, in a sentence.
    pub why: String,
}

impl Stop {
    /// The note as [`NOTE`] keeps it.
    pub fn to_text(&self) -> String {
        format!(
            "{} {} request {}: {}\n",
            self.kind.name(),
            self.capability.name(),
            self.request,
            self.why.replace('\n', " ")
        )
    }

    /// A note read back, or none when the text is not one.
    pub fn read(text: &str) -> Option<Stop> {
        let line = text.strip_suffix('\n').unwrap_or(text);
        let (head, why) = line.split_once(": ")?;
        let mut words = head.split(' ');
        let kind = match words.next()? {
            "diverged" => StopKind::Diverged,
            "unanswerable" => StopKind::Unanswerable,
            _ => return None,
        };
        let capability = match words.next()? {
            "segment" => Capability::Segment,
            "fill" => Capability::Fill,
            "judge" => Capability::Judge,
            _ => return None,
        };
        (words.next()? == "request").then_some(())?;
        let request = words.next()?.parse().ok()?;
        words.next().is_none().then_some(())?;
        Some(Stop {
            kind,
            capability,
            request,
            why: why.to_owned(),
        })
    }
}

/// How far a replay has got: the next request of each capability, and where it stopped
/// when it did. One for a replay, shared by every handle of its family.
#[derive(Debug, Default)]
pub struct Course {
    state: Mutex<CourseState>,
}

#[derive(Debug, Default)]
struct CourseState {
    next: [usize; 3],
    stopped: Option<Stop>,
}

impl Course {
    /// Where the replay stopped, when it did.
    pub fn stopped(&self) -> Option<Stop> {
        self.lock().stopped.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CourseState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn slot(capability: Capability) -> usize {
    match capability {
        Capability::Segment => 0,
        Capability::Fill => 1,
        Capability::Judge => 2,
    }
}

/// What a recorded request is answered with.
#[derive(Debug, Clone)]
pub enum Given<A> {
    /// What the original was answered.
    Answer(A),
    /// The fault the original's request ended with.
    Fault(FaultKind, String),
    /// An answer the store cannot give, and why.
    Unanswerable(String),
}

/// One request the original family sent: the run that sent it, the model that answered,
/// and what it is answered with.
#[derive(Debug, Clone)]
pub struct Asked<A> {
    pub run: String,
    pub model: ModelIdentity,
    /// What the original's record says it was sent, as far as it says.
    pub sent: Sent,
    pub given: Given<A>,
}

/// What a recorded request was sent, as the original's record and its inputs say, which
/// the replay's request is held to before anything is answered (ADR 0049): a part it
/// cannot say is not compared.
#[derive(Debug, Clone, Default)]
pub enum Sent {
    /// A segmenter's: the binding of its image, its one query, and the most instances.
    Segment {
        binding: Option<BindingId>,
        query: Option<Query>,
        most: Option<u32>,
    },
    /// A filler's: the picture's pixels, the area to paint, which the action makes of
    /// its mask, and its prompt.
    Fill {
        pixels: Option<crate::identity::PixelDigest>,
        support: Option<Support>,
        prompt: Option<String>,
    },
    /// A judge's: the exchange as the run stored it.
    Judge { stored: Option<Vec<u8>> },
    /// Nothing is said of it.
    #[default]
    Unknown,
}

/// A segmenter's answer, rebuilt: the binding of the image it was about, each instance
/// with its predicted IoU in the fixed order, and the backend's lines.
#[derive(Debug, Clone)]
pub struct SegmentAnswer {
    pub binding: BindingId,
    pub instances: Vec<(Instance, Option<f64>)>,
    pub said: String,
}

/// A filler's answer, as the run kept it.
#[derive(Debug, Clone)]
pub struct FillAnswer {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// A judge's exchange, as the run stored it, and the answer read back from it.
#[derive(Debug, Clone)]
pub struct JudgeAnswer {
    pub stored: Vec<u8>,
    pub answer: Result<Turn, Fault>,
}

/// What the store holds of a family, as the answers are made from it: the store's own
/// runs, or a bundle's entry.
pub trait Held {
    /// A diagnostic of a run by its name in the run (`diagnostics/<name>`), when it is
    /// held.
    fn diagnostic(&self, run: &str, name: &str) -> Result<Option<Vec<u8>>, String>;
    /// The canonical file of an output of a run, at its port and place, when it is held.
    fn output(&self, run: &str, port: &str, place: u32) -> Result<Option<Vec<u8>>, String>;
    /// The canonical file of an input of a run, at its port and place, of the artifact
    /// this name names, as the run read it, when it is held. `root` is its family's
    /// root, whose names the runs under it read.
    fn input(
        &self,
        run: &str,
        root: &str,
        port: &str,
        place: u32,
        name: &str,
    ) -> Result<Option<Vec<u8>>, String>;
}

/// The answers of a family, and how far a replay has used them.
pub struct Recorded {
    segment: Vec<Asked<SegmentAnswer>>,
    fill: Vec<Asked<FillAnswer>>,
    judge: Vec<Asked<JudgeAnswer>>,
    /// Each choice the records' settings name, with the model it answered with.
    aliases: Vec<(Capability, String, ModelIdentity)>,
    course: Arc<Course>,
}

impl std::fmt::Debug for Recorded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorded")
            .field("segment", &self.segment.len())
            .field("fill", &self.fill.len())
            .field("judge", &self.judge.len())
            .finish()
    }
}

impl Recorded {
    /// The answers of a family whose records are these, the root first and each run
    /// before the runs it called, in the order of their steps.
    pub fn of_family(runs: &[RunRecord], held: &dyn Held) -> Recorded {
        let mut recorded = Recorded {
            segment: Vec::new(),
            fill: Vec::new(),
            judge: Vec::new(),
            aliases: Vec::new(),
            course: Arc::new(Course::default()),
        };
        let root = runs
            .first()
            .map(|r| r.identity().run.clone())
            .unwrap_or_default();
        for record in runs {
            recorded.add(record, &root, held);
        }
        recorded
    }

    /// How far the replay has got, and where it stopped.
    pub fn course(&self) -> &Arc<Course> {
        &self.course
    }

    /// The backends to offer a replayed run: these answers, under the choices found.
    pub fn offered(self: &Arc<Self>) -> Arc<dyn Offered> {
        Arc::new(RecordedOffered(self.clone()))
    }

    fn add(&mut self, record: &RunRecord, root: &str, held: &dyn Held) {
        let run = record.identity().run.clone();
        let action = record.asked().action.as_deref().unwrap_or_default();
        // The choice each capability was resolved under, when the settings name one
        // alone: `backends.<capability>.<alias>.…` (`config/settings.rs`).
        for capability in [Capability::Segment, Capability::Fill, Capability::Judge] {
            let prefix = format!("backends.{}.", capability.name());
            let mut named: Vec<&str> = (record.asked().settings.iter())
                .filter_map(|setting| setting.name().strip_prefix(&prefix)?.split('.').next())
                .collect();
            named.dedup();
            let model = (record.models().iter()).find(|model| model.capability == capability);
            if let ([alias], Some(model)) = (named.as_slice(), model) {
                let identity = ModelIdentity::new(&model.backend, &model.model, &model.version);
                if !self
                    .aliases
                    .iter()
                    .any(|(c, a, _)| *c == capability && a == alias)
                {
                    self.aliases
                        .push((capability, (*alias).to_owned(), identity));
                }
            }
        }
        // A check the decision cache answered sent no request of its own: a replay,
        // which reads no cache, asks the judge, and that is unanswerable here.
        let cached = (record.asked().settings.iter()).any(|setting| {
            setting.name() == crate::bundle::CACHE_SETTING
                && matches!(setting.value(), crate::contract::record::ArgumentValue::Text(t) if t == crate::bundle::CACHE_HIT)
        });
        if cached {
            self.judge.push(Asked {
                run: run.clone(),
                model: unrecorded(),
                sent: Sent::Unknown,
                given: Given::Unanswerable(format!(
                    "the run {run} was answered from the decision cache, and the turns that \
                     answered it are another run's"
                )),
            });
        }
        for (place, model) in record.models().iter().enumerate() {
            let identity = ModelIdentity::new(&model.backend, &model.model, &model.version);
            let ended = model.ended;
            match model.capability {
                Capability::Segment => {
                    let given = segment_answer(record, action, ended, held);
                    self.segment.push(Asked {
                        run: run.clone(),
                        model: identity,
                        sent: segment_sent(record),
                        given,
                    });
                }
                Capability::Fill => {
                    let given = fill_answer(record, action, ended, held);
                    self.fill.push(Asked {
                        run: run.clone(),
                        model: identity,
                        sent: fill_sent(record, root, held),
                        given,
                    });
                }
                Capability::Judge => {
                    let given = judge_answer(&run, place, ended, held);
                    let stored = match &given {
                        Given::Answer(answer) => Some(answer.stored.clone()),
                        _ => None,
                    };
                    self.judge.push(Asked {
                        run: run.clone(),
                        model: identity,
                        sent: Sent::Judge { stored },
                        given,
                    });
                }
            }
        }
    }

    /// The model a capability is resolved to under a choice: the one the settings name
    /// under it, else the first the family's requests of it were answered by, else one
    /// that answers nothing.
    fn model_of(&self, capability: Capability, choice: Option<&str>) -> ModelIdentity {
        let aliased = (self.aliases.iter())
            .find(|(c, alias, _)| *c == capability && Some(alias.as_str()) == choice)
            .map(|(_, _, model)| model.clone());
        let first = || match capability {
            Capability::Segment => self.segment.first().map(|asked| asked.model.clone()),
            Capability::Fill => self.fill.first().map(|asked| asked.model.clone()),
            Capability::Judge => (self.judge.iter())
                .find(|asked| !matches!(asked.given, Given::Unanswerable(_)))
                .map(|asked| asked.model.clone()),
        };
        aliased.or_else(first).unwrap_or_else(unrecorded)
    }

    /// The next recorded request of a capability for this model and this request, or the
    /// fault that answers it. `matches` says why an answer is not this request's.
    fn next<'a, A>(
        &self,
        capability: Capability,
        queue: &'a [Asked<A>],
        model: &ModelIdentity,
        bounds: &Bounds,
        matches: impl Fn(&Sent) -> Result<(), String>,
    ) -> Result<&'a A, Fault> {
        if bounds.is_cancelled() {
            return Err(Fault::new(FaultKind::Cancelled, "the replay was cancelled"));
        }
        let mut state = self.course.lock();
        if let Some(stop) = &state.stopped {
            return Err(stopped(stop, true));
        }
        let at = state.next[slot(capability)];
        state.next[slot(capability)] += 1;
        let request = at + 1;
        let stop = |kind, why: String| Stop {
            kind,
            capability,
            request,
            why,
        };
        let Some(asked) = queue.get(at) else {
            let made = stop(
                StopKind::Diverged,
                format!(
                    "the replay sent a {} request past the {} the original sent",
                    capability.name(),
                    queue.len()
                ),
            );
            return Err(state.stop(made));
        };
        let made = match &asked.given {
            Given::Unanswerable(why) => stop(StopKind::Unanswerable, why.clone()),
            _ if asked.model != *model => stop(
                StopKind::Diverged,
                format!(
                    "the replay asked {} {} {}, and the original's request {request} was \
                     answered by {} {} {}",
                    model.backend(),
                    model.model(),
                    model.version(),
                    asked.model.backend(),
                    asked.model.model(),
                    asked.model.version()
                ),
            ),
            // The request is held to what the original was sent before anything is
            // answered, the original's fault among it.
            _ => match (matches(&asked.sent), &asked.given) {
                (Err(why), _) => stop(StopKind::Diverged, why),
                (Ok(()), Given::Fault(kind, sentence)) => return Err(Fault::new(*kind, sentence)),
                (Ok(()), Given::Answer(answer)) => return Ok(answer),
                (Ok(()), Given::Unanswerable(why)) => stop(StopKind::Unanswerable, why.clone()),
            },
        };
        Err(state.stop(made))
    }
}

impl CourseState {
    /// Notes the first stop, and answers it.
    fn stop(&mut self, stop: Stop) -> Fault {
        let fault = stopped(&stop, false);
        self.stopped.get_or_insert(stop);
        fault
    }
}

/// The fault a stopped replay answers a request with.
fn stopped(stop: &Stop, earlier: bool) -> Fault {
    let said = match (earlier, stop.kind) {
        (true, _) => format!(
            "the replay stopped at an earlier request, {} {} request {}: {}",
            stop.kind.name(),
            stop.capability.name(),
            stop.request,
            stop.why
        ),
        (false, StopKind::Diverged) => format!("the replay diverged: {}", stop.why),
        (false, StopKind::Unanswerable) => {
            format!("the replay cannot answer this request: {}", stop.why)
        }
    };
    Fault::new(FaultKind::Unavailable, &said)
}

fn unrecorded() -> ModelIdentity {
    ModelIdentity::new(UNRECORDED.0, UNRECORDED.1, UNRECORDED.2)
}

/// The fault of a request that ended without an answer, or none for one that was
/// answered, or that unwound and so has no fault to give.
fn fault_of(ended: RequestEnded) -> Option<FaultKind> {
    Some(match ended {
        RequestEnded::Answered | RequestEnded::Unwound => return None,
        RequestEnded::Cancelled => FaultKind::Cancelled,
        RequestEnded::Unsupported => FaultKind::Unsupported,
        RequestEnded::Credential => FaultKind::Credential,
        RequestEnded::Rejected => FaultKind::Rejected,
        RequestEnded::Unavailable => FaultKind::Unavailable,
        RequestEnded::TimedOut => FaultKind::TimedOut,
        RequestEnded::Malformed => FaultKind::Malformed,
        RequestEnded::TooLarge => FaultKind::TooLarge,
    })
}

/// What a request that did not end answered is given: its fault, said as the original's.
fn ended_otherwise<A>(run: &str, ended: RequestEnded, said: Option<String>) -> Given<A> {
    match fault_of(ended) {
        Some(kind) => Given::Fault(
            kind,
            said.unwrap_or_else(|| format!("the original request of the run {run} ended so")),
        ),
        None => Given::Unanswerable(format!(
            "the original request of the run {run} ended with its backend's call unwinding"
        )),
    }
}

fn segment_answer(
    record: &RunRecord,
    action: &str,
    ended: RequestEnded,
    held: &dyn Held,
) -> Given<SegmentAnswer> {
    let run = record.identity().run.as_str();
    let count = (record.models().iter())
        .filter(|model| model.capability == Capability::Segment)
        .count();
    if action != SEGMENT_ACTION || count != 1 {
        return Given::Unanswerable(format!(
            "the run {run} of {action} sent a segment request whose answer no run keeps: \
             only a run of {SEGMENT_ACTION} with its one request keeps its masks and its \
             diagnostic"
        ));
    }
    let name = format!("{DIAGNOSTICS_PREFIX}{SEGMENT_DIAGNOSTIC}");
    let said = match held.diagnostic(run, &name) {
        Ok(Some(bytes)) => String::from_utf8(bytes).ok(),
        Ok(None) => None,
        Err(why) => return Given::Unanswerable(why),
    };
    if ended != RequestEnded::Answered {
        // `failed <kind>: <sentence>`, as the action wrote it.
        let sentence = (said.as_deref())
            .and_then(|said| said.strip_prefix("failed "))
            .and_then(|rest| rest.split_once(": "))
            .map(|(_, sentence)| sentence.trim_end().to_owned());
        return ended_otherwise(run, ended, sentence);
    }
    let Some(said) = said else {
        return Given::Unanswerable(format!(
            "the run {run} keeps no {SEGMENT_DIAGNOSTIC}, which holds each mask's score and box"
        ));
    };
    match rebuilt(record, &said, held) {
        Ok(answer) => Given::Answer(answer),
        Err(why) => Given::Unanswerable(format!(
            "the answer of the run {run} cannot be made again from its masks and its \
             {SEGMENT_DIAGNOSTIC}: {why}"
        )),
    }
}

/// A segmenter's answer made again from its run's masks and its diagnostic.
fn rebuilt(record: &RunRecord, said: &str, held: &dyn Held) -> Result<SegmentAnswer, String> {
    let run = record.identity().run.as_str();
    let image = (record.inputs().iter())
        .find(|input| input.port == "image")
        .ok_or("the run took no image")?;
    let mut lines = said.lines();
    let count: usize = lines
        .next()
        .and_then(|line| line.strip_prefix("instances "))
        .and_then(|count| count.parse().ok())
        .ok_or("its first line does not count the instances")?;
    let mut instances = Vec::new();
    let limits = DecodeLimits::default();
    for place in 0..count {
        let line = lines.next().ok_or("it lists fewer masks than it counts")?;
        let words: Vec<&str> = line.split(' ').collect();
        let number = |at: usize| -> Result<f64, String> {
            (words.get(at).and_then(|word| word.parse::<f64>().ok()))
                .ok_or_else(|| format!("its line {} is not a mask's", place + 2))
        };
        if words.first() != Some(&"mask") || words.get(2) != Some(&"score") {
            return Err(format!("its line {} is not a mask's", place + 2));
        }
        let bounds = [number(5)?, number(6)?, number(7)?, number(8)?];
        let iou = match words.get(9) {
            Some(&"iou") => Some(number(10)?),
            _ => None,
        };
        let bytes = held
            .output(run, "mask", place as u32)?
            .ok_or_else(|| format!("its mask {place} is not held"))?;
        let mask = Artifact::from_bytes_as(ArtifactType::Mask, &bytes, &limits)
            .and_then(Artifact::into_mask)
            .map_err(|why| format!("its mask {place} cannot be read: {}", why.without_paths()))?;
        if words.get(1).copied() != Some(mask.id().0.to_string().as_str()) {
            return Err(format!("its line {} names another mask", place + 2));
        }
        let space = mask.space();
        let instance = Instance::new(
            mask.binding().id(),
            space.width(),
            space.height(),
            mask.core().to_vec(),
            bounds,
            number(3)?,
        )
        .map_err(|why| why.to_string())?;
        instances.push((instance, iou));
    }
    let backend: Vec<&str> = lines
        .map(|line| {
            line.strip_prefix("backend ")
                .ok_or("a line after the masks is not the backend's")
        })
        .collect::<Result<_, _>>()?;
    Ok(SegmentAnswer {
        binding: BindingId(image.binding),
        instances,
        said: backend.join("\n"),
    })
}

/// What a run of `segment` was sent, as its record says: its image's binding, its query
/// made of its kind and its text as the action makes it, and the most instances.
fn segment_sent(record: &RunRecord) -> Sent {
    use crate::contract::record::ArgumentValue;
    let argument = |name: &str| {
        (record.asked().arguments.iter())
            .find(|argument| argument.name == name)
            .map(|argument| &argument.value)
    };
    let text = |name: &str| match argument(name) {
        Some(ArgumentValue::Text(text)) => Some(text.as_str()),
        _ => None,
    };
    let query = text("query").and_then(|query| query_of(text("kind").unwrap_or("phrase"), query));
    let most = match argument("max_instances") {
        Some(ArgumentValue::Integer(most)) => u32::try_from(*most).ok(),
        _ => None,
    };
    let binding = (record.inputs().iter())
        .find(|input| input.port == "image")
        .map(|input| BindingId(input.binding));
    Sent::Segment {
        binding,
        query,
        most,
    }
}

/// The query of a kind and a text, as `segment` makes it
/// (`whizbang_rascal::segment::query_of`; a test of the binary holds the two to agree).
pub fn query_of(kind: &str, text: &str) -> Option<Query> {
    let numbers = || -> Option<Vec<f64>> {
        text.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|word| !word.is_empty())
            .map(|word| word.parse::<f64>().ok().filter(|n| n.is_finite()))
            .collect()
    };
    match kind {
        feature::PHRASE => Query::phrase(text).ok(),
        feature::POINT => match numbers().as_deref() {
            Some(&[x, y]) => Query::point(x, y).ok(),
            _ => None,
        },
        feature::BOX => match numbers().as_deref() {
            Some(&[left, top, right, bottom]) => Query::boxed(left, top, right, bottom).ok(),
            _ => None,
        },
        _ => None,
    }
}

/// What a run of `fill` sent, as its inputs say: its image's pixels and the area its mask
/// paints, as the action makes the two, and no prompt.
fn fill_sent(record: &RunRecord, root: &str, held: &dyn Held) -> Sent {
    let run = record.identity().run.as_str();
    let limits = DecodeLimits::default();
    let read = |port: &str, artifact: ArtifactType| {
        let input = record.inputs().iter().find(|input| input.port == port)?;
        let name = crate::artifact::name(artifact, crate::artifact::ArtifactId(input.artifact));
        let bytes = held.input(run, root, port, 0, &name).ok()??;
        Artifact::from_bytes_as(artifact, &bytes, &limits).ok()
    };
    let pixels = read("image", ArtifactType::Image)
        .and_then(|image| match image {
            Artifact::Image(image) => Some(image),
            _ => None,
        })
        .and_then(|image| image.pixels(&limits).ok())
        .and_then(|pixels| super::Picture::new(pixels).ok())
        .map(|picture| picture.digest());
    let support = read("mask", ArtifactType::Mask)
        .and_then(|mask| mask.into_mask().ok())
        .map(|mask| Support::of(&mask));
    Sent::Fill {
        pixels,
        support,
        prompt: None,
    }
}

fn fill_answer(
    record: &RunRecord,
    action: &str,
    ended: RequestEnded,
    held: &dyn Held,
) -> Given<FillAnswer> {
    let run = record.identity().run.as_str();
    let count = (record.models().iter())
        .filter(|model| model.capability == Capability::Fill)
        .count();
    if action != FILL_ACTION || count != 1 {
        return Given::Unanswerable(format!(
            "the run {run} of {action} sent a fill request whose answer no run keeps: only a \
             run of {FILL_ACTION} with its one request keeps its painted answer"
        ));
    }
    if ended != RequestEnded::Answered {
        return ended_otherwise(run, ended, None);
    }
    let name = format!("{DIAGNOSTICS_PREFIX}{FILL_ANSWER}");
    let bytes = match held.diagnostic(run, &name) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return Given::Unanswerable(format!(
                "the fill run {run} keeps no {FILL_ANSWER}: it was made before a fill kept \
                 its painted answer, or the store let go of it"
            ));
        }
        Err(why) => return Given::Unanswerable(why),
    };
    match identify(&bytes, &DecodeLimits::default()) {
        Ok(identified) => {
            let pixels = identified.pixels;
            let rgb = (pixels.rgba().chunks(4))
                .flat_map(|rgba| [rgba[0], rgba[1], rgba[2]])
                .collect();
            Given::Answer(FillAnswer {
                width: pixels.width(),
                height: pixels.height(),
                rgb,
            })
        }
        Err(why) => Given::Unanswerable(format!(
            "the {FILL_ANSWER} of the run {run} cannot be read: {why}"
        )),
    }
}

fn judge_answer(
    run: &str,
    place: usize,
    ended: RequestEnded,
    held: &dyn Held,
) -> Given<JudgeAnswer> {
    let name = format!("{DIAGNOSTICS_PREFIX}{EXCHANGE_PREFIX}{:04}", place + 1);
    let stored = match held.diagnostic(run, &name) {
        Ok(Some(bytes)) => bytes,
        Ok(None) if ended == RequestEnded::Unwound => {
            return ended_otherwise(run, ended, None);
        }
        Ok(None) => {
            return Given::Unanswerable(format!(
                "the run {run} does not keep its exchange {name}, which holds that turn"
            ));
        }
        Err(why) => return Given::Unanswerable(why),
    };
    match written::recorded_answer(&stored) {
        Ok(answer) => Given::Answer(JudgeAnswer { stored, answer }),
        Err(written::Unread::Withheld) => {
            let named = written::named_outside(&stored).ok();
            let parts = named.map(|(parts, _)| parts).unwrap_or_default();
            let why = match parts.first() {
                Some(part) => format!(
                    "the turn of {name} of the run {run} held an opaque part of {}, {} bytes, \
                     which the store keeps as its digest and length alone, never its body",
                    part.adapter, part.bytes
                ),
                None => format!("the turn of {name} of the run {run} was not written"),
            };
            Given::Unanswerable(why)
        }
        Err(why) => Given::Unanswerable(format!(
            "the exchange {name} of the run {run} cannot be read: {why}"
        )),
    }
}

/// The backends a replayed run is offered: one handle on the recorded answers for each
/// model a capability may be resolved to.
struct RecordedOffered(Arc<Recorded>);

impl RecordedOffered {
    fn made_for(&self, capability: Capability, model: ModelIdentity) -> Option<Made> {
        let features: &[&str] = match capability {
            Capability::Segment => &[feature::PHRASE, feature::POINT, feature::BOX],
            Capability::Fill => &[feature::PROMPT],
            Capability::Judge => &[feature::IMAGE, feature::TOOL],
        };
        let declaration = Declaration::new(model, features).ok()?;
        let handle = Handle {
            declaration,
            recorded: self.0.clone(),
        };
        Some(match capability {
            Capability::Segment => Made::Segment(Arc::new(handle)),
            Capability::Fill => Made::Fill(Arc::new(handle)),
            Capability::Judge => Made::Judge(Arc::new(handle)),
        })
    }
}

impl Backends for RecordedOffered {
    fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        _features: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Ok(self.0.model_of(capability, choice))
    }

    fn bound(&self, _capability: Capability, _model: &ModelIdentity) -> Estimate {
        Estimate::default()
    }
}

impl Offered for RecordedOffered {
    fn default_of(&self, capability: Capability) -> Option<&str> {
        let aliased = self.0.aliases.iter().find(|(c, _, _)| *c == capability);
        Some(aliased.map_or("recorded", |(_, alias, _)| alias.as_str()))
    }

    fn made(&self, capability: Capability, name: &str) -> Option<Made> {
        let model = self.0.model_of(capability, Some(name));
        self.made_for(capability, model)
    }
}

/// A handle on the recorded answers, declaring one model.
struct Handle {
    declaration: Declaration,
    recorded: Arc<Recorded>,
}

impl Segmenter for Handle {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn segment(&self, request: &SegmentRequest, bounds: &Bounds) -> Result<SegmentResponse, Fault> {
        let recorded = &self.recorded;
        let model = self.declaration.model();
        let answer = recorded.next(
            Capability::Segment,
            &recorded.segment,
            model,
            bounds,
            |sent| {
                let Sent::Segment {
                    binding,
                    query,
                    most,
                } = sent
                else {
                    return Ok(());
                };
                let differs = |what: &str| {
                    Err(format!(
                        "the replay asked a segmenter with another {what} than the original did"
                    ))
                };
                if binding.is_some_and(|binding| binding != request.binding()) {
                    return differs("image");
                }
                if request.queries().len() != 1
                    || query
                        .as_ref()
                        .is_some_and(|query| request.queries()[0] != *query)
                {
                    return differs("query");
                }
                if most.is_some_and(|most| most != request.max_instances()) {
                    return differs("most instances");
                }
                Ok(())
            },
        )?;
        let (instances, iou): (Vec<Instance>, Vec<Option<f64>>) =
            answer.instances.iter().cloned().unzip();
        Ok(SegmentResponse::new(vec![instances])
            .predicted(vec![iou])
            .said(&answer.said))
    }
}

impl Filler for Handle {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        let recorded = &self.recorded;
        let model = self.declaration.model();
        let answer = recorded.next(Capability::Fill, &recorded.fill, model, bounds, |sent| {
            let Sent::Fill {
                pixels,
                support,
                prompt,
            } = sent
            else {
                return Ok(());
            };
            let differs = |what: &str| {
                Err(format!(
                    "the replay asked a filler with another {what} than the original did"
                ))
            };
            if pixels.is_some_and(|pixels| pixels != request.image().digest()) {
                return differs("picture");
            }
            if support
                .as_ref()
                .is_some_and(|support| support != request.support())
            {
                return differs("area to paint");
            }
            if prompt.as_deref() != request.prompt() {
                return differs("prompt");
            }
            Ok(())
        })?;
        let size = (request.image().width(), request.image().height());
        if (answer.width, answer.height) != size {
            return Err(Fault::new(
                FaultKind::Malformed,
                "the recorded answer is not the picture's size",
            ));
        }
        Painted::from_rgb(answer.width, answer.height, answer.rgb.clone())
            .map_err(|why| Fault::new(FaultKind::Malformed, &why.to_string()))
    }
}

impl Judge for Handle {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn send(&self, request: &JudgeRequest, bounds: &Bounds) -> Result<Turn, Fault> {
        let recorded = &self.recorded;
        let model = self.declaration.model();
        let answer =
            recorded.next(
                Capability::Judge,
                &recorded.judge,
                model,
                bounds,
                |sent| match sent {
                    Sent::Judge {
                        stored: Some(stored),
                    } if !written::sent_as_recorded(stored, request) => Err(
                        "the replay sent the judge another request than the original did"
                            .to_owned(),
                    ),
                    _ => Ok(()),
                },
            )?;
        answer.answer.clone()
    }
}

/// The settings of a family's records whose text is exactly as long as a setting's text
/// may be, and so may have been cut: their runs and names, in order.
pub fn at_the_limit(runs: &[RunRecord]) -> Vec<(String, String)> {
    let limit = crate::contract::record::SETTING_TEXT_LIMIT;
    let mut found = Vec::new();
    for record in runs {
        for setting in &record.asked().settings {
            if let crate::contract::record::ArgumentValue::Text(text) = setting.value()
                && text.chars().count() == limit
            {
                found.push((record.identity().run.clone(), setting.name().to_owned()));
            }
        }
    }
    found
}

/// The names of the digests the family's records note for each tool file a run called
/// (`tool_file:<name>`), by run.
pub fn tool_files(runs: &[RunRecord]) -> BTreeMap<String, Vec<String>> {
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for record in runs {
        for setting in &record.asked().settings {
            if let Some(name) = setting.name().strip_prefix(TOOL_FILE_SETTING) {
                (found.entry(record.identity().run.clone()).or_default()).push(name.to_owned());
            }
        }
    }
    found
}

/// What the name of a setting that notes a tool file's digest starts with
/// (`whizbang_squint::check`, `dispatch_file`).
pub const TOOL_FILE_SETTING: &str = "tool_file:";
