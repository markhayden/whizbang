//! A fault: the typed error of a capability (ADR 0006, "Faults").
//!
//! A fault ends up in a run record, so it is made to hold nothing private. It is a kind,
//! one sentence and, for a backend that said when to ask again, a wait. The sentence is
//! cleaned when the fault is made: every character that could spell a path, an address,
//! a header or markup is replaced. So what a fault says, and what `{:?}` prints of it,
//! holds no place on the machine whatever an adapter passed in. A secret an adapter
//! passed in by its value is not found here: redaction takes it out when a run is
//! written (ADR 0005).

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
    /// character becomes `?`; and it is cut at the limit.
    pub fn new(kind: FaultKind, text: &str) -> Fault {
        let sentence = clean(text, FAULT_LIMIT);
        Fault {
            kind,
            sentence: if sentence.is_empty() {
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
fn clean(text: &str, limit: usize) -> String {
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
