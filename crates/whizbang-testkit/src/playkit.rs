//! The playground's fakes (plan m5, task W1): a kind of backend, `playkit`, whose
//! segmenter, filler and judge a script steers, so that a Chrome case or a test of the
//! host can run every task of the build with no model and nothing sent.
//!
//! - The segmenter finds any phrase as one box over the middle half of the picture, and
//!   answers a point or a box with the pixels it names.
//! - The filler paints what its support covers one colour, and reports the price of
//!   one request as its charge.
//! - The judge reads the checklist it is shown (each line `- chk:…: …`) and submits a
//!   disposition for every item: each held (`pass`), or one a confident violation
//!   (`fail`): the loop's requirement that the thing named is gone, where the job has it,
//!   so that the loop tries again, else the first. It reports its price as its charge.
//!
//! **The script** is a few words, comma separated ([`PlayKit::set`]): `hold` makes every
//! request wait, after it is counted, until `release` or until its run is cancelled (which
//! is answered with the fault `cancelled`; `release,hold` lets go every request held then and
//! holds those that come after); `pass` and `fail` choose the judge's answer;
//! `hold-pictures` holds every picture a host asks [`PlayKit::picture_asked`] about until
//! `release`; `unsure` makes the judge say it cannot tell, tentatively, of the item a
//! `fail` violates; `placed` makes a violation cite a place, a box over the middle half of
//! the candidate; `markup` puts markup into every reason the judge gives (plan m5, task
//! W3: a report shows them as text); `reset` is `pass` and `release`, without either. A test or the fixture host sets it; nothing else does.
//!
//! A table of the kind may name a credential (`key_env`) and an address (`url`): a
//! backend whose credential is not set is not ready, as a real one is, and the address
//! is the recipient it declares. It sends nothing to it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};
use whizbang_core::capability::written::read_script;
use whizbang_core::capability::{
    Asked, Bounds, Charge, Declaration, Fault, FaultKind, FillRequest, Filler, Instance, Judge,
    JudgeRequest, Painted, Part, Price, SegmentRequest, SegmentResponse, Segmenter, Turn, feature,
};
use whizbang_core::config::{Kind, KindRefused, Table};
use whizbang_core::contract::{Capability, Made, ModelIdentity};
use whizbang_core::egress::Recipient;

use crate::capabilities::{FakeFiller, Paint};

/// The name of the kind.
pub const KIND: &str = "playkit";

/// The longest a held request waits for `release`, so that nothing is held for ever.
pub const HOLD_LIMIT: Duration = Duration::from_secs(25);

/// What a request costs, in millionths of a US dollar: a fill four cents, a judge's turn
/// a fifth of a cent; a segmenter is local and free.
pub const FILL_MICROS: u64 = 40_000;
pub const JUDGE_MICROS: u64 = 2_000;

/// What the judge answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Pass,
    Fail,
    Unsure,
}

/// The reason the judge gives each item under the script's `markup`.
pub const MARKUP_REASON: &str =
    "<img src=x onerror=\"document.title='ran'\"><script>document.title='ran'</script> held";

#[derive(Debug)]
struct Script {
    hold: bool,
    /// Whether a picture the page asks for is held, as a request is by `hold`.
    hold_pictures: bool,
    answer: Answer,
    /// Whether the judge's reasons hold markup.
    markup: bool,
    /// Whether a violation cites a place.
    placed: bool,
    /// How many times `release` was said: a request held lets go once it changes, so
    /// `release,hold` lets every request held go and holds those that come after.
    released: u64,
}

/// The script and the requests it held, shared by every backend the kind makes.
#[derive(Debug)]
pub struct PlayKit {
    script: Mutex<Script>,
    changed: Condvar,
    requests: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PlayKit {
    pub fn new() -> Arc<PlayKit> {
        Arc::new(PlayKit {
            script: Mutex::new(Script {
                hold: false,
                hold_pictures: false,
                answer: Answer::Pass,
                markup: false,
                placed: false,
                released: 0,
            }),
            changed: Condvar::new(),
            requests: AtomicU64::new(0),
        })
    }

    /// Sets the script from its words, comma separated: `hold`, `release`, `pass`,
    /// `fail`, `unsure`, `markup`, `reset`. A word it does not know is refused, and nothing is changed.
    pub fn set(&self, words: &str) -> Result<(), String> {
        let words: Vec<&str> = words
            .split(',')
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .collect();
        if let Some(unknown) = words.iter().find(|w| {
            ![
                "hold",
                "hold-pictures",
                "release",
                "pass",
                "fail",
                "unsure",
                "markup",
                "placed",
                "reset",
            ]
            .contains(w)
        }) {
            let _ = unknown;
            return Err(
                "a script is hold, hold-pictures, release, pass, fail, unsure, markup, placed or reset"
                    .to_owned(),
            );
        }
        let mut script = lock(&self.script);
        for word in words {
            match word {
                "hold" => script.hold = true,
                "hold-pictures" => script.hold_pictures = true,
                "release" => {
                    script.hold = false;
                    script.hold_pictures = false;
                    script.released += 1;
                }
                "pass" => script.answer = Answer::Pass,
                "fail" => script.answer = Answer::Fail,
                "unsure" => script.answer = Answer::Unsure,
                "markup" => script.markup = true,
                "placed" => script.placed = true,
                _ => {
                    script.markup = false;
                    script.placed = false;
                    script.hold = false;
                    script.hold_pictures = false;
                    script.released += 1;
                    script.answer = Answer::Pass;
                }
            }
        }
        drop(script);
        self.changed.notify_all();
        Ok(())
    }

    /// How many requests its backends were sent.
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::SeqCst)
    }

    /// Counts a request, then holds it while the script says `hold`: until released,
    /// its run is cancelled, or [`HOLD_LIMIT`] has passed.
    fn arrive(&self, bounds: &Bounds) -> Result<Answer, Fault> {
        self.arrive_saying(bounds).map(|(answer, _, _)| answer)
    }

    /// [`PlayKit::arrive`], and whether the judge's reasons hold markup and a violation
    /// cites a place.
    fn arrive_saying(&self, bounds: &Bounds) -> Result<(Answer, bool, bool), Fault> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let limit = Instant::now() + HOLD_LIMIT;
        let mut script = lock(&self.script);
        let arrived = script.released;
        while script.hold && script.released == arrived && Instant::now() < limit {
            if bounds.is_cancelled() {
                return Err(Fault::new(
                    FaultKind::Cancelled,
                    "the run was cancelled while the request was held",
                )
                .unresolved());
            }
            script = self
                .changed
                .wait_timeout(script, Duration::from_millis(20))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        if bounds.is_cancelled() {
            return Err(Fault::new(FaultKind::Cancelled, "the run was cancelled").unresolved());
        }
        Ok((script.answer, script.markup, script.placed))
    }

    /// Holds a picture the page asked for while the script says `hold-pictures`: until
    /// released, or [`HOLD_LIMIT`] has passed. What a host that serves pictures calls
    /// first, so that a test can hold the page's connections open.
    pub fn picture_asked(&self) {
        let limit = Instant::now() + HOLD_LIMIT;
        let mut script = lock(&self.script);
        let arrived = script.released;
        while script.hold_pictures && script.released == arrived && Instant::now() < limit {
            script = self
                .changed
                .wait_timeout(script, Duration::from_millis(20))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// The kind that makes this kit's backends.
    pub fn kind(self: &Arc<Self>) -> Arc<dyn Kind> {
        Arc::new(Making(self.clone()))
    }
}

fn charge(micros: u64) -> Option<Charge> {
    Charge::whole(micros).ok()
}

struct Making(Arc<PlayKit>);

impl Kind for Making {
    fn name(&self) -> &str {
        KIND
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let backend = table.backend();
        let model = ModelIdentity::new(KIND, backend.model().unwrap_or(table.name()), "1");
        let (features, micros): (Vec<&str>, u64) = match table.capability() {
            Capability::Segment => (vec![feature::PHRASE, feature::POINT, feature::BOX], 0),
            Capability::Fill => (vec![feature::PROMPT], FILL_MICROS),
            Capability::Judge => (vec![feature::IMAGE, feature::TOOL], JUDGE_MICROS),
        };
        let mut declaration = Declaration::new(model, &features)
            .map_err(|_| KindRefused::Table("a playkit backend cannot declare that model"))?;
        #[allow(clippy::cast_precision_loss)]
        let price = match micros {
            0 => Price::free(),
            _ => Price::at_most(micros as f64)
                .map_err(|_| KindRefused::Table("a playkit price is a number"))?,
        };
        declaration = declaration.priced(price);
        // A credential it names is carried to its address, as a real backend's is.
        if let (Some(key), Some(address)) = (table.key(), backend.address()) {
            key.carried_to(address, "x-playkit-key", table.secrets())
                .map_err(KindRefused::Credential)?;
        }
        if let Some(address) = backend.address() {
            declaration = declaration.sending_to(Recipient::new(address.origin().clone()));
        }
        let kit = self.0.clone();
        Ok(match table.capability() {
            Capability::Segment => Made::Segment(Arc::new(PlaySegmenter { kit, declaration })),
            Capability::Fill => Made::Fill(Arc::new(PlayFiller {
                kit,
                inner: FakeFiller::new(Paint::Support([96, 160, 112]))
                    .declared_as(declaration)
                    .charging(FILL_MICROS),
            })),
            Capability::Judge => Made::Judge(Arc::new(PlayJudge { kit, declaration })),
        })
    }
}

struct PlaySegmenter {
    kit: Arc<PlayKit>,
    declaration: Declaration,
}

/// The instance that is the whole pixels of a box, in the request's picture.
fn boxed(request: &SegmentRequest, bounds: [u32; 4]) -> Result<Instance, Fault> {
    let (width, height) = (request.image().width(), request.image().height());
    let [left, top, right, bottom] = bounds;
    let (right, bottom) = (
        right.min(width).max(left + 1),
        bottom.min(height).max(top + 1),
    );
    let mut plane = vec![0; width as usize * height as usize];
    for y in top..bottom {
        for x in left..right {
            plane[y as usize * width as usize + x as usize] = 255;
        }
    }
    Ok(Instance::new(
        request.binding(),
        width,
        height,
        plane,
        [left, top, right, bottom].map(f64::from),
        0.9,
    )?)
}

impl Segmenter for PlaySegmenter {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn segment(&self, request: &SegmentRequest, bounds: &Bounds) -> Result<SegmentResponse, Fault> {
        self.kit.arrive(bounds)?;
        let (width, height) = (request.image().width(), request.image().height());
        let mut answers = Vec::new();
        for query in request.queries() {
            let found = match query.asked() {
                Asked::Phrase(_) => [width / 4, height / 4, 3 * width / 4, 3 * height / 4],
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                Asked::Point { x, y } => {
                    let (x, y) = ((x as u32).min(width - 1), (y as u32).min(height - 1));
                    [x, y, x + 1, y + 1]
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                Asked::Box {
                    left,
                    top,
                    right,
                    bottom,
                } => [
                    left.floor() as u32,
                    top.floor() as u32,
                    right.ceil() as u32,
                    bottom.ceil() as u32,
                ],
            };
            answers.push(vec![boxed(request, found)?]);
        }
        Ok(SegmentResponse::new(answers))
    }
}

struct PlayFiller {
    kit: Arc<PlayKit>,
    inner: FakeFiller,
}

impl Filler for PlayFiller {
    fn declaration(&self) -> &Declaration {
        self.inner.declaration()
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        self.kit.arrive(bounds)?;
        self.inner.fill(request, bounds)
    }
}

struct PlayJudge {
    kit: Arc<PlayKit>,
    declaration: Declaration,
}

/// The checklist's items in what the judge was shown: each line `- <id>: <words>` whose
/// id starts `chk:`.
fn checklist(request: &JudgeRequest) -> Vec<String> {
    let mut ids = Vec::new();
    for message in request.messages() {
        for part in message.parts() {
            let Part::Text(text) = part else { continue };
            for line in text.lines() {
                let Some(rest) = line.strip_prefix("- chk:") else {
                    continue;
                };
                let id = format!("chk:{}", rest.split(": ").next().unwrap_or_default());
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    ids
}

/// Where a violation is: the whole candidate, which cites no place.
fn whole() -> Json {
    json!({"kind": "whole_image", "frame": "raw:candidate", "bbox": null, "polygon": null,
           "yaw": null, "pitch": null, "fov": null})
}

/// Where a violation is under `placed`: the middle half of the candidate, where the
/// segmenter finds anything, as a box in fractions of the picture.
fn middle() -> Json {
    json!({"kind": "bbox", "frame": "raw:candidate", "bbox": [0.25, 0.25, 0.75, 0.75],
           "polygon": null, "yaw": null, "pitch": null, "fov": null})
}

impl Judge for PlayJudge {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn send(&self, request: &JudgeRequest, bounds: &Bounds) -> Result<Turn, Fault> {
        let (answer, markup, placed) = self.kit.arrive_saying(bounds)?;
        let reason = match markup {
            true => MARKUP_REASON,
            false => "The fixture's judge says so.",
        };
        let ids = checklist(request);
        // The item a failed check violates: the loop's own requirement, that the thing
        // named is gone, where the job has it; else the first.
        let violated = ids.iter().position(|id| id.contains(":RQ1@")).unwrap_or(0);
        let items: Vec<Json> = ids
            .iter()
            .enumerate()
            .map(|(at, id)| {
                let failed = answer == Answer::Fail && at == violated;
                let unsure = answer == Answer::Unsure && at == violated;
                let (disposition, confidence) = match (failed, unsure) {
                    (true, _) => ("violation", "confident"),
                    (_, true) => ("cant_tell", "tentative"),
                    _ => ("held", "confident"),
                };
                json!({"id": id, "disposition": disposition, "confidence": confidence,
                "reason": reason, "allowed_by": null,
                "evidence": [], "location": match (failed, placed) {
                    (true, true) => middle(),
                    (true, false) => whole(),
                    _ => Json::Null,
                }})
            })
            .collect();
        let verdict = match answer {
            Answer::Pass => "pass",
            Answer::Fail => "fail",
            Answer::Unsure => "unsure",
        };
        let submission = json!({"schema_version": 1, "items": items, "findings": [],
                                "opinion": {"verdict": verdict, "reason": reason}});
        let call =
            json!({"tool_call": {"id": "s1", "name": "submit_verdict", "arguments": submission}});
        let script = json!({"script": "judge-script/1", "entries": [
            {"turn": {"parts": [call], "stop": "tool_use", "usage": null, "request_id": null}}
        ]});
        let turn = read_script(script.to_string().as_bytes())
            .ok()
            .and_then(|mut turns| turns.pop())
            .unwrap_or_else(|| Err(Fault::new(FaultKind::Malformed, "the fixture's turn")))?;
        // It says which model answered, as a real provider does, so that a decision it
        // made is kept for the decision cache.
        let turn = turn.reported_by(Some("playkit-judge"));
        Ok(match charge(JUDGE_MICROS) {
            Some(charge) => turn.charged(charge),
            None => turn,
        })
    }
}
