//! A replay's report (plan m7, task R1, items 5 and 7; ADR 0049): once a replayed run has
//! ended, its family against the family it replays, read from the store, run matched to
//! run by place. What `whizbang runs replay` prints, as text or as JSON, and what the
//! playground will show.
//!
//! The family a run replays is read from where the replayed record says it came from:
//! the store's own runs, or the entry of the bundle it names. Everything printed that
//! came from a store is escaped where it is printed.

use std::path::Path;
use std::sync::Arc;

use serde_json::json;
use whizbang_core::bundle;
use whizbang_core::contract::record::{ReplayMode, RunRecord};
use whizbang_core::contract::replay::{self, Difference, Records, Report, Stored};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;

use crate::door::printable;

/// What a replay came to: its run, the run it replays, how, its report, and, for a live
/// replay, each setting it ran under that differs from the original's.
#[derive(Debug, Clone)]
pub struct Reported {
    pub replay: String,
    pub original: String,
    pub bundle: Option<(String, u32)>,
    pub mode: ReplayMode,
    pub report: Report,
    pub settings: Vec<Difference>,
}

impl Reported {
    /// Whether the replay reproduced the run.
    pub fn reproduced(&self) -> bool {
        self.report == Report::Reproduced
    }

    /// The report, as `runs replay` prints it.
    pub fn text(&self) -> String {
        let mode = match self.mode {
            ReplayMode::Offline => "offline",
            ReplayMode::Live => "live",
        };
        let from = match &self.bundle {
            Some((digest, step)) => format!(" (step {step} of bundle {digest})"),
            None => String::new(),
        };
        let mut text = format!(
            "replay: {}\nof: {}{from}\nmode: {mode}\nreport: {}\n",
            self.replay,
            self.original,
            self.report.name()
        );
        match &self.report {
            Report::Reproduced => {}
            Report::Stopped(stop) => text.push_str(&printable(&format!(
                "stopped at: {} request {}: {}\n",
                stop.capability.name(),
                stop.request,
                stop.why
            ))),
            Report::Differs { differences, more } => {
                text.push_str(&format!("differences: {}\n", differences.len() + more));
                text.push_str(&lines(differences));
                if *more > 0 {
                    text.push_str(&format!("  and {more} more\n"));
                }
            }
        }
        if !self.settings.is_empty() {
            text.push_str(&format!("settings that differ: {}\n", self.settings.len()));
            text.push_str(&lines(&self.settings));
        }
        printable_lines(&text)
    }

    /// The report as one JSON object, and a newline.
    pub fn json(&self) -> String {
        let (differences, more, stop) = match &self.report {
            Report::Differs { differences, more } => (differences.clone(), *more, None),
            Report::Stopped(stop) => (
                Vec::new(),
                0,
                Some(json!({
                    "kind": stop.kind.name(),
                    "capability": stop.capability.name(),
                    "request": stop.request,
                    "why": stop.why,
                })),
            ),
            Report::Reproduced => (Vec::new(), 0, None),
        };
        let value = json!({
            "replay": self.replay,
            "of": self.original,
            "bundle": self.bundle.as_ref().map(|(digest, step)| json!({"digest": digest, "step": step})),
            "mode": match self.mode { ReplayMode::Offline => "offline", ReplayMode::Live => "live" },
            "report": self.report.name(),
            "differences": differences,
            "more": more,
            "stopped": stop,
            "settings": self.settings,
        });
        format!("{value}\n")
    }
}

fn lines(differences: &[Difference]) -> String {
    let mut text = String::new();
    for difference in differences {
        text.push_str(&format!(
            "  {}: {}: {} -> {}\n",
            difference.place, difference.what, difference.original, difference.replay
        ));
    }
    text
}

/// Each line of a text escaped by itself.
fn printable_lines(text: &str) -> String {
    text.lines().map(|line| printable(line) + "\n").collect()
}

/// The report of the replayed run `run` of the store at `root`, once it has ended.
pub fn report_of(root: &Path, run: &str) -> Result<Reported, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let here = Stored::of_store(&reader, &generation);
    let record: RunRecord = here
        .record(run)?
        .ok_or_else(|| format!("the run {run} has no record: it has not ended"))?;
    let Some(replayed) = record.replayed().cloned() else {
        return Err(format!("the run {run} is not a replay"));
    };
    let there = match &replayed.bundle {
        Some(from) => Stored::of_bundle(&reader, &generation, &from.digest)
            .map_err(|why| format!("the bundle the replay came from cannot be read: {why}"))?,
        None => Stored::of_store(&reader, &generation),
    };
    let original = replay::placed(&there, &replayed.run)
        .map_err(|why| format!("the run the replay replays cannot be read: {why}"))?;
    let made = replay::placed(&here, run)?;
    let note = here.kept(run, &replay::note_name())?;
    let report = replay::report(&original, &made, note.as_deref());
    let settings = match replayed.mode {
        ReplayMode::Live => replay::settings_differ(&original[0].record, &made[0].record),
        ReplayMode::Offline => Vec::new(),
    };
    Ok(Reported {
        replay: run.to_owned(),
        original: replayed.run.clone(),
        bundle: (replayed.bundle.as_ref()).map(|from| (from.digest.to_string(), from.step)),
        mode: replayed.mode,
        report,
        settings,
    })
}

/// The digest of the bundle file at `file`, read as a stream.
pub fn digest_of_file(file: &Path) -> Result<whizbang_core::identity::Digest, String> {
    let opened = crate::bundling::opened(file)?;
    bundle::file_digest(std::io::BufReader::new(opened))
        .map_err(|why| format!("cannot read {}: {why}", file.display()))
}
