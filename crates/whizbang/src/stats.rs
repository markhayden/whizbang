//! `whizbang runs stats` and `runs export` (`SPEC.md` section 3, "Metrics from ordinary
//! runs"; plan 2C, task E1, item 5): a query over the store's run records, under a filter.
//! Both only read, so both work while another process writes the store.
//!
//! The population is the runs that have a record: a run admitted and not ended has none
//! yet, and is counted apart. A record is the run's as it is stored, and holds no label of
//! an evaluation set: a matrix's report joins labels from the set when it is read.

use std::path::Path;
use std::sync::Arc;

use clap::Args;
use whizbang_core::contract::record::{self, ArgumentValue, Judged, RunRecord, Status};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;

/// Which runs a query is of. Each flag given narrows it; none given is every run.
#[derive(Args, Default, Clone)]
pub struct Filter {
    /// Runs of this action or tool, by name, child runs included: `check`, `align`.
    #[arg(long, visible_alias = "tool", value_name = "NAME")]
    action: Option<String>,
    /// Runs that asked a backend of this model.
    #[arg(long, value_name = "MODEL")]
    model: Option<String>,
    /// Runs that asked the backend of this name.
    #[arg(long, value_name = "NAME")]
    backend: Option<String>,
    /// Runs of `check` under this use case.
    #[arg(long, value_name = "USE_CASE")]
    use_case: Option<String>,
    /// Runs of `check` that are cells of the matrix of this name.
    #[arg(long, value_name = "NAME")]
    matrix: Option<String>,
    /// Runs that started on or after this day, in UTC: YYYY-MM-DD.
    #[arg(long, value_name = "DAY")]
    since: Option<String>,
    /// Runs that started before this day, in UTC: YYYY-MM-DD.
    #[arg(long, value_name = "DAY")]
    until: Option<String>,
}

/// Milliseconds since the Unix epoch at the start of a day in UTC, `YYYY-MM-DD`.
fn day(text: &str, flag: &str) -> Result<u64, String> {
    let refused = || format!("--{flag} is a day, YYYY-MM-DD");
    let parts: Vec<&str> = text.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return Err(refused());
    };
    let number = |part: &str, len: usize| {
        (part.len() == len && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<i64>().ok())
            .flatten()
    };
    let (Some(y), Some(m), Some(d)) = (number(year, 4), number(month, 2), number(day, 2)) else {
        return Err(refused());
    };
    if !(1970..=9999).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(refused());
    }
    // Days since the epoch of a civil date, by the usual arithmetic on eras of 400 years.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let shifted = if m > 2 { m - 3 } else { m + 9 };
    let day_of_year = (153 * shifted + 2) / 5 + d - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    u64::try_from(days * 86_400_000).map_err(|_| refused())
}

fn text_argument<'a>(record: &'a RunRecord, name: &str) -> Option<&'a str> {
    let argument = record.asked().arguments.iter().find(|a| a.name == name)?;
    match &argument.value {
        ArgumentValue::Text(text) => Some(text),
        _ => None,
    }
}

impl Filter {
    fn admits(&self, record: &RunRecord, since: Option<u64>, until: Option<u64>) -> bool {
        let asked = record.asked();
        let models = record.models();
        let start = record.time().start_ms;
        self.action
            .as_deref()
            .is_none_or(|name| asked.action.as_deref() == Some(name))
            && (self.model.as_deref()).is_none_or(|m| models.iter().any(|one| one.model == m))
            && (self.backend.as_deref()).is_none_or(|b| models.iter().any(|one| one.backend == b))
            && (self.use_case.as_deref())
                .is_none_or(|u| text_argument(record, "use_case") == Some(u))
            && (self.matrix.as_deref()).is_none_or(|name| {
                whizbang::evaluation::cell_of(record).is_some_and(|cell| cell.matrix == name)
            })
            && since.is_none_or(|since| start.is_some_and(|start| start >= since))
            && until.is_none_or(|until| start.is_some_and(|start| start < until))
    }
}

/// Every run with a record that the filter admits, in the order of their ids, with the
/// count of runs that have no record.
fn matching(root: &Path, filter: &Filter) -> Result<(Vec<(String, RunRecord)>, usize), String> {
    let since = filter
        .since
        .as_deref()
        .map(|t| day(t, "since"))
        .transpose()?;
    let until = filter
        .until
        .as_deref()
        .map(|t| day(t, "until"))
        .transpose()?;
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let mut found = Vec::new();
    let mut without = 0;
    for id in generation.runs().keys() {
        match record::stored(&generation, id, |entry| reader.read(entry)) {
            Some(Ok(record)) => {
                if filter.admits(&record, since, until) {
                    found.push((id.clone(), record));
                }
            }
            _ => without += 1,
        }
    }
    Ok((found, without))
}

/// `runs export`: each matching run's record as it is stored, one JSON object a line.
pub fn export(root: &Path, filter: &Filter) -> Result<Vec<u8>, String> {
    let (found, _) = matching(root, filter)?;
    let mut out = Vec::new();
    for (_, record) in found {
        let bytes = record.to_bytes();
        out.extend_from_slice(bytes.trim_ascii_end());
        out.push(b'\n');
    }
    Ok(out)
}

/// The value at this share of a sorted list, by the nearest rank.
fn rank(sorted: &[u64], percent: usize) -> Option<u64> {
    let at = (sorted.len() * percent).div_ceil(100).checked_sub(1)?;
    sorted.get(at).copied()
}

fn share(part: usize, whole: usize) -> String {
    if whole == 0 {
        return "-".to_owned();
    }
    format!("{:.1}%", part as f64 * 100.0 / whole as f64)
}

/// `runs stats`: how many runs; how they ended; for runs that judged, their verdicts;
/// retries and repairs; time as the median and the 95th percentile; and cost.
pub fn stats(root: &Path, filter: &Filter) -> Result<String, String> {
    let (found, without) = matching(root, filter)?;
    let runs = found.len();
    let count = |status: Status| {
        (found.iter())
            .filter(|(_, r)| r.outcome().status == status && r.outcome().lost.is_empty())
            .count()
    };
    let interrupted = found
        .iter()
        .filter(|(_, r)| !r.outcome().lost.is_empty())
        .count();
    let mut text = format!("runs: {runs}\n");
    if without > 0 {
        text.push_str(&format!(
            "admitted and not ended, or with no record read: {without}\n"
        ));
    }
    for (word, status) in [
        ("completed", Status::Completed),
        ("error", Status::Error),
        ("invalid", Status::Invalid),
        ("cancelled", Status::Cancelled),
    ] {
        let n = count(status);
        text.push_str(&format!("{word}: {n} ({})\n", share(n, runs)));
    }
    text.push_str(&format!(
        "interrupted: {interrupted} ({})\n",
        share(interrupted, runs)
    ));
    // Verdicts, of the runs that reached one.
    let (mut pass, mut fail, mut unsure) = (0, 0, 0);
    for (_, record) in &found {
        if let Some((decision, _)) = record.verdict() {
            match decision {
                record::Decision::Pass => pass += 1,
                record::Decision::Fail => fail += 1,
                record::Decision::Unsure => unsure += 1,
            }
        }
    }
    let judged = pass + fail + unsure;
    text.push_str(&format!(
        "verdicts: {judged} (pass {pass}, fail {fail}, unsure {unsure}); unsure rate {}\n",
        share(unsure, judged)
    ));
    // Retries: turns the harness excluded and sent again. Repairs: from what a run of a
    // use case kept of its judging.
    let mut retries = 0u64;
    let mut repairs = 0u64;
    for (id, record) in &found {
        retries += (record.judge().iter())
            .filter(|j| {
                matches!(
                    j,
                    Judged::Turn {
                        excluded: Some(_),
                        ..
                    }
                )
            })
            .count() as u64;
        let kept = whizbang::door::diagnostic(
            root,
            &match whizbang_core::contract::RunId::parse(id) {
                Ok(run) => run,
                Err(_) => continue,
            },
            whizbang_squint::check::JUDGED,
        );
        if let Ok(bytes) = kept
            && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            let length = |key: &str| json[key].as_array().map_or(0, Vec::len) as u64;
            retries += length("retries");
            repairs += length("repairs");
        }
    }
    text.push_str(&format!("retries: {retries}\nrepairs: {repairs}\n"));
    let mut times: Vec<u64> = (found.iter())
        .filter_map(|(_, r)| {
            r.time()
                .start_ms
                .map(|start| r.time().end_ms.saturating_sub(start))
        })
        .collect();
    times.sort_unstable();
    match (rank(&times, 50), rank(&times, 95)) {
        (Some(median), Some(p95)) => text.push_str(&format!(
            "time: median {median} ms, 95th percentile {p95} ms\n"
        )),
        _ => text.push_str("time: none\n"),
    }
    let settled: u128 = (found.iter())
        .flat_map(|(_, r)| r.spend().requests.iter())
        .map(|paid| u128::from(paid.settled_micros()))
        .sum();
    let unresolved: u128 = (found.iter())
        .flat_map(|(_, r)| r.spend().requests.iter())
        .map(|paid| u128::from(paid.unresolved_micros()))
        .sum();
    text.push_str(&format!(
        "cost: {} settled, {} unresolved\n",
        crate::spend::usd(settled),
        crate::spend::usd(unresolved)
    ));
    text.push_str("cache hits: not recorded (the decision cache is not built yet)\n");
    Ok(text)
}
