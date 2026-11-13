//! A fault: the typed error of a capability (ADR 0006, "Faults").
//!
//! A fault ends up in a run record. It is a kind, one sentence and, for a backend that
//! said when to ask again, a wait. The sentence is cleaned when the fault is made, and
//! what `{:?}` prints of a fault is the kind, that sentence and the wait.
//!
//! **What cleaning guarantees.** A sentence is at most [`FAULT_LIMIT`] characters, cut
//! between characters. It holds no `/` and no `\`; no `@`, `=` or `&`; no line break,
//! no double quote and no angle bracket. So it holds no path with its separators, no
//! address in its URL form, and no second line that could pass for another record or
//! header.
//!
//! **What cleaning does not find.** It replaces characters and knows nothing of what a
//! text means. A host name, an IP address and its port, a header's name and value on
//! one line, and a token made of letters, digits, `-`, `_` and `.` all go through
//! unchanged. Redaction (ADR 0005) takes a secret out when a run is written, and only
//! a secret the executor was told of: a token an adapter copied out of a provider's
//! answer is found by neither.
//!
//! **So the rule for an adapter** is the rest of the guarantee: it builds a fault's
//! sentence from its own words and from typed fields (a status, a count, a size), and
//! never copies a provider's body, a header or an address into it (ADR 0006,
//! "Faults").

use super::Unfit;

/// The most characters a fault's sentence holds.
pub const FAULT_LIMIT: usize = 240;

/// The marks a sentence keeps, beside ASCII letters, digits and spaces.
const KEPT_MARKS: &str = ".,;:'()%+-_";

/// What kind of failure a capability's request ended in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaultKind {
    /// The run was cancelled. Nothing more is sent.
    Cancelled,
    /// The request needs a feature the backend does not declare. It was not sent.
    Unsupported,
    /// The backend refused the credential. Never retried.
    Credential,
    /// The backend refused the request as it is: the same request would be refused
    /// again.
    Rejected,
    /// The backend could not answer now: it is overloaded, or it could not be reached.
    /// The one kind where the same request may be answered later.
    Unavailable,
    /// No answer came within the time allowed.
    TimedOut,
    /// An answer came and cannot be used: not the dimensions asked for, a value out of
    /// its range, nothing to continue from.
    Malformed,
    /// The request, or the answer, is over a bound of the backend or of whizbang.
    TooLarge,
}

impl FaultKind {
    /// Every kind, in the order ADR 0006 lists them.
    pub const ALL: [FaultKind; 8] = [
        FaultKind::Cancelled,
        FaultKind::Unsupported,
        FaultKind::Credential,
        FaultKind::Rejected,
        FaultKind::Unavailable,
        FaultKind::TimedOut,
        FaultKind::Malformed,
        FaultKind::TooLarge,
    ];

    /// The kind's name, as a record spells it.
    pub fn name(self) -> &'static str {
        match self {
            FaultKind::Cancelled => "cancelled",
            FaultKind::Unsupported => "unsupported",
            FaultKind::Credential => "credential",
            FaultKind::Rejected => "rejected",
            FaultKind::Unavailable => "unavailable",
            FaultKind::TimedOut => "timed_out",
            FaultKind::Malformed => "malformed",
            FaultKind::TooLarge => "too_large",
        }
    }
}

/// The typed error of a capability's request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    kind: FaultKind,
    sentence: String,
    retry_after_ms: Option<u64>,
}

impl Fault {
    /// A fault of this kind. The text is cleaned ([`FAULT_LIMIT`]): what is kept of it
    /// is ASCII letters, digits, spaces and `. , ; : ' ( ) % + - _`; every other
    /// character becomes `?`; and it is cut at the limit. White space at the ends of
    /// the text is dropped first, so a text of nothing, or of white space alone of any
    /// kind, becomes `no reason was given`.
    pub fn new(kind: FaultKind, text: &str) -> Fault {
        let sentence = clean(text.trim(), FAULT_LIMIT);
        Fault {
            kind,
            sentence: if sentence.trim().is_empty() {
                "no reason was given".to_owned()
            } else {
                sentence
            },
            retry_after_ms: None,
        }
    }

    /// The same fault, with how long the backend said to wait before asking again.
    pub fn retry_after_ms(mut self, wait: u64) -> Fault {
        self.retry_after_ms = Some(wait);
        self
    }

    pub fn kind(&self) -> FaultKind {
        self.kind
    }

    /// Why, in one cleaned sentence.
    pub fn sentence(&self) -> &str {
        &self.sentence
    }

    /// How long the backend said to wait, when it said.
    pub fn wait_ms(&self) -> Option<u64> {
        self.retry_after_ms
    }
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind.name(), self.sentence)
    }
}

impl std::error::Error for Fault {}

impl From<Unfit> for Fault {
    /// An answer that is not fit to hold is a malformed answer.
    fn from(unfit: Unfit) -> Fault {
        Fault::new(FaultKind::Malformed, &unfit.to_string())
    }
}

/// The first `limit` characters of a text, each kept when it is an ASCII letter, a
/// digit, a space or one of [`KEPT_MARKS`], and replaced by `?` otherwise.
pub(super) fn clean(text: &str, limit: usize) -> String {
    text.chars()
        .take(limit)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == ' ' || KEPT_MARKS.contains(c) {
                c
            } else {
                '?'
            }
        })
        .collect()
}
