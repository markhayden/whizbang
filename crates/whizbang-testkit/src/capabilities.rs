//! Fakes of the three capabilities (ADR 0006): a segmenter that finds what it was told
//! to find, a filler that can repaint the whole image, and a judge that plays a
//! scripted transcript. No model runs, nothing is sent anywhere, and each fake counts
//! the requests it was sent, so a test can show that something asked, or did not ask.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use whizbang_core::capability::{
    Asked, Bounds, Declaration, Fault, FaultKind, FillRequest, Filler, Instance, Judge,
    JudgeRequest, Painted, SegmentRequest, SegmentResponse, Segmenter, Support, Turn, feature,
};
use whizbang_core::contract::ModelIdentity;

/// The backend every fake declares.
const BACKEND: &str = "fake";

fn declaration(model: &str, features: &[&str]) -> Declaration {
    match Declaration::new(ModelIdentity::new(BACKEND, model, "1"), features) {
        Ok(declaration) => declaration,
        // The features are written in this file or by a test, and a test builds each.
        Err(why) => panic!("{why}"),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
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
    /// One colour in the support, and the image's own colours everywhere else: a
    /// filler that behaves.
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
}

impl FakeFiller {
    pub fn new(paint: Paint) -> FakeFiller {
        FakeFiller {
            declaration: declaration("fake-filler", &[feature::PROMPT]),
            paint,
            requests: AtomicU64::new(0),
        }
    }

    /// The same filler, declaring these features and no others.
    pub fn declaring(mut self, features: &[&str]) -> FakeFiller {
        self.declaration = declaration("fake-filler", features);
        self
    }

    /// How many requests it was sent, answered or not.
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::SeqCst)
    }
}

impl Filler for FakeFiller {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, request: &FillRequest, _: &Bounds) -> Result<Painted, Fault> {
        self.requests.fetch_add(1, Ordering::SeqCst);
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
        }
    }

    /// The same segmenter, declaring these features and no others.
    pub fn declaring(mut self, features: &[&str]) -> FakeSegmenter {
        self.declaration = declaration("fake-segmenter", features);
        self
    }

    /// The same segmenter, which also finds, for this phrase, the whole pixels of this
    /// box: left, top, right and bottom.
    pub fn finding(mut self, phrase: &str, bounds: [u32; 4]) -> FakeSegmenter {
        self.told.push((phrase.to_owned(), bounds));
        self
    }

    /// How many requests it was sent, answered or not. A batch is one request.
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::SeqCst)
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

    fn segment(&self, request: &SegmentRequest, _: &Bounds) -> Result<SegmentResponse, Fault> {
        self.requests.fetch_add(1, Ordering::SeqCst);
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
        Ok(SegmentResponse::new(answers))
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
        }
    }

    /// The same judge, declaring these features and no others.
    pub fn declaring(mut self, features: &[&str]) -> FakeJudge {
        self.declaration = declaration("fake-judge", features);
        self
    }

    /// How many requests it was sent, answered or not.
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

impl Judge for FakeJudge {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn send(&self, request: &JudgeRequest, _: &Bounds) -> Result<Turn, Fault> {
        let mut played = lock(&self.played);
        played.received.push(request.clone());
        played.script.pop_front().unwrap_or_else(|| {
            Err(Fault::new(
                FaultKind::Malformed,
                "the scripted transcript has no turn left",
            ))
        })
    }
}
