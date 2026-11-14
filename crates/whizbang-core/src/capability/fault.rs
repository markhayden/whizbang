//! A fault: the typed error of a capability (ADR 0006, "Faults").
//!
//! A fault ends up in a run record. It is a kind, one sentence and, for a backend that
//! said when to ask again, a wait. The sentence is cleaned when the fault is made. A
//! fault also says how far its request got, and what the answer said it cost when one
//! came (ADR 0010), which is what the ledger settles a failed request by. What `{:?}`
//! prints of a fault is the kind, that sentence, the wait, the stage and the charge.
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

use super::{Charge, Stage, Unfit};

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

/// The most bytes that are kept of a provider's words as they were given, until the
/// handle has looked for secrets in them ([`Raw`]).
pub(super) const RAW_LIMIT: usize = 1 << 20;

/// A provider's words as they were given, before anything was cleaned or cut: what a
/// handle looks for secrets in (ADR 0009, "A provider's words are redacted whole").
/// Words over [`RAW_LIMIT`] bytes are not kept: they cannot be searched whole, so
/// what is shown of them, once a handle has looked, is fixed words.
#[derive(Clone)]
pub(super) enum Raw {
    Whole(String),
    TooLong,
}

impl Raw {
    pub(super) fn of(text: &str) -> Raw {
        if text.len() > RAW_LIMIT {
            Raw::TooLong
        } else {
            Raw::Whole(text.to_owned())
        }
    }

    /// The words as they are shown once the secrets are out: redacted while whole,
    /// then cleaned and cut by `shape`. When what was shaped still holds something
    /// redaction finds, as cleaning can make it, or the words were too long to search,
    /// nothing of them is shown: `otherwise` is.
    pub(super) fn shown(
        &self,
        redact: &dyn Fn(&str) -> String,
        shape: impl Fn(&str) -> String,
        otherwise: &str,
    ) -> String {
        let Raw::Whole(text) = self else {
            return otherwise.to_owned();
        };
        let shaped = shape(&redact(text));
        if redact(&shaped) == shaped {
            shaped
        } else {
            otherwise.to_owned()
        }
    }
}

/// The typed error of a capability's request. Two faults are equal when their kind,
/// their sentence, their wait, their stage and their charge are; what `{:?}` prints is
/// those five.
#[derive(Clone)]
pub struct Fault {
    kind: FaultKind,
    sentence: String,
    retry_after_ms: Option<u64>,
    /// The text the fault was made of, until a handle has looked for secrets in it.
    raw: Option<Raw>,
    /// How far the request got, when whoever made the fault said.
    stage: Option<Stage>,
    /// What the answer said the request cost. Only an answered fault holds one.
    charge: Option<Charge>,
}

impl PartialEq for Fault {
    fn eq(&self, other: &Fault) -> bool {
        let all = |fault: &Fault| {
            (
                fault.kind,
                fault.retry_after_ms,
                fault.stage(),
                fault.charge,
            )
        };
        self.sentence == other.sentence && all(self) == all(other)
    }
}

impl Eq for Fault {}

impl std::fmt::Debug for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fault")
            .field("kind", &self.kind)
            .field("sentence", &self.sentence)
            .field("retry_after_ms", &self.retry_after_ms)
            .field("stage", &self.stage())
            .field("charge", &self.charge)
            .finish()
    }
}

/// A fault's sentence, from its text: trimmed, cleaned and cut.
fn sentence_of(text: &str) -> String {
    let sentence = clean(text.trim(), FAULT_LIMIT);
    if sentence.trim().is_empty() {
        "no reason was given".to_owned()
    } else {
        sentence
    }
}

impl Fault {
    /// A fault of this kind. The text is cleaned ([`FAULT_LIMIT`]): what is kept of it
    /// is ASCII letters, digits, spaces and `. , ; : ' ( ) % + - _`; every other
    /// character becomes `?`; and it is cut at the limit. White space at the ends of
    /// the text is dropped first, so a text of nothing, or of white space alone of any
    /// kind, becomes `no reason was given`.
    pub fn new(kind: FaultKind, text: &str) -> Fault {
        Fault {
            kind,
            sentence: sentence_of(text),
            retry_after_ms: None,
            raw: Some(Raw::of(text)),
            stage: None,
            charge: None,
        }
    }

    /// The same fault, of a request that did not leave whizbang: nothing was handed to
    /// anyone, so nothing was charged. An adapter says this only when no byte of the
    /// request went to anything outside the process.
    pub fn not_sent(mut self) -> Fault {
        (self.stage, self.charge) = (Some(Stage::NotSent), None);
        self
    }

    /// The same fault, of a request that was answered, by an answer that settles it:
    /// with what the answer said it cost, when it said. No charge means that the
    /// answer did not say, and the request then settles at its bound, never at
    /// nothing: an answer known to have cost nothing is given a charge of nothing.
    pub fn answered(mut self, charge: Option<Charge>) -> Fault {
        (self.stage, self.charge) = (Some(Stage::Answered), charge);
        self
    }

    /// The same fault, of a request that left, or may have, with no answer that
    /// settles it.
    pub fn unresolved(mut self) -> Fault {
        (self.stage, self.charge) = (Some(Stage::Unresolved), None);
        self
    }

    /// How far the request got: what whoever made the fault stated, and unresolved
    /// when nobody did, whatever the kind. No kind says how far a request got: a
    /// backend that returns `unsupported` was called, and may have found out what it
    /// does not support after the request left. That is the conservative reading: an
    /// unresolved request may have been charged, and is counted at its bound. A fake
    /// or an adapter that knows better must state the stage (ADR 0010).
    pub fn stage(&self) -> Stage {
        self.stage.unwrap_or(Stage::Unresolved)
    }

    /// What the answer said the request cost, for an answered fault whose answer said.
    /// None, for an answered fault, does not mean that the request cost nothing: the
    /// answer did not say, and the request settles at its bound. An answer known to
    /// have cost nothing says a charge of nothing.
    pub fn charge(&self) -> Option<Charge> {
        self.charge
    }

    /// The same fault as a run may be told of it and may store it: its sentence made
    /// again from the text it was given, with the secrets taken out while the text was
    /// whole, and only then cleaned and cut. A handle that knows its run's secrets does
    /// this to every fault before it hands it on. A second time changes nothing.
    pub(crate) fn redacted(mut self, redact: &dyn Fn(&str) -> String) -> Fault {
        if let Some(raw) = self.raw.take() {
            self.sentence = raw.shown(redact, sentence_of, "the reason is not kept");
        }
        self
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
