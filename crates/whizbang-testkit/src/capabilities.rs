//! Fakes of the three capabilities (ADR 0006): a segmenter that finds what it was told
//! to find, a filler that can repaint the whole image, and a judge that plays a
//! scripted transcript. No model runs, nothing is sent anywhere, and each fake counts
//! the requests it was sent, so a test can show that something asked, or did not ask.
//!
//! Each can say what a real backend says of cost and of where its requests go (ADR
//! 0010, ADR 0012): it can be built with a price and with a recipient, can report a
//! charge with every answer, and can fail every request at a named stage. None of it
//! spends or sends anything: a fake only says so.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use whizbang_core::capability::written::{Unread, read_script};
use whizbang_core::capability::{
    Asked, Bounds, Charge, Declaration, Fault, FaultKind, FillRequest, Filler, Instance, Judge,
    JudgeRequest, Painted, Price, Roster, SegmentRequest, SegmentResponse, Segmenter, Stage,
    Support, Turn, feature,
};
use whizbang_core::contract::{Capability, ModelIdentity};
use whizbang_core::egress::Recipient;

use crate::barrier::Barriers;

/// The backend every fake declares.
const BACKEND: &str = "fake";

fn declaration(model: &str, features: &[&str]) -> Declaration {
    match Declaration::new(ModelIdentity::new(BACKEND, model, "1"), features) {
        Ok(declaration) => declaration,
        // The features are written in this file or by a test, and a test builds each.
        Err(why) => panic!("{why}"),
    }
}

/// The roster a build makes of these backends: each is offered under the name `fake`
/// and is its capability's default, so a call that needs the capability resolves the
/// model the backend declares, and an executor that holds the backend hands the call's
/// action a handle on it (ADR 0009).
pub fn roster(
    segmenter: Option<&dyn Segmenter>,
    filler: Option<&dyn Filler>,
    judge: Option<&dyn Judge>,
) -> Roster {
    let declared = [
        (
            Capability::Segment,
            segmenter.map(|backend| backend.declaration()),
        ),
        (
            Capability::Fill,
            filler.map(|backend| backend.declaration()),
        ),
        (
            Capability::Judge,
            judge.map(|backend| backend.declaration()),
        ),
    ];
    let mut roster = Roster::new();
    for (capability, declaration) in declared {
        let Some(declaration) = declaration else {
            continue;
        };
        let offered = roster.offer(capability, BACKEND, declaration.clone());
        let offered = offered.and_then(|roster| roster.default_for(capability, BACKEND));
        roster = match offered {
            Ok(roster) => roster,
            // One backend of each capability, under a name that is one.
            Err(why) => panic!("{why}"),
        };
    }
    roster
}

/// The same declaration for other features, and for another model when one is given:
/// what it said of its price and of its recipient stays.
fn redeclared(old: &Declaration, model: Option<&str>, features: &[&str]) -> Declaration {
    let model = model.unwrap_or(old.model().model());
    let mut new = declaration(model, features);
    if let Some(price) = old.price() {
        new = new.priced(*price);
    }
    if let Some(recipient) = old.recipient() {
        new = new.sending_to(recipient.clone());
    }
    new
}

/// A charge of this many millionths of a US dollar.
fn charge(micros: u64) -> Charge {
    match Charge::whole(micros) {
        Ok(charge) => charge,
        // A test wrote the number.
        Err(why) => panic!("{why}"),
    }
}

/// What a fake was told of money beside its declaration: the charge each of its
/// answers reports, and the stage every request fails at.
#[derive(Default)]
struct Terms {
    charge: Option<Charge>,
    fails_at: Option<Stage>,
}

impl Terms {
    /// The fault of a fake that was told to fail: `unavailable`, saying the stage it
    /// was told. Told to fail as answered, the fault carries the charge the fake
    /// reports, when it reports one: only an answer says what it cost.
    fn failed(&self) -> Result<(), Fault> {
        let Some(stage) = self.fails_at else {
            return Ok(());
        };
        let words = format!("the fake was told to fail, {}", stage.name());
        let fault = Fault::new(FaultKind::Unavailable, &words);
        Err(match stage {
            Stage::NotSent => fault.not_sent(),
            Stage::Answered => fault.answered(self.charge),
            Stage::Unresolved => fault.unresolved(),
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a fake that stops at a barrier saw of the run it answered for, in the bounds
/// it was handed with one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    on_arrival: bool,
    on_release: bool,
    deadline: Option<Instant>,
}

impl Observed {
    /// Whether the run was cancelled when the request arrived.
    pub fn cancelled_on_arrival(&self) -> bool {
        self.on_arrival
    }

    /// Whether the run was cancelled when the test let the request go.
    pub fn cancelled_on_release(&self) -> bool {
        self.on_release
    }

    /// The deadline the request was sent with.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
}

/// Where a fake holds every request it is sent, and what it saw of each.
struct Stop {
    barriers: Barriers,
    name: String,
    observed: Mutex<Vec<Observed>>,
}

impl Stop {
    fn at(barriers: &Barriers, name: &str) -> Option<Stop> {
        Some(Stop {
            barriers: barriers.clone(),
            name: name.to_owned(),
            observed: Mutex::new(Vec::new()),
        })
    }

    /// Holds one request at the barrier until a test lets it go, and notes what its
    /// bounds said when it arrived and when it went on. A request whose run was
    /// cancelled by then stops here, with the fault `cancelled`.
    fn hold(&self, bounds: &Bounds) -> Result<(), Fault> {
        let on_arrival = bounds.is_cancelled();
        self.barriers.stop_at(&self.name);
        let on_release = bounds.is_cancelled();
        lock(&self.observed).push(Observed {
            on_arrival,
            on_release,
            deadline: bounds.deadline(),
        });
        if on_release {
            // The request was open: nothing settles it, and the fake says so.
            return Err(Fault::new(
                FaultKind::Cancelled,
                "the run was cancelled while the request was open",
            )
            .unresolved());
        }
        Ok(())
    }
}

/// Holds a request where the fake was told to stop, if it was, and gives what it saw.
fn held(stop: &Option<Stop>, bounds: &Bounds) -> Result<(), Fault> {
    stop.as_ref().map_or(Ok(()), |stop| stop.hold(bounds))
}

fn observed(stop: &Option<Stop>) -> Vec<Observed> {
    stop.as_ref()
        .map_or_else(Vec::new, |stop| lock(&stop.observed).clone())
}

/// What a [`FakeFiller`] paints.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    /// The whole image repainted: every channel of every pixel is 255 less what the
    /// image holds there, inside the support and outside it. No pixel is left as it
    /// was, which is what a composite must be able to undo.
    Inverse,
    /// The whole image one colour.
    Flat([u8; 3]),
    /// One colour where the request's support plane is 255, and the image's own colours
    /// everywhere else: a filler that behaves. It reads the support plane and nothing
    /// of a mask: a support plane holds 0 and 255 only, so a pixel under the faintest
    /// blend is in it and is painted the whole colour. Mixing by a weight is the
    /// composite's.
    Support([u8; 3]),
    /// A grey answer of this size, whatever was asked.
    Size(u32, u32),
    /// No answer: this fault.
    Fails(Fault),
}

/// A fake filler. It may be shared across threads: it holds a count and nothing else
/// that changes. It declares the model `fake-filler` of the backend `fake`, and the
/// feature `prompt` unless told otherwise.
pub struct FakeFiller {
    declaration: Declaration,
    paint: Paint,
    requests: AtomicU64,
    stop: Option<Stop>,
    terms: Terms,
}

impl FakeFiller {
    pub fn new(paint: Paint) -> FakeFiller {
        FakeFiller {
            declaration: declaration("fake-filler", &[feature::PROMPT]),
            paint,
            requests: AtomicU64::new(0),
            stop: None,
            terms: Terms::default(),
        }
    }

    /// The same filler, declaring these features and no others. What it states of its
    /// price and its recipient stays.
    pub fn declaring(mut self, features: &[&str]) -> FakeFiller {
        self.declaration = redeclared(&self.declaration, None, features);
        self
    }

    /// The same filler, stating this price (ADR 0010).
    pub fn priced(mut self, price: Price) -> FakeFiller {
        self.declaration = self.declaration.priced(price);
        self
    }

    /// The same filler, naming who receives its requests (ADR 0012). Nothing is sent:
    /// the fake only says so, as a remote backend's declaration would.
    pub fn sending_to(mut self, recipient: Recipient) -> FakeFiller {
        self.declaration = self.declaration.sending_to(recipient);
        self
    }

    /// The same filler, reporting this charge, in millionths of a US dollar, with every
    /// answer: under its bound, over it, as the test chooses. A fake that was not
    /// told reports none.
    pub fn charging(mut self, micros: u64) -> FakeFiller {
        self.terms.charge = Some(charge(micros));
        self
    }

    /// The same filler, which answers every request with a fault that says the request
    /// got this far ([`Terms::failed`]). The request is counted first, as any is.
    pub fn failing_at(mut self, stage: Stage) -> FakeFiller {
        self.terms.fails_at = Some(stage);
        self
    }

    /// How many requests it was sent, answered or not: how many times the backend was
    /// called. A request it was told to fail is counted, at whatever stage it says it
    /// failed, `not_sent` among them: the count is of calls, not of what left.
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::SeqCst)
    }
}

impl FakeFiller {
    /// The same filler, which holds every request it is sent open at the barrier
    /// `name` until a test lets it go, after it has counted the request. A request
    /// whose run is cancelled by then is answered with the fault `cancelled`.
    pub fn stopping_at(mut self, barriers: &Barriers, name: &str) -> FakeFiller {
        self.stop = Stop::at(barriers, name);
        self
    }

    /// What it saw of each request it held open, in the order they were let go.
    pub fn observed(&self) -> Vec<Observed> {
        observed(&self.stop)
    }
}

impl Filler for FakeFiller {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        held(&self.stop, bounds)?;
        self.terms.failed()?;
        let painted = self.painted(request)?;
        Ok(match self.terms.charge {
            Some(charge) => painted.charged(charge),
            None => painted,
        })
    }
}

impl FakeFiller {
    /// What the fake paints for a request.
    fn painted(&self, request: &FillRequest) -> Result<Painted, Fault> {
        let image = request.image();
        let pixels = image.rgba().as_chunks::<4>().0.iter();
        let rgb: Vec<u8> = match &self.paint {
            Paint::Inverse => pixels
                .flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2]])
                .collect(),
            Paint::Flat(colour) => pixels.flat_map(|_| *colour).collect(),
            Paint::Support(colour) => pixels
                .zip(request.support().plane())
                .flat_map(|(p, sample)| {
                    if *sample == Support::PAINT {
                        *colour
                    } else {
                        [p[0], p[1], p[2]]
                    }
                })
                .collect(),
            Paint::Size(width, height) => {
                let bytes = u64::from(*width) * u64::from(*height) * 3;
                return Ok(Painted::from_rgb(
                    *width,
                    *height,
                    vec![128; bytes as usize],
                )?);
            }
            Paint::Fails(fault) => return Err(fault.clone()),
        };
        Ok(Painted::from_rgb(image.width(), image.height(), rgb)?)
    }
}

/// A fake segmenter. It may be shared across threads, as the filler may. It declares the model `fake-segmenter` of the backend `fake`, and
/// the features `phrase`, `point` and `box` unless told otherwise.
///
/// A phrase is answered with the boxes it was told to find for it, in the order it was
/// told, with scores from 0.9 down by 0.1; a phrase it was told nothing of is answered
/// with no instance. A point is answered with the pixel under it, and a box with the
/// whole pixels it touches, each with a score of 1. No answer holds more instances than
/// the request asks for.
pub struct FakeSegmenter {
    declaration: Declaration,
    told: Vec<(String, [u32; 4])>,
    requests: AtomicU64,
    stop: Option<Stop>,
    fails: Option<Fault>,
    terms: Terms,
}

impl FakeSegmenter {
    pub fn new() -> FakeSegmenter {
        FakeSegmenter {
            declaration: declaration(
                "fake-segmenter",
                &[feature::PHRASE, feature::POINT, feature::BOX],
            ),
            told: Vec::new(),
            requests: AtomicU64::new(0),
            stop: None,
            fails: None,
            terms: Terms::default(),
        }
    }

    /// The same segmenter, declaring these features and no others. What it states of
    /// its price and its recipient stays.
    pub fn declaring(mut self, features: &[&str]) -> FakeSegmenter {
        self.declaration = redeclared(&self.declaration, None, features);
        self
    }

    /// The same segmenter, which also finds, for this phrase, the whole pixels of this
    /// box: left, top, right and bottom.
    pub fn finding(mut self, phrase: &str, bounds: [u32; 4]) -> FakeSegmenter {
        self.told.push((phrase.to_owned(), bounds));
        self
    }

    /// The same segmenter, stating this price (ADR 0010).
    pub fn priced(mut self, price: Price) -> FakeSegmenter {
        self.declaration = self.declaration.priced(price);
        self
    }

    /// The same segmenter, naming who receives its requests (ADR 0012). Nothing is sent:
    /// the fake only says so, as a remote backend's declaration would.
    pub fn sending_to(mut self, recipient: Recipient) -> FakeSegmenter {
        self.declaration = self.declaration.sending_to(recipient);
        self
    }

    /// The same segmenter, reporting this charge, in millionths of a US dollar, with every
    /// answer: under its bound, over it, as the test chooses. A fake that was not
    /// told reports none.
    pub fn charging(mut self, micros: u64) -> FakeSegmenter {
        self.terms.charge = Some(charge(micros));
        self
    }

    /// The same segmenter, which answers every request with a fault that says the request
    /// got this far ([`Terms::failed`]). The request is counted first, as any is.
    pub fn failing_at(mut self, stage: Stage) -> FakeSegmenter {
        self.terms.fails_at = Some(stage);
        self
    }

    /// How many requests it was sent, answered or not: how many times the backend was
    /// called, a request it was told to fail as `not_sent` among them. A batch is one
    /// request.
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::SeqCst)
    }
}

impl FakeSegmenter {
    /// The same segmenter, which holds every request open at the barrier `name`, as
    /// [`FakeFiller::stopping_at`] does.
    pub fn stopping_at(mut self, barriers: &Barriers, name: &str) -> FakeSegmenter {
        self.stop = Stop::at(barriers, name);
        self
    }

    /// What it saw of each request it held open, in the order they were let go.
    pub fn observed(&self) -> Vec<Observed> {
        observed(&self.stop)
    }

    /// The same segmenter, which answers every request with this fault.
    pub fn failing(mut self, fault: Fault) -> FakeSegmenter {
        self.fails = Some(fault);
        self
    }
}

impl Default for FakeSegmenter {
    fn default() -> FakeSegmenter {
        FakeSegmenter::new()
    }
}

/// The instance that is the whole pixels of a box, in an image of this size.
fn boxed(width: u32, height: u32, bounds: [u32; 4], score: f64) -> Result<Instance, Fault> {
    let [left, top, right, bottom] = bounds;
    let mut plane = vec![0; width as usize * height as usize];
    for y in top..bottom.min(height) {
        for x in left..right.min(width) {
            plane[y as usize * width as usize + x as usize] = 255;
        }
    }
    Ok(Instance::new(
        width,
        height,
        plane,
        bounds.map(f64::from),
        score,
    )?)
}

impl Segmenter for FakeSegmenter {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn segment(&self, request: &SegmentRequest, bounds: &Bounds) -> Result<SegmentResponse, Fault> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        held(&self.stop, bounds)?;
        if let Some(fault) = &self.fails {
            return Err(fault.clone());
        }
        self.terms.failed()?;
        let (width, height) = (request.image().width(), request.image().height());
        let most = request.max_instances() as usize;
        let mut answers = Vec::new();
        for query in request.queries() {
            let found = match query.asked() {
                Asked::Phrase(phrase) => self
                    .told
                    .iter()
                    .filter(|(told, _)| told == phrase)
                    .take(most)
                    .enumerate()
                    .map(|(at, (_, bounds))| {
                        boxed(width, height, *bounds, (0.9 - 0.1 * at as f64).max(0.0))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                Asked::Point { x, y } => {
                    let (x, y) = ((x as u32).min(width - 1), (y as u32).min(height - 1));
                    vec![boxed(width, height, [x, y, x + 1, y + 1], 1.0)?]
                }
                Asked::Box {
                    left,
                    top,
                    right,
                    bottom,
                } => {
                    let touched = [
                        left.floor() as u32,
                        top.floor() as u32,
                        right.ceil() as u32,
                        bottom.ceil() as u32,
                    ];
                    vec![boxed(width, height, touched, 1.0)?]
                }
            };
            answers.push(found);
        }
        let response = SegmentResponse::new(answers);
        Ok(match self.terms.charge {
            Some(charge) => response.charged(charge),
            None => response,
        })
    }
}

/// A fake judge: it plays a scripted transcript. Each request is answered with the
/// next entry of the script, a turn or a fault, whatever the request holds: it does not
/// look at what it is asked, so a request the script's writer did not expect is
/// answered like any other. Past the end of the script it answers with the fault
/// `malformed`, and never panics. It keeps every request it was sent.
///
/// It may be shared across threads. Taking a request and playing the next entry are
/// one step, so of many simultaneous requests each gets one entry, and the request
/// kept in place `n` is the one that was answered with entry `n`.
///
/// It declares the model `fake-judge` of the backend `fake`, and the features `image`
/// and `tool` unless told otherwise. An opaque part in its script names the adapter
/// [`FakeJudge::ADAPTER`], or the handle refuses the turn.
pub struct FakeJudge {
    declaration: Declaration,
    played: Mutex<Played>,
    stop: Option<Stop>,
    terms: Terms,
}

/// What is left of the script, and what was received, behind one lock.
struct Played {
    script: VecDeque<Result<Turn, Fault>>,
    received: Vec<JudgeRequest>,
}

impl FakeJudge {
    /// The adapter the fake's opaque parts are marked with: its backend's name.
    pub const ADAPTER: &'static str = BACKEND;

    /// A judge that answers with these, in order.
    pub fn scripted(script: Vec<Result<Turn, Fault>>) -> FakeJudge {
        FakeJudge {
            declaration: declaration("fake-judge", &[feature::IMAGE, feature::TOOL]),
            played: Mutex::new(Played {
                script: script.into(),
                received: Vec::new(),
            }),
            stop: None,
            terms: Terms::default(),
        }
    }

    /// A judge that plays the script these bytes are (`judge-script/1`, ADR 0009): each
    /// entry is read through the constructors that check a turn and a fault.
    pub fn from_script(bytes: &[u8]) -> Result<FakeJudge, Unread> {
        Ok(FakeJudge::scripted(read_script(bytes)?))
    }

    /// The same judge, declaring this model in place of `fake-judge`, with the features
    /// it declared: a second judge of one build.
    pub fn as_model(mut self, model: &str) -> FakeJudge {
        let features: Vec<String> = self.declaration.features().map(str::to_owned).collect();
        let features: Vec<&str> = features.iter().map(String::as_str).collect();
        self.declaration = redeclared(&self.declaration, Some(model), &features);
        self
    }

    /// The same judge, declaring this in place of what it declared.
    pub fn declared_as(mut self, declaration: Declaration) -> FakeJudge {
        self.declaration = declaration;
        self
    }

    /// The same judge, declaring these features and no others. The model stays, the
    /// one given by [`FakeJudge::as_model`] included, and so do what it states of its
    /// price and its recipient.
    pub fn declaring(mut self, features: &[&str]) -> FakeJudge {
        self.declaration = redeclared(&self.declaration, None, features);
        self
    }

    /// The same judge, stating this price (ADR 0010).
    pub fn priced(mut self, price: Price) -> FakeJudge {
        self.declaration = self.declaration.priced(price);
        self
    }

    /// The same judge, naming who receives its requests (ADR 0012). Nothing is sent:
    /// the fake only says so, as a remote backend's declaration would.
    pub fn sending_to(mut self, recipient: Recipient) -> FakeJudge {
        self.declaration = self.declaration.sending_to(recipient);
        self
    }

    /// The same judge, reporting this charge, in millionths of a US dollar, with every
    /// answer: under its bound, over it, as the test chooses. It replaces the charge a
    /// scripted turn carries of its own. A fake that was not told reports none, and
    /// plays each turn with the charge the script gave it.
    pub fn charging(mut self, micros: u64) -> FakeJudge {
        self.terms.charge = Some(charge(micros));
        self
    }

    /// The same judge, which answers every request with a fault that says the request
    /// got this far ([`Terms::failed`]). The request is counted first, as any is.
    pub fn failing_at(mut self, stage: Stage) -> FakeJudge {
        self.terms.fails_at = Some(stage);
        self
    }

    /// How many requests it was sent, answered or not: how many times the backend was
    /// called, a request it was told to fail as `not_sent` among them.
    pub fn requests(&self) -> u64 {
        lock(&self.played).received.len() as u64
    }

    /// Every request it was sent, in order.
    pub fn received(&self) -> Vec<JudgeRequest> {
        lock(&self.played).received.clone()
    }

    /// How many entries of the script have not been played.
    pub fn remaining(&self) -> usize {
        lock(&self.played).script.len()
    }
}

impl FakeJudge {
    /// The same judge, which holds every request open at the barrier `name`, as
    /// [`FakeFiller::stopping_at`] does: after it has kept the request, and before it
    /// plays an entry. So a request that is cancelled while it is held plays none, and
    /// of several held at once each plays the next entry when it is let go.
    pub fn stopping_at(mut self, barriers: &Barriers, name: &str) -> FakeJudge {
        self.stop = Stop::at(barriers, name);
        self
    }

    /// What it saw of each request it held open, in the order they were let go.
    pub fn observed(&self) -> Vec<Observed> {
        observed(&self.stop)
    }
}

impl Judge for FakeJudge {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn send(&self, request: &JudgeRequest, bounds: &Bounds) -> Result<Turn, Fault> {
        if self.stop.is_some() {
            lock(&self.played).received.push(request.clone());
            held(&self.stop, bounds)?;
        }
        let mut played = lock(&self.played);
        if self.stop.is_none() {
            played.received.push(request.clone());
        }
        // A judge told to fail keeps the request and plays no entry of its script.
        self.terms.failed()?;
        let entry = played.script.pop_front().unwrap_or_else(|| {
            Err(Fault::new(
                FaultKind::Malformed,
                "the scripted transcript has no turn left",
            )
            .unresolved())
        });
        match (entry, self.terms.charge) {
            (Ok(turn), Some(charge)) => Ok(turn.charged(charge)),
            (entry, _) => entry,
        }
    }
}
