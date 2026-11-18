//! The comparison of two transcripts (docs/baseline/transcript.md).
//!
//! The baseline gate of milestone 2 replays each recorded decision and compares the transcript
//! whizbang produces with the recorded one. The comparison is exact: everything in a transcript
//! is compared except the places the format declares volatile. A field nobody has heard of is
//! compared like any other, and a recorded transcript cannot widen what is let through.

use std::fmt;

use crate::json::{Json, parse};

/// The version of this comparison, written into the gate receipt (docs/baseline/README.md).
/// It changes whenever what counts as a difference changes.
pub const COMPARISON_VERSION: &str = "4";

/// The transcript format the converter writes.
pub const TRANSCRIPT_VERSION: u64 = 1;

/// Format 2 (docs/baseline/transcript.md, "Version 2"): version 1, except that a tool
/// image squint scaled down is compared by its size and mapping and not by its prepared
/// bytes. Both transcripts of a pair are of one version.
pub const SCALED_FORMAT: u64 = 2;

/// The longest side `view` cuts without scaling the cut down (squint's `MAX_SIDE`).
pub const SCALED_ABOVE: f64 = 1024.0;

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
    /// A transcript is not one JSON value.
    NotJson { which: Side },
    /// A transcript is not of a version this comparison reads, or the replayed one is not
    /// of the recorded one's.
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
            Refusal::NotJson { which } => write!(f, "the {which} transcript is not JSON"),
            Refusal::Version { which } => {
                write!(
                    f,
                    "the {which} transcript is not version {TRANSCRIPT_VERSION} or \
                     {SCALED_FORMAT}, or not the recorded one's"
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

/// Compares what whizbang replayed with what was recorded, each as the text of a transcript.
/// `Ok` holds every difference, in the order of the recorded transcript; an empty list is a
/// match. Numbers are compared by the text they were written with, so `1` and `1.0` differ
/// and no two large numbers are confused. One thing about that text is lost before the
/// comparison sees it: the reader writes an exponent's `E` as `e`, and a `+` where no sign
/// was written, so `1E2`, `1e2` and `1e+2` are equal. Texts are compared as JSON decodes
/// them, so `"\u00e9"` and `"é"` are equal.
pub fn compare(recorded: &str, replayed: &str) -> Result<Vec<Difference>, Refusal> {
    let recorded = parse(recorded).ok_or(Refusal::NotJson {
        which: Side::Recorded,
    })?;
    let replayed = parse(replayed).ok_or(Refusal::NotJson {
        which: Side::Replayed,
    })?;
    // Digits only (docs/baseline/transcript.md, "whole number").
    let version = [TRANSCRIPT_VERSION, SCALED_FORMAT]
        .into_iter()
        .map(|v| Json::Number(v.to_string()))
        .find(|v| recorded.get("transcript_version") == Some(v))
        .ok_or(Refusal::Version {
            which: Side::Recorded,
        })?;
    if replayed.get("transcript_version") != Some(&version) {
        return Err(Refusal::Version {
            which: Side::Replayed,
        });
    }
    for (which, t) in [(Side::Recorded, &recorded), (Side::Replayed, &replayed)] {
        // At least one turn, and an outcome that says what kind it is: an empty list or an
        // empty outcome on both sides would otherwise compare as equal.
        let turns = t
            .get("turns")
            .and_then(Json::as_list)
            .is_some_and(|turns| !turns.is_empty());
        let terminal = t
            .get("terminal")
            .and_then(|terminal| terminal.get("kind"))
            .is_some_and(|kind| kind.as_text().is_some());
        if !turns || !terminal {
            return Err(Refusal::Shape { which });
        }
    }
    let nothing_missing = recorded
        .get("incomplete")
        .and_then(Json::as_list)
        .is_some_and(Vec::is_empty);
    if recorded.get("complete") != Some(&Json::Bool(true)) || !nothing_missing {
        return Err(Refusal::RecordedIncomplete);
    }
    let declared: Option<Vec<&str>> = recorded
        .get("volatile")
        .and_then(Json::as_list)
        .and_then(|list| list.iter().map(Json::as_text).collect());
    if declared.as_deref() != Some(VOLATILE) {
        return Err(Refusal::Volatile);
    }
    let scaled: Vec<String> = if version == Json::Number(SCALED_FORMAT.to_string()) {
        scaled_tool_images(&recorded)
            .into_iter()
            .map(|i| format!("/images/{i}/prepared"))
            .collect()
    } else {
        Vec::new()
    };
    let volatile: Vec<Vec<&str>> = VOLATILE
        .iter()
        .copied()
        .chain(scaled.iter().map(String::as_str))
        .map(|path| path.trim_start_matches('/').split('/').collect())
        .collect();
    let mut differences = Vec::new();
    let mut path = Vec::new();
    walk(
        Some(&recorded),
        Some(&replayed),
        &mut path,
        &volatile,
        &mut differences,
    );
    Ok(differences)
}

/// The places in `images` of the recorded transcript's tool images that squint scaled
/// down: a crop (its mapping a box, as fractions of its parent) whose cut is longer than
/// [`SCALED_ABOVE`] pixels on a side of the parent it was cut from. The cut is counted, not
/// the crop's own size: squint's `view` fitted a longer cut to 1,024, so a crop of 1,024 is
/// either. `scripts/baseline/audit.py --count-scaled-cuts` counts the same way.
pub(crate) fn scaled_tool_images(transcript: &Json) -> Vec<usize> {
    let Some(images) = transcript.get("images").and_then(Json::as_list) else {
        return Vec::new();
    };
    let number = |j: Option<&Json>| match j {
        Some(Json::Number(text)) => text.parse::<f64>().ok(),
        _ => None,
    };
    let mut out = Vec::new();
    for (index, image) in images.iter().enumerate() {
        if image.get("origin").and_then(Json::as_text) == Some("job") {
            continue;
        }
        let Some(b) = image
            .get("mapping")
            .and_then(|m| m.get("bbox"))
            .and_then(Json::as_list)
            .and_then(|b| {
                b.iter()
                    .map(|x| number(Some(x)))
                    .collect::<Option<Vec<f64>>>()
            })
            .filter(|b| b.len() == 4)
        else {
            continue;
        };
        let parent = image.get("parent").and_then(Json::as_text);
        let Some(parent) = images
            .iter()
            .find(|p| parent.is_some() && p.get("frame").and_then(Json::as_text) == parent)
        else {
            continue;
        };
        let (Some(w), Some(h)) = (number(parent.get("width")), number(parent.get("height"))) else {
            continue;
        };
        let cut_w = ((b[2] - b[0]) * w).round();
        let cut_h = ((b[3] - b[1]) * h).round();
        if cut_w > SCALED_ABOVE || cut_h > SCALED_ABOVE {
            out.push(index);
        }
    }
    out
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
    recorded: Option<&Json>,
    replayed: Option<&Json>,
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
        (Some(Json::Object(a)), Some(Json::Object(b))) => {
            // Keys are kept sorted, so the order is the same on every run.
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                path.push(key.clone());
                walk(a.get(key), b.get(key), path, volatile, out);
                path.pop();
            }
        }
        (Some(Json::List(a)), Some(Json::List(b))) => {
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
