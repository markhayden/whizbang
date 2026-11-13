//! A run's journal: its progress events, as they are stored, and what a run keeps
//! while it executes, to be stored when it ends (ADR 0005).
//!
//! Squint kept a journal as a file it appended to and synced one event at a time
//! (`crates/squint-core/src/journal.rs`), and dropped a torn last line on reading. Here
//! the store is the one owner of everything durable, and it has one way in: a
//! publication. So a journal is stored whole, with its run's record; one in the store is
//! never torn, and reading one refuses any line that is not an event.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::record::{Diagnostic, quoted};
use super::{Diagnostics, Events, Secrets};

/// What tells a run the time: milliseconds since the Unix epoch.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
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
}

/// The journal and the diagnostics of one run, in memory until the run ends. Each line
/// and each diagnostic is made through its constructor, with the secrets taken out.
pub(super) struct Kept {
    clock: Arc<dyn Clock>,
    secrets: Secrets,
    lines: Mutex<Vec<JournalLine>>,
    /// By the name the action gave: the diagnostic and the bytes to store under it.
    diagnostics: Mutex<BTreeMap<String, (Diagnostic, Vec<u8>)>>,
}

impl Kept {
    pub(super) fn new(clock: Arc<dyn Clock>, secrets: Secrets) -> Kept {
        Kept {
            clock,
            secrets,
            lines: Mutex::new(Vec::new()),
            diagnostics: Mutex::new(BTreeMap::new()),
        }
    }

    /// The journal's lines, and each diagnostic with the bytes to store under it, in the
    /// order of their names: what was kept until now. What is written after this is not
    /// part of the run.
    pub(super) fn take(&self) -> (Vec<JournalLine>, Vec<(Diagnostic, Vec<u8>)>) {
        // The lines stay: a run whose end the store then refuses has them nowhere
        // else, and whoever asks after it is still shown them.
        let lines = self.lines();
        let diagnostics =
            std::mem::take(&mut *self.diagnostics.lock().unwrap_or_else(|e| e.into_inner()));
        (lines, diagnostics.into_values().collect())
    }
}

impl Kept {
    /// The journal's lines so far: the run goes on.
    pub(super) fn lines(&self) -> Vec<JournalLine> {
        self.lines.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Events for Kept {
    fn progress(&self, message: &str) {
        let mut lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        let line = JournalLine::new(
            lines.len() as u64,
            self.clock.now_ms(),
            message,
            &self.secrets,
        );
        lines.push(line);
    }
}

impl Diagnostics for Kept {
    fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let made = Diagnostic::written(name, bytes, &self.secrets)?;
        let mut diagnostics = self.diagnostics.lock().unwrap_or_else(|e| e.into_inner());
        if diagnostics.contains_key(name) {
            return Err(format!(
                "the run already wrote a diagnostic named {name}: a name is written once"
            ));
        }
        diagnostics.insert(name.to_owned(), made);
        Ok(())
    }
}

/// One line of a journal: a progress event, with its place in the run and its time.
///
/// It is written as JSON and cannot be read from JSON by anything but [`read_journal`]:
/// a line comes from [`JournalLine::new`], which takes the secrets out, or from a stored
/// journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JournalLine {
    seq: u64,
    at_ms: u64,
    message: String,
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

    /// What the action said, with the secrets taken out.
    pub fn message(&self) -> &str {
        &self.message
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
}

/// A journal as it is stored: each event as JSON on a line of its own.
pub fn journal_bytes(lines: &[JournalLine]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for line in lines {
        // A line holds two whole numbers and a string: writing it cannot fail.
        bytes.extend(serde_json::to_vec(line).unwrap_or_default());
        bytes.push(b'\n');
    }
    bytes
}

/// Reads a stored journal. Every line is an event, in order from 0, and the last ends
/// with a newline: a journal is published whole, so there is no torn line to forgive.
pub fn read_journal(bytes: &[u8]) -> Result<Vec<JournalLine>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "a journal is UTF-8".to_owned())?;
    if !text.is_empty() && !text.ends_with('\n') {
        return Err("a journal's last line ends with a newline".to_owned());
    }
    let mut lines = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        // The reason can quote the line: it is shown escaped and cut.
        let refused = |error: serde_json::Error| {
            let why = quoted(&error.to_string());
            format!("line {} is not an event: {why}", index + 1)
        };
        let mut reader = serde_json::Deserializer::from_str(raw);
        let line = JournalLineWire::deserialize(&mut reader).map_err(refused)?;
        reader.end().map_err(refused)?;
        if line.seq != index as u64 {
            return Err(format!(
                "line {} is event {}, out of its place",
                index + 1,
                line.seq
            ));
        }
        lines.push(line);
    }
    Ok(lines)
}
