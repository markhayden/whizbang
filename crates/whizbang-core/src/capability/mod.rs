//! The capability interfaces (`SPEC.md` section 4, ADR 0006): the request and the
//! response of segment, fill and judge, what a backend declares, and how a call's choice
//! of backend is resolved.
//!
//! Nothing here implements a capability. A backend does, in another crate, and a fake
//! does for the tests. An action asks one through a handle in its
//! [`Context`](crate::contract::Context), which is bound to its run's cancellation and
//! deadline, and holds the request to what the backend declared and the answer to the
//! request.
//!
//! What a backend sends back is not trusted. Every type a response is made of is built
//! through a constructor that checks it, has private fields, and is never deserialised,
//! so a value of the type is a value that passed: a support plane in range, a fill of
//! three bytes a pixel, an instance whose box holds its plane, a turn with something in
//! it. What does not pass is [`Unfit`].

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use crate::contract::{Cancellation, Capability, Clock, ModelIdentity};
use crate::identity::{CanonicalPixels, PixelDigest};

mod declare;
mod fault;
mod fill;
mod judge;
mod segment;
pub mod written;

pub use declare::{
    Declaration, DeclarationError, MODEL_NAME_LIMIT, Resolved, Roster, RosterError, Unresolved,
    feature,
};
pub use fault::{FAULT_LIMIT, Fault, FaultKind};
pub use fill::{FillHandle, FillRequest, Filler, PROMPT_LIMIT, Painted, Support};
pub(crate) use judge::Judged;
pub use judge::{
    CONVERSATION_LIMIT, Controls, DESCRIPTION_LIMIT, Excluded, Judge, JudgeHandle, JudgeRequest,
    MESSAGE_LIMIT, Message, Opaque, PART_LIMIT, Part, RESULT_IMAGE_LIMIT, Role, StopReason,
    TEXT_LIMIT, TOOL_LIMIT, ToolCall, ToolDefinition, ToolResult, Turn, Usage,
};
pub use segment::{
    Asked, INSTANCE_LIMIT, Instance, PHRASE_LIMIT, QUERY_LIMIT, Query, SegmentHandle,
    SegmentRequest, SegmentResponse, Segmenter,
};

/// The most pixels an image, a plane or a fill may have: ADR 0001's default limit on a
/// decoded image, held here for everything a capability is sent or sends back.
pub const PIXEL_LIMIT: u64 = 50_000_000;

/// Why a value is not one a capability's request or response may hold. Each is a plain
/// sentence made of fixed words and numbers: it quotes nothing a backend sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unfit {
    /// Nothing where something is required: no pixels, no text, no content.
    Empty { what: &'static str },
    /// Over a bound.
    TooLarge {
        what: &'static str,
        size: u64,
        limit: u64,
    },
    /// Not as many bytes as its dimensions and its layout give: another channel count,
    /// or a plane cut short.
    WrongLength {
        what: &'static str,
        expected: u64,
        found: u64,
    },
    /// Not the dimensions of the image it belongs to.
    WrongSize {
        what: &'static str,
        expected: (u32, u32),
        found: (u32, u32),
    },
    /// A value outside its range: the first one, where it is when it is one of many,
    /// and what is allowed.
    OutOfRange {
        what: &'static str,
        at: Option<u64>,
        found: String,
        allowed: &'static str,
    },
    /// Something that may not be where it is.
    NotAllowed {
        what: &'static str,
        why: &'static str,
    },
}

impl std::fmt::Display for Unfit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unfit::Empty { what } => write!(f, "{what} is empty"),
            Unfit::TooLarge { what, size, limit } => {
                write!(f, "{what} is {size}, over the limit of {limit}")
            }
            Unfit::WrongLength {
                what,
                expected,
                found,
            } => write!(f, "{what} has {found} bytes, not {expected}"),
            Unfit::WrongSize {
                what,
                expected,
                found,
            } => write!(
                f,
                "{what} is {} by {}, not {} by {}",
                found.0, found.1, expected.0, expected.1
            ),
            Unfit::OutOfRange {
                what,
                at: Some(at),
                found,
                allowed,
            } => write!(f, "{what} holds {found} at {at}: a value is {allowed}"),
            Unfit::OutOfRange {
                what,
                at: None,
                found,
                allowed,
            } => write!(f, "{what} is {found}: a value is {allowed}"),
            Unfit::NotAllowed { what, why } => write!(f, "{what} {why}"),
        }
    }
}

impl std::error::Error for Unfit {}

/// The canonical pixels of an image (ADR 0001), as a capability is sent them: four bytes
/// a pixel, red, green, blue and straight alpha, sRGB, row by row from the top left.
///
/// A clone shares the pixels. What `{:?}` prints is the digest and the size, never the
/// pixels. Two pictures are equal when their digests are.
#[derive(Clone)]
pub struct Picture {
    digest: PixelDigest,
    pixels: Arc<CanonicalPixels>,
}

impl Picture {
    /// The picture of these pixels, within [`PIXEL_LIMIT`]. Its digest is worked out
    /// here, once.
    ///
    /// No test refuses a picture over the limit: to make one, a test would have to hold
    /// over 200 MB of pixels. The check is the one every plane and fill goes through
    /// (`pixel_count`), which is tested at the limit and one over it without a byte.
    pub fn new(pixels: CanonicalPixels) -> Result<Picture, Unfit> {
        pixel_count("the image", pixels.width(), pixels.height())?;
        Ok(Picture {
            digest: pixels.digest(),
            pixels: Arc::new(pixels),
        })
    }

    pub fn width(&self) -> u32 {
        self.pixels.width()
    }

    pub fn height(&self) -> u32 {
        self.pixels.height()
    }

    /// The canonical pixel digest (ADR 0001).
    pub fn digest(&self) -> PixelDigest {
        self.digest
    }

    pub fn pixels(&self) -> &CanonicalPixels {
        &self.pixels
    }

    /// Red, green, blue and alpha for each pixel, row by row from the top left.
    pub fn rgba(&self) -> &[u8] {
        self.pixels.rgba()
    }
}

impl PartialEq for Picture {
    fn eq(&self, other: &Picture) -> bool {
        self.digest == other.digest
    }
}

impl Eq for Picture {}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Picture({} by {}, {})",
            self.width(),
            self.height(),
            self.digest
        )
    }
}

/// The count of pixels of an image this wide and this high, when it is one a
/// capability may hold: neither side zero, and within [`PIXEL_LIMIT`].
pub(crate) fn pixel_count(what: &'static str, width: u32, height: u32) -> Result<u64, Unfit> {
    let size = u64::from(width) * u64::from(height);
    if size == 0 {
        return Err(Unfit::Empty { what });
    }
    if size > PIXEL_LIMIT {
        return Err(Unfit::TooLarge {
            what,
            size,
            limit: PIXEL_LIMIT,
        });
    }
    Ok(size)
}

/// What bounds one request: the run's cancellation signal, and its deadline when it has
/// one. A handle makes it, from the run it is bound to, and hands it to the backend with
/// every request; nothing else can make one. What a later task needs a backend to know
/// of its run is added here, and no trait changes for it.
#[derive(Clone)]
#[non_exhaustive]
pub struct Bounds {
    cancellation: Cancellation,
    deadline: Option<Instant>,
    /// Where the handle counts each request it sends, when its run counts them.
    tally: Option<Arc<Tally>>,
    /// What tells the time the deadline is held to, when it is not the machine's: the
    /// clock of the executor that made the run (ADR 0008, "A deadline in a run").
    clock: Option<Arc<dyn Clock>>,
}

impl std::fmt::Debug for Bounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bounds")
            .field("cancelled", &self.is_cancelled())
            .field("deadline", &self.deadline)
            .finish()
    }
}

/// The most requests the handles of one run send. One more is not sent: the fault is
/// `too_large`. A run's record lists every request it sent (ADR 0009), and a record is
/// read back only under a bound, so the list has one.
pub const REQUESTS_LIMIT: usize = 1024;

/// What the handles of one run are given by the executor to keep what they send: the
/// run's clock, and where the whole of an exchange with a judge is kept. The executor's
/// own keeper of a run's diagnostics is the one implementation.
pub(crate) trait Recorder: Send + Sync {
    /// The time, in milliseconds since the Unix epoch, by the executor's clock.
    fn now_ms(&self) -> u64;

    /// The text with the run's secrets taken out.
    fn redact(&self, text: &str) -> String;

    /// Keeps the written exchange with a judge that was the run's request at this
    /// place, from 0, when what would be stored of it, with the secrets taken out, is
    /// at most `room` bytes. What comes back is the name it is kept under among the
    /// run's names and how many bytes are stored; or none when it was not kept.
    fn exchange(&self, place: usize, written: &[u8], room: u64) -> Option<(String, u64)>;
}

/// The most bytes of exchanges with a judge that one run stores, as they are stored
/// (ADR 0009). A request whose exchange would pass it is not sent: the fault is
/// `too_large`.
pub const EXCHANGES_MAX_BYTES: u64 = 256 << 20;

/// What an exchange is given beside what was sent, when a handle asks whether there is
/// room for it: an answer that is not written takes less than this.
const ANSWER_ROOM: u64 = 1024;

/// How a request that was sent ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HowEnded {
    /// The backend answered, and the handle took the answer.
    Answered,
    /// It ended in a fault of this kind, the backend's or the handle's refusal of what
    /// the backend answered.
    Fault(FaultKind),
    /// The backend's call unwound: it panicked.
    Unwound,
}

/// One request a handle of the run sent: to which capability, which model answered,
/// when it was sent and when it was over, and how it ended.
#[derive(Debug, Clone)]
pub(crate) struct Logged {
    pub(crate) capability: Capability,
    pub(crate) model: ModelIdentity,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) ended: HowEnded,
    /// For a request to a judge: what the record keeps of the exchange.
    pub(crate) judged: Option<judge::Judged>,
}

/// The requests the handles of one run were sent, and how many of them are accounted
/// for: answered, failed, or given up by a backend that returned or panicked (ADR 0006,
/// "Accounting for requests"; ADR 0008, "Nested calls"). It is the decorator inside
/// the handle: a handle counts a request here just before its backend is called and
/// accounts for it when the backend's call is over, whatever it came to. A run is
/// terminal only once every request counted here is accounted for, and from the moment
/// its action has come back nothing more is sent.
///
/// It also keeps each request for the run's record (ADR 0009): what was asked of
/// which model, how long it took and how it ended.
#[derive(Default)]
pub(crate) struct Tally {
    counts: Mutex<Counts>,
    changed: Condvar,
}

impl std::fmt::Debug for Tally {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let counts = self.counts();
        write!(f, "Tally({} sent, {} over)", counts.sent, counts.accounted)
    }
}

#[derive(Default)]
struct Counts {
    sent: u64,
    accounted: u64,
    closed: bool,
    /// How many callers are waiting, now, for every request to be accounted for.
    waiting: usize,
    /// Every request that was sent, in the order they were sent.
    asked: Vec<Logged>,
    recorder: Option<Arc<dyn Recorder>>,
    /// How many requests may be sent, when it is fewer than [`REQUESTS_LIMIT`].
    limit: Option<usize>,
    /// How many bytes of exchanges the run may store, when it is fewer than
    /// [`EXCHANGES_MAX_BYTES`], and how many it has stored.
    budget: Option<u64>,
    stored: u64,
}

impl Tally {
    fn counts(&self) -> MutexGuard<'_, Counts> {
        self.counts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// From here on each request is timed by this recorder's clock, and each exchange
    /// with a judge is kept by it.
    /// At most `limit` requests are sent, and never more than [`REQUESTS_LIMIT`]; at
    /// most `budget` bytes of exchanges are stored, and never more than
    /// [`EXCHANGES_MAX_BYTES`].
    pub(crate) fn record_with(&self, recorder: Arc<dyn Recorder>, limit: usize, budget: u64) {
        let mut counts = self.counts();
        counts.recorder = Some(recorder);
        counts.limit = Some(limit.min(REQUESTS_LIMIT));
        counts.budget = Some(budget.min(EXCHANGES_MAX_BYTES));
    }

    fn recorder(&self) -> Option<Arc<dyn Recorder>> {
        self.counts().recorder.clone()
    }

    /// Counts one request that is about to be sent to this model. Refused once the
    /// tally is closed, and when the run has sent as many as one may: the look and
    /// the count are one step, so a request is either waited for or not sent.
    fn send(
        self: &Arc<Tally>,
        capability: Capability,
        model: &ModelIdentity,
    ) -> Result<Sent, Fault> {
        let mut counts = self.counts();
        if counts.closed {
            return Err(Fault::new(
                FaultKind::Cancelled,
                "the action of the run had come back before the request was sent",
            ));
        }
        let limit = counts.limit.unwrap_or(REQUESTS_LIMIT);
        if counts.asked.len() >= limit {
            return Err(Fault::new(
                FaultKind::TooLarge,
                &format!("the run has sent {limit} requests, which is as many as one run may"),
            ));
        }
        let now = counts
            .recorder
            .as_ref()
            .map_or(0, |recorder| recorder.now_ms());
        counts.sent += 1;
        counts.asked.push(Logged {
            capability,
            model: model.clone(),
            start_ms: now,
            end_ms: now,
            ended: HowEnded::Unwound,
            judged: None,
        });
        Ok(Sent {
            tally: self.clone(),
            place: counts.asked.len() - 1,
        })
    }

    /// From here on nothing is sent.
    pub(crate) fn close(&self) {
        self.counts().closed = true;
    }

    /// Returns once every request that was sent is accounted for. It has no bound: a
    /// backend that never returns is waited for.
    pub(crate) fn wait(&self) {
        let mut counts = self.counts();
        while counts.accounted < counts.sent {
            counts.waiting += 1;
            counts = self
                .changed
                .wait(counts)
                .unwrap_or_else(PoisonError::into_inner);
            counts.waiting -= 1;
        }
    }

    /// Whether a caller is waiting, now, for a request that is not accounted for.
    pub(crate) fn waited_for(&self) -> bool {
        self.counts().waiting > 0
    }

    /// How many requests were sent and are not accounted for, now.
    pub(crate) fn outstanding(&self) -> u64 {
        let counts = self.counts();
        counts.sent - counts.accounted
    }

    /// Every request that was sent, in order, as each is now.
    pub(crate) fn asked(&self) -> Vec<Logged> {
        self.counts().asked.clone()
    }

    /// Marks the answer of the last request sent to a judge as left out of the
    /// conversation that went on. False when no judge was asked.
    fn exclude_last(&self, why: judge::Excluded) -> bool {
        let mut counts = self.counts();
        let last = counts
            .asked
            .iter_mut()
            .rev()
            .find_map(|asked| asked.judged.as_mut());
        last.map(|judged| judged.excluded = Some(why)).is_some()
    }
}

/// One request that was counted as sent. Dropped, by a return or by a panic that
/// unwinds through the handle, it is accounted for.
pub(crate) struct Sent {
    tally: Arc<Tally>,
    place: usize,
}

impl Sent {
    /// The request ended this way: with an answer the handle took, or in a fault.
    pub(crate) fn ended<T>(&mut self, result: &Result<T, Fault>) {
        let ended = match result {
            Ok(_) => HowEnded::Answered,
            Err(fault) => HowEnded::Fault(fault.kind()),
        };
        self.tally.counts().asked[self.place].ended = ended;
    }

    /// What was sent, written, when the run keeps its exchanges: made before the
    /// request is sent.
    pub(crate) fn head(&self, request: &JudgeRequest) -> Option<written::Head> {
        let recorder = self.tally.recorder();
        recorder.map(|_| written::head(self.place, request, written::Keep::Digests))
    }

    /// Whether the run has room to store the exchange that begins with this. When it
    /// has not, the request is not to be sent: the fault says so.
    pub(crate) fn room(&self, head: &written::Head) -> Result<(), Fault> {
        let counts = self.tally.counts();
        let budget = counts.budget.unwrap_or(EXCHANGES_MAX_BYTES);
        let needed = head.len() + ANSWER_ROOM;
        let left = budget.saturating_sub(counts.stored);
        if needed > left.min(written::EXCHANGE_MAX_BYTES as u64) {
            return Err(Fault::new(
                FaultKind::TooLarge,
                &format!(
                    "the exchange would take {needed} bytes or more, and the run may store \
                     {left} more of its {budget}"
                ),
            ));
        }
        Ok(())
    }

    /// Keeps the exchange with a judge that this request was: the whole of it with
    /// the run's recorder, when the run has one, and what the record says of it here.
    /// The answer is written whole unless `unwritten` holds the turn to write the
    /// sizes of in its place, or the whole would be more than the run has room for.
    pub(crate) fn judged(
        &mut self,
        head: Option<&written::Head>,
        request: &JudgeRequest,
        answer: &Result<Turn, Fault>,
        unwritten: Option<&Turn>,
    ) {
        let (recorder, left) = {
            let counts = self.tally.counts();
            let budget = counts.budget.unwrap_or(EXCHANGES_MAX_BYTES);
            (
                counts.recorder.clone(),
                budget.saturating_sub(counts.stored),
            )
        };
        let room = left.min(written::EXCHANGE_MAX_BYTES as u64);
        let kept = recorder.zip(head).and_then(|(recorder, head)| {
            let whole = match unwritten {
                None => recorder.exchange(self.place, &head.answered(answer), room),
                Some(_) => None,
            };
            // What does not fit whole is kept with its turn not written.
            let turn = unwritten.or(answer.as_ref().ok());
            whole.or_else(|| recorder.exchange(self.place, &head.unwritten(turn?), room))
        });
        let (kept_as, stored) = kept.unzip();
        let judged = judge::Judged::of(request, answer, kept_as);
        let mut counts = self.tally.counts();
        counts.stored += stored.unwrap_or(0);
        counts.asked[self.place].judged = Some(judged);
    }
}

impl Drop for Sent {
    fn drop(&mut self) {
        let mut counts = self.tally.counts();
        let now = counts
            .recorder
            .as_ref()
            .map_or(0, |recorder| recorder.now_ms());
        let asked = &mut counts.asked[self.place];
        asked.end_ms = now.max(asked.start_ms);
        counts.accounted += 1;
        self.tally.changed.notify_all();
    }
}

impl Bounds {
    pub(crate) fn new(cancellation: Cancellation) -> Bounds {
        Bounds {
            cancellation,
            deadline: None,
            tally: None,
            clock: None,
        }
    }

    /// Counts every request sent under these bounds in this tally.
    pub(crate) fn counted(&mut self, tally: Arc<Tally>) {
        self.tally = Some(tally);
    }

    /// Counts the request that is about to be sent to this backend, when the run
    /// counts them. What comes back is held until the request is over.
    pub(crate) fn sending(
        &self,
        capability: Capability,
        declaration: &Declaration,
    ) -> Result<Option<Sent>, Fault> {
        let tally = self.tally.as_ref();
        tally
            .map(|tally| tally.send(capability, declaration.model()))
            .transpose()
    }

    /// Sends one request: counts it, calls `ask`, which calls the backend and holds
    /// its answer to the request, and keeps how it ended. A panic in `ask` unwinds
    /// through here, and the request is accounted for as unwound.
    pub(crate) fn send<T>(
        &self,
        capability: Capability,
        declaration: &Declaration,
        ask: impl FnOnce() -> Result<T, Fault>,
    ) -> Result<T, Fault> {
        let mut sent = self.sending(capability, declaration)?;
        let result = ask();
        if let Some(sent) = &mut sent {
            sent.ended(&result);
        }
        result
    }

    /// What takes the run's secrets out of a text, when the run's handles know them.
    pub(crate) fn redactor(&self) -> Option<Arc<dyn Recorder>> {
        self.tally.as_ref().and_then(|tally| tally.recorder())
    }

    /// A fault as the run's action may be told of it: its sentence made again with
    /// the run's secrets taken out of the whole text first (ADR 0009). Every fault a
    /// handle hands on goes through here.
    pub(crate) fn told<T>(&self, result: Result<T, Fault>) -> Result<T, Fault> {
        result.map_err(|fault| match self.redactor() {
            Some(recorder) => fault.redacted(&|text| recorder.redact(text)),
            None => fault,
        })
    }

    /// Marks the answer to the last request the run sent to a judge as left out.
    pub(crate) fn exclude_last(&self, why: judge::Excluded) -> bool {
        let tally = self.tally.as_ref();
        tally.is_some_and(|tally| tally.exclude_last(why))
    }

    /// Holds the deadline to this clock, in place of the machine's.
    pub(crate) fn timed_by(&mut self, clock: Arc<dyn Clock>) {
        self.clock = Some(clock);
    }

    /// Narrows the deadline: the earlier of the one already set and this one is kept.
    /// A deadline is never moved later.
    pub(crate) fn until(&mut self, deadline: Instant) {
        self.deadline = Some(match self.deadline {
            Some(set) => set.min(deadline),
            None => deadline,
        });
    }

    /// The run's cancellation signal. A backend is expected to stop early when it is
    /// set.
    pub fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }

    /// Whether the run was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// When the run must be over, if it has a limit. A backend is expected to give up
    /// by then, with the fault `timed_out`; nothing cuts off one that does not
    /// (`docs/carry-over.md`).
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// What every handle checks before it sends: the run is not cancelled, its
    /// deadline has not passed, and the backend declares each feature the request
    /// needs.
    pub(crate) fn before_sending<'a>(
        &self,
        declaration: &Declaration,
        needed: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), Fault> {
        if self.is_cancelled() {
            return Err(Fault::new(
                FaultKind::Cancelled,
                "the run was cancelled before the request was sent",
            ));
        }
        let now = || self.clock.as_ref().map_or_else(Instant::now, |c| c.now());
        if self.deadline.is_some_and(|deadline| now() >= deadline) {
            return Err(Fault::new(
                FaultKind::TimedOut,
                "the deadline of the run passed before the request was sent",
            ));
        }
        let model = declaration.model();
        for feature in needed {
            if !declaration.has(feature) {
                return Err(Fault::new(
                    FaultKind::Unsupported,
                    &format!(
                        "the model {} of the backend {} does not declare the feature {feature}",
                        model.model(),
                        model.backend()
                    ),
                ));
            }
        }
        Ok(())
    }
}

/// How many characters of a text a caller typed are shown.
const SHOWN: usize = 64;

/// A text that a caller typed, as an error shows it: escaped, its first 64 characters
/// at most, and its length when it was longer. A sentence and a `{:?}` print it alike.
pub(crate) struct Shown<'a>(pub &'a str);

impl std::fmt::Display for Shown<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.0.chars().count();
        let start: String = self.0.chars().take(SHOWN).collect();
        write!(f, "{start:?}")?;
        if count > SHOWN {
            write!(f, "... ({count} characters)")?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for Shown<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
