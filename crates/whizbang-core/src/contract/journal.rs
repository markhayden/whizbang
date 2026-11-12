//! What a run keeps while it executes, to be stored when it ends: its journal of
//! progress events, and the diagnostics its action wrote.
//!
//! Squint kept a journal as a file it appended to and synced one event at a time
//! (`crates/squint-core/src/journal.rs`), and dropped a torn last line on reading. Here
//! the store is the one owner of everything durable, and it has one way in: a
//! publication. So a run's journal is collected while the run executes and published
//! whole, with the run's record, when the run ends. A journal in the store is never
//! torn, and reading one refuses any line that is not an event.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

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

/// One line of a journal: a progress event, with its place in the run and its time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalLine {
    /// Its place among the run's events, from 0.
    pub seq: u64,
    /// When it was reported, in milliseconds since the Unix epoch.
    pub at_ms: u64,
    /// What the action said, with the secrets taken out.
    pub message: String,
}

/// The longest name of a diagnostic, in characters.
pub const DIAGNOSTIC_NAME_LIMIT: usize = 64;

/// Whether this can be the name of a diagnostic: a lower-case letter or a digit, then
/// lower-case letters, digits, `_`, `-` or `.`; 64 characters at most. So it is one path
/// segment and one word on a command line, as a run's id is.
pub fn is_diagnostic_name(name: &str) -> bool {
    let plain = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    name.len() <= DIAGNOSTIC_NAME_LIMIT
        && name.chars().next().is_some_and(plain)
        && name
            .chars()
            .all(|c| plain(c) || c == '_' || c == '-' || c == '.')
}

/// The journal and the diagnostics of one run, in memory until the run ends.
pub(super) struct Kept {
    clock: Arc<dyn Clock>,
    secrets: Secrets,
    lines: Mutex<Vec<JournalLine>>,
    diagnostics: Mutex<BTreeMap<String, Vec<u8>>>,
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

    /// The journal as it is stored, and each diagnostic by its name, in order: what was
    /// kept until now. What is written after this is not part of the run.
    pub(super) fn take(&self) -> (Vec<u8>, BTreeMap<String, Vec<u8>>) {
        let lines = std::mem::take(&mut *self.lines.lock().unwrap_or_else(|e| e.into_inner()));
        let diagnostics =
            std::mem::take(&mut *self.diagnostics.lock().unwrap_or_else(|e| e.into_inner()));
        (journal_bytes(&lines), diagnostics)
    }
}

impl Events for Kept {
    fn progress(&self, message: &str) {
        let mut lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        let line = JournalLine {
            seq: lines.len() as u64,
            at_ms: self.clock.now_ms(),
            message: self.secrets.redact(message),
        };
        lines.push(line);
    }
}

impl Diagnostics for Kept {
    fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        if !is_diagnostic_name(name) {
            let shown: String = name.chars().take(DIAGNOSTIC_NAME_LIMIT).collect();
            return Err(format!(
                "{shown:?} is no name for a diagnostic: a name is a lower-case letter or a \
                 digit, then lower-case letters, digits, _, - or ., {DIAGNOSTIC_NAME_LIMIT} \
                 characters at most"
            ));
        }
        let mut diagnostics = self.diagnostics.lock().unwrap_or_else(|e| e.into_inner());
        if diagnostics.contains_key(name) {
            return Err(format!(
                "the run already wrote a diagnostic named {name}: a name is written once"
            ));
        }
        diagnostics.insert(name.to_owned(), self.secrets.redact_bytes(bytes));
        Ok(())
    }
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
        let line: JournalLine = serde_json::from_str(raw)
            .map_err(|error| format!("line {} is not an event: {error}", index + 1))?;
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
