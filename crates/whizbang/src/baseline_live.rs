//! `whizbang eval baseline --live` (plan m3, task G1, item 2; plan 2C, task E2, item 8): the
//! baseline's descriptive live rerun. Arm C of one model is run fresh on one set, one
//! repetition, through the matrix runner ([`crate::evaluation`]); its verdicts are set
//! against the recorded run's in the baseline package. Two numbers come of it, and nothing
//! of an item: the share of items whose fresh verdict is the recorded run's first
//! repetition's (its agreement), and the share of the recorded run's later repetitions
//! whose verdict is not its first's (the recorded self-disagreement).
//!
//! **What is read of the package**: its listing, which must hash to its digest, and each
//! decision's `result.json` of its study inputs (`study/<run>/decisions/…`), each the bytes
//! the listing names: its `cell` (`<profile>:<model>/<arm>`), `repetition`, `item` and
//! `verdict`. Of the runs that hold the model's arm C on the set's items, the recorded run
//! is the one with the most repetitions; two with as many are refused, since which one the
//! rerun is set against would be a guess. A missing verdict is an error.
//!
//! **What leaves the machine** is what any run of `check` sends, to the config's judge, under
//! the store's allowance: the matrix's forecast is set against what it has left before the
//! first run. Run again, it resumes, as a matrix does.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value as Json;
use whizbang_core::contract::RunId;
use whizbang_eval::matrix::Matrix;

use crate::baseline::Package;
use crate::door::printable;
use crate::evaluation::{self, Ended, Running};

/// The set the live rerun is made on.
pub const LIVE_SET: &str = "source-fidelity/blind-v3";
/// The model whose arm C is rerun.
pub const LIVE_MODEL: &str = "claude-haiku-5-5";

/// What the live rerun is asked to do.
pub struct Live<'a> {
    pub package: &'a Path,
    pub receipt: &'a Path,
    /// The folder of the evaluation sets.
    pub sets: &'a Path,
    pub set: &'a str,
    pub model: &'a str,
    /// The config's judge to run it under: `default` for the config's default.
    pub judge: &'a str,
}

/// What the live rerun came to: counts only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rerun {
    pub package_digest: String,
    /// Cells of the fresh run, and how many of them failed (an error is a verdict of its own).
    pub decisions: usize,
    pub failed: usize,
    /// Items with a fresh verdict and a recorded first repetition, and how many agree.
    pub compared: usize,
    pub agreed: usize,
    /// The recorded run's later repetitions of an item it decided first, and how many of
    /// them differ from its first.
    pub recorded_later: usize,
    pub recorded_differing: usize,
}

fn share(count: usize, of: usize) -> String {
    if of == 0 {
        "-".to_owned()
    } else {
        format!("{:.1}%", 100.0 * count as f64 / of as f64)
    }
}

impl Rerun {
    /// Counts only.
    pub fn text(&self) -> String {
        format!(
            "live decisions: {} (failed {})\nagreement with the recorded run: {} of {} ({})\n\
             recorded self-disagreement: {} of {} ({})\n",
            self.decisions,
            self.failed,
            self.agreed,
            self.compared,
            share(self.agreed, self.compared),
            self.recorded_differing,
            self.recorded_later,
            share(self.recorded_differing, self.recorded_later)
        )
    }

    /// The receipt: the package, what was rerun, and the counts.
    pub fn receipt(&self, set: &str, model: &str) -> String {
        format!(
            "package_digest = \"{}\"\nset = \"{}\"\nmodel = \"{}\"\narm = \"C\"\n\
             live_decisions = {}\nlive_failed = {}\ncompared = {}\nagreed = {}\n\
             recorded_later = {}\nrecorded_differing = {}\n",
            self.package_digest,
            printable(set),
            printable(model),
            self.decisions,
            self.failed,
            self.compared,
            self.agreed,
            self.recorded_later,
            self.recorded_differing
        )
    }
}

/// The recorded verdict of each (item, repetition) of the model's arm C on these items, of
/// the one run with the most repetitions.
pub(crate) fn recorded(
    package: &Package<'_>,
    model: &str,
    items: &[String],
) -> Result<BTreeMap<(String, u32), String>, String> {
    let mut runs: BTreeMap<String, BTreeMap<(String, u32), String>> = BTreeMap::new();
    let paths: Vec<String> = (package.listed.keys())
        .filter(|p| {
            p.starts_with("study/") && p.ends_with("/result.json") && p.contains("/decisions/")
        })
        .cloned()
        .collect();
    for path in paths {
        let result: Json = package.json(&path)?;
        let cell = result["cell"].as_str().unwrap_or_default();
        let (head, arm) = cell.rsplit_once('/').unwrap_or_default();
        let named = head.split_once(':').map_or(head, |(_, m)| m);
        let item = result["item"].as_str().unwrap_or_default();
        if arm != "C" || named != model || !items.iter().any(|i| i == item) {
            continue;
        }
        let repetition = result["repetition"]
            .as_u64()
            .and_then(|r| u32::try_from(r).ok())
            .ok_or("a recorded decision names no repetition")?;
        let verdict = match result["verdict"].as_str() {
            Some(word @ ("pass" | "fail" | "unsure")) => word.to_owned(),
            _ => "error".to_owned(),
        };
        let run = path.split('/').nth(1).unwrap_or_default().to_owned();
        let decided = runs.entry(run).or_default();
        if decided
            .insert((item.to_owned(), repetition), verdict)
            .is_some()
        {
            return Err("a recorded run holds two decisions of one item and repetition".to_owned());
        }
    }
    let most = |d: &BTreeMap<(String, u32), String>| d.keys().map(|(_, r)| *r).max().unwrap_or(0);
    let top = runs.values().map(most).max().unwrap_or(0);
    let mut chosen = runs.into_values().filter(|d| most(d) == top);
    match (chosen.next(), chosen.next()) {
        (Some(one), None) => Ok(one),
        (None, _) => {
            Err("the package records no decision of the model's arm C on the set".to_owned())
        }
        (Some(_), Some(_)) => Err(
            "two recorded runs of the model's arm C have as many repetitions, and the rerun \
             would be set against a guess"
                .to_owned(),
        ),
    }
}

/// Runs arm C of the model on the set once, fresh, sets its verdicts against the recorded
/// run's, writes the receipt, and says what it came to in counts. Refused before anything
/// runs when the package cannot be read or records no run to set it against.
pub fn run(live: &Live<'_>, running: &Running<'_>) -> Result<Rerun, String> {
    if live.receipt.starts_with(live.package) {
        return Err("the receipt is not written into the package".to_owned());
    }
    let package = Package::open(live.package)?;
    let set = whizbang_eval::sets::find(live.sets, live.set)?;
    let items: Vec<String> = set.jobs.iter().map(|(item, _)| item.clone()).collect();
    let recorded = recorded(&package, live.model, &items)?;
    let text = format!(
        "[matrix]\nname = \"baseline-live\"\nsets = [\"{}\"]\narms = [\"C\"]\nrepetitions = 1\n\
         [[matrix.judges]]\nbackend = \"{}\"\nmodel = \"{}\"\n",
        live.set, live.judge, live.model
    );
    let matrix = Matrix::parse(&text)?;
    let (planned, labels) = evaluation::plan(&matrix, live.sets, running.root)?;
    evaluation::run(&matrix, &planned, &labels, running)?;
    let identity: String = matrix.identity().chars().take(16).collect();
    let attempts = evaluation::attempts(running.root, &identity)?;
    let mut rerun = Rerun {
        package_digest: package.digest.clone(),
        ..Rerun::default()
    };
    for planned in &planned {
        let last = attempts.get(&planned.cell).and_then(|list| list.last());
        let fresh = match last {
            Some((_, run, Ended::Completed)) => RunId::parse(run)
                .ok()
                .and_then(|run| crate::door::verdict(running.root, &run))
                .map_or("error".to_owned(), |stated| stated.word),
            Some((_, _, Ended::Failed)) => "error".to_owned(),
            _ => continue,
        };
        rerun.decisions += 1;
        rerun.failed += usize::from(fresh == "error");
        if let Some(first) = recorded.get(&(planned.cell.item.clone(), 1)) {
            rerun.compared += 1;
            rerun.agreed += usize::from(*first == fresh);
        }
    }
    for ((item, repetition), verdict) in &recorded {
        if *repetition > 1
            && let Some(first) = recorded.get(&(item.clone(), 1))
        {
            rerun.recorded_later += 1;
            rerun.recorded_differing += usize::from(first != verdict);
        }
    }
    std::fs::write(live.receipt, rerun.receipt(live.set, live.model))
        .map_err(|_| "the receipt cannot be written".to_owned())?;
    Ok(rerun)
}
