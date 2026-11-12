//! The comparison of two transcripts (docs/baseline/transcript.md).
//!
//! The baseline gate of milestone 2 replays each recorded decision and compares the transcript
//! whizbang produces with the recorded one. The comparison is exact: everything in a transcript
//! is compared except the places the format declares volatile. A field nobody has heard of is
//! compared like any other, and a recorded transcript cannot widen what is let through.

use std::fmt;

use serde_json::Value;

/// The version of this comparison, written into the gate receipt (docs/baseline/README.md).
/// It changes whenever what counts as a difference changes.
pub const COMPARISON_VERSION: &str = "3";

/// The transcript format this comparison reads.
pub const TRANSCRIPT_VERSION: u64 = 1;

/// The only places that are not compared: what version 1 of the format declares volatile
/// (docs/baseline/transcript.md). `*` stands for any one position in a list. A recorded
/// transcript must declare exactly this list. The converter writes it;
/// `scripts/tests/test-baseline.sh` holds the two copies together.
pub const VOLATILE: &[&str] = &[
    "/origin",
    "/meta",
    "/files",
    "/turns/*/meta",
    "/turns/*/calls/*/meta",
];

/// How two transcripts differ at one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Both hold a value there and the values differ.
    Changed,
    /// The recorded transcript holds something there and the other does not.
    Missing,
    /// The other transcript holds something there and the recorded one does not.
    Extra,
}

/// One difference. It names the place and never the values, so it can be printed anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub path: String,
    pub kind: Kind,
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            Kind::Changed => "changed",
            Kind::Missing => "missing",
            Kind::Extra => "extra",
        };
        write!(f, "{kind} at {}", self.path)
    }
}

/// Why a comparison could not be made at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// A transcript is not of the version this comparison reads.
    Version { which: Side },
    /// A transcript is not an object with at least one turn and a terminal outcome of a
    /// stated kind.
    Shape { which: Side },
    /// The recorded transcript is not marked complete, or lists something missing, so it is
    /// not a baseline.
    RecordedIncomplete,
    /// The recorded transcript's volatile list is not exactly the format's.
    Volatile,
}

/// Which of the two transcripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Recorded,
    Replayed,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Side::Recorded => "recorded",
            Side::Replayed => "replayed",
        })
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Version { which } => {
                write!(
                    f,
                    "the {which} transcript is not version {TRANSCRIPT_VERSION}"
                )
            }
            Refusal::Shape { which } => {
                write!(
                    f,
                    "the {which} transcript has no turns or no terminal outcome"
                )
            }
            Refusal::RecordedIncomplete => {
                write!(
                    f,
                    "the recorded transcript is not complete, so nothing is compared with it"
                )
            }
            Refusal::Volatile => {
                write!(
                    f,
                    "the recorded transcript's volatile list is not the format's"
                )
            }
        }
    }
}

impl std::error::Error for Refusal {}

/// Compares what whizbang replayed with what was recorded. `Ok` holds every difference, in the
/// order of the recorded transcript; an empty list is a match. Numbers are compared by the
/// text they were written with, so `1` and `1.0` differ and no two large numbers are confused.
/// One thing about that text is lost before the comparison sees it: the JSON reader writes
/// an exponent's `E` as `e`, and a `+` where no sign was written, so `1E2`, `1e2` and `1e+2`
/// are equal.
pub fn compare(recorded: &Value, replayed: &Value) -> Result<Vec<Difference>, Refusal> {
    for (which, t) in [(Side::Recorded, recorded), (Side::Replayed, replayed)] {
        if t.get("transcript_version").and_then(Value::as_u64) != Some(TRANSCRIPT_VERSION) {
            return Err(Refusal::Version { which });
        }
        // At least one turn, and an outcome that says what kind it is: an empty list or an
        // empty outcome on both sides would otherwise compare as equal.
        let turns = t
            .get("turns")
            .and_then(Value::as_array)
            .is_some_and(|turns| !turns.is_empty());
        let terminal = t
            .get("terminal")
            .and_then(|terminal| terminal.get("kind"))
            .is_some_and(Value::is_string);
        if !turns || !terminal {
            return Err(Refusal::Shape { which });
        }
    }
    let nothing_missing = recorded
        .get("incomplete")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty);
    if recorded.get("complete") != Some(&Value::Bool(true)) || !nothing_missing {
        return Err(Refusal::RecordedIncomplete);
    }
    let declared: Option<Vec<&str>> = recorded
        .get("volatile")
        .and_then(Value::as_array)
        .and_then(|list| list.iter().map(Value::as_str).collect());
    if declared.as_deref() != Some(VOLATILE) {
        return Err(Refusal::Volatile);
    }
    let volatile: Vec<Vec<&str>> = VOLATILE
        .iter()
        .map(|path| path.trim_start_matches('/').split('/').collect())
        .collect();
    let mut differences = Vec::new();
    let mut path = Vec::new();
    walk(
        Some(recorded),
        Some(replayed),
        &mut path,
        &volatile,
        &mut differences,
    );
    Ok(differences)
}

fn is_volatile(path: &[String], volatile: &[Vec<&str>]) -> bool {
    volatile.iter().any(|v| {
        v.len() == path.len()
            && v.iter()
                .zip(path)
                .all(|(want, have)| *want == "*" || want == have)
    })
}

fn render(path: &[String]) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    path.iter().map(|s| format!("/{s}")).collect()
}

fn walk(
    recorded: Option<&Value>,
    replayed: Option<&Value>,
    path: &mut Vec<String>,
    volatile: &[Vec<&str>],
    out: &mut Vec<Difference>,
) {
    if is_volatile(path, volatile) {
        return;
    }
    let mut note = |kind| {
        out.push(Difference {
            path: render(path),
            kind,
        });
    };
    match (recorded, replayed) {
        (None, None) => {}
        (Some(_), None) => note(Kind::Missing),
        (None, Some(_)) => note(Kind::Extra),
        (Some(Value::Object(a)), Some(Value::Object(b))) => {
            // serde_json keeps keys sorted, so the order is the same on every run.
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                path.push(key.clone());
                walk(a.get(key), b.get(key), path, volatile, out);
                path.pop();
            }
        }
        (Some(Value::Array(a)), Some(Value::Array(b))) => {
            for index in 0..a.len().max(b.len()) {
                path.push(index.to_string());
                walk(a.get(index), b.get(index), path, volatile, out);
                path.pop();
            }
        }
        (Some(a), Some(b)) => {
            if a != b {
                note(Kind::Changed);
            }
        }
    }
}
