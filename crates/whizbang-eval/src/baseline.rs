//! The comparison of two transcripts (docs/baseline/transcript.md).
//!
//! The baseline gate of milestone 2 replays each recorded decision and compares the transcript
//! whizbang produces with the recorded one. The comparison is exact: the whole ordered sequence
//! of requests, calls and results, and the terminal outcome. The only differences let through
//! are in fields the recorded transcript declares volatile, and a transcript cannot declare
//! volatile anything the gate exists to compare.

use std::fmt;

use serde_json::Value;

/// The version of this comparison, written into the gate receipt (docs/baseline/README.md).
/// It changes whenever what counts as a difference changes.
pub const COMPARISON_VERSION: &str = "1";

/// The transcript format this comparison reads.
pub const TRANSCRIPT_VERSION: u64 = 1;

/// What version 1 of the format declares volatile (docs/baseline/transcript.md). The converter
/// writes exactly this list; `scripts/tests/test-baseline.sh` holds the two copies together.
pub const VOLATILE: &[&str] = &[
    "/origin",
    "/meta",
    "/files",
    "/turns/*/meta",
    "/turns/*/calls/*/meta",
];

/// Paths a transcript may never declare volatile, nor anything above or below them. `*` stands
/// for any one position in a list.
const COMPARED: &[&str] = &[
    "/transcript_version",
    "/complete",
    "/incomplete",
    "/subject",
    "/preparation",
    "/evidence_tools",
    "/images",
    "/turns/*/turn",
    "/turns/*/request",
    "/turns/*/response",
    "/turns/*/excluded",
    "/turns/*/calls/*/id",
    "/turns/*/calls/*/name",
    "/turns/*/calls/*/arguments",
    "/turns/*/calls/*/disposition",
    "/turns/*/calls/*/result",
    "/repairs",
    "/terminal",
    "/volatile",
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A transcript is not of the version this comparison reads.
    Version { which: Side },
    /// The recorded transcript is not marked complete, so it is not a baseline.
    RecordedIncomplete,
    /// The recorded transcript has no list of volatile paths, or an entry is not a path.
    VolatileMalformed,
    /// The recorded transcript declares volatile something the gate compares.
    VolatileCovers {
        declared: String,
        compared: &'static str,
    },
}

/// Which of the two transcripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Recorded,
    Replayed,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Version { which } => {
                let which = match which {
                    Side::Recorded => "recorded",
                    Side::Replayed => "replayed",
                };
                write!(
                    f,
                    "the {which} transcript is not version {TRANSCRIPT_VERSION}"
                )
            }
            Refusal::RecordedIncomplete => {
                write!(
                    f,
                    "the recorded transcript is not complete, so nothing is compared with it"
                )
            }
            Refusal::VolatileMalformed => {
                write!(
                    f,
                    "the recorded transcript's volatile list is not a list of paths"
                )
            }
            Refusal::VolatileCovers { declared, compared } => write!(
                f,
                "the recorded transcript declares {declared} volatile, which covers {compared}"
            ),
        }
    }
}

impl std::error::Error for Refusal {}

/// Compares what whizbang replayed with what was recorded. `Ok` holds every difference, in the
/// order of the recorded transcript; an empty list is a match. Only the recorded transcript's
/// volatile list is honoured.
pub fn compare(recorded: &Value, replayed: &Value) -> Result<Vec<Difference>, Refusal> {
    for (which, t) in [(Side::Recorded, recorded), (Side::Replayed, replayed)] {
        if t.get("transcript_version").and_then(Value::as_u64) != Some(TRANSCRIPT_VERSION) {
            return Err(Refusal::Version { which });
        }
    }
    if recorded.get("complete") != Some(&Value::Bool(true)) {
        return Err(Refusal::RecordedIncomplete);
    }
    let volatile = volatile_paths(recorded)?;
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

/// The recorded transcript's volatile list as path segments, refused if it reaches anything
/// the gate compares.
fn volatile_paths(recorded: &Value) -> Result<Vec<Vec<String>>, Refusal> {
    let list = recorded
        .get("volatile")
        .and_then(Value::as_array)
        .ok_or(Refusal::VolatileMalformed)?;
    let mut out = Vec::new();
    for entry in list {
        let declared = entry.as_str().ok_or(Refusal::VolatileMalformed)?;
        let segments = segments(declared).ok_or(Refusal::VolatileMalformed)?;
        for compared in COMPARED {
            let kept = segments_of(compared);
            let shared = segments.len().min(kept.len());
            let overlaps =
                (0..shared).all(|i| segments[i] == "*" || kept[i] == "*" || segments[i] == kept[i]);
            if overlaps {
                return Err(Refusal::VolatileCovers {
                    declared: declared.to_string(),
                    compared,
                });
            }
        }
        out.push(segments);
    }
    Ok(out)
}

fn segments(path: &str) -> Option<Vec<String>> {
    let rest = path.strip_prefix('/')?;
    if rest.is_empty() || rest.split('/').any(str::is_empty) {
        return None;
    }
    Some(rest.split('/').map(str::to_string).collect())
}

fn segments_of(path: &'static str) -> Vec<&'static str> {
    path.trim_start_matches('/').split('/').collect()
}

fn is_volatile(path: &[String], volatile: &[Vec<String>]) -> bool {
    volatile.iter().any(|v| {
        v.len() == path.len()
            && v.iter()
                .zip(path)
                .all(|(want, have)| want == "*" || want == have)
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
    volatile: &[Vec<String>],
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
