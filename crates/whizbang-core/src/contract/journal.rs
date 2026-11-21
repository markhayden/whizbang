//! A run's journal: its progress events, as they are stored, and what a run keeps
//! while it executes, to be stored when it ends (ADR 0005).
//!
//! Squint kept a journal as a file it appended to and synced one event at a time
//! (`crates/squint-core/src/journal.rs`), and dropped a torn last line on reading. Here
//! the store is the one owner of everything durable, and it has one way in: a
//! publication. So a journal is stored whole, with its run's record; one in the store is
//! never torn, and reading one refuses any line that is not an event.
//!
//! A journal is held to a bound (plan m5, task K1, item 4): a run keeps its first
//! [`MESSAGES_KEPT_LIMIT`] progress messages and their first [`MESSAGES_KEPT_MAX_BYTES`]
//! of text, and its first [`EVENTS_KEPT_LIMIT`] typed events, each at most
//! [`EVENT_MAX_BYTES`]; the rest are counted and not kept. Every event is given its
//! place, kept or not, and nothing kept is dropped later, so what a run kept is the
//! same in memory, in the journal and after a restart.
//!
//! Two formats are read. The first, `journal/1`, is a line of JSON for each event, in
//! order from 0, and nothing else. The second, `journal/2`, begins with the line
//! `{"journal":2}`, holds the events kept in the order of their places, which may
//! skip what was not kept, and ends with the line that counts what was not kept. A
//! journal is written in the first format when it holds no typed event and nothing
//! was left out, which is every journal of a run that reported progress messages
//! alone, and in the second otherwise.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::capability::Recorder;

use super::record::{DIAGNOSTICS_LIMIT, DIAGNOSTICS_PREFIX, Diagnostic, quoted};
use super::{Diagnostics, Events, Secrets};

/// What tells a run the time: milliseconds since the Unix epoch.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;

    /// The instant it is now, for what is held to a deadline: the machine's, unless a
    /// clock says otherwise. A test moves it.
    fn now(&self) -> Instant {
        Instant::now()
    }

    /// Lets this much time pass, for one who tries a thing again and again until a
    /// time of this clock and must not spin meanwhile. The machine's clock sleeps
    /// ([`SystemClock`]). A clock that does not say otherwise gives way to other
    /// threads and returns at once, so a test's clock never sleeps unless it was
    /// written to: it counts the call, or moves itself by `span`.
    fn pause(&self, span: Duration) {
        let _ = span;
        std::thread::yield_now();
    }
}

/// The machine's clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }

    fn pause(&self, span: Duration) {
        std::thread::sleep(span);
    }
}

/// The journal and the diagnostics of one run, in memory until the run ends. Each line
/// and each diagnostic is made through its constructor, with the secrets taken out.
pub(super) struct Kept {
    clock: Arc<dyn Clock>,
    secrets: Secrets,
    bounds: JournalBounds,
    lines: Mutex<Lines>,
    /// By the name the action gave: the diagnostic and the bytes to store under it.
    diagnostics: Mutex<BTreeMap<String, (Diagnostic, Vec<u8>)>>,
}

impl Kept {
    pub(super) fn new(clock: Arc<dyn Clock>, secrets: Secrets) -> Kept {
        Kept {
            clock,
            secrets,
            bounds: JournalBounds::default(),
            lines: Mutex::new(Lines::default()),
            diagnostics: Mutex::new(BTreeMap::new()),
        }
    }

    /// The journal, and each diagnostic with the bytes to store under it, in the order
    /// of their names: what was kept until now. What is written after this is not part
    /// of the run.
    pub(super) fn take(&self) -> (Journal, Vec<(Diagnostic, Vec<u8>)>) {
        // The lines stay: a run whose end the store then refuses has them nowhere
        // else, and whoever asks after it is still shown them.
        let journal = {
            let lines = self.held();
            Journal {
                lines: lines.kept.clone(),
                not_kept: lines.not_kept,
            }
        };
        let diagnostics =
            std::mem::take(&mut *self.diagnostics.lock().unwrap_or_else(|e| e.into_inner()));
        (journal, diagnostics.into_values().collect())
    }

    /// The same, keeping no more than these bounds.
    pub(super) fn bounded(mut self, bounds: JournalBounds) -> Kept {
        self.bounds = bounds;
        self
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Lines> {
        self.lines.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The events kept so far from the place `seen` on, the run going on: what a
    /// caller that has seen the first `seen` places has not. With them, how many of
    /// those places were not kept, and the place the next event will have. Only what
    /// is new is copied.
    pub(super) fn heard(&self, seen: u64) -> Heard {
        let lines = self.held();
        let from = lines.kept.partition_point(|line| line.seq < seen);
        let events = lines.kept[from..].to_vec();
        let asked = lines.next.saturating_sub(seen);
        Heard {
            skipped: asked - events.len() as u64,
            next: lines.next,
            from: seen,
            events,
        }
    }
}

/// What a run's journal keeps: the real bounds, or smaller ones a test sets
/// ([`super::Engine::with_journal_bounds`]).
#[derive(Debug, Clone, Copy)]
pub(super) struct JournalBounds {
    messages: usize,
    message_bytes: usize,
    events: usize,
}

impl Default for JournalBounds {
    fn default() -> JournalBounds {
        JournalBounds {
            messages: MESSAGES_KEPT_LIMIT,
            message_bytes: MESSAGES_KEPT_MAX_BYTES,
            events: EVENTS_KEPT_LIMIT,
        }
    }
}

impl JournalBounds {
    /// These bounds, each never past the real one.
    pub(super) fn at_most(messages: usize, message_bytes: usize, events: usize) -> JournalBounds {
        JournalBounds {
            messages: messages.min(MESSAGES_KEPT_LIMIT),
            message_bytes: message_bytes.min(MESSAGES_KEPT_MAX_BYTES),
            events: events.min(EVENTS_KEPT_LIMIT),
        }
    }
}

/// What a run kept of its events while it goes on, and what it counted and did not
/// keep.
#[derive(Default)]
struct Lines {
    kept: Vec<JournalLine>,
    /// The place the next event is given.
    next: u64,
    /// How many progress messages are kept, and the bytes of their text.
    messages: usize,
    message_bytes: usize,
    /// Whether a progress message was left out: every one after it is too.
    messages_full: bool,
    /// How many typed events are kept.
    events: usize,
    not_kept: NotKept,
}

/// The events of a run from some place on, as it goes on: those kept, how many of
/// the places asked about were not kept, and the place the next event will have.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Heard {
    pub(super) events: Vec<JournalLine>,
    pub(super) skipped: u64,
    pub(super) next: u64,
    /// The place asked from: no event before it is here.
    pub(super) from: u64,
}

impl Events for Kept {
    fn progress(&self, message: &str) {
        let at_ms = self.clock.now_ms();
        let mut lines = self.held();
        let seq = lines.next;
        lines.next += 1;
        // Once a message was left out, or as many as are kept are, nothing more is
        // made of one: it is counted.
        if lines.messages_full || lines.messages >= self.bounds.messages {
            lines.messages_full = true;
            lines.not_kept.messages += 1;
            return;
        }
        let line = JournalLine::new(seq, at_ms, message, &self.secrets);
        let bytes = line.message.len();
        if lines.message_bytes + bytes > self.bounds.message_bytes {
            lines.messages_full = true;
            lines.not_kept.messages += 1;
            return;
        }
        lines.messages += 1;
        lines.message_bytes += bytes;
        lines.kept.push(line);
    }

    fn event(&self, event: &Event) {
        let at_ms = self.clock.now_ms();
        let mut lines = self.held();
        let seq = lines.next;
        lines.next += 1;
        // Past the bound nothing is made of the event: it is counted.
        if lines.events >= self.bounds.events {
            lines.not_kept.events += 1;
            return;
        }
        lines.events += 1;
        let line = JournalLine::typed(seq, at_ms, event, &self.secrets);
        lines.kept.push(line);
    }
}

/// How many progress messages a run keeps: those after are counted, not kept.
pub const MESSAGES_KEPT_LIMIT: usize = 4096;

/// How many bytes of text the progress messages a run keeps hold together: the message
/// that would pass it, and every one after it, is counted, not kept.
pub const MESSAGES_KEPT_MAX_BYTES: usize = 1 << 20;

/// How many typed events a run keeps: those after are counted, not kept.
pub const EVENTS_KEPT_LIMIT: usize = 16_384;

/// The most bytes a typed event takes, its message and what it says written as JSON.
pub const EVENT_MAX_BYTES: usize = 512;

/// What the name of every exchange with a judge starts with, among the diagnostics of
/// a run: `exchange-`, then the exchange's number among the run's requests, from 1, in
/// four digits. The executor's handles write these, and an action cannot (ADR 0009).
pub const EXCHANGE_PREFIX: &str = "exchange-";

impl Recorder for Kept {
    fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }

    fn redact(&self, text: &str) -> String {
        self.secrets.redact(text)
    }

    fn answered(&self, event: &Event) {
        Events::event(self, event);
    }

    fn exchange(&self, place: usize, written: &[u8], room: &dyn Fn(u64) -> bool) -> Option<String> {
        // A name is refused only when it holds a secret: the exchange is then not
        // kept, and the record says so. Nor is one kept that would take, with the
        // secrets taken out, more than there is room for: redaction can lengthen it.
        // An exchange is written as JSON by the handle, and is redacted as JSON, so
        // that what is stored can be read back as one (ADR 0014, "A JSON body").
        let name = format!("{EXCHANGE_PREFIX}{:04}", place + 1);
        let made = Diagnostic::written_json(&name, written, &self.secrets).ok()?;
        if !room(made.1.len() as u64) {
            return None;
        }
        let mut diagnostics = self.diagnostics.lock().unwrap_or_else(|e| e.into_inner());
        diagnostics.insert(name.clone(), made);
        Some(format!("{DIAGNOSTICS_PREFIX}{name}"))
    }
}

impl Kept {
    /// Keeps what an action wrote under a name of its own: `exact` when the bytes are
    /// kept only if taking the secrets out leaves them as they were given.
    fn keep(&self, name: &str, bytes: &[u8], exact: bool) -> Result<(), String> {
        if name.starts_with(EXCHANGE_PREFIX) {
            return Err(format!(
                "a diagnostic whose name starts with {EXCHANGE_PREFIX} is an exchange with a \
                 judge, which the executor writes"
            ));
        }
        let made = Diagnostic::written(name, bytes, &self.secrets)?;
        if exact && made.1 != bytes {
            return Err(
                "the bytes hold something that must not be written, and they are a file \
                 that cannot be changed: nothing was kept"
                    .to_owned(),
            );
        }
        let mut diagnostics = self.diagnostics.lock().unwrap_or_else(|e| e.into_inner());
        if diagnostics.contains_key(name) {
            return Err(format!(
                "the run already wrote a diagnostic named {name}: a name is written once"
            ));
        }
        // The exchanges are the executor's, and are bounded with the run's requests.
        let own = |name: &&String| !name.starts_with(EXCHANGE_PREFIX);
        if diagnostics.keys().filter(own).count() >= DIAGNOSTICS_LIMIT {
            return Err(format!(
                "the run has written {DIAGNOSTICS_LIMIT} diagnostics, which is as many as \
                 one run may"
            ));
        }
        diagnostics.insert(name.to_owned(), made);
        Ok(())
    }
}

impl Diagnostics for Kept {
    fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.keep(name, bytes, false)
    }

    fn write_exact(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.keep(name, bytes, true)
    }
}

/// What a typed line of a journal says beside its message (plan m5, task K1, item 3):
/// what a recipe, a loop and the executor say as a run goes, so that whoever shows a
/// run as it goes reads where it is without reading a sentence. Each text in one is
/// taken out of secrets and cut when its line is made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    /// A step began: its name and the action it calls; and the attempt it is of, for
    /// a loop's step.
    StepStarted {
        step: String,
        action: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attempt: Option<u32>,
    },
    /// A step ended: its name, how its run ended (the outcome's name, as a record
    /// spells it), and its run; and the attempt it is of, for a loop's step.
    StepEnded {
        step: String,
        outcome: String,
        run: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attempt: Option<u32>,
    },
    /// An attempt of a loop began: its number, from 1, and what its retry changed,
    /// when it is a retry.
    AttemptStarted {
        number: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        changed: Option<String>,
    },
    /// A request was sent to a capability and is over: the backend and the model that
    /// were asked, how long it took in milliseconds, and how it ended (`answered`, or
    /// the kind of its fault).
    Answered {
        capability: String,
        backend: String,
        model: String,
        ms: u64,
        ended: String,
    },
}

impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let of = |attempt: &Option<u32>| match attempt {
            Some(number) => format!(" of attempt {number}"),
            None => String::new(),
        };
        match self {
            Event::StepStarted {
                step,
                action,
                attempt,
            } => write!(f, "step {step} ({action}){} started", of(attempt)),
            Event::StepEnded {
                step,
                outcome,
                run,
                attempt,
            } => write!(f, "step {step}{} ended {outcome}, run {run}", of(attempt)),
            Event::AttemptStarted {
                number,
                changed: Some(changed),
            } => write!(f, "attempt {number} started: {changed}"),
            Event::AttemptStarted { number, .. } => write!(f, "attempt {number} started"),
            Event::Answered {
                capability,
                backend,
                model,
                ms,
                ended,
            } => write!(
                f,
                "{capability} request to {backend} {model}: {ended} in {ms} ms"
            ),
        }
    }
}

impl Event {
    /// The same event, each text in it taken out of secrets, and each that may be
    /// long cut at `limit` characters: an action's and a backend's name, a model's,
    /// what a retry changed and how a request ended. The name of a step and a run's
    /// id are never cut: they are what a caller finds the step and the run by, and
    /// each is bounded where it is made (a recipe's step id, [`super::RUN_ID_LIMIT`]).
    fn cut(&self, secrets: &Secrets, limit: usize) -> Event {
        let cut = |text: &String| super::engine::cut_to(secrets, text, limit, "...");
        let whole = |text: &String| secrets.redact(text);
        match self {
            Event::StepStarted {
                step,
                action,
                attempt,
            } => Event::StepStarted {
                step: whole(step),
                action: cut(action),
                attempt: *attempt,
            },
            Event::StepEnded {
                step,
                outcome,
                run,
                attempt,
            } => Event::StepEnded {
                step: whole(step),
                outcome: cut(outcome),
                run: whole(run),
                attempt: *attempt,
            },
            Event::AttemptStarted { number, changed } => Event::AttemptStarted {
                number: *number,
                changed: changed.as_ref().map(cut),
            },
            Event::Answered {
                capability,
                backend,
                model,
                ms,
                ended,
            } => Event::Answered {
                capability: whole(capability),
                backend: cut(backend),
                model: cut(model),
                ms: *ms,
                ended: cut(ended),
            },
        }
    }
}

/// One line of a journal: a progress event, with its place in the run and its time;
/// and, for a typed line, what it says beside its message ([`Event`]).
///
/// It is written as JSON and cannot be read from JSON by anything but [`read_journal`]:
/// a line comes from [`JournalLine::new`], which takes the secrets out, or from a stored
/// journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JournalLine {
    seq: u64,
    at_ms: u64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    event: Option<Event>,
}

impl JournalLine {
    /// The event at this place and time, saying this, with the secrets taken out and
    /// then cut at [`super::MESSAGE_LIMIT`] characters, as every text a run keeps is
    /// (ADR 0008, "What an action or a panic says is cut"). It is the one way to make a
    /// line, beside reading a stored journal: a line in a journal was redacted and cut
    /// when it was made.
    pub fn new(seq: u64, at_ms: u64, message: &str, secrets: &Secrets) -> JournalLine {
        JournalLine {
            seq,
            at_ms,
            message: super::engine::said(secrets, message),
            event: None,
        }
    }

    /// A typed line at this place and time: the event, with the secrets taken out of
    /// each text in it, and its message, the event in words; the two cut until,
    /// written as JSON together, they take at most [`EVENT_MAX_BYTES`]. What is cut
    /// is the message and the texts that may be long, never a step's name or a run's
    /// id ([`Event`]); those are bounded where they are made, so the line is held to
    /// its bound by what is cut.
    pub fn typed(seq: u64, at_ms: u64, event: &Event, secrets: &Secrets) -> JournalLine {
        let mut limit = 96;
        loop {
            let event = event.cut(secrets, limit);
            let message = super::engine::cut_to(secrets, &event.to_string(), 2 * limit, "...");
            let bytes = serde_json::to_vec(&(&message, &event)).map_or(usize::MAX, |b| b.len());
            if bytes <= EVENT_MAX_BYTES || limit <= 4 {
                return JournalLine {
                    seq,
                    at_ms,
                    message,
                    event: Some(event),
                };
            }
            limit /= 2;
        }
    }

    /// Its place among the run's events, from 0.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// When it was reported, in milliseconds since the Unix epoch.
    pub fn at_ms(&self) -> u64 {
        self.at_ms
    }

    /// What the action said, with the secrets taken out; of a typed line, its event
    /// in words.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// What a typed line says beside its message; none for a progress message.
    pub fn event(&self) -> Option<&Event> {
        self.event.as_ref()
    }
}

/// A line as a stored journal holds it. Private: only [`read_journal`] makes a line from
/// JSON.
#[derive(Deserialize)]
#[serde(remote = "JournalLine", deny_unknown_fields)]
struct JournalLineWire {
    seq: u64,
    at_ms: u64,
    message: String,
    #[serde(default)]
    event: Option<Event>,
}

/// How many events of a run were counted and not kept: progress messages, and typed
/// events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotKept {
    messages: u64,
    events: u64,
}

impl NotKept {
    /// How many progress messages were not kept.
    pub fn messages(&self) -> u64 {
        self.messages
    }

    /// How many typed events were not kept.
    pub fn events(&self) -> u64 {
        self.events
    }

    fn is_none(&self) -> bool {
        self.messages == 0 && self.events == 0
    }
}

/// A run's journal: the events it kept, and how many it counted and did not keep.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Journal {
    lines: Vec<JournalLine>,
    not_kept: NotKept,
}

/// The first line of a journal of the second format.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    journal: u32,
}

/// The last line of a journal of the second format.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Trailer {
    not_kept: NotKept,
}

impl Journal {
    /// The events kept, in the order of their places.
    pub fn lines(&self) -> &[JournalLine] {
        &self.lines
    }

    /// How many were counted and not kept.
    pub fn not_kept(&self) -> NotKept {
        self.not_kept
    }

    /// The journal as it is stored: in the first format when it holds no typed line
    /// and nothing was left out, and in the second otherwise.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Each line holds whole numbers and strings: writing one cannot fail.
        let mut put = |json: Vec<u8>| {
            bytes.extend(json);
            bytes.push(b'\n');
        };
        let second = !self.not_kept.is_none() || self.lines.iter().any(|l| l.event.is_some());
        if second {
            put(serde_json::to_vec(&Header { journal: 2 }).unwrap_or_default());
        }
        for line in &self.lines {
            put(serde_json::to_vec(line).unwrap_or_default());
        }
        if second {
            let trailer = Trailer {
                not_kept: self.not_kept,
            };
            put(serde_json::to_vec(&trailer).unwrap_or_default());
        }
        bytes
    }

    /// Reads a stored journal, of either format. Every line is what its place in the
    /// format says, and the last ends with a newline: a journal is published whole, so
    /// there is no torn line to forgive.
    pub fn read(bytes: &[u8]) -> Result<Journal, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "a journal is UTF-8".to_owned())?;
        if !text.is_empty() && !text.ends_with('\n') {
            return Err("a journal's last line ends with a newline".to_owned());
        }
        let raw: Vec<&str> = text.lines().collect();
        let second = raw.first().is_some_and(|first| {
            serde_json::from_str::<Header>(first).is_ok_and(|header| header.journal == 2)
        });
        if !second {
            let mut lines = Vec::new();
            for (index, raw) in raw.iter().enumerate() {
                let line = event_line(raw, index)?;
                if line.seq != index as u64 {
                    return Err(format!(
                        "line {} is event {}, out of its place",
                        index + 1,
                        line.seq
                    ));
                }
                if line.event.is_some() {
                    return Err(format!(
                        "line {} is a typed event, which a journal of the first format \
                         does not hold",
                        index + 1
                    ));
                }
                lines.push(line);
            }
            return Ok(Journal {
                lines,
                not_kept: NotKept::default(),
            });
        }
        let Some((last, middle)) = raw[1..].split_last() else {
            return Err("a journal of the second format ends with what it did not keep".into());
        };
        let not_kept = serde_json::from_str::<Trailer>(last)
            .map_err(|_| {
                format!(
                    "line {} is not what the journal did not keep, which ends it",
                    raw.len()
                )
            })?
            .not_kept;
        let mut lines: Vec<JournalLine> = Vec::new();
        for (index, raw) in middle.iter().enumerate() {
            let line = event_line(raw, index + 1)?;
            if lines.last().is_some_and(|before| before.seq >= line.seq) {
                return Err(format!(
                    "line {} is event {}, out of its place",
                    index + 2,
                    line.seq
                ));
            }
            lines.push(line);
        }
        // Every event was given its place, kept or not.
        let places = (lines.len() as u64)
            .saturating_add(not_kept.messages)
            .saturating_add(not_kept.events);
        if lines.last().is_some_and(|line| line.seq >= places) {
            return Err(format!(
                "the journal keeps an event at a place past the {places} it counts"
            ));
        }
        Ok(Journal { lines, not_kept })
    }
}

/// One event's line, the `index`th of its journal from 0.
fn event_line(raw: &str, index: usize) -> Result<JournalLine, String> {
    // The reason can quote the line: it is shown escaped and cut.
    let refused = |error: serde_json::Error| {
        let why = quoted(&error.to_string());
        format!("line {} is not an event: {why}", index + 1)
    };
    let mut reader = serde_json::Deserializer::from_str(raw);
    let line = JournalLineWire::deserialize(&mut reader).map_err(refused)?;
    reader.end().map_err(refused)?;
    Ok(line)
}

/// A journal as it is stored, of these lines with nothing left out
/// ([`Journal::to_bytes`]).
pub fn journal_bytes(lines: &[JournalLine]) -> Vec<u8> {
    Journal {
        lines: lines.to_vec(),
        not_kept: NotKept::default(),
    }
    .to_bytes()
}

/// Reads a stored journal's lines, of either format ([`Journal::read`]).
pub fn read_journal(bytes: &[u8]) -> Result<Vec<JournalLine>, String> {
    Journal::read(bytes).map(|journal| journal.lines)
}
