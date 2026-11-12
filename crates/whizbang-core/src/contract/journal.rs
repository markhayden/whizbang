//! A run's journal: its progress events, as they are stored (ADR 0005).
//!
//! Squint kept a journal as a file it appended to and synced one event at a time
//! (`crates/squint-core/src/journal.rs`), and dropped a torn last line on reading. Here
//! the store is the one owner of everything durable, and it has one way in: a
//! publication. So a journal is stored whole, with its run's record; one in the store is
//! never torn, and reading one refuses any line that is not an event.

use serde::{Deserialize, Serialize};

use super::Secrets;

/// One line of a journal: a progress event, with its place in the run and its time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalLine {
    seq: u64,
    at_ms: u64,
    message: String,
}

impl JournalLine {
    /// The event at this place and time, saying this, with the secrets taken out. It is
    /// the one way to make a line, beside reading a stored journal: a line in a journal
    /// was redacted when it was made.
    pub fn new(seq: u64, at_ms: u64, message: &str, secrets: &Secrets) -> JournalLine {
        JournalLine {
            seq,
            at_ms,
            message: secrets.redact(message),
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
        let line: JournalLine = serde_json::from_str(raw).map_err(|error| {
            // The reason can quote the line: it is shown escaped.
            let why = error.to_string();
            format!("line {} is not an event: {}", index + 1, why.escape_debug())
        })?;
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
