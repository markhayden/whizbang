//! `whizbang runs stats` and `runs export` (`SPEC.md` section 3, "Metrics from ordinary
//! runs"; plan 2C, task E1, item 5), and the playground's history and figures (plan m5,
//! task W3, items 1 and 2): a query over the store's run records, under a filter. All
//! only read, so all work while another process writes the store.
//!
//! The choosing of runs ([`Filter`], [`Runs::matching`]) and the aggregation
//! ([`Figures`]) are one code for the command and the page: `runs stats` prints
//! [`Figures::text`], and `GET /api/stats` answers the same [`Figures`] as JSON, so the
//! two cannot differ.
//!
//! The population is the runs that have a record: a run admitted and not ended has none
//! yet, and is counted apart. A record is the run's as it is stored, and holds no label of
//! an evaluation set: a matrix's report joins labels from the set when it is read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use clap::Args;
use serde::Serialize;
use whizbang_core::contract::record::{
    self, ArgumentValue, DIAGNOSTICS_PREFIX, Judged, RunRecord, Status,
};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Entry, Generation, Reader};

/// The results a run is listed under, as the playground's filter names them.
pub const RESULTS: [&str; 7] = [
    "pass",
    "fail",
    "unsure",
    "done",
    "cancelled",
    "error",
    "interrupted",
];

/// The most characters of the words a filter looks for.
pub const WORDS_LIMIT: usize = 200;

/// Which runs a query is of. Each flag given narrows it; none given is every run.
#[derive(Args, Default, Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    /// Runs of this action or tool, by name, child runs included: `check`, `align`.
    #[arg(long, visible_alias = "tool", value_name = "NAME")]
    pub action: Option<String>,
    /// Runs that asked a backend of this model.
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
    /// Runs that asked the backend of this name.
    #[arg(long, value_name = "NAME")]
    pub backend: Option<String>,
    /// Runs of `check` under this use case.
    #[arg(long, value_name = "USE_CASE")]
    pub use_case: Option<String>,
    /// Runs of `check` that are cells of the matrix of this name.
    #[arg(long, value_name = "NAME")]
    pub matrix: Option<String>,
    /// Runs that started on or after this day, in UTC: YYYY-MM-DD.
    #[arg(long, value_name = "DAY")]
    pub since: Option<String>,
    /// Runs that started before this day, in UTC: YYYY-MM-DD.
    #[arg(long, value_name = "DAY")]
    pub until: Option<String>,
    /// Runs of an action of this kit: `rascal`, `squint`.
    #[arg(long, value_name = "KIT")]
    pub kit: Option<String>,
    /// Runs that ended so: pass, fail, unsure (a verdict), done (completed with none),
    /// cancelled, error (an error or an invalid run), interrupted.
    #[arg(long, value_name = "RESULT", value_parser = RESULTS)]
    pub result: Option<String>,
    /// Runs whose arguments' text, or the name a picture they were given came with,
    /// holds these words, matched without case.
    #[arg(long, value_name = "WORDS")]
    pub words: Option<String>,
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

/// The result a run is listed under: one of [`RESULTS`].
pub fn result_of(record: &RunRecord) -> &'static str {
    let outcome = record.outcome();
    if !outcome.lost.is_empty() {
        return "interrupted";
    }
    match outcome.status {
        Status::Cancelled => "cancelled",
        Status::Error | Status::Invalid => "error",
        Status::Completed => match record.verdict() {
            Some((record::Decision::Pass, _)) => "pass",
            Some((record::Decision::Fail, _)) => "fail",
            Some((record::Decision::Unsure, _)) => "unsure",
            None => "done",
        },
    }
}

/// What a filter looks a run up in beyond its record: the kit of each action, and the
/// name each picture the playground kept came with.
#[derive(Debug, Clone, Default)]
pub struct Lookups {
    /// The kit of each action, by its name.
    pub kits: BTreeMap<String, String>,
    /// The name of each picture the playground kept, by the hex of its identity.
    pub names: BTreeMap<String, String>,
}

impl Lookups {
    /// The kits of the build's actions, by their descriptors.
    pub fn of_build() -> Lookups {
        let mut kits = BTreeMap::new();
        if let Ok(registry) = crate::wiring::registry() {
            for descriptor in registry.descriptors() {
                if let Some(kit) = descriptor.kit() {
                    kits.insert(descriptor.name().to_owned(), kit.to_owned());
                }
            }
        }
        Lookups {
            kits,
            names: BTreeMap::new(),
        }
    }

    /// The same, with the names of the pictures the playground kept in this generation.
    pub fn with_names(
        mut self,
        generation: &Generation,
        read: &dyn Fn(&Entry) -> whizbang_core::store::Result<Vec<u8>>,
    ) -> Lookups {
        let prefix = crate::play_kept::PICTURES;
        let kept = generation.entries().range(prefix.to_owned()..);
        for (name, entry) in kept.take_while(|(name, _)| name.starts_with(prefix)) {
            let Ok(bytes) = read(entry) else { continue };
            if let Ok(named) = serde_json::from_slice::<crate::play_kept::Named>(&bytes) {
                self.names
                    .insert(name[prefix.len()..].to_owned(), named.name);
            }
        }
        self
    }

    /// The names of the pictures a run was given, in its order.
    pub fn names_of(&self, record: &RunRecord) -> Vec<String> {
        (record.inputs().iter())
            .filter_map(|input| self.names.get(&input.artifact.hex()).cloned())
            .collect()
    }
}

/// A filter whose days are read.
struct Read<'a> {
    filter: &'a Filter,
    since: Option<u64>,
    until: Option<u64>,
    words: Option<String>,
}

impl Filter {
    /// Refuses a filter that cannot be read: a day that is no day, a result that is not
    /// one of [`RESULTS`], words over [`WORDS_LIMIT`].
    fn read(&self) -> Result<Read<'_>, String> {
        let since = (self.since.as_deref())
            .map(|t| day(t, "since"))
            .transpose()?;
        let until = (self.until.as_deref())
            .map(|t| day(t, "until"))
            .transpose()?;
        if let Some(result) = &self.result
            && !RESULTS.contains(&result.as_str())
        {
            return Err(format!("--result is one of {}", RESULTS.join(", ")));
        }
        if let Some(words) = &self.words
            && words.chars().count() > WORDS_LIMIT
        {
            return Err(format!("--words is at most {WORDS_LIMIT} characters"));
        }
        let words = (self.words.as_deref())
            .map(|words| words.trim().to_lowercase())
            .filter(|words| !words.is_empty());
        Ok(Read {
            filter: self,
            since,
            until,
            words,
        })
    }
}

impl Read<'_> {
    fn admits(&self, record: &RunRecord, lookups: &Lookups) -> bool {
        let filter = self.filter;
        let asked = record.asked();
        let models = record.models();
        let start = record.time().start_ms;
        let action = asked.action.as_deref();
        let said = |words: &str| {
            // What a row of history shows: the arguments' words, not a job's JSON.
            let texts = (asked.arguments.iter()).filter_map(|a| match &a.value {
                ArgumentValue::Text(text) if a.name != "job" => Some(text.to_lowercase()),
                _ => None,
            });
            let names = lookups
                .names_of(record)
                .into_iter()
                .map(|n| n.to_lowercase());
            texts.chain(names).any(|text| text.contains(words))
        };
        (filter.action.as_deref()).is_none_or(|name| action == Some(name))
            && (filter.model.as_deref()).is_none_or(|m| models.iter().any(|one| one.model == m))
            && (filter.backend.as_deref()).is_none_or(|b| models.iter().any(|one| one.backend == b))
            && (filter.use_case.as_deref())
                .is_none_or(|u| text_argument(record, "use_case") == Some(u))
            && (filter.matrix.as_deref()).is_none_or(|name| {
                crate::evaluation::cell_of(record).is_some_and(|cell| cell.matrix == name)
            })
            && self
                .since
                .is_none_or(|since| start.is_some_and(|start| start >= since))
            && self
                .until
                .is_none_or(|until| start.is_some_and(|start| start < until))
            && (filter.kit.as_deref()).is_none_or(|kit| {
                action.and_then(|a| lookups.kits.get(a)).map(String::as_str) == Some(kit)
            })
            && (filter.result.as_deref()).is_none_or(|result| result_of(record) == result)
            && self.words.as_deref().is_none_or(said)
    }
}

/// Every run record of one generation of a store, read once, with what reads what the
/// runs kept beside their records: what history and the figures choose from. The
/// playground keeps one for its store's current generation and reads the store again
/// only when it has published since (plan m5, task W3).
pub struct Loaded {
    /// The generation's number.
    pub number: u64,
    /// Each record read, by its run's id.
    pub all: BTreeMap<String, RunRecord>,
    /// Each run's children, by the run that called them.
    pub children: BTreeMap<String, Vec<String>>,
    /// How many runs have no record that was read.
    pub without: usize,
    /// The name of each picture the playground kept, by the hex of its identity.
    pub names: BTreeMap<String, String>,
    /// Every run of a replayed family (ADR 0049): a run whose record names the run it
    /// replays, one a dead process left whose admission does, and every run under one.
    pub replayed: BTreeSet<String>,
    generation: Generation,
    reader: Reader,
    /// What the runs kept as JSON, read once each: by run and name.
    kept: std::sync::Mutex<BTreeMap<(String, String), Option<serde_json::Value>>>,
}

impl Loaded {
    /// Reads every record of the store at `root`, by a reader of its own.
    pub fn load(root: &Path) -> Result<Loaded, String> {
        let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
        let generation = reader.snapshot().map_err(|e| e.to_string())?;
        let names = Lookups::default()
            .with_names(&generation, &|entry| reader.read(entry))
            .names;
        let mut all = BTreeMap::new();
        let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut without = 0;
        for id in generation.runs().keys() {
            match record::stored(&generation, id, |entry| reader.read(entry)) {
                Some(Ok(record)) => {
                    if let Some(parent) = &record.identity().parent {
                        children.entry(parent.clone()).or_default().push(id.clone());
                    }
                    all.insert(id.clone(), record);
                }
                _ => without += 1,
            }
        }
        let replayed = replayed_of(&generation, &reader, &all, &children);
        Ok(Loaded {
            number: generation.number(),
            all,
            children,
            without,
            names,
            replayed,
            generation,
            reader,
            kept: std::sync::Mutex::new(BTreeMap::new()),
        })
    }

    /// What the run `id` kept among its diagnostics under `name`, when it kept it.
    pub fn diagnostic(&self, id: &str, name: &str) -> Option<Vec<u8>> {
        let held = self.generation.run(id)?;
        let entry = held.get(&format!("{DIAGNOSTICS_PREFIX}{name}"))?;
        match entry.kind == whizbang_core::store::Kind::Diagnostic {
            true => self.reader.read(entry).ok(),
            false => None,
        }
    }

    /// [`Loaded::diagnostic`], read as JSON once and kept.
    fn diagnostic_json(&self, id: &str, name: &str) -> Option<serde_json::Value> {
        let key = (id.to_owned(), name.to_owned());
        let lock = || {
            self.kept
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        };
        if let Some(said) = lock().get(&key) {
            return said.clone();
        }
        let read =
            (self.diagnostic(id, name)).and_then(|bytes| serde_json::from_slice(&bytes).ok());
        lock().insert(key, read.clone());
        read
    }
}

/// The runs of a [`Loaded`] generation that a filter admits.
pub struct Runs {
    /// Each run with a record that the filter admits, in the order of their ids.
    pub found: Vec<(String, RunRecord)>,
    pub lookups: Lookups,
    pub loaded: Arc<Loaded>,
}

impl Runs {
    /// The runs of the store at `root`, read by a reader of its own, that `filter`
    /// admits; `lookups` names the kits (the names of the pictures are read here).
    pub fn matching(root: &Path, filter: &Filter, lookups: Lookups) -> Result<Runs, String> {
        Runs::within(Arc::new(Loaded::load(root)?), filter, lookups)
    }

    /// The runs of what was loaded that `filter` admits: nothing is read from the store.
    pub fn within(loaded: Arc<Loaded>, filter: &Filter, lookups: Lookups) -> Result<Runs, String> {
        let read = filter.read()?;
        let lookups = Lookups {
            kits: lookups.kits,
            names: loaded.names.clone(),
        };
        let found = (loaded.all.iter())
            .filter(|(_, record)| read.admits(record, &lookups))
            .map(|(id, record)| (id.clone(), record.clone()))
            .collect();
        Ok(Runs {
            found,
            lookups,
            loaded,
        })
    }

    /// How many runs have no record that was read.
    pub fn without(&self) -> usize {
        self.loaded.without
    }

    /// What the run `id` kept among its diagnostics under `name`, when it kept it.
    pub fn diagnostic(&self, id: &str, name: &str) -> Option<Vec<u8>> {
        self.loaded.diagnostic(id, name)
    }

    fn diagnostic_json(&self, id: &str, name: &str) -> Option<serde_json::Value> {
        self.loaded.diagnostic_json(id, name)
    }

    /// The children of a run.
    pub fn children_of(&self, id: &str) -> impl Iterator<Item = &RunRecord> {
        (self.loaded.children.get(id).into_iter().flatten())
            .filter_map(|child| self.loaded.all.get(child))
    }

    /// What the run and every run under it settled, in millionths of a US dollar.
    pub fn settled_under(&self, id: &str) -> u128 {
        let mut total = 0u128;
        let mut seen = BTreeSet::new();
        let mut next = vec![id.to_owned()];
        while let Some(run) = next.pop() {
            if !seen.insert(run.clone()) {
                continue;
            }
            if let Some(record) = self.loaded.all.get(&run) {
                let paid = record.spend().requests.iter();
                total += paid
                    .map(|paid| u128::from(paid.settled_micros()))
                    .sum::<u128>();
            }
            next.extend(self.loaded.children.get(&run).cloned().unwrap_or_default());
        }
        total
    }
}

/// Which runs of a replayed family a figure takes (`runs stats --replayed`; ADR 0049).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Replays {
    /// Leaves every replayed family out: the figures of ordinary use.
    #[default]
    Exclude,
    /// Takes them as any run.
    Include,
    /// Takes them alone.
    Only,
}

impl Replays {
    /// As `--replayed` names it: `exclude`, `include` or `only`.
    pub fn named(name: &str) -> Option<Replays> {
        match name {
            "exclude" => Some(Replays::Exclude),
            "include" => Some(Replays::Include),
            "only" => Some(Replays::Only),
            _ => None,
        }
    }
}

impl Runs {
    /// The same runs, with the runs of replayed families taken as `replays` says.
    pub fn keeping(mut self, replays: Replays) -> Runs {
        let replayed = &self.loaded.replayed;
        self.found.retain(|(id, _)| match replays {
            Replays::Include => true,
            Replays::Exclude => !replayed.contains(id),
            Replays::Only => replayed.contains(id),
        });
        self
    }
}

/// Every run of a replayed family: each whose record names the run it replays; each a
/// dead process left whose admission does, since the record of a dead process holds no
/// `replayed` (ADR 0049, "Provenance"); and every run under one of them.
fn replayed_of(
    generation: &Generation,
    reader: &Reader,
    all: &BTreeMap<String, RunRecord>,
    children: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    let admitted = |id: &str| -> bool {
        let entry = generation
            .run(id)
            .and_then(|run| run.get(whizbang_core::contract::ADMITTED_NAME));
        let bytes = entry.and_then(|entry| reader.read(entry).ok());
        let admission =
            bytes.and_then(|bytes| whizbang_core::contract::Admission::from_bytes(&bytes).ok());
        admission.is_some_and(|admission| admission.replayed().is_some())
    };
    let mut next: Vec<String> = (all.iter())
        .filter(|(id, record)| {
            record.replayed().is_some() || (result_of(record) == "interrupted" && admitted(id))
        })
        .map(|(id, _)| id.clone())
        .collect();
    let mut replayed = BTreeSet::new();
    while let Some(id) = next.pop() {
        if replayed.insert(id.clone()) {
            next.extend(children.get(&id).cloned().unwrap_or_default());
        }
    }
    replayed
}

/// `runs export`: each matching run's record as it is stored, one JSON object a line.
pub fn export(root: &Path, filter: &Filter) -> Result<Vec<u8>, String> {
    let runs = Runs::matching(root, filter, Lookups::of_build())?;
    let mut out = Vec::new();
    for (_, record) in runs.found {
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

#[allow(clippy::cast_precision_loss)]
fn share(part: usize, whole: usize) -> String {
    if whole == 0 {
        return "-".to_owned();
    }
    format!("{:.1}%", part as f64 * 100.0 / whole as f64)
}

/// Millionths of a US dollar as money, as `spend` prints it.
fn usd(micros: u128) -> String {
    let mut part = format!("{:06}", micros % 1_000_000);
    while part.len() > 2 && part.ends_with('0') {
        part.pop();
    }
    format!("${}.{part}", micros / 1_000_000)
}

/// How the runs ended, by the record's words.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Ends {
    pub completed: usize,
    pub error: usize,
    pub invalid: usize,
    pub cancelled: usize,
    pub interrupted: usize,
}

/// The verdicts of the runs that reached one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Verdicts {
    pub pass: usize,
    pub fail: usize,
    pub unsure: usize,
}

/// What a loop's runs did (plan m4, task L1, item 11).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Loops {
    pub runs: usize,
    pub attempts: usize,
    pub retries: usize,
    /// How many kept an attempt that was checked, one that was not, and none.
    pub kept_checked: usize,
    pub kept_unchecked: usize,
    pub kept_none: usize,
    /// The runs by why their attempts ended, most first: by the record's word of a closed
    /// set (`run/8`), or, in an older record, by the word its sentence reads as, or by
    /// the sentence itself, the attempt's number as `N`, where it reads as none.
    pub ended: Vec<(String, usize)>,
}

/// One cause of an unsure verdict: the use case, the rule, an item or a finding, the
/// disposition and the confidence; how many answers, and in how many verdicts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnsureCause {
    pub use_case: String,
    pub rule: String,
    pub answer: &'static str,
    pub disposition: String,
    pub confidence: String,
    pub answers: usize,
    pub verdicts: usize,
}

/// The unsure verdicts by the rule that made each unsure (plan m3, task U1, item 1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UnsureByRule {
    pub verdicts: usize,
    /// How many of them kept no checklist.
    pub unkept: usize,
    pub causes: Vec<UnsureCause>,
}

/// Time as the median and the 95th percentile, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Times {
    pub median_ms: u64,
    pub p95_ms: u64,
}

/// The figures of `runs stats` (`SPEC.md` section 3): one aggregation, which the
/// command prints and the playground answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Figures {
    pub runs: usize,
    /// Runs admitted and not ended, or whose record was not read.
    pub without_record: usize,
    pub ended: Ends,
    pub verdicts: Verdicts,
    /// Turns the harness excluded and sent again, and the retries and repairs a run of a
    /// use case kept of its judging.
    pub retries: u64,
    pub repairs: u64,
    pub loops: Option<Loops>,
    pub time: Option<Times>,
    pub settled_micros: u64,
    pub unresolved_micros: u64,
    /// Runs answered from the decision cache: each record's `decision_cache` note.
    pub cache_hits: usize,
    pub unsure_by_rule: UnsureByRule,
}

impl Figures {
    /// The figures of these runs.
    pub fn of(runs: &Runs) -> Figures {
        let found = &runs.found;
        let mut ended = Ends::default();
        for (_, record) in found {
            let outcome = record.outcome();
            let count = match (outcome.lost.is_empty(), outcome.status) {
                (false, _) => &mut ended.interrupted,
                (true, Status::Completed) => &mut ended.completed,
                (true, Status::Error) => &mut ended.error,
                (true, Status::Invalid) => &mut ended.invalid,
                (true, Status::Cancelled) => &mut ended.cancelled,
            };
            *count += 1;
        }
        let mut verdicts = Verdicts::default();
        for (_, record) in found {
            match record.verdict() {
                Some((record::Decision::Pass, _)) => verdicts.pass += 1,
                Some((record::Decision::Fail, _)) => verdicts.fail += 1,
                Some((record::Decision::Unsure, _)) => verdicts.unsure += 1,
                None => {}
            }
        }
        // Retries: turns the harness excluded and sent again. Repairs: from what a run
        // of a use case kept of its judging.
        let mut retries = 0u64;
        let mut repairs = 0u64;
        for (id, record) in found {
            let excluded = (record.judge().iter()).filter(|j| {
                matches!(
                    j,
                    Judged::Turn {
                        excluded: Some(_),
                        ..
                    }
                )
            });
            retries += excluded.count() as u64;
            if let Some(json) = runs.diagnostic_json(id, whizbang_squint::check::JUDGED) {
                let length = |key: &str| json[key].as_array().map_or(0, Vec::len) as u64;
                retries += length("retries");
                repairs += length("repairs");
            }
        }
        let mut times: Vec<u64> = (found.iter())
            .filter_map(|(_, r)| {
                let time = r.time();
                time.start_ms.map(|start| time.end_ms.saturating_sub(start))
            })
            .collect();
        times.sort_unstable();
        let time = match (rank(&times, 50), rank(&times, 95)) {
            (Some(median_ms), Some(p95_ms)) => Some(Times { median_ms, p95_ms }),
            _ => None,
        };
        let paid = || found.iter().flat_map(|(_, r)| r.spend().requests.iter());
        let settled: u128 = paid().map(|p| u128::from(p.settled_micros())).sum();
        let unresolved: u128 = paid().map(|p| u128::from(p.unresolved_micros())).sum();
        let cache_hits = (found.iter())
            .filter(|(_, record)| {
                (record.asked().settings.iter()).any(|setting| {
                    setting.name() == whizbang_squint::cache::SETTING
                        && *setting.value() == ArgumentValue::Text("hit".to_owned())
                })
            })
            .count();
        Figures {
            runs: found.len(),
            without_record: runs.without(),
            ended,
            verdicts,
            retries,
            repairs,
            loops: loops(found),
            time,
            settled_micros: u64::try_from(settled).unwrap_or(u64::MAX),
            unresolved_micros: u64::try_from(unresolved).unwrap_or(u64::MAX),
            cache_hits,
            unsure_by_rule: unsure_by_rule_of(runs),
        }
    }

    /// The figures as `runs stats` prints them.
    pub fn text(&self) -> String {
        let runs = self.runs;
        let mut text = format!("runs: {runs}\n");
        if self.without_record > 0 {
            text.push_str(&format!(
                "admitted and not ended, or with no record read: {}\n",
                self.without_record
            ));
        }
        let ended = &self.ended;
        for (word, n) in [
            ("completed", ended.completed),
            ("error", ended.error),
            ("invalid", ended.invalid),
            ("cancelled", ended.cancelled),
            ("interrupted", ended.interrupted),
        ] {
            text.push_str(&format!("{word}: {n} ({})\n", share(n, runs)));
        }
        let Verdicts { pass, fail, unsure } = self.verdicts;
        let judged = pass + fail + unsure;
        text.push_str(&format!(
            "verdicts: {judged} (pass {pass}, fail {fail}, unsure {unsure}); unsure rate {}\n",
            share(unsure, judged)
        ));
        text.push_str(&format!(
            "retries: {}\nrepairs: {}\n",
            self.retries, self.repairs
        ));
        if let Some(loops) = &self.loops {
            text.push_str(&format!(
                "loops: {} runs, {} attempts, {} retries; kept an attempt that was \
                 checked {}, not checked {}, none {}\n",
                loops.runs,
                loops.attempts,
                loops.retries,
                loops.kept_checked,
                loops.kept_unchecked,
                loops.kept_none
            ));
            for (why, count) in &loops.ended {
                text.push_str(&format!(
                    "  attempts ended\t{count}\t{}\n",
                    crate::door::printable(why)
                ));
            }
        }
        match self.time {
            Some(Times { median_ms, p95_ms }) => text.push_str(&format!(
                "time: median {median_ms} ms, 95th percentile {p95_ms} ms\n"
            )),
            None => text.push_str("time: none\n"),
        }
        text.push_str(&format!(
            "cost: {} settled, {} unresolved\n",
            usd(u128::from(self.settled_micros)),
            usd(u128::from(self.unresolved_micros))
        ));
        text.push_str(&format!("cache hits: {}\n", self.cache_hits));
        text
    }

    /// The unsure verdicts by rule, as `runs stats --unsure-by rule` prints them.
    /// Aggregates only: no run, item or reason is printed.
    pub fn unsure_text(&self) -> String {
        let by = &self.unsure_by_rule;
        let mut text = format!(
            "unsure by rule: {} unsure verdicts, {} of them with no checklist kept\n\
             use_case\trule\tanswer\tdisposition\tconfidence\tanswers\tverdicts\n",
            by.verdicts, by.unkept
        );
        let clean = |text: &str| -> String {
            let cut: String = text.chars().take(32).collect();
            cut.escape_debug().to_string()
        };
        for cause in &by.causes {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                clean(&cause.use_case),
                clean(&cause.rule),
                cause.answer,
                clean(&cause.disposition),
                clean(&cause.confidence),
                cause.answers,
                cause.verdicts
            ));
        }
        text
    }
}

/// The figures of the runs at `root` that `filter` admits, every replayed family left
/// out.
pub fn figures(root: &Path, filter: &Filter) -> Result<Figures, String> {
    figures_keeping(root, filter, Replays::Exclude)
}

/// The figures of the runs at `root` that `filter` admits, with the runs of replayed
/// families taken as `replays` says.
pub fn figures_keeping(root: &Path, filter: &Filter, replays: Replays) -> Result<Figures, String> {
    let runs = Runs::matching(root, filter, Lookups::of_build())?.keeping(replays);
    Ok(Figures::of(&runs))
}

/// `runs stats`: how many runs; how they ended; for runs that judged, their verdicts;
/// retries and repairs; time as the median and the 95th percentile; cost; cache hits.
pub fn stats(root: &Path, filter: &Filter) -> Result<String, String> {
    Ok(figures(root, filter)?.text())
}

/// What `runs stats` says of the loops among these runs (plan m4, task L1, item 11;
/// `SPEC.md` section 3, "retries and repairs"): how many runs listed attempts; the
/// attempts they made, and how many of those were retries, each attempt after a run's
/// first; how many kept an attempt that was checked, and how many one that was not;
/// and the runs by why their attempts ended, most first: by the word of a closed set the
/// record holds (`run/8`; plan mE, task L1, item 1), and in an older record by the word
/// its sentence reads as ([`crate::loop_score::reason_of`]), so old and new count
/// together; a sentence that reads as none is counted as itself, with each attempt's
/// number left out. None where no run is a loop's. Public for the tests, which hand it
/// records of a version this build no longer writes for a loop.
pub fn loops(found: &[(String, RunRecord)]) -> Option<Loops> {
    let looped: Vec<&record::Attempted> = (found.iter())
        .filter_map(|(_, record)| record.attempts())
        .collect();
    if looped.is_empty() {
        return None;
    }
    let attempts: usize = looped.iter().map(|attempts| attempts.list.len()).sum();
    let retries: usize = (looped.iter())
        .map(|attempts| attempts.list.len().saturating_sub(1))
        .sum();
    // Whether the attempt a run kept was checked: none where it kept no attempt.
    let kept = |attempts: &record::Attempted| -> Option<bool> {
        let mut list = attempts.list.iter();
        let kept = list.find(|attempt| Some(attempt.attempt) == attempts.kept);
        kept.map(|kept| kept.verdict.is_some())
    };
    let checked = looped.iter().filter(|a| kept(a) == Some(true)).count();
    let unchecked = looped.iter().filter(|a| kept(a) == Some(false)).count();
    // Why the attempts ended: the record's word, or the word its sentence reads as, or
    // else the sentence with the number of the attempt taken out so that runs that
    // ended the same way count together.
    let mut ended: BTreeMap<String, usize> = BTreeMap::new();
    for attempts in &looped {
        let word = match attempts.reason {
            Some(reason) => reason.word(),
            None => crate::loop_score::reason_of(&attempts.ended),
        };
        if word != "other" {
            *ended.entry(word.to_owned()).or_default() += 1;
            continue;
        }
        let words = attempts.ended.split(' ');
        let plain: Vec<&str> = words
            .map(|word| match word.parse::<u32>() {
                Ok(_) => "N",
                Err(_) => word,
            })
            .collect();
        *ended.entry(plain.join(" ")).or_default() += 1;
    }
    let mut ended: Vec<(String, usize)> = ended.into_iter().collect();
    ended.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Some(Loops {
        runs: looped.len(),
        attempts,
        retries,
        kept_checked: checked,
        kept_unchecked: unchecked,
        kept_none: looped.len() - checked - unchecked,
        ended,
    })
}

/// The rule an answer of a kept checklist bears on, as `--unsure-by rule` counts it: the
/// obligation of an item (`chk:ob:RE1@candidate` is `RE1`), `reference` for the item of a
/// reference, and `RQ` for a requirement of the job, whose number names nothing across
/// jobs. A finding counts once for each obligation it cites. Nothing of an item's own
/// words or images is kept: an id that is none of these is `other`.
fn rules_of(answer: &serde_json::Value) -> Vec<String> {
    let rule = |id: &str| {
        let letters: String = id.chars().take_while(char::is_ascii_uppercase).collect();
        let digits = &id[letters.len()..];
        let plain = letters.len() == 2 && !digits.is_empty();
        match () {
            _ if !plain || !digits.bytes().all(|b| b.is_ascii_digit()) => "other".to_owned(),
            _ if letters == "RQ" => "RQ".to_owned(),
            _ => id.to_owned(),
        }
    };
    if let Some(id) = answer["id"].as_str() {
        if id.starts_with("chk:ref:") {
            return vec!["reference".to_owned()];
        }
        let obligation = id
            .strip_prefix("chk:ob:")
            .and_then(|rest| rest.split('@').next());
        return vec![obligation.map_or_else(|| "other".to_owned(), rule)];
    }
    let cited = answer["obligation_ids"].as_array().map(Vec::as_slice);
    let mut rules: Vec<String> = (cited.unwrap_or_default().iter())
        .filter_map(|id| id.as_str().map(rule))
        .collect();
    rules.dedup();
    rules
}

/// One line of the count: the use case, the rule, an item or a finding, the disposition
/// and the confidence.
type Cause = (String, String, &'static str, String, String);

/// Of the runs whose verdict is unsure, each answer of the checklist they kept whose
/// row was unsure, counted by its use case, its rule, whether it was an item or a
/// finding, its disposition and its confidence: how many such answers, and in how many
/// verdicts. A verdict may have more than one.
fn unsure_by_rule_of(runs: &Runs) -> UnsureByRule {
    let mut verdicts = 0usize;
    let mut unkept = 0usize;
    let mut counted: BTreeMap<Cause, (usize, usize)> = BTreeMap::new();
    for (id, record) in &runs.found {
        if !matches!(record.verdict(), Some((record::Decision::Unsure, _))) {
            continue;
        }
        verdicts += 1;
        let json = runs.diagnostic_json(id, whizbang_squint::check::JUDGED);
        let Some(answers) = json.as_ref().and_then(|j| j["answers"].as_array()) else {
            unkept += 1;
            continue;
        };
        let use_case = json.as_ref().and_then(|j| j["use_case"].as_str());
        let use_case = use_case.unwrap_or("none").to_owned();
        let mut here: BTreeSet<Cause> = BTreeSet::new();
        for answer in answers.iter().filter(|a| a["row"] == "unsure") {
            let word = |key: &str| answer[key].as_str().unwrap_or("none").to_owned();
            let what = if answer["id"].is_string() {
                "item"
            } else {
                "finding"
            };
            for rule in rules_of(answer) {
                let cause = (
                    use_case.clone(),
                    rule,
                    what,
                    word("disposition"),
                    word("confidence"),
                );
                counted.entry(cause.clone()).or_default().0 += 1;
                here.insert(cause);
            }
        }
        for cause in here {
            counted.entry(cause).or_default().1 += 1;
        }
    }
    let mut rows: Vec<(Cause, (usize, usize))> = counted.into_iter().collect();
    // The commonest causes first; ties in the order of the cause.
    rows.sort_by(|a, b| {
        b.1.1
            .cmp(&a.1.1)
            .then(b.1.0.cmp(&a.1.0))
            .then(a.0.cmp(&b.0))
    });
    let causes = rows
        .into_iter()
        .map(
            |((use_case, rule, answer, disposition, confidence), (answers, verdicts))| {
                UnsureCause {
                    use_case,
                    rule,
                    answer,
                    disposition,
                    confidence,
                    answers,
                    verdicts,
                }
            },
        )
        .collect();
    UnsureByRule {
        verdicts,
        unkept,
        causes,
    }
}

/// `runs stats --unsure-by rule` (plan m3, task U1, item 1): [`Figures::unsure_text`].
pub fn unsure_by_rule(root: &Path, filter: &Filter) -> Result<String, String> {
    Ok(figures(root, filter)?.unsure_text())
}

/// `runs stats --rejections` (plan e1, task T1, item 3): of the runs the filter admits that
/// kept their rejections (a run of `check` under a use case of the table, which answered
/// its judge), the rejections counted by class and the runs by why each ended, every class
/// and cause in their fixed order, and how many runs of `check` under a use case kept none
/// (answered from the decision cache, or ended before the judge was asked). A name the build does not
/// know is counted as `unknown`: nothing read from the store is printed. Aggregates only.
pub fn rejections(root: &Path, filter: &Filter) -> Result<String, String> {
    use whizbang_squint::check::{Cause, REJECTIONS};
    use whizbang_squint::use_case::Class;
    let runs = Runs::matching(root, filter, Lookups::of_build())?;
    let found = &runs.found;
    let classes: Vec<&str> = Class::ALL.iter().map(|c| c.name()).collect();
    let causes: Vec<&str> = Cause::ALL.iter().map(|c| c.name()).collect();
    let mut by_class = vec![0usize; classes.len() + 1];
    let mut by_cause = vec![0usize; causes.len() + 1];
    let (mut kept, mut rejected, mut unkept) = (0usize, 0usize, 0usize);
    // The place of a name in its list, or the last place, `unknown`.
    let place = |names: &[&str], name: Option<&str>| {
        let at = name.and_then(|name| names.iter().position(|known| *known == name));
        at.unwrap_or(names.len())
    };
    for (id, record) in found {
        // A run of `check` under a use case of the table, which keeps them.
        let judged = record.asked().action.as_deref() == Some(whizbang_squint::check::NAME)
            && text_argument(record, "use_case").is_some_and(|u| u != "general");
        let Some(json) = runs.diagnostic_json(id, REJECTIONS) else {
            unkept += usize::from(judged);
            continue;
        };
        kept += 1;
        by_cause[place(&causes, json["cause"].as_str())] += 1;
        let listed = json["rejections"].as_array().map(Vec::as_slice);
        let listed = listed.unwrap_or_default();
        if !listed.is_empty() {
            rejected += 1;
        }
        for rejection in listed {
            by_class[place(&classes, rejection["class"].as_str())] += 1;
        }
    }
    let total: usize = by_class.iter().sum();
    let mut text = format!(
        "rejections: {total} in {rejected} of {kept} runs that kept them; {unkept} runs of \
         check under a use case kept none\n"
    );
    let unknown = ["unknown"];
    for (name, count) in causes.iter().chain(&unknown).zip(&by_cause) {
        text.push_str(&format!("  cause\t{name}\t{count}\n"));
    }
    for (name, count) in classes.iter().chain(&unknown).zip(&by_class) {
        text.push_str(&format!("  class\t{name}\t{count}\n"));
    }
    Ok(text)
}
